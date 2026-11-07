//! Agent 工程工具面（[RFC-0006](../../docs/architecture/rfc/RFC-0006-agent-engineering.zh-CN.md) P4）：
//! `forma_search` / `forma_get_context` / `forma_plan` / `forma_report`。
//!
//! 目标：**一次 Agent 会话可完成 §8.5 全流程，无需读取目标源码**——
//! 发现（search）→ 收敛（get_context）→ 计划（plan：校验 + 影响面 + 迁移风险 + 审批判定）→
//! 应用（forma_patch）→ 汇报（report：变更面 + 证据）。
//!
//! 这些工具是任务型的（RFC-0006 §9）：不镜像 CLI 命令，而是按 Agent 的意图组织输入输出。

use crate::dsl::DslPatchDocument;
use crate::graph::{EdgeKind, UnifiedSemanticGraph};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// 按工程类型取语义图：DSL 工程走 `dsl::lower` 桥接，联邦工程走导入图。
fn semantic_graph(project_dir: &Path) -> Result<(UnifiedSemanticGraph, String)> {
    if crate::engine::is_dsl_project(project_dir) {
        let check = crate::engine::dsl_check(project_dir)?;
        let (graph, ir) = crate::engine::dsl_graph_result(&check.program)?;
        Ok((graph, ir))
    } else {
        let outcome = crate::engine::load_project(project_dir)?;
        Ok((outcome.graph, crate::dsl::IR_VERSION.to_string()))
    }
}

/// 目标解析：`forma://` URI、实体名、`Entity.field`、api 名等。
/// （与 MCP `forma_impact` 共用同一套解析语义。）
pub fn resolve_target(graph: &UnifiedSemanticGraph, target: &str) -> Option<String> {
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

fn node_brief(graph: &UnifiedSemanticGraph, uri: &str) -> serde_json::Value {
    match graph.get(uri) {
        Some(n) => serde_json::json!({
            "uri": n.uri,
            "kind": n.kind.as_str(),
            "name": n.name,
            "file": n.source.file,
            "line": n.source.line,
        }),
        None => serde_json::json!({ "uri": uri }),
    }
}

// ── forma_search ────────────────────────────────────────────────

/// 语义模型检索：按关键词匹配节点（实体 / 字段 / API 操作 / 页面 / 图表 / 枚举 / 表列…）。
pub fn search(project_dir: &Path, query: &str, kind: Option<&str>) -> Result<serde_json::Value> {
    if query.trim().is_empty() {
        bail!("query must not be empty");
    }
    let (graph, ir) = semantic_graph(project_dir)?;
    let q = query.to_lowercase();
    let results: Vec<serde_json::Value> = graph
        .all_nodes()
        .into_iter()
        .filter(|n| kind.is_none_or(|k| n.kind.as_str() == k))
        .filter(|n| n.name.to_lowercase().contains(&q) || n.uri.to_lowercase().contains(&q))
        .take(50)
        .map(|n| {
            serde_json::json!({
                "uri": n.uri,
                "kind": n.kind.as_str(),
                "name": n.name,
                "file": n.source.file,
                "line": n.source.line,
            })
        })
        .collect();
    if results.is_empty() && kind.is_none() {
        // 空结果时给出最接近的类型提示（Agent 不必猜词表）
        return Ok(serde_json::json!({
            "query": query,
            "kind": kind,
            "total": 0,
            "results": [],
            "hint": "no match; searchable kinds: entity, entity-field, type, api-operation, ui-form, chart, table, table-column, migration",
            "irVersion": ir,
        }));
    }
    Ok(serde_json::json!({
        "query": query,
        "kind": kind,
        "total": results.len(),
        "results": results,
        "irVersion": ir,
    }))
}

// ── forma_get_context ───────────────────────────────────────────

/// 目标上下文收敛：节点声明块摘录 + 定义产物 + 依赖 / 引用者 + 影响闭包。
pub fn get_context(project_dir: &Path, target: &str) -> Result<serde_json::Value> {
    let (graph, ir) = semantic_graph(project_dir)?;
    let uri = resolve_target(&graph, target)
        .with_context(|| format!("cannot resolve target: {target}"))?;
    let node = graph.require(&uri)?;

    // 声明块摘录（DSL 来源才有；迁移链等派生节点没有源文件）
    let snippet: Option<serde_json::Value> = if node.source.file.is_empty() {
        None
    } else {
        let abs = project_dir.join(&node.source.file);
        if !abs.is_file() {
            None
        } else {
            let text = crate::util::read_text(&abs)?;
            let line = node.source.line.unwrap_or(1);
            match crate::dsl::patch::decl_block_line_span(&text, line) {
                Some((start, end)) => {
                    // 大块保护：最多摘录 120 行（超长块只给头部 + 行数说明）
                    let cap = 120usize;
                    let end_capped = end.min(start + cap - 1);
                    let lines: Vec<&str> = text
                        .lines()
                        .skip(start - 1)
                        .take(end_capped - start + 1)
                        .collect();
                    Some(serde_json::json!({
                        "file": node.source.file,
                        "startLine": start,
                        "endLine": end,
                        "truncated": end > end_capped,
                        "text": lines.join("\n"),
                    }))
                }
                None => None,
            }
        }
    };

    let edges_from = graph.edges_from(&uri);
    let edges_to = graph.edges_to(&uri);
    let collect = |kind: EdgeKind, from: bool| -> Vec<serde_json::Value> {
        let edges = if from {
            edges_from.as_slice()
        } else {
            edges_to.as_slice()
        };
        edges
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| {
                let other = if from { e.to.as_str() } else { e.from.as_str() };
                node_brief(&graph, other)
            })
            .collect()
    };

    // 影响闭包（改我 ⇒ 谁受影响）
    let impact: Vec<serde_json::Value> = graph
        .affected_by(&uri)
        .iter()
        .map(|n| {
            serde_json::json!({
                "uri": n.uri,
                "kind": n.kind.as_str(),
                "name": n.name,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "target": uri,
        "irVersion": ir,
        "node": {
            "uri": node.uri,
            "kind": node.kind.as_str(),
            "name": node.name,
            "standard": node.standard.as_str(),
            "file": node.source.file,
            "line": node.source.line,
            "data": node.data,
        },
        "snippet": snippet,
        "defines": collect(EdgeKind::Defines, true),
        "references": collect(EdgeKind::References, true),
        "referencedBy": collect(EdgeKind::References, false),
        "belongsTo": collect(EdgeKind::BelongsTo, true),
        "generates": collect(EdgeKind::Generates, true),
        "impactCount": impact.len(),
        "impact": impact,
    }))
}

// ── forma_plan ──────────────────────────────────────────────────

/// 变更计划（**试运行，零副作用**）：校验 + 编辑清单 + 影响面 + 迁移风险 + 审批判定。
///
/// 在临时副本上执行补丁事务（不重建产物），对比前后程序计算迁移操作与影响面。
pub fn plan(project_dir: &Path, patch: &DslPatchDocument) -> Result<serde_json::Value> {
    if !crate::engine::is_dsl_project(project_dir) {
        bail!("forma_plan 仅适用于 DSL 工程（forma.yaml 带 `source`）；联邦工程请用 forma_impact / forma_patch");
    }
    let loaded = crate::manifest::load_manifest(project_dir)?;
    let source = loaded.manifest.source.clone().unwrap_or_default();
    let (pre_program, _) = crate::dsl::load_program(&loaded.dir, &source)?;

    // ── 试运行：副本工程（源码 + manifest），零副作用 ──
    let tmp = crate::dsl::patch::planning_copy(project_dir, &loaded.dir, &source)?;
    let dry = crate::dsl::patch::apply_dsl_patch_dry(&tmp, patch)?;

    // ── 前后程序 → 迁移操作与风险（相对最近一次构建的快照）──
    let (post_program, post_diags) = crate::dsl::load_program(&tmp, &source)?;
    let _ = post_diags;
    let db_out = loaded
        .manifest
        .targets
        .database
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/db".into());
    // 风险评估与审批门共用同一权威实现（P5，不漂移）：
    // 迁移风险 + 权限规则变更（§8.7 → High）
    let assessment = crate::dsl::patch::assess_risk(project_dir, &db_out, &post_program, patch)?;
    let migrations = serde_json::json!({
        "pendingOps": assessment.ops.len(),
        "ops": assessment.ops.iter().map(|op| serde_json::json!({
            "kind": op.kind,
            "table": op.table,
            "summary": op.summary,
            "risk": crate::emit::classify_op(op.kind).as_str(),
        })).collect::<Vec<_>>(),
        "risk": {
            "overall": assessment.overall.as_str(),
            "approvalRequired": assessment.approval_required,
            "accessRuleChange": assessment.access_rule_change,
        },
    });

    // ── 影响面：受影响实体在**后置图**中的闭包 + 新增节点 ──
    let lowered = crate::dsl::lower::lower_to_graph(&post_program)?;
    let post_uris: std::collections::BTreeSet<String> = lowered
        .graph
        .all_nodes()
        .iter()
        .map(|n| n.uri.clone())
        .collect();
    let pre_lowered = crate::dsl::lower::lower_to_graph(&pre_program)?;
    let pre_uris: std::collections::BTreeSet<String> = pre_lowered
        .graph
        .all_nodes()
        .iter()
        .map(|n| n.uri.clone())
        .collect();
    let new_nodes: Vec<&String> = post_uris.difference(&pre_uris).collect();

    let mut impacted_entities: Vec<serde_json::Value> = Vec::new();
    for op in &dry.operations {
        if op.status != "applied" {
            continue;
        }
        let entity_uri = crate::dsl::lower::entity_node_uri(&pre_program, &op.entity);
        if !lowered.graph.has(&entity_uri) {
            continue;
        }
        let closure = lowered.graph.affected_by(&entity_uri);
        impacted_entities.push(serde_json::json!({
            "entity": op.entity,
            "closureCount": closure.len(),
            "closes": closure.iter().take(20).map(|n| n.uri.clone()).collect::<Vec<_>>(),
        }));
    }

    // expected：目标实体的**当前**声明块哈希（pre 状态）——Agent 把它带回补丁文档，
    // 应用时即可做冲突检测（RFC-0006 §16）
    let expected: Vec<serde_json::Value> = patch
        .operations
        .iter()
        .map(|op| match op {
            crate::dsl::DslPatchOperation::AddEntityField { entity, .. }
            | crate::dsl::DslPatchOperation::SetEntityFieldRequired { entity, .. }
            | crate::dsl::DslPatchOperation::AddAccessRule { entity, .. } => entity.clone(),
            crate::dsl::DslPatchOperation::AddTablePage { query_api, .. } => pre_program
                .apis
                .iter()
                .find(|a| a.name == *query_api)
                .map(|a| a.entity.clone())
                .unwrap_or_default(),
            crate::dsl::DslPatchOperation::AddEntity { entity, .. } => entity.clone(),
            crate::dsl::DslPatchOperation::AddFieldRule { entity, .. } => entity.clone(),
            crate::dsl::DslPatchOperation::AddFormField { page, .. } => pre_program
                .pages
                .iter()
                .find(|p| p.name == *page)
                .map(|p| match &p.body {
                    crate::dsl::PageBody::Form(f) => f.entity.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default(),
            crate::dsl::DslPatchOperation::SetFieldRule { entity, .. } => entity.clone(),
        })
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .filter_map(|entity| {
            crate::dsl::patch::entity_block_hash(project_dir, &pre_program, &entity)
                .map(|sha256| serde_json::json!({ "entity": entity, "sha256": sha256 }))
        })
        .collect();

    let applied = dry
        .operations
        .iter()
        .filter(|o| o.status == "applied")
        .count();
    let failed = dry.operations.len() - applied;
    let verdict = if failed > 0 {
        "blocked"
    } else if assessment.approval_required {
        "requires-approval"
    } else {
        "agent-safe"
    };

    let base_revision = crate::dsl::patch::current_revision(project_dir)?;
    Ok(serde_json::json!({
        "verdict": verdict,
        "baseRevision": base_revision,
        "operations": dry.operations,
        "diagnostics": dry.diagnostics,
        "migrations": migrations,
        "expected": expected,
        "impact": {
            "newNodes": new_nodes.len(),
            "newNodeUris": new_nodes.iter().take(30).cloned().collect::<Vec<_>>(),
            "impactedEntities": impacted_entities,
        },
        "build": dry.build,
    }))
}

// ── forma_report ────────────────────────────────────────────────

/// 变更与状态汇报：模型摘要 + 校验证据 + 待执行迁移风险 + 产物面。
///
/// 典型用法：改完 .fm 之后、提交之前——回答「现在处于什么状态、还欠什么、风险多大」。
pub fn report(project_dir: &Path) -> Result<serde_json::Value> {
    if !crate::engine::is_dsl_project(project_dir) {
        bail!("forma_report 仅适用于 DSL 工程（forma.yaml 带 `source`）");
    }
    let loaded = crate::manifest::load_manifest(project_dir)?;
    let check = crate::engine::dsl_check(project_dir)?;
    let program = &check.program;
    let errors = check.error_count();
    let warnings = check
        .diagnostics
        .iter()
        .filter(|d| d.severity == "warning")
        .count();

    // 待执行迁移（相对最近一次构建的快照）+ 风险分级
    let db_out = loaded
        .manifest
        .targets
        .database
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/db".into());
    let ops = crate::emit::pending_migration_ops(project_dir, &db_out, program)?;
    let risk = crate::emit::MigrationRiskReport::new(&ops);

    // 证据：迁移链一致性（重放模拟器，无需真库）
    let verify_issues = crate::emit::migration_verify(project_dir, program, &db_out)?;

    // 产物面：各 target 的 out 目录与关键产物存在性
    let mut targets: Vec<serde_json::Value> = Vec::new();
    {
        let t = &loaded.manifest.targets;
        let push = |targets: &mut Vec<serde_json::Value>,
                    name: &str,
                    cfg: Option<&crate::manifest::TargetConfig>,
                    key: &str| {
            if let Some(out) = cfg.and_then(|c| c.out.as_deref()) {
                let exists = project_dir.join(out).is_dir();
                targets.push(serde_json::json!({
                    "target": name,
                    "out": out,
                    "generated": exists,
                    "keyArtifact": format!("{out}/{key}"),
                }));
            }
        };
        push(
            &mut targets,
            "db",
            t.database.as_ref(),
            "migrations/0001_init.sql",
        );
        push(&mut targets, "backend", t.backend.as_ref(), "src/routes.rs");
        push(&mut targets, "web", t.frontend.as_ref(), "package.json");
        push(&mut targets, "mobile", t.mobile.as_ref(), "lib/main.dart");
    }

    Ok(serde_json::json!({
        "project": check.project_name,
        "version": check.version,
        "irVersion": program.ir_version,
        "model": {
            "entities": program.entities.len(),
            "enums": program.enums.len(),
            "apis": program.apis.len(),
            "commands": program.commands.len(),
            "pages": program.pages.len(),
            "charts": program.charts.len(),
        },
        "check": {
            "status": if errors == 0 { "pass" } else { "fail" },
            "errors": errors,
            "warnings": warnings,
            "lintCount": check.lint.len(),
        },
        "migrations": {
            "pendingOps": ops.len(),
            "requiresApproval": risk.approval_required,
            "overallRisk": risk.overall.as_str(),
            "ops": ops.iter().map(|op| serde_json::json!({
                "kind": op.kind,
                "table": op.table,
                "summary": op.summary,
                "risk": crate::emit::classify_op(op.kind).as_str(),
            })).collect::<Vec<_>>(),
        },
        "verify": {
            "migrationChain": if verify_issues.is_empty() { "pass" } else { "attention" },
            "issues": verify_issues,
        },
        "audit": {
            "recent": crate::auditing::read_recent(project_dir, 5),
        },
        "targets": targets,
    }))
}
