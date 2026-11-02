//! Forma MCP server（stdio，最小 JSON-RPC 实现）。
//!
//! 工具：forma_validate / forma_graph / forma_impact / forma_patch
//! AI 工作流：graph → impact → patch → validate。AI 不直接改文件。

use crate::engine::load_project;
use crate::validate::run_doctor;
use anyhow::Result;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TOOLS_JSON: &str = r#"[
  {
    "name": "forma_validate",
    "description": "Run all cross-standard validations on the Forma project. Returns diagnostics list.",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_graph",
    "description": "Get the Unified Semantic Graph summary: entities, their per-standard artifacts (SQL table, JSON Schema type, OpenAPI operations, UI forms, workflows), and edges.",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_impact",
    "description": "Given an entity or entity-field URI (or plain name like \"Order\" / \"Order.status\"), return the affected-node closure — every node impacted by changing it.",
    "inputSchema": {
      "type": "object",
      "properties": { "target": { "type": "string" } },
      "required": ["target"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_patch",
    "description": "Apply a semantic patch: business-level operations (add-entity-field, rename-entity-field, set-entity-field-required) propagated across SQL DDL, JSON Schema, OpenAPI and JSON Forms, then re-validated and incrementally rebuilt. Transactional.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "operations": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["add-entity-field", "rename-entity-field", "set-entity-field-required"] },
              "entity": { "type": "string" }
            },
            "required": ["op", "entity"]
          }
        }
      },
      "required": ["operations"],
      "additionalProperties": false
    }
  }
]"#;

pub fn start_mcp_server(project_dir: PathBuf) -> Result<()> {
    let dir = Arc::new(project_dir);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let method = req
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string();
        let id = req.get("id").cloned();

        let response = match method.as_str() {
            "initialize" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "forma", "version": "0.2.0" }
                }
            }),
            "notifications/initialized" => continue,
            "tools/list" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": serde_json::from_str::<serde_json::Value>(TOOLS_JSON)? }
            }),
            "tools/call" => {
                let name = req
                    .pointer("/params/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let empty = serde_json::Map::new();
                let args = req
                    .pointer("/params/arguments")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or(empty);
                let result = call_tool(&dir, &name, &args);
                match result {
                    Ok(value) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value)? }] }
                    }),
                    Err(err) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": format!("error: {err}") }],
                            "isError": true
                        }
                    }),
                }
            }
            other => {
                if let Some(id) = id {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": format!("unknown method: {other}") }
                    })
                } else {
                    continue;
                }
            }
        };
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }
    Ok(())
}

fn call_tool(
    project_dir: &Path,
    name: &str,
    args: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value> {
    match name {
        "forma_validate" => {
            let outcome = load_project(project_dir)?;
            let all: Vec<serde_json::Value> = outcome
                .diagnostics
                .iter()
                .chain(run_doctor(&outcome.graph).iter())
                .map(|d| serde_json::to_value(d).map_err(|e| anyhow::anyhow!(e)))
                .collect::<Result<_>>()?;
            let ok = all
                .iter()
                .all(|d| d.get("severity").and_then(|s| s.as_str()) != Some("error"));
            Ok(serde_json::json!({
                "ok": ok,
                "diagnostics": all,
                "nodeCount": outcome.graph.all_nodes().len(),
            }))
        }
        "forma_graph" => {
            let outcome = load_project(project_dir)?;
            let summary = outcome.graph.to_summary();
            let entities: Vec<serde_json::Value> = summary
                .nodes
                .iter()
                .filter(|n| n.kind == "entity")
                .map(|e| {
                    let artifacts: Vec<serde_json::Value> = summary
                        .edges
                        .iter()
                        .filter(|edge| edge.from == e.uri)
                        .filter_map(|edge| {
                            let target = summary.nodes.iter().find(|n| n.uri == edge.to)?;
                            Some(serde_json::json!({
                                "standard": target.standard,
                                "kind": target.kind,
                                "name": target.name,
                                "file": target.source.file,
                            }))
                        })
                        .collect();
                    serde_json::json!({ "name": e.name, "uri": e.uri, "artifacts": artifacts })
                })
                .collect();
            Ok(serde_json::json!({
                "project": project_dir.file_name().map(|s| s.to_string_lossy()).unwrap_or_default(),
                "entityCount": entities.len(),
                "nodeCount": summary.nodes.len(),
                "edgeCount": summary.edges.len(),
                "entities": entities,
            }))
        }
        "forma_impact" => {
            let outcome = load_project(project_dir)?;
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing target"))?;
            let uri = resolve_target(&outcome.graph, target)
                .ok_or_else(|| anyhow::anyhow!("cannot resolve target: {target}"))?;
            let affected = outcome.graph.affected_by(&uri);
            let nodes: Vec<serde_json::Value> = affected
                .iter()
                .map(|n| {
                    serde_json::json!({
                        "uri": n.uri,
                        "kind": n.kind.as_str(),
                        "standard": n.standard.as_str(),
                        "name": n.name,
                        "file": n.source.file,
                        "line": n.source.line,
                    })
                })
                .collect();
            Ok(
                serde_json::json!({ "target": uri, "affectedCount": nodes.len(), "affected": nodes }),
            )
        }
        "forma_patch" => {
            let patch: crate::patch::PatchDocument =
                serde_json::from_value(serde_json::Value::Object(args.clone()))?;
            let report = crate::patch::apply_patch(project_dir, &patch)?;
            Ok(serde_json::to_value(&report)?)
        }
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    }
}

fn resolve_target(graph: &crate::graph::UnifiedSemanticGraph, target: &str) -> Option<String> {
    if target.starts_with("forma://") && graph.has(target) {
        return Some(target.to_string());
    }
    let (entity_name, field_name) = match target.split_once('.') {
        Some((e, f)) => (e, Some(f)),
        None => (target, None),
    };
    let entity = graph
        .entities()
        .into_iter()
        .find(|e| e.name.eq_ignore_ascii_case(entity_name))?;
    if let Some(field) = field_name {
        let field_uri = format!("{}/field/{field}", entity.uri);
        if graph.has(&field_uri) {
            return Some(field_uri);
        }
    }
    Some(entity.uri.clone())
}
