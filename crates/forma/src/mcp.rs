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
    "description": "影响闭包：给定实体 / 字段 / 图表名（或 forma:// URI），返回受影响节点闭包（实体→字段→表列→API→页面→图表→迁移）。DSL 与联邦工程均可用（DSL 走 dsl::lower 桥接图）。",
    "inputSchema": {
      "type": "object",
      "properties": { "target": { "type": "string" } },
      "required": ["target"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_search",
    "description": "语义模型检索：按关键词匹配实体 / 字段 / 枚举 / API 操作 / 页面 / 图表 / 表列，返回 URI、类型与来源位置。发现阶段的入口——不需要读取 .fm 源码。",
    "inputSchema": {
      "type": "object",
      "properties": {
        "query": { "type": "string" },
        "kind": { "type": "string", "description": "可选：entity | entity-field | type | api-operation | ui-form | chart | table | table-column | migration" }
      },
      "required": ["query"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_get_context",
    "description": "目标上下文收敛：给定实体 / 字段 / 页面 / 图表名（或 forma:// URI），返回声明块摘录（源码片段）、定义产物（字段 / 表列）、依赖与引用者、以及影响闭包。修改前先看这个。",
    "inputSchema": {
      "type": "object",
      "properties": { "target": { "type": "string" } },
      "required": ["target"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_plan",
    "description": "变更计划（试运行，零副作用）：校验补丁操作、返回编辑清单、影响面与迁移操作风险分级（low/medium/high/extreme + 是否需要人工审批）。verdict = agent-safe | requires-approval | blocked。DSL 工程专用；应用前必调。输出 expected = 目标实体当前声明块 sha256，请原样提交为 patch 的 expected 做冲突检测。",
    "inputSchema": {
      "type": "object",
      "properties": {
        "operations": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["add-entity-field", "set-entity-field-required", "add-access-rule", "add-table-page"] },
              "entity": { "type": "string" }
            }
          }
        }
      },
      "required": ["operations"],
      "additionalProperties": false
    }
  },
  {
    "name": "forma_report",
    "description": "变更与状态汇报：模型摘要、校验证据（check / lint / 迁移链重放）、待执行迁移及其风险分级、各 target 产物面。改完 .fm 之后、提交之前调用。",
    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
  },
  {
    "name": "forma_patch",
    "description": "应用语义补丁并重建产物（事务式：编辑→重校验→重建，新引入语义错误即整体回滚）。按工程类型分发：DSL 工程用主线操作集（add-entity-field / set-entity-field-required / add-access-rule / add-table-page），联邦工程用规格导入操作集。改前先调 forma_plan 预览；支持 baseRevision（IR 版本绑定）、patchId（同 id 重复提交幂等零变更）、expected（声明块 sha256 冲突检测）与 approval（High/Extreme 或权限变更）。",
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
    crate::engine::is_dsl_project(dir)
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
                "irVersion": report.ir_version,
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
            // M3 桥接：DSL 工程用 `Program → 语义图` 的投影做影响闭包，
            // 联邦工程用导入图——两者共用同一套图 API（spec:ir §15）。
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .context("missing target")?;
            let impact_graph = if is_dsl_project(project_dir) {
                let check = crate::engine::dsl_check(project_dir)?;
                let (g, _v) = crate::engine::dsl_graph_result(&check.program)?;
                g
            } else {
                load_project(project_dir)?.graph
            };
            let uri =
                resolve_target(&impact_graph, target).context("cannot resolve target: {target}")?;
            let affected = impact_graph.affected_by(&uri);
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
            Ok(serde_json::json!({
                "target": uri,
                "affectedCount": nodes.len(),
                "affected": nodes,
            }))
        }
        // ── Agent 会话闭环（RFC-0006 P4）────────────────────
        "forma_search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .context("missing argument: query")?;
            crate::agent::search(
                project_dir,
                query,
                args.get("kind").and_then(|v| v.as_str()),
            )
        }
        "forma_get_context" => {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .context("missing argument: target")?;
            crate::agent::get_context(project_dir, target)
        }
        "forma_plan" => {
            let patch: crate::dsl::DslPatchDocument =
                serde_json::from_value(serde_json::Value::Object(args.clone()))?;
            crate::agent::plan(project_dir, &patch)
        }
        "forma_report" => crate::agent::report(project_dir),

        "forma_patch" => {
            // RFC-0006 P2：按工程类型解析补丁文档——DSL 工程用 `dsl::patch` 操作集
            // （加字段 / 加规则 / 加页面），联邦工程用规格导入侧操作集。
            let input = if is_dsl_project(project_dir) {
                crate::patch::PatchInput::Dsl(serde_json::from_value(serde_json::Value::Object(
                    args.clone(),
                ))?)
            } else {
                crate::patch::PatchInput::Federal(serde_json::from_value(
                    serde_json::Value::Object(args.clone()),
                )?)
            };
            crate::patch::apply_patch_input(project_dir, input)
        }
        other => bail!("unknown tool: {other}"),
    }
}

/// DSL 工程：解析诊断 + lint + 结构摘要。
fn dsl_check(project_dir: &Path) -> Result<serde_json::Value> {
    let check = crate::engine::dsl_check(project_dir)?;

    let parse: Vec<serde_json::Value> = check
        .diagnostics
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
    let lint: Vec<serde_json::Value> = check
        .lint
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

    Ok(serde_json::json!({
        "project": check.project_name,
        "version": check.version,
        "errors": check.error_count(),
        "diagnostics": parse,
        "lint": lint,
        "summary": crate::engine::dsl_model_summary(&check.program),
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
    let check = crate::engine::dsl_check(project_dir)?;
    if check.error_count() > 0 {
        bail!(
            "DSL has {} error(s) — run forma_check for details",
            check.error_count()
        );
    }
    // M3 桥接：DSL 工程同样导出语义图（稳定标识 + 依赖边 + 来源位置）
    let (graph, ir_version) = crate::engine::dsl_graph_result(&check.program)?;
    let graph = graph.to_summary();
    Ok(serde_json::json!({
        "project": check.project_name,
        "version": check.version,
        "irVersion": ir_version,
        "summary": crate::engine::dsl_model_summary(&check.program),
        "graph": graph,
        "nodeCount": graph.nodes.len(),
        "edgeCount": graph.edges.len(),
    }))
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
