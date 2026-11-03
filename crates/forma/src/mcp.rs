//! Forma MCP server（stdio，最小 JSON-RPC 实现）。
//!
//! 工具分两类：
//! - **DSL 工程**（forma.yaml 带 `source`，v0.3 主线）：
//!   forma_check / forma_build / forma_migrate / forma_graph（DSL 摘要）
//! - **联邦工程**（specifications 导入，v0.2 原型）：
//!   forma_validate / forma_graph（联邦图）/ forma_impact / forma_patch
//!
//! AI 工作流（DSL）：forma_graph（看模型）→ forma_check（改 .fm 后验证）→
//! forma_build（重新生成产物）→ forma_migrate（看迁移链状态）。

use crate::build::run_build;
use crate::dsl::load_program;
use crate::engine::load_project;
use crate::manifest::load_manifest;
use crate::validate::run_doctor;
use anyhow::{bail, Context, Result};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TOOLS_JSON: &str = r#"[
  {
    "name": "forma_check",
    "description": "DSL 工程：解析 .fm 源并返回语义诊断（error/warning/info），附实体/枚举/API/页面/规则摘要。改完 .fm 后先跑这个。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_build",
    "description": "DSL 工程：重新生成全部已配置 target 的产物（MySQL DDL 与迁移链、Rust 后端、React Web、Flutter、OpenAPI、图、确定性种子数据），返回每个 emitter 的产物清单。与 CLI `forma build` 完全同一条管线。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_migrate",
    "description": "DSL 工程：返回数据库迁移链状态——每个版本的编号/描述/applied/pending，以及 verify 结果（编号连续、升级回滚配对、哈希、前向重放）。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_graph",
    "description": "语义图摘要。DSL 工程：实体（字段/类型/关系）、枚举、API、页面、规则的完整清单。联邦工程（specifications 导入）：实体及其跨标准产物与边。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_viz_catalog",
    "description": "列出 Forma 支持的图表类型、形状家族、编码通道、变换算子与格式词表。AI 生成 chart 前先查这个，不读图表库文档。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_viz_describe",
    "description": "返回某个图表类型的规格说明：形状家族、必需/可选编码通道、默认格式。",
    "inputSchema": {
      "type": "object",
      "properties": { "chartType": { "type": "string" } },
      "required": ["chartType"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_viz_validate",
    "description": "DSL 工程：校验全部 chart 规格并返回 viz-* 诊断，附每张图表的语义摘要（数据源、编码、变换计划）。改完 chart 块后先跑这个再 build。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_validate",
    "description": "联邦工程（specifications 导入）：跨标准校验并返回诊断列表。DSL 工程请改用 forma_check。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_impact",
    "description": "联邦工程（specifications 导入）：给定实体或字段名，返回受影响节点闭包。DSL 工程请改用 forma_graph。",
    "inputSchema": {
      "type": "object",
      "properties": { "target": { "type": "string" } },
      "required": ["target"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_patch",
    "description": "联邦工程（specifications 导入）：应用语义补丁（add-entity-field 等）并增量重建。DSL 工程请直接编辑 .fm 后调 forma_build。",
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
                    "serverInfo": { "name": "forma", "version": env!("CARGO_PKG_VERSION") }
                }
            }),
            "tools/list" => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": serde_json::from_str::<serde_json::Value>(TOOLS_JSON)? }
            }),
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or_default();
                let name = params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string();
                let args = params
                    .get("arguments")
                    .and_then(|a| a.as_object())
                    .cloned()
                    .unwrap_or_default();
                let result = call_tool(&dir, &name, &args);
                match result {
                    Ok(value) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "content": [ { "type": "text", "text": value.to_string() } ] }
                    }),
                    Err(err) => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": format!("{err:#}") }
                    }),
                }
            }
            other => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("unknown method: {other}") }
            }),
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

/// 工程是否为 v0.3 DSL 工程（manifest 带 `source`）。
fn is_dsl_project(dir: &Path) -> bool {
    load_manifest(dir)
        .map(|m| m.manifest.source.is_some())
        .unwrap_or(false)
}

/// 迁移台账（lenient：不存在/损坏时返回空台账，与 CLI 行为一致）。
fn read_ledger_lenient(project_dir: &Path) -> crate::emit::MigrationLedger {
    let path = project_dir.join(".forma/migrations.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 工具分发（pub 供集成测试直接调用）。
pub fn call_tool(
    project_dir: &Path,
    name: &str,
    args: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value> {
    match name {
        // ── DSL 工程 ─────────────────────────────────────
        "forma_check" => dsl_check(project_dir),
        "forma_build" => {
            if !is_dsl_project(project_dir) {
                bail!("forma_build 仅适用于 DSL 工程（forma.yaml 带 `source`）");
            }
            let report = run_build(project_dir)?;
            let steps: Vec<serde_json::Value> = report
                .steps
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "emitter": s.name,
                        "fileCount": s.files.len(),
                        "files": s.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "project": report.project_name,
                "totalFiles": report.total_files(),
                "steps": steps,
            }))
        }
        "forma_migrate" => {
            if !is_dsl_project(project_dir) {
                bail!("forma_migrate 仅适用于 DSL 工程（forma.yaml 带 `source`）");
            }
            let lines = crate::emit::migration_status(project_dir)?;
            let ledger = read_ledger_lenient(project_dir);
            let entries: Vec<serde_json::Value> = ledger
                .entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "version": e.version,
                        "applied": e.applied,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "migrations": lines,
                "ledger": entries,
            }))
        }
        "forma_graph" => {
            if is_dsl_project(project_dir) {
                dsl_graph(project_dir)
            } else {
                federated_graph(project_dir)
            }
        }

        // ── 联邦工程 ─────────────────────────────────────
        // ── DSL 工程：可视化（RFC-0001）──────────────────
        "forma_viz_catalog" => Ok(crate::visualization::validate::catalog()),
        "forma_viz_describe" => {
            let chart_type = args
                .get("chartType")
                .and_then(|v| v.as_str())
                .context("missing argument: chartType")?;
            crate::visualization::validate::describe(chart_type)
                .context(format!("unknown chart type '{chart_type}'"))
        }
        "forma_viz_validate" => dsl_viz_validate(project_dir),

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
        "forma_impact" => {
            reject_dsl_only_tool(project_dir, "forma_impact")?;
            let outcome = load_project(project_dir)?;
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .context("missing target")?;
            let uri = resolve_target(&outcome.graph, target)
                .context("cannot resolve target: {target}")?;
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
            reject_dsl_only_tool(project_dir, "forma_patch")?;
            let patch: crate::patch::PatchDocument =
                serde_json::from_value(serde_json::Value::Object(args.clone()))?;
            let report = crate::patch::apply_patch(project_dir, &patch)?;
            Ok(serde_json::to_value(&report)?)
        }
        other => bail!("unknown tool: {other}"),
    }
}

/// DSL 工程禁用联邦工具时给出可操作的提示。
fn reject_dsl_only_tool(project_dir: &Path, tool: &str) -> Result<()> {
    if is_dsl_project(project_dir) {
        bail!(
            "{tool} 仅适用于联邦工程（specifications 导入）。\
             当前是 DSL 工程：请用 forma_graph（模型摘要）/ forma_check（校验）/ forma_build（重建产物）。"
        );
    }
    Ok(())
}

/// DSL 工程：解析诊断 + lint + 结构摘要。
fn dsl_check(project_dir: &Path) -> Result<serde_json::Value> {
    let loaded = load_manifest(project_dir)?;
    let source = loaded
        .manifest
        .source
        .clone()
        .context("not a DSL project (forma.yaml has no `source`)")?;
    let (program, diagnostics) = load_program(&loaded.dir, &source)?;
    let lint_diags = crate::lint::run_lint(&program, None);

    let parse: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "severity": d.severity,
                "code": d.code,
                "message": d.message,
                "file": d.file,
                "line": d.line,
            })
        })
        .collect();
    let lint: Vec<serde_json::Value> = lint_diags
        .iter()
        .map(|d| {
            serde_json::json!({
                "severity": d.severity,
                "rule": d.rule,
                "message": d.message,
                "entity": d.entity,
                "field": d.field,
            })
        })
        .collect();

    let version = program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| loaded.manifest.project_version.clone());
    Ok(serde_json::json!({
        "project": loaded.manifest.project_name,
        "version": version,
        "errors": parse.iter().filter(|d| d["severity"] == "error").count(),
        "diagnostics": parse,
        "lint": lint,
        "summary": dsl_summary(&program),
    }))
}

/// DSL 工程：模型摘要（实体/枚举/API/页面/规则）。
/// `forma_viz_validate`：全部 chart 规格的校验 + 语义摘要。
fn dsl_viz_validate(project_dir: &Path) -> Result<serde_json::Value> {
    if !is_dsl_project(project_dir) {
        bail!("forma_viz_validate 仅适用于 DSL 工程（forma.yaml 带 `source`）");
    }
    let loaded = load_manifest(project_dir)?;
    let source = loaded
        .manifest
        .source
        .clone()
        .context("not a DSL project (forma.yaml has no `source`)")?;
    let (program, mut diagnostics) = load_program(&loaded.dir, &source)?;
    diagnostics.extend(crate::dsl::check_program(&program));

    let viz: Vec<serde_json::Value> = diagnostics
        .iter()
        .filter(|d| d.code.starts_with("viz-"))
        .map(|d| {
            serde_json::json!({
                "severity": d.severity,
                "code": d.code,
                "message": d.message,
                "file": d.file,
                "line": d.line,
            })
        })
        .collect();
    let error_count = viz
        .iter()
        .filter(|d| d.get("severity").and_then(|s| s.as_str()) == Some("error"))
        .count();

    let charts: Vec<serde_json::Value> = program
        .charts
        .iter()
        .filter_map(|decl| crate::visualization::lower_chart(&program, decl).ok())
        .map(|spec| crate::visualization::spec_json(&spec))
        .collect();

    Ok(serde_json::json!({
        "chartCount": program.charts.len(),
        "errors": error_count,
        "diagnostics": viz,
        "charts": charts,
    }))
}

fn dsl_graph(project_dir: &Path) -> Result<serde_json::Value> {
    let loaded = load_manifest(project_dir)?;
    let source = loaded
        .manifest
        .source
        .clone()
        .context("not a DSL project (forma.yaml has no `source`)")?;
    let (program, diagnostics) = load_program(&loaded.dir, &source)?;
    let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
    if errors > 0 {
        bail!("DSL has {errors} error(s) — run forma_check for details");
    }
    let version = program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| loaded.manifest.project_version.clone());
    Ok(serde_json::json!({
        "project": loaded.manifest.project_name,
        "version": version,
        "summary": dsl_summary(&program),
    }))
}

fn dsl_summary(program: &crate::dsl::Program) -> serde_json::Value {
    let entities: Vec<serde_json::Value> = program
        .entities
        .iter()
        .map(|e| {
            let fields: Vec<serde_json::Value> = e
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "type": f.type_name,
                        "optional": f.optional,
                        "relation": crate::dsl::is_relation(program, &f.type_name),
                        "enum": crate::dsl::is_enum(program, &f.type_name),
                    })
                })
                .collect();
            serde_json::json!({
                "name": e.name,
                "table": crate::identity::to_snake_case(&crate::dsl::pluralize(&e.name)),
                "fields": fields,
                "indexes": e.indexes,
                "uniques": e.uniques,
            })
        })
        .collect();
    let enums: Vec<serde_json::Value> = program
        .enums
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "variants": e.variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let apis: Vec<serde_json::Value> = program
        .apis
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "entity": a.entity,
                "operations": a.operations,
            })
        })
        .collect();
    let pages: Vec<serde_json::Value> = program
        .pages
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "route": p.route,
                "kind": match p.body {
                    crate::dsl::PageBody::Table(_) => "table",
                    crate::dsl::PageBody::Form(_) => "form",
                    crate::dsl::PageBody::Detail(_) => "detail",
                    crate::dsl::PageBody::Chart(_) => "chart",
                },
            })
        })
        .collect();
    serde_json::json!({
        "entityCount": entities.len(),
        "entities": entities,
        "enums": enums,
        "apis": apis,
        "pages": pages,
        "ruleBlocks": program.rules.len(),
        "chartCount": program.charts.len(),
        "charts": program
            .charts
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "type": c.chart_type,
                    "source": c.source.as_ref().map(|(a, op)| format!("{a}.{op}")),
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// 联邦工程：原语义图摘要。
fn federated_graph(project_dir: &Path) -> Result<serde_json::Value> {
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
