//! DSL 主线补丁事务（[RFC-0006](../../docs/architecture/rfc/RFC-0006-agent-engineering.zh-CN.md) P2）。
//!
//! patch → 影响闭包 → 原位修改 **DSL 规格**（.fm）→ 重校验 → 重建产物。
//!
//! 事务语义与联邦侧一致（`crate::patch`）：快照被改文件 → 应用编辑 →
//! `check_program` 重校验；任一环节失败 ⇒ **整体回滚**到快照并报告错误诊断。
//!
//! 定位机制来自 M1 的声明级来源位置（`line` 解析期确定、`file` 合并期回填）——
//! 没有稳定定位就没有可寻址的补丁（RFC-0006 P3 的关键路径）。
//!
//! 操作集（P2 验收：**加字段 + 加规则 + 加页面**）：
//!
//! | op | 语义 |
//! |---|---|
//! | `add-entity-field` | 向实体块插入字段行（含 `@required` 属性） |
//! | `set-entity-field-required` | 切换字段行的 `@required` 属性 |
//! | `add-access-rule` | 追加实体级访问规则（优先并入既有 `access {Entity}` 块） |
//! | `add-table-page` | 追加表格页（绑定某 api 的 list 操作） |

use crate::dsl::{DslPatchDocument, DslPatchOperation, Program};
use crate::validate::Diagnostic;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 单条编辑指令：字节区间 `[start, end)` 替换为 `replacement`。
#[derive(Debug, Clone)]
struct DslEdit {
    file: String,
    action: &'static str,
    detail: String,
    start: usize,
    end: usize,
    replacement: String,
}

/// 操作级报告（与联邦 `patch::OperationReport` 同形，Agent 看到一致的补丁报告形状）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DslOpReport {
    pub op: String,
    pub entity: String,
    pub status: String, // applied | failed
    pub edits: Vec<DslEditReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DslEditReport {
    pub file: String,
    pub standard: String,
    pub action: String,
    pub detail: String,
}

/// 补丁事务报告。
#[derive(Debug, serde::Serialize)]
pub struct DslPatchReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
    /// true = patchId 已应用过，本次零变更（RFC-0006 §16 幂等）
    #[serde(default)]
    pub idempotent: bool,
    pub operations: Vec<DslOpReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub build: Option<crate::patch::BuildSummary>,
}

/// 应用 DSL 补丁（事务式，成功后重建产物）。
pub fn apply_dsl_patch(project_dir: &Path, patch: &DslPatchDocument) -> Result<DslPatchReport> {
    apply_dsl_patch_with(project_dir, patch, true, true)
}

/// 应用 DSL 补丁但**不重建产物、不跑审批门**（审批门自身的试运行路径：在副本工程上执行）。
pub fn apply_dsl_patch_dry(project_dir: &Path, patch: &DslPatchDocument) -> Result<DslPatchReport> {
    apply_dsl_patch_with(project_dir, patch, false, false)
}

fn apply_dsl_patch_with(
    project_dir: &Path,
    patch: &DslPatchDocument,
    rebuild: bool,
    // 审批门开关——只有最外层调用开启；门自身的副本试运行必须关闭（否则无限递归）
    gate: bool,
) -> Result<DslPatchReport> {
    if patch.operations.is_empty() {
        bail!("patch document has no operations");
    }
    let mut report = DslPatchReport {
        patch_id: patch.patch_id.clone(),
        base_revision: patch.base_revision.clone(),
        idempotent: false,
        operations: Vec::new(),
        diagnostics: Vec::new(),
        build: None,
    };
    // 基线错误集合：补丁只对「新引入」的错误负责（工程可带既有错误，但不许恶化）
    let baseline = crate::engine::dsl_check(project_dir)?;
    let baseline_errors: std::collections::BTreeSet<String> = baseline
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .map(|d| {
            format!(
                "{}|{}|{}",
                d.code,
                d.file.as_deref().unwrap_or(""),
                d.line.unwrap_or(0)
            )
        })
        .collect();
    // ── 幂等（RFC-0006 §16）：同一 patchId 已成功应用 ⇒ 直接返回幂等报告，零写入 ──
    let patch_id = patch.patch_id.as_deref().unwrap_or_default();
    if !patch_id.is_empty() {
        if let Some(prev) = read_patch_ledger(project_dir).patches.get(patch_id) {
            if prev.status == "applied" {
                audit_event(
                    project_dir,
                    patch,
                    "idempotent",
                    &RiskAssessment {
                        ops: vec![],
                        overall: crate::emit::MigrationRisk::Low,
                        approval_required: false,
                        access_rule_change: false,
                    },
                    patch.approval.as_ref().map(|a| a.by.clone()),
                    vec![],
                    0,
                    Some(format!("patchId {patch_id} already applied at {}", prev.ts)),
                );
                return Ok(DslPatchReport {
                    patch_id: Some(patch_id.to_string()),
                    base_revision: Some(current_revision(project_dir)?),
                    idempotent: true,
                    operations: vec![DslOpReport {
                        op: "patch".into(),
                        entity: prev.entity_list.clone(),
                        status: "already-applied".into(),
                        edits: vec![],
                        message: Some(format!(
                            "patchId {patch_id} was already applied at {}; no changes made",
                            prev.ts
                        )),
                    }],
                    diagnostics: vec![],
                    build: None,
                });
            }
        }
    }

    // ── 基线版本门（RFC-0006 §9 / §16）：baseRevision 不匹配 ⇒ 拒绝，零写入 ──
    if let Some(base) = patch.base_revision.as_deref() {
        let current = current_revision(project_dir)?;
        if !revision_matches(base, &current) {
            let message = format!(
                "patch baseRevision `{base}` does not match current revision `{current}`; rerun forma_plan"
            );
            audit_event(
                project_dir,
                patch,
                "rejected",
                &RiskAssessment {
                    ops: vec![],
                    overall: crate::emit::MigrationRisk::High,
                    approval_required: false,
                    access_rule_change: false,
                },
                patch.approval.as_ref().map(|a| a.by.clone()),
                vec![],
                0,
                Some(message.clone()),
            );
            return Ok(DslPatchReport {
                patch_id: patch.patch_id.clone(),
                base_revision: Some(current),
                idempotent: false,
                operations: vec![DslOpReport {
                    op: "base-revision".into(),
                    entity: String::new(),
                    status: "rejected".into(),
                    edits: vec![],
                    message: Some(message.clone()),
                }],
                diagnostics: vec![Diagnostic {
                    severity: "error".into(),
                    code: "patch-base-revision".into(),
                    message,
                    file: None,
                    line: None,
                    nodes: None,
                }],
                build: None,
            });
        }
    }

    // ── 冲突检测（RFC-0006 §16）：expected 声明块哈希不匹配 ⇒ 拒绝，零写入 ──
    if let Some(expected) = &patch.expected {
        let check = crate::engine::dsl_check(project_dir)?;
        let mismatches = check_expectations(project_dir, &check.program, expected);
        if !mismatches.is_empty() {
            audit_event(
                project_dir,
                patch,
                "rejected",
                &RiskAssessment {
                    ops: vec![],
                    overall: crate::emit::MigrationRisk::High,
                    approval_required: false,
                    access_rule_change: false,
                },
                patch.approval.as_ref().map(|a| a.by.clone()),
                vec![],
                0,
                Some(format!("spec conflict: {}", mismatches.join("; "))),
            );
            return Ok(DslPatchReport {
                patch_id: patch.patch_id.clone(),
                base_revision: patch.base_revision.clone(),
                idempotent: false,
                operations: expected
                    .iter()
                    .map(|e| DslOpReport {
                        op: "expectation".into(),
                        entity: e.entity.clone(),
                        status: "rejected".into(),
                        edits: vec![],
                        message: Some(format!(
                            "expected sha256 {} — rerun forma_plan for the current hash",
                            &e.sha256[..e.sha256.len().min(12)]
                        )),
                    })
                    .collect(),
                diagnostics: vec![Diagnostic {
                    severity: "error".into(),
                    code: "patch-conflict".into(),
                    message: format!(
                        "spec changed since planning: {} — rerun forma_plan and re-apply",
                        mismatches.join("; ")
                    ),
                    file: None,
                    line: None,
                    nodes: None,
                }],
                build: None,
            });
        }
    }

    // ── 审批门（P5 / RFC-0006 §8.7）：写入之前先在副本上试运行，评估风险 ──
    // 高风险（High/Extreme 迁移、权限规则变更）**必须**携带审批凭据，否则拒绝执行。
    // 零写入：此阶段只读真实工程 + 写临时副本。
    // gate=false（门自身的试运行）直接跳过——否则无限递归。
    let (assessment, dry) = if gate {
        let manifest_loaded = crate::manifest::load_manifest(project_dir)?;
        let source_globs = manifest_loaded.manifest.source.clone().unwrap_or_default();
        let tmp = planning_copy(project_dir, &manifest_loaded.dir, &source_globs)?;
        let dry = apply_dsl_patch_dry(&tmp, patch)?;
        let (post_program, _) = crate::dsl::load_program(&tmp, &source_globs)?;
        let db_out = manifest_loaded
            .manifest
            .targets
            .database
            .as_ref()
            .and_then(|t| t.out.clone())
            .unwrap_or_else(|| "generated/db".into());
        let a = assess_risk(project_dir, &db_out, &post_program, patch)?;
        (a, Some(dry))
    } else {
        // 门关闭（副本试运行）：最小风险评估仅供审计记录使用
        let manifest_loaded = crate::manifest::load_manifest(project_dir)?;
        let source_globs = manifest_loaded.manifest.source.clone().unwrap_or_default();
        let (post_program, _) = crate::dsl::load_program(project_dir, &source_globs)?;
        let db_out = manifest_loaded
            .manifest
            .targets
            .database
            .as_ref()
            .and_then(|t| t.out.clone())
            .unwrap_or_else(|| "generated/db".into());
        let a = assess_risk(project_dir, &db_out, &post_program, patch)?;
        (a, None)
    };
    if gate && assessment.approval_required && patch.approval.is_none() {
        let reasons = assessment
            .ops
            .iter()
            .filter(|op| {
                crate::emit::classify_op(op.kind) == crate::emit::MigrationRisk::High
                    || crate::emit::classify_op(op.kind) == crate::emit::MigrationRisk::Extreme
            })
            .map(|op| format!("{}: {}", op.kind, op.summary))
            .collect::<Vec<_>>();
        let mut message = if assessment.access_rule_change {
            "patch changes access rules — human approval required (RFC-0006 §8.7)".to_string()
        } else {
            "patch requires human approval (high/extreme migration risk)".to_string()
        };
        if !reasons.is_empty() {
            message.push_str(&format!("; high/extreme ops: [{}]", reasons.join("; ")));
        }
        message.push_str(
            "; provide `approval: { by, reason }` in the patch document, or preview via forma_plan",
        );
        audit_event(
            project_dir,
            patch,
            "rejected",
            &assessment,
            None,
            vec![],
            0,
            Some(message.clone()),
        );
        let planned_ops = dry
            .as_ref()
            .map(|d| d.operations.clone())
            .unwrap_or_default();
        return Ok(DslPatchReport {
            patch_id: patch.patch_id.clone(),
            base_revision: patch.base_revision.clone(),
            idempotent: false,
            operations: planned_ops
                .iter()
                .map(|o| DslOpReport {
                    op: o.op.clone(),
                    entity: o.entity.clone(),
                    status: "rejected".into(),
                    edits: vec![],
                    message: Some("rejected by approval gate".into()),
                })
                .collect(),
            diagnostics: vec![Diagnostic {
                severity: "error".into(),
                code: "patch-approval-required".into(),
                message,
                file: None,
                line: None,
                nodes: None,
            }],
            build: None,
        });
    }

    // 快照：本事务内被改过的文件 → 原始内容
    let mut snapshots: HashMap<PathBuf, String> = HashMap::new();
    let rollback = |snapshots: &HashMap<PathBuf, String>| -> Result<()> {
        for (abs, content) in snapshots {
            crate::util::write_text(abs, content)?;
        }
        Ok(())
    };

    for op in patch.operations.iter() {
        let (op_name, entity_name) = describe_op(op);
        // 每个操作前重载 Program：前序操作已改变源码，定位必须基于最新状态
        let check = crate::engine::dsl_check(project_dir)?;
        // 计划阶段失败（未知实体 / 非法操作名等）→ failed 报告并继续（无编辑，无需回滚）
        let edits = match plan_operation(project_dir, &check.program, op) {
            Ok(edits) if !edits.is_empty() => edits,
            Ok(_) => {
                report.operations.push(DslOpReport {
                    op: op_name,
                    entity: entity_name,
                    status: "failed".into(),
                    edits: vec![],
                    message: Some("no edit plan produced (unsupported combination)".into()),
                });
                continue;
            }
            Err(err) => {
                report.operations.push(DslOpReport {
                    op: op_name,
                    entity: entity_name,
                    status: "failed".into(),
                    edits: vec![],
                    message: Some(format!("{err:#}")),
                });
                continue;
            }
        };

        let mut summaries: Vec<DslEditReport> = Vec::new();
        let mut failure: Option<String> = None;

        // 同文件的多个编辑按 start 降序应用，避免偏移漂移
        let mut edits = edits;
        edits.sort_by_key(|e| std::cmp::Reverse(e.start));
        for edit in &edits {
            let abs = project_dir.join(&edit.file);
            if !abs.is_file() {
                bail!("patch target file does not exist: {}", edit.file);
            }
            // 快照只存**原始**内容（回滚用）；编辑必须基于**当前磁盘状态**——
            // 同一事务内前序操作可能已修改同一文件（否则会互相覆盖 / 偏移越界）。
            snapshots
                .entry(abs.clone())
                .or_insert_with(|| crate::util::read_text(&abs).unwrap_or_default());
            let before = crate::util::read_text(&abs)?;
            let after =
                apply_edit_span(&before, edit).context(format!("edit failed on {}", edit.file))?;
            if after == before {
                failure = Some(format!("edit produced no change in {}", edit.file));
                break;
            }
            crate::util::write_text(&abs, &after)?;
            summaries.push(DslEditReport {
                file: edit.file.clone(),
                standard: "dsl".into(),
                action: edit.action.to_string(),
                detail: edit.detail.clone(),
            });
        }

        if failure.is_none() {
            // 重校验：**新引入**的语义错误 ⇒ 回滚整个事务（设计宪法：不静默放过）
            let check = crate::engine::dsl_check(project_dir)?;
            let new_errors: Vec<String> = check
                .diagnostics
                .iter()
                .filter(|d| {
                    d.severity == "error"
                        && !baseline_errors.contains(&format!(
                            "{}|{}|{}",
                            d.code,
                            d.file.as_deref().unwrap_or(""),
                            d.line.unwrap_or(0)
                        ))
                })
                .map(crate::validate::format_diagnostic)
                .collect();
            if !new_errors.is_empty() {
                failure = Some(format!(
                    "patch introduced {} semantic error(s): {}",
                    new_errors.len(),
                    new_errors.join("; ")
                ));
            }
        }

        if let Some(msg) = failure {
            rollback(&snapshots)?;
            let files_changed: Vec<String> = summaries.iter().map(|e| e.file.clone()).collect();
            report.operations.push(DslOpReport {
                op: op_name,
                entity: entity_name,
                status: "failed".into(),
                edits: summaries,
                message: Some(msg.clone()),
            });
            report.diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "patch-rollback".into(),
                message: "patch failed and rolled back to original state".into(),
                file: None,
                line: None,
                nodes: None,
            });
            audit_event(
                project_dir,
                patch,
                "failed",
                &assessment,
                patch.approval.as_ref().map(|a| a.by.clone()),
                files_changed,
                0,
                Some(msg),
            );
            return Ok(report);
        }

        report.operations.push(DslOpReport {
            op: op_name,
            entity: entity_name,
            status: "applied".into(),
            edits: summaries,
            message: None,
        });
    }

    // 全部操作成功 → 重建产物（与 CLI / MCP forma_build 同一管线）；
    // 存在失败操作时不重建——失败的补丁不得触碰产物。
    if rebuild
        && !report.operations.is_empty()
        && report.operations.iter().all(|o| o.status == "applied")
    {
        let build = crate::build::run_build(project_dir)?;
        let files: Vec<String> = build
            .steps
            .iter()
            .flat_map(|s| s.files.iter().map(|f| f.path.clone()))
            .collect();
        report.build = Some(crate::patch::BuildSummary { files, skipped: 0 });
        report.diagnostics = build.capability_diagnostics;
    }
    audit_event(
        project_dir,
        patch,
        "applied",
        &assessment,
        patch.approval.as_ref().map(|a| a.by.clone()),
        report
            .operations
            .iter()
            .flat_map(|o| o.edits.iter().map(|e| e.file.clone()))
            .collect(),
        report.build.as_ref().map(|b| b.files.len()).unwrap_or(0),
        None,
    );
    // 幂等台账（RFC-0006 §16）：记录成功应用，供同 patchId 重复提交时去重
    if let Some(pid) = patch.patch_id.as_deref() {
        if !pid.is_empty() {
            let names: Vec<String> = patch
                .operations
                .iter()
                .map(|op| describe_op(op).0)
                .collect();
            if let Err(err) = record_patch_ledger(project_dir, pid, &names) {
                eprintln!("[warn] patch ledger update failed: {err}");
            }
        }
    }
    Ok(report)
}

/// 风险评估（P5）：迁移风险（相对最近一次构建快照）+ 权限规则变更标记。
///
/// 共享权威实现——`forma_plan` 与审批门都走这里，不允许漂移。
pub struct RiskAssessment {
    pub ops: Vec<crate::emit::MigrationOp>,
    pub overall: crate::emit::MigrationRisk,
    /// High / Extreme 迁移或权限规则变更 → 需要人工审批
    pub approval_required: bool,
    /// 补丁包含 `add-access-rule`（权限规则变更，§8.7 → High）
    pub access_rule_change: bool,
}

pub fn assess_risk(
    project_dir: &Path,
    db_out: &str,
    post_program: &Program,
    patch: &DslPatchDocument,
) -> Result<RiskAssessment> {
    let ops = crate::emit::pending_migration_ops(project_dir, db_out, post_program)?;
    let report = crate::emit::MigrationRiskReport::new(&ops);
    let access_rule_change = patch
        .operations
        .iter()
        .any(|op| matches!(op, DslPatchOperation::AddAccessRule { .. }));
    Ok(RiskAssessment {
        approval_required: report.approval_required || access_rule_change,
        overall: report.overall,
        ops,
        access_rule_change,
    })
}

/// 写审计记录（失败不阻断主流程——审计是旁路，不得让补丁本身失败）。
#[allow(clippy::too_many_arguments)]
fn audit_event(
    project_dir: &Path,
    patch: &DslPatchDocument,
    status: &str,
    assessment: &RiskAssessment,
    approval_by: Option<String>,
    files_changed: Vec<String>,
    build_files: usize,
    message: Option<String>,
) {
    let event = crate::auditing::PatchAuditEvent {
        ts: crate::util::rfc3339_now(),
        project: crate::manifest::load_manifest(project_dir)
            .map(|m| m.manifest.project_name)
            .unwrap_or_default(),
        status: status.to_string(),
        operations: patch
            .operations
            .iter()
            .map(|op| describe_op(op).0)
            .collect(),
        risk: serde_json::json!({
            "overall": assessment.overall.as_str(),
            "approvalRequired": assessment.approval_required,
            "accessRuleChange": assessment.access_rule_change,
            "pendingOps": assessment.ops.len(),
        }),
        approval_by,
        approval_reason: patch.approval.as_ref().map(|a| a.reason.clone()),
        files_changed,
        build_files,
        message,
    };
    if let Err(err) = crate::auditing::record_patch_event(project_dir, &event) {
        eprintln!("[warn] audit record failed: {err}");
    }
}

/// 把工程源（manifest + source globs 命中的 .fm）拷贝到临时目录，供试运行。
///（审批门与 `forma_plan` 共用同一实现——不允许漂移。）
pub fn planning_copy(
    project_dir: &Path,
    dir: &Path,
    source: &[String],
) -> Result<std::path::PathBuf> {
    let tmp = std::env::temp_dir().join(format!(
        "forma-plan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp)?;
    // manifest
    let manifest_src = dir.join("forma.yaml");
    if manifest_src.is_file() {
        std::fs::copy(&manifest_src, tmp.join("forma.yaml"))?;
    } else {
        bail!("forma.yaml not found");
    }
    // 源文件（保持相对路径）
    let _ = project_dir;
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for pattern in source {
        files.extend(crate::dsl::glob_files(dir, pattern));
    }
    files.sort();
    files.dedup();
    for f in &files {
        let rel = f.strip_prefix(dir)?;
        let target = tmp.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(f, &target)?;
    }
    Ok(tmp)
}

/// 操作名与目标实体（报告用；计划失败时也要能给出）。
fn describe_op(op: &DslPatchOperation) -> (String, String) {
    match op {
        DslPatchOperation::AddEntityField { entity, .. } => {
            ("add-entity-field".into(), entity.clone())
        }
        DslPatchOperation::SetEntityFieldRequired { entity, .. } => {
            ("set-entity-field-required".into(), entity.clone())
        }
        DslPatchOperation::AddAccessRule { entity, .. } => {
            ("add-access-rule".into(), entity.clone())
        }
        DslPatchOperation::AddTablePage { name, .. } => ("add-table-page".into(), name.clone()),
        DslPatchOperation::AddEntity { entity, .. } => ("add-entity".into(), entity.clone()),
        DslPatchOperation::AddFieldRule { entity, .. } => ("add-field-rule".into(), entity.clone()),
        DslPatchOperation::AddFormField { page, .. } => ("add-form-field".into(), page.clone()),
        DslPatchOperation::SetFieldRule { entity, .. } => ("set-field-rule".into(), entity.clone()),
    }
}

fn plan_operation(
    project_dir: &Path,
    program: &Program,
    op: &DslPatchOperation,
) -> Result<Vec<DslEdit>> {
    match op {
        DslPatchOperation::AddEntityField {
            entity,
            field,
            required,
        } => plan_add_field(project_dir, program, entity, field, *required),
        DslPatchOperation::SetEntityFieldRequired {
            entity,
            field,
            required,
        } => plan_set_required(project_dir, program, entity, field, *required),
        DslPatchOperation::AddAccessRule {
            entity,
            access_op: rule_op,
            roles,
        } => plan_add_access_rule(project_dir, program, entity, rule_op, roles),
        DslPatchOperation::AddTablePage {
            name,
            title,
            query_api,
            columns,
        } => plan_add_table_page(project_dir, program, name, title, query_api, columns),
        DslPatchOperation::AddEntity { entity, fields } => {
            plan_add_entity(project_dir, program, entity, fields)
        }
        DslPatchOperation::AddFieldRule {
            entity,
            field,
            constraint,
            value,
            message,
        } => plan_add_field_rule(
            project_dir,
            program,
            entity,
            field,
            constraint,
            *value,
            message,
        ),
        DslPatchOperation::AddFormField {
            page,
            field,
            widget,
        } => plan_add_form_field(project_dir, program, page, field, widget),
        DslPatchOperation::SetFieldRule {
            entity,
            field,
            constraint,
            value,
            message,
        } => plan_set_field_rule(
            project_dir,
            program,
            entity,
            field,
            constraint,
            *value,
            message,
        ),
    }
}

// ---------------------------------------------------------------------------
// 定位辅助：声明块扫描（跳过字符串 / 行注释 / 块注释）
// ---------------------------------------------------------------------------

/// 行号（1-based）→ 字节偏移。
fn line_start(text: &str, line: usize) -> usize {
    let mut off = 0usize;
    for (i, l) in text.split('\n').enumerate() {
        if i + 1 == line {
            return off;
        }
        off += l.len() + 1;
    }
    text.len()
}

/// 从 `open_idx`（`{` 的字节偏移）扫描匹配的 `}` 字节偏移。
/// 跳过字符串字面量、`//` 行注释与 `/* */` 块注释（与 DSL 词法器一致的语义）。
fn matching_brace_fm(text: &str, open_idx: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = open_idx;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' {
            // 字符串字面量（支持 \" 转义）
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// 声明块的行区间（`decl_line` → 匹配闭括号行，均 1-based）。
///
/// `forma_get_context` 用它摘录规格声明块，Agent 无需读取整个源文件。
pub fn decl_block_line_span(text: &str, decl_line: usize) -> Option<(usize, usize)> {
    let base = line_start(text, decl_line);
    let Some(rel) = text[base..].find('{') else {
        // 无块的声明（如字段行）→ 单行
        return Some((decl_line, decl_line));
    };
    let close = matching_brace_fm(text, base + rel)?;
    let end_line = text[..=close].matches('\n').count() + 1;
    Some((decl_line, end_line))
}

/// 实体块闭括号所在行的行首偏移（新行插在这里 = 块内最后一行）。
fn block_close_line_start(text: &str, decl_line: usize) -> Option<usize> {
    let open = text[line_start(text, decl_line)..].find('{')? + line_start(text, decl_line);
    let close = matching_brace_fm(text, open)?;
    // 闭括号行的行首
    let line_start = text[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
    Some(line_start)
}

fn find_entity<'a>(program: &'a Program, name: &str) -> Option<&'a crate::dsl::EntityDecl> {
    program.entities.iter().find(|e| e.name == name)
}

fn read_file(project_dir: &Path, rel: &str) -> Result<String> {
    crate::util::read_text(&project_dir.join(rel))
}

// ---------------------------------------------------------------------------
// 各操作的编辑计划
// ---------------------------------------------------------------------------

fn plan_add_field(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
    field: &crate::dsl::DslPatchField,
    required: bool,
) -> Result<Vec<DslEdit>> {
    let Some(entity) = find_entity(program, entity_name) else {
        bail!("entity \"{entity_name}\" not found in DSL sources");
    };
    let Some(file) = entity.file.clone() else {
        bail!("entity {entity_name} has no source file recorded");
    };
    let text = read_file(project_dir, &file)?;
    let Some(insert_at) = block_close_line_start(&text, entity.line) else {
        bail!("cannot locate the closing brace of entity {entity_name}");
    };
    // 字段名重复 → 编辑后 check 会报 duplicate；这里直接给出可读错误
    if entity.fields.iter().any(|f| f.name == field.name) {
        bail!("field {}.{} already exists", entity_name, field.name);
    }
    // DSL 词汇映射：`required: true` → `@required`（校验必填 + 非空列）；
    // `required: false` → `Type?`（可空列，可选语义由 `?` 承载——与解析器一致）。
    let line = if required {
        format!("  {} {} @required", field.name, field.r#type)
    } else {
        format!("  {} {}?", field.name, field.r#type)
    };
    Ok(vec![DslEdit {
        file,
        action: "insert-field",
        detail: format!("{}.{}: {}", entity_name, field.name, field.r#type),
        start: insert_at,
        end: insert_at,
        replacement: format!("{line}\n"),
    }])
}

fn plan_set_required(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
    field_name: &str,
    required: bool,
) -> Result<Vec<DslEdit>> {
    let Some(entity) = find_entity(program, entity_name) else {
        bail!("entity \"{entity_name}\" not found in DSL sources");
    };
    let Some(field) = entity.fields.iter().find(|f| f.name == field_name) else {
        bail!("field {}.{} not found", entity_name, field_name);
    };
    let Some(file) = entity.file.clone() else {
        bail!("entity {entity_name} has no source file recorded");
    };
    let text = read_file(project_dir, &file)?;
    let start = line_start(&text, field.line);
    let end = text[start..]
        .find('\n')
        .map(|p| start + p)
        .unwrap_or(text.len());
    let line = &text[start..end];
    let has_required = line.contains("@required");
    if required == has_required {
        bail!(
            "field {}.{} is already {}",
            entity_name,
            field_name,
            if required { "required" } else { "optional" }
        );
    }
    let new_line = if required {
        format!("{line} @required")
    } else {
        line.replace(" @required", "")
    };
    Ok(vec![DslEdit {
        file,
        action: "toggle-required",
        detail: format!(
            "{}.{} → {}",
            entity_name,
            field_name,
            if required { "required" } else { "optional" }
        ),
        start,
        end,
        replacement: new_line,
    }])
}

fn plan_add_access_rule(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
    rule_op: &str,
    roles: &[String],
) -> Result<Vec<DslEdit>> {
    if find_entity(program, entity_name).is_none() {
        bail!("entity \"{entity_name}\" not found in DSL sources");
    }
    if !crate::dsl::ACCESS_OPS.contains(&rule_op) {
        bail!(
            "access op '{rule_op}' must be one of {}",
            crate::dsl::ACCESS_OPS.join("|")
        );
    }
    if roles.is_empty() {
        bail!("add-access-rule requires at least one role");
    }
    let expr = if roles.len() == 1 {
        format!("role == \"{}\"", roles[0])
    } else {
        format!(
            "role in [{}]",
            roles
                .iter()
                .map(|r| format!("\"{r}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let rule_line = format!("    {rule_op} → {expr}");

    // 1) 已有 `access {Entity} {` 块 → 并入
    let files = collect_source_files(project_dir);
    for file in &files {
        let text = read_file(project_dir, file)?;
        let needle = format!("access {entity_name} {{");
        let Some(pos) = text.find(&needle) else {
            continue;
        };
        let Some(close) = matching_brace_fm(&text, pos + needle.len() - 1) else {
            continue;
        };
        let close_line_start = text[..close].rfind('\n').map(|p| p + 1).unwrap_or(0);
        return Ok(vec![DslEdit {
            file: file.clone(),
            action: "insert-access-rule",
            detail: format!("{entity_name}.{rule_op} → {}", roles.join("|")),
            start: close_line_start,
            end: close_line_start,
            replacement: format!("{rule_line}\n"),
        }]);
    }

    // 2) 任一 rules 块 → 插入 access 子块
    if let Some(rules) = program.rules.first() {
        if let Some(file) = rules.file.clone() {
            let text = read_file(project_dir, &file)?;
            if let Some(insert_at) = block_close_line_start(&text, rules.line) {
                let block = format!("  access {entity_name} {{\n{rule_line}\n  }}\n");
                return Ok(vec![DslEdit {
                    file,
                    action: "insert-access-block",
                    detail: format!("{entity_name}.{rule_op} → {}", roles.join("|")),
                    start: insert_at,
                    end: insert_at,
                    replacement: block,
                }]);
            }
        }
    }

    // 3) 没有 rules 块 → 追加新块（落在最后一个源文件，确定性）
    let file = default_target_file(project_dir)?;
    let end = file_len(project_dir, &file)?;
    let block =
        format!("\nrules PatchRules {{\n  access {entity_name} {{\n{rule_line}\n  }}\n}}\n");
    Ok(vec![DslEdit {
        file,
        action: "append-rules-block",
        detail: format!("{entity_name}.{rule_op} → {}", roles.join("|")),
        start: end,
        end,
        replacement: block,
    }])
}

fn plan_add_table_page(
    project_dir: &Path,
    program: &Program,
    name: &str,
    title: &str,
    query_api: &str,
    columns: &[String],
) -> Result<Vec<DslEdit>> {
    if program.pages.iter().any(|p| p.name == name) {
        bail!("page {name} already exists");
    }
    let Some(api) = program.apis.iter().find(|a| a.name == query_api) else {
        bail!("api \"{query_api}\" not found in DSL sources");
    };
    if !api.operations.iter().any(|o| o == "list") {
        bail!("api {query_api} does not declare the `list` operation");
    }
    if columns.is_empty() {
        bail!("add-table-page requires at least one column");
    }
    let Some(entity) = find_entity(program, &api.entity) else {
        bail!("api {query_api} references unknown entity {}", api.entity);
    };
    for c in columns {
        if !entity.fields.iter().any(|f| &f.name == c) {
            bail!("column '{c}' is not a field of entity {}", api.entity);
        }
    }
    let file = default_target_file(project_dir)?;
    let route = format!(
        "/{}",
        crate::identity::to_snake_case(name).replace('_', "-")
    );
    let block = format!(
        "\npage {name} {{\n  route \"{route}\"\n  title {{ zh \"{title}\" }}\n  table {{\n    query   {query_api}.list\n    columns [{}]\n  }}\n}}\n",
        columns.join(", ")
    );
    let end = file_len(project_dir, &file)?;
    Ok(vec![DslEdit {
        file,
        action: "append-page",
        detail: format!("{name} → {query_api}.list"),
        start: end,
        end,
        replacement: block,
    }])
}

// ---------------------------------------------------------------------------
// 文件辅助
// ---------------------------------------------------------------------------

fn collect_source_files(project_dir: &Path) -> Vec<String> {
    let Ok(loaded) = crate::manifest::load_manifest(project_dir) else {
        return vec![];
    };
    let Some(source) = loaded.manifest.source else {
        return vec![];
    };
    let (program, _) = match crate::dsl::load_program(&loaded.dir, &source) {
        Ok(x) => x,
        Err(_) => return vec![],
    };
    let mut files: Vec<String> = program
        .entities
        .iter()
        .filter_map(|e| e.file.clone())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// 补丁追加内容的默认落点：工程最后一个 .fm 源文件（确定性）。
fn last_source(project_dir: &Path) -> Result<String> {
    let files = collect_source_files(project_dir);
    files
        .last()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("project has no DSL source files"))
}

/// 与 last_source 相同（保持语义清晰：追加一律落最后一个源文件）。
fn default_target_file(project_dir: &Path) -> Result<String> {
    last_source(project_dir)
}

fn file_len(project_dir: &Path, rel: &str) -> Result<usize> {
    Ok(read_file(project_dir, rel)?.len())
}

fn apply_edit_span(text: &str, edit: &DslEdit) -> Result<String> {
    if edit.start > text.len() || edit.end > text.len() || edit.start > edit.end {
        bail!(
            "edit span [{}, {}) out of bounds (file len {})",
            edit.start,
            edit.end,
            text.len()
        );
    }
    let mut out = String::with_capacity(text.len() + edit.replacement.len());
    out.push_str(&text[..edit.start]);
    out.push_str(&edit.replacement);
    out.push_str(&text[edit.end..]);
    Ok(out)
}

/// 当前修订：项目名 + IR 版本（`<project>@<IR_VERSION>`）。
pub fn current_revision(project_dir: &Path) -> Result<String> {
    let check = crate::engine::dsl_check(project_dir)?;
    Ok(format!(
        "{}@{}",
        check.project_name, check.program.ir_version
    ))
}

/// baseRevision 兼容两种形态：裸 IR 版本或 `<project>@<IR_VERSION>`。
fn revision_matches(base: &str, current: &str) -> bool {
    base == current || base == current.rsplit('@').next().unwrap_or(current)
}

// ---------------------------------------------------------------------------
// 幂等台账 + 冲突检测（RFC-0006 §16）
// ---------------------------------------------------------------------------

/// 一条已应用补丁的台账记录（`.forma/patches.json`，运行时状态，gitignored）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatchLedgerEntry {
    /// RFC3339 UTC
    pub ts: String,
    /// applied
    pub status: String,
    /// 操作名列表（逗号拼接，便于展示）
    pub entity_list: String,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PatchLedger {
    #[serde(default)]
    pub patches: std::collections::BTreeMap<String, PatchLedgerEntry>,
}

/// 读取补丁台账（lenient：不存在/损坏时返回空）。
pub fn read_patch_ledger(project_dir: &Path) -> PatchLedger {
    let path = project_dir.join(".forma/patches.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 记录一次成功应用（幂等键 → 台账）。
fn record_patch_ledger(project_dir: &Path, patch_id: &str, op_names: &[String]) -> Result<()> {
    let dir = project_dir.join(".forma");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("patches.json");
    let mut ledger = read_patch_ledger(project_dir);
    ledger.patches.insert(
        patch_id.to_string(),
        PatchLedgerEntry {
            ts: crate::util::rfc3339_now(),
            status: "applied".into(),
            entity_list: op_names.join(","),
        },
    );
    crate::util::write_text(&path, &serde_json::to_string_pretty(&ledger)?)?;
    Ok(())
}

/// 冲突检测（RFC-0006 §16）：expected 声明块哈希 vs 当前源码。
/// 返回不匹配描述列表（空 = 全部匹配）。
fn check_expectations(
    project_dir: &Path,
    program: &Program,
    expected: &[crate::dsl::DslPatchExpectation],
) -> Vec<String> {
    let mut out = Vec::new();
    for exp in expected {
        let hash = entity_block_hash(project_dir, program, &exp.entity);
        match hash {
            None => out.push(format!(
                "entity \"{}\" not found (expected sha256 {})",
                exp.entity,
                &exp.sha256[..exp.sha256.len().min(12)]
            )),
            Some(h) if h != exp.sha256 => out.push(format!(
                "entity {} block hash mismatch (expected {}, current {})",
                exp.entity,
                &exp.sha256[..exp.sha256.len().min(12)],
                &h[..h.len().min(12)]
            )),
            _ => {}
        }
    }
    out
}

/// 实体声明块的 sha256（decl 行 → 闭括号行；`forma_plan` 回传给 Agent 构造 expected 用）。
pub fn entity_block_hash(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
) -> Option<String> {
    let entity = find_entity(program, entity_name)?;
    let file = entity.file.as_deref()?;
    let text = std::fs::read_to_string(project_dir.join(file)).ok()?;
    let (start, end) = decl_block_line_span(&text, entity.line)?;
    let lines: Vec<&str> = text.lines().skip(start - 1).take(end - start + 1).collect();
    Some(crate::util::sha256_hex(&lines.join("\n")))
}

// ---------------------------------------------------------------------------
// 扩展操作集（RFC-0006 §16 / §17）：加实体 / 加字段约束 / 加表单字段
// ---------------------------------------------------------------------------

/// 属性白名单（越界属性显式拒绝——不静默忽略）。
const ALLOWED_FIELD_ATTRS: &[&str] = &[
    "@id",
    "@default(uuid)",
    "@default(now)",
    "@required",
    "@readonly",
];

/// 控件白名单（表单字段）。
const ALLOWED_FORM_WIDGETS: &[&str] = &[
    "Input",
    "Input email",
    "Input money",
    "Input datetime",
    "Textarea",
];

/// 加实体：在最后一个实体声明之后插入 `entity E { ... }`（无实体时追加到文件末）。
fn plan_add_entity(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
    fields: &[crate::dsl::DslPatchFieldSpec],
) -> Result<Vec<DslEdit>> {
    if find_entity(program, entity_name).is_some() {
        bail!("entity \"{entity_name}\" already exists");
    }
    if fields.is_empty() {
        bail!("add-entity requires at least one field");
    }
    let mut body = String::new();
    for f in fields {
        for attr in &f.attrs {
            if !ALLOWED_FIELD_ATTRS.contains(&attr.as_str()) {
                bail!(
                    "field attribute '{attr}' is not allowed (allowed: {})",
                    ALLOWED_FIELD_ATTRS.join(", ")
                );
            }
        }
        body.push_str(&format!("  {} {}", f.name, f.r#type));
        if !f.attrs.is_empty() {
            body.push(' ');
            body.push_str(&f.attrs.join(" "));
        }
        body.push('\n');
    }
    let block = format!("\nentity {entity_name} {{\n{body}}}\n");

    // 插入点：最后一个实体块之后（保持实体聚在一起）；没有实体 → 追加到最后一个源文件末
    let (file, insert_at) = match program.entities.last() {
        Some(last) => {
            let file = last
                .file
                .clone()
                .ok_or_else(|| anyhow::anyhow!("entity {} has no source file", last.name))?;
            let text = read_file(project_dir, &file)?;
            let (_, end_line) = decl_block_line_span(&text, last.line)
                .ok_or_else(|| anyhow::anyhow!("cannot locate entity {} block", last.name))?;
            let offset = line_start(&text, end_line)
                + text.lines().nth(end_line - 1).map(|l| l.len()).unwrap_or(0);
            (file, offset)
        }
        None => {
            let file = last_source(project_dir)?;
            let text = read_file(project_dir, &file)?;
            (file, text.len())
        }
    };
    Ok(vec![DslEdit {
        file,
        action: "insert-entity",
        detail: format!("{entity_name} ({} fields)", fields.len()),
        start: insert_at,
        end: insert_at,
        replacement: block,
    }])
}

/// 加字段约束：并入既有 `field E.f { ... }`，否则在 rules 块内新增子块；无 rules 块则新建。
fn plan_add_field_rule(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
    field_name: &str,
    constraint: &str,
    value: Option<f64>,
    message: &Option<String>,
) -> Result<Vec<DslEdit>> {
    let Some(entity) = find_entity(program, entity_name) else {
        bail!("entity \"{entity_name}\" not found in DSL sources");
    };
    if !entity.fields.iter().any(|f| f.name == field_name) {
        bail!("field {entity_name}.{field_name} not found");
    }
    // 约束文案是 message block（`{ zh "…" }`）——与 DSL 的 `parse_message_block` 同形
    let msg = message
        .as_ref()
        .map(|m| format!(" {{ zh \"{}\" }}", m.replace('"', "\\\"")))
        .unwrap_or_default();
    let constraint_line = match constraint {
        "min" | "max" => {
            let Some(v) = value else {
                bail!("constraint '{constraint}' requires a numeric value");
            };
            format!("    {constraint} {v}{msg}")
        }
        "required" => format!("    required{msg}"),
        other => bail!("constraint '{other}' is not supported (allowed: min, max, required)"),
    };

    // 1) 既有 rules 块内找 `field E.f {`
    for rules in &program.rules {
        let Some(file) = rules.file.clone() else {
            continue;
        };
        let text = read_file(project_dir, &file)?;
        let Some((start_line, end_line)) = decl_block_line_span(&text, rules.line) else {
            continue;
        };
        let start_off = line_start(&text, start_line);
        let end_off = line_start(&text, end_line)
            + text.lines().nth(end_line - 1).map(|l| l.len()).unwrap_or(0);
        let needle = format!("field {entity_name}.{field_name} {{");
        if let Some(idx) = text[start_off..end_off].find(&needle) {
            let brace = start_off + idx + needle.len() - 1;
            let Some(close) = matching_brace_fm(&text, brace) else {
                continue;
            };
            let insert_at = line_start(&text, text[..close].matches('\n').count() + 1);
            return Ok(vec![DslEdit {
                file,
                action: "insert-field-constraint",
                detail: format!("{entity_name}.{field_name} {constraint}"),
                start: insert_at,
                end: insert_at,
                replacement: format!("{constraint_line}\n"),
            }]);
        }
    }

    // 2) 有 rules 块但没有该字段子块 → 在 rules 块内新增 `field E.f { ... }`
    if let Some(rules) = program.rules.first() {
        if let Some(file) = rules.file.clone() {
            let text = read_file(project_dir, &file)?;
            if let Some((_start_line, end_line)) = decl_block_line_span(&text, rules.line) {
                let close_line = end_line;
                let insert_at = line_start(&text, close_line);
                let block =
                    format!("  field {entity_name}.{field_name} {{\n{constraint_line}\n  }}\n");
                return Ok(vec![DslEdit {
                    file,
                    action: "insert-field-rule-block",
                    detail: format!("{entity_name}.{field_name} {constraint}"),
                    start: insert_at,
                    end: insert_at,
                    replacement: block,
                }]);
            }
        }
    }

    // 3) 没有 rules 块 → 追加新块
    let file = last_source(project_dir)?;
    let text = read_file(project_dir, &file)?;
    let block = format!(
        "\nrules PatchRules {{\n  field {entity_name}.{field_name} {{\n{constraint_line}\n  }}\n}}\n"
    );
    Ok(vec![DslEdit {
        file,
        action: "append-rules-block",
        detail: format!("{entity_name}.{field_name} {constraint}"),
        start: text.len(),
        end: text.len(),
        replacement: block,
    }])
}

/// 加表单字段：向页面 form 的 layout 块追加一行控件。
fn plan_add_form_field(
    project_dir: &Path,
    program: &Program,
    page_name: &str,
    field_name: &str,
    widget: &Option<String>,
) -> Result<Vec<DslEdit>> {
    let Some(page) = program.pages.iter().find(|p| p.name == page_name) else {
        bail!("page \"{page_name}\" not found in DSL sources");
    };
    let Some(file) = page.file.clone() else {
        bail!("page {page_name} has no source file recorded");
    };
    let widget = widget.clone().unwrap_or_else(|| "Input".to_string());
    if !ALLOWED_FORM_WIDGETS.contains(&widget.as_str()) {
        bail!(
            "widget '{widget}' is not allowed (allowed: {})",
            ALLOWED_FORM_WIDGETS.join(", ")
        );
    }
    let text = read_file(project_dir, &file)?;
    let Some((start_line, end_line)) = decl_block_line_span(&text, page.line) else {
        bail!("cannot locate page {page_name} block");
    };
    let start_off = line_start(&text, start_line);
    let end_off =
        line_start(&text, end_line) + text.lines().nth(end_line - 1).map(|l| l.len()).unwrap_or(0);
    let block = &text[start_off..end_off];
    let Some(layout_idx) = block.find("layout") else {
        bail!("page {page_name} has no form layout to extend");
    };
    let Some(brace_rel) = block[layout_idx..].find('{') else {
        bail!("page {page_name} has no form layout block");
    };
    let brace_rel = brace_rel + layout_idx;
    let brace = start_off + brace_rel;
    let Some(close) = matching_brace_fm(&text, brace) else {
        bail!("cannot locate the layout block of page {page_name}");
    };
    let close_line = text[..close].matches('\n').count() + 1;
    let insert_at = line_start(&text, close_line);
    // 缩进沿用块内既有条目（前一个非空行的前导空白）
    let indent = text[..insert_at]
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| {
            l.chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>()
        })
        .unwrap_or_else(|| "    ".to_string());
    Ok(vec![DslEdit {
        file,
        action: "insert-form-field",
        detail: format!("{page_name}.{field_name} ({widget})"),
        start: insert_at,
        end: insert_at,
        replacement: format!("{indent}{field_name} {widget}\n"),
    }])
}

/// 改字段约束：替换既有 min / max 值（约束行按 DSL 名匹配后原位重写；message 保留）。
fn plan_set_field_rule(
    project_dir: &Path,
    program: &Program,
    entity_name: &str,
    field_name: &str,
    constraint: &str,
    value: f64,
    _message: &Option<String>,
) -> Result<Vec<DslEdit>> {
    if !matches!(constraint, "min" | "max") {
        bail!(
            "set-field-rule only supports min | max (required cannot be changed — use set-entity-field-required)"
        );
    }
    if find_entity(program, entity_name).is_none() {
        bail!("entity \"{entity_name}\" not found in DSL sources");
    }
    for rules in &program.rules {
        let Some(file) = rules.file.clone() else {
            continue;
        };
        let text = read_file(project_dir, &file)?;
        let needle = format!("field {entity_name}.{field_name} {{");
        let Some(needle_off) = text.find(&needle) else {
            continue;
        };
        let Some(close) = matching_brace_fm(&text, needle_off + needle.len() - 1) else {
            continue;
        };
        let block_start = needle_off + needle.len();
        let block = &text[block_start..close];
        // 在子块内找以 constraint 开头且后跟数字的行
        let mut off = block_start;
        for line in block.split('\n') {
            let trimmed = line.trim();
            if trimmed.starts_with(constraint)
                && trimmed[constraint.len()..]
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() || c == '-')
            {
                let indent_len = line.len() - line.trim_start().len();
                let abs_start = off + indent_len;
                let abs_end = off + line.len();
                let rest = &trimmed[constraint.len()..]
                    .trim_start()
                    .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-')
                    .to_string();
                let indent = line[..indent_len].to_string();
                let new_line = format!("{}{} {}{}", indent, constraint, value, rest);
                return Ok(vec![DslEdit {
                    file: file.clone(),
                    action: "replace-field-constraint",
                    detail: format!("{entity_name}.{field_name} {constraint} → {value}"),
                    start: abs_start,
                    end: abs_end,
                    replacement: new_line,
                }]);
            }
            off += line.len() + 1; // +1 for \n
        }
    }
    bail!(
        "no rules block contains field \"{entity}.{field}\"; use add-field-rule first",
        entity = entity_name,
        field = field_name
    )
}
