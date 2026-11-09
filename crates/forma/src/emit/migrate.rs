//! 数据库迁移引擎：IR 快照 diff → 多版本 upgrade/rollback 迁移 + 版本台账。
//!
//! 与真实开发流程一致：
//! - `gen-mysql` 首次构建写不可变基线 `0001_init.sql`
//! - 之后每次 DSL 演进 → 结构化 diff → `{index:04}_upgrade.sql`（升级）+
//!   `{index:04}_rollback.sql`（回滚），登记进 `.forma/migrations.json` 台账，
//!   并生成 `MIGRATIONS.md` 让 SQL 更新与版本链可见。
//! - CLI `forma migrate {status|history|up|down}` 模拟应用/回滚并维护 applied 状态。

use super::mysql::{plan_tables_with_internal, render_create_table, ColumnPlan, TablePlan};
use super::replay::{replay_sql, schema_diff, schema_from_plans, SimSchema};
use super::Program;
use crate::util::{read_text, write_text};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SNAPSHOT_VERSION: u32 = 2;
const LEDGER_VERSION: u32 = 1;

// ── 快照：上次构建的 IR（含 DSL 版本） ─────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct IrSnapshot {
    pub version: u32,
    pub app_version: String,
    pub tables: Vec<TablePlan>,
}

/// 从入库的迁移文件重建台账（applied 一律 pending）。
fn ledger_from_chain(
    project_dir: &Path,
    out_dir: &str,
    app_version: &str,
) -> Result<MigrationLedger> {
    let mut ledger = MigrationLedger::default();
    for f in chain_files(project_dir, out_dir) {
        let up_abs = project_dir.join(&f.upgrade_file);
        let up_hash = if up_abs.is_file() {
            let text = read_text(&up_abs)?;
            let section = extract_section(&up_abs, UP_MARKER).unwrap_or(text);
            crate::util::sha256_hex(&section)
        } else {
            String::new()
        };
        let changes = vec![format!("迁移 #{} 已入库", f.index)];
        // 版本从迁移文件头注释恢复（"-- Migration #N — upgrade to vX.Y.Z"），
        // 拿不到时回退到当前 app_version
        let version = crate::util::read_text(&up_abs)
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|l| l.contains("upgrade to v"))
                    .and_then(|l| l.split("upgrade to v").nth(1))
                    .map(|v| v.trim().trim_end_matches('*').trim().to_string())
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| app_version.to_string());
        ledger.entries.push(MigrationEntry {
            index: f.index,
            version,
            upgrade_file: f.upgrade_file.clone(),
            rollback_file: f.rollback_file.clone(),
            changes,
            up_hash,
            down_hash: None,
            applied: false,
        });
    }
    Ok(ledger)
}

/// 链上的一个迁移（以**入库文件**为准）。
pub struct ChainFile {
    pub index: u32,
    pub upgrade_file: String,
    pub rollback_file: Option<String>,
}

/// 扫描迁移目录 → 按编号排序的链（0001_init.sql 视作 #1）。
fn chain_files(project_dir: &Path, out_dir: &str) -> Vec<ChainFile> {
    let dir = project_dir.join(out_dir).join("migrations");
    let mut ups: Vec<(u32, String)> = Vec::new();
    let mut rollbacks: Vec<(u32, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !name.ends_with(".sql") {
            continue;
        }
        let Some(num) = name.split('_').next() else {
            continue;
        };
        let Ok(index) = num.parse::<u32>() else {
            continue;
        };
        let rel = format!("{out_dir}/migrations/{name}");
        if name.contains("rollback") {
            rollbacks.push((index, rel));
        } else {
            ups.push((index, rel));
        }
    }
    ups.sort();
    rollbacks.sort();
    ups.into_iter()
        .map(|(index, upgrade_file)| ChainFile {
            index,
            upgrade_file,
            rollback_file: rollbacks
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, p)| p.clone()),
        })
        .collect()
}

/// SimSchema（重放结果）→ TablePlan（迁移 diff 的输入形态）。
fn sim_to_plans(schema: &crate::emit::replay::SimSchema) -> Vec<TablePlan> {
    schema
        .tables
        .iter()
        .map(|t| TablePlan {
            entity: t.name.clone(),
            table: t.name.clone(),
            fulltext: vec![],
            columns: t
                .columns
                .iter()
                .map(|c| ColumnPlan {
                    name: c.name.clone(),
                    sql_type: c.sql_type.clone(),
                    nullable: c.nullable,
                    unique: c.unique
                        || t.uniques
                            .iter()
                            .any(|(_, cols)| cols.len() == 1 && cols[0] == c.name),
                })
                .collect(),
            primary_key: t.primary_key.clone(),
            // 与 plan_tables 的表示对齐：单列 UNIQUE KEY → 列级 unique 标记；
            // 多列 UNIQUE KEY → 表级 uniques（否则 diff 会产生伪 ALTER）
            uniques: t
                .uniques
                .iter()
                .filter(|(_, cols)| cols.len() > 1)
                .map(|(_, cols)| cols.clone())
                .collect(),
            indexes: t.indexes.iter().map(|(_, cols)| cols.clone()).collect(),
            fks: t
                .fks
                .iter()
                .map(|(_, col, ref_table)| crate::emit::mysql::FkPlan {
                    column: col.clone(),
                    ref_table: ref_table.clone(),
                })
                .collect(),
        })
        .collect()
}

/// 从**已入库的迁移链**重建「链上最新 schema」：
/// 新克隆仓库没有 IR 快照，但迁移 SQL 是权威历史——重放它们即可还原基线，
/// 增量链因此可以在一台新机器上继续生成（快照只是加速缓存）。
fn rebuild_snapshot_from_chain(project_dir: &Path, out_dir: &str) -> Option<IrSnapshot> {
    let dir = project_dir.join(out_dir).join("migrations");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.ends_with(".sql") && !name.contains("rollback") && !name.starts_with("ir-snapshot")
        })
        .collect();
    files.sort();
    let mut schema = crate::emit::replay::SimSchema::default();
    let mut any = false;
    for f in files {
        let sql = crate::util::read_text(&f).ok()?;
        if crate::emit::replay::replay_sql(&mut schema, &sql).is_err() {
            // 重放失败（超出模拟器子集）→ 放弃重建，交由快照路径处理
            return None;
        }
        any = true;
    }
    if !any {
        return None;
    }
    Some(IrSnapshot {
        version: SNAPSHOT_VERSION,
        app_version: String::new(),
        tables: sim_to_plans(&schema),
    })
}

/// IR 快照随迁移链入库（`{out_dir}/migrations/ir-snapshot.json`）：
/// 增量迁移必须能在一台新机器上从「已入库的迁移链」继续——
/// 若快照只存本机（.forma/，gitignored），克隆仓库就无法重建后续迁移。
fn snapshot_path(project_dir: &Path, out_dir: &str) -> PathBuf {
    project_dir
        .join(out_dir)
        .join("migrations")
        .join("ir-snapshot.json")
}

fn read_snapshot(project_dir: &Path, out_dir: &str) -> Option<IrSnapshot> {
    let p = snapshot_path(project_dir, out_dir);
    let text = crate::util::read_text(&p).ok()?;
    let snap: IrSnapshot = serde_json::from_str(&text).ok()?;
    if snap.version == SNAPSHOT_VERSION {
        Some(snap)
    } else {
        None
    }
}

fn save_snapshot(
    project_dir: &Path,
    out_dir: &str,
    app_version: &str,
    tables: &[TablePlan],
) -> Result<()> {
    let snap = IrSnapshot {
        version: SNAPSHOT_VERSION,
        app_version: app_version.into(),
        tables: tables.to_vec(),
    };
    write_text(
        &snapshot_path(project_dir, out_dir),
        &serde_json::to_string_pretty(&snap)?,
    )?;
    Ok(())
}

// ── 版本台账：每一次 schema 变更的不可变记录 + applied 状态 ─

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEntry {
    pub index: u32,
    /// 该迁移升级到的 DSL 版本
    pub version: String,
    pub upgrade_file: String,
    pub rollback_file: Option<String>,
    pub changes: Vec<String>,
    pub up_hash: String,
    pub down_hash: Option<String>,
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationLedger {
    pub version: u32,
    pub entries: Vec<MigrationEntry>,
}

impl Default for MigrationLedger {
    fn default() -> Self {
        Self {
            version: LEDGER_VERSION,
            entries: Vec::new(),
        }
    }
}

fn ledger_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".forma").join("migrations.json")
}

/// 台账缺失（新克隆 / CI 干净 checkout）时从入库迁移链**只读**重建。
/// 不写 `.forma/migrations.json`，避免只读工具产生副作用。
pub fn read_ledger_lenient_public(project_dir: &Path) -> Result<MigrationLedger> {
    let ledger = read_ledger(project_dir);
    if !ledger.entries.is_empty() {
        return Ok(ledger);
    }
    // 非 DSL 工程 / 语义错误工程：没有迁移链可读，返回空台账（不报错）。
    let loaded = match crate::manifest::load_manifest(project_dir) {
        Ok(m) => m,
        Err(_) => return Ok(MigrationLedger::default()),
    };
    let program = match crate::build::load_dsl_program(project_dir) {
        Ok((_, p)) => p,
        Err(_) => return Ok(MigrationLedger::default()),
    };
    let out = loaded
        .manifest
        .targets
        .database
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/db".into());
    ledger_from_committed_chain(project_dir, &out, &program)
}

/// 从入库迁移链重建台账（只读，不写盘）。
pub fn ledger_from_committed_chain(
    project_dir: &Path,
    out_dir: &str,
    program: &Program,
) -> Result<MigrationLedger> {
    let app_version = app_version_of(program);
    ledger_from_chain(project_dir, out_dir, &app_version)
}

fn read_ledger(project_dir: &Path) -> MigrationLedger {
    read_text(&ledger_path(project_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_ledger(project_dir: &Path, ledger: &MigrationLedger) -> Result<()> {
    write_text(
        &ledger_path(project_dir),
        &serde_json::to_string_pretty(ledger)?,
    )?;
    Ok(())
}

/// 目录内已有的最大迁移序号（rollback 文件不计入）。
fn migration_index(migrations_dir: &Path) -> u32 {
    let mut max = 0u32;
    if let Ok(entries) = std::fs::read_dir(migrations_dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with("_rollback.sql") {
                continue;
            }
            if let Some(idx) = name.split('_').next().and_then(|s| s.parse::<u32>().ok()) {
                if idx > max {
                    max = idx;
                }
            }
        }
    }
    max
}

// ── 结构化 diff：每一步变更都携带升级 + 回滚语句 ──────────

#[derive(Debug, Clone)]
pub struct MigrationOp {
    pub kind: &'static str,
    pub table: String,
    pub summary: String,
    /// 升级 SQL（按序执行）
    pub up: Vec<String>,
    /// 回滚 SQL（逆序执行）
    pub down: Vec<String>,
}

// ── 迁移风险分级（见 docs/migration-safety.zh-CN.md） ──────────
//
// 不是所有 SQL 变更都能安全自动回滚：删除列/表、改类型、改主键在回滚时
// 会丢失数据或破坏兼容。分级是 `forma migrate risk` 与 apply 前置门禁的依据。

/// 迁移操作的破坏性风险等级（升序：Low < Medium < High < Extreme）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigrationRisk {
    /// 纯增量、可逆（建表 / 加列 / 加索引 / 加唯一 / 加外键）。
    Low,
    /// 可逆但影响查询计划（删索引 / 删唯一 / 删外键）。
    Medium,
    /// 数据兼容风险或回滚丢数据（改列类型 / 删列）。
    High,
    /// 数据销毁（删表）。
    Extreme,
}

impl MigrationRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            MigrationRisk::Low => "low",
            MigrationRisk::Medium => "medium",
            MigrationRisk::High => "high",
            MigrationRisk::Extreme => "extreme",
        }
    }

    /// 稳定机器可读代码（结构化诊断 / 自动化门禁用）。
    pub fn code(self) -> &'static str {
        match self {
            MigrationRisk::Low => "RISK_LOW",
            MigrationRisk::Medium => "RISK_MEDIUM",
            MigrationRisk::High => "RISK_HIGH",
            MigrationRisk::Extreme => "RISK_EXTREME",
        }
    }

    /// 高风险（High/Extreme）变更在 `up --apply` 前必须经过人工审批。
    pub fn requires_approval(self) -> bool {
        matches!(self, MigrationRisk::High | MigrationRisk::Extreme)
    }
}

/// 单条迁移操作 → 风险等级。
///
/// 分级依据（与 docs/migration-safety.zh-CN.md 的对照表一致）。
/// 未识别操作按 fail-safe 归为 High，避免静默放过破坏性变更。
pub fn classify_op(kind: &str) -> MigrationRisk {
    match kind {
        "create_table" | "add_column" | "add_index" | "add_unique" | "add_fk" | "add_fulltext" => {
            MigrationRisk::Low
        }
        "drop_index" | "drop_unique" | "drop_fk" | "drop_fulltext" => MigrationRisk::Medium,
        "modify_column" | "drop_column" => MigrationRisk::High,
        "drop_table" => MigrationRisk::Extreme,
        _ => MigrationRisk::High,
    }
}

/// 一组操作 → 整体风险（取最高级）+ 各级计数 + 是否需审批。
#[derive(Debug, Clone)]
pub struct MigrationRiskReport {
    pub overall: MigrationRisk,
    pub counts: std::collections::BTreeMap<MigrationRisk, usize>,
    pub approval_required: bool,
}

impl MigrationRiskReport {
    pub fn new(ops: &[MigrationOp]) -> Self {
        let mut counts = std::collections::BTreeMap::new();
        let mut overall = MigrationRisk::Low;
        for op in ops {
            let r = classify_op(op.kind);
            *counts.entry(r).or_insert(0) += 1;
            if r > overall {
                overall = r;
            }
        }
        let approval_required = overall.requires_approval();
        Self {
            overall,
            counts,
            approval_required,
        }
    }
}

/// 生成人类可读的风险报告行（供 `forma migrate risk` 与 verify 复用）。
pub fn migration_risk_lines(ops: &[MigrationOp]) -> Vec<String> {
    if ops.is_empty() {
        return vec!["无待执行迁移（schema 未变）".into()];
    }
    let report = MigrationRiskReport::new(ops);
    let mut lines = Vec::new();
    lines.push(format!(
        "迁移风险概览：整体 {}（{}）{}",
        report.overall.as_str().to_uppercase(),
        report.overall.code(),
        if report.approval_required {
            " — 需人工审批"
        } else {
            ""
        }
    ));
    for r in [
        MigrationRisk::Extreme,
        MigrationRisk::High,
        MigrationRisk::Medium,
        MigrationRisk::Low,
    ] {
        if let Some(&n) = report.counts.get(&r) {
            lines.push(format!("  {} {:>2} 项", r.as_str().to_uppercase(), n));
        }
    }
    lines.push("逐项：".into());
    for op in ops {
        let r = classify_op(op.kind);
        let flag = if r.requires_approval() {
            " ⚠ 需审批"
        } else {
            ""
        };
        lines.push(format!(
            "  [{}] {} {}",
            r.as_str().to_uppercase(),
            op.summary,
            flag
        ));
    }
    lines
}

/// 计算「自上次构建以来」的待执行迁移操作（旧快照 → 当前 DSL schema）。
/// 首次构建（无快照）视为全量建表，均为 Low。
/// 供 `forma migrate risk` 在 apply 之前评估破坏性。
pub fn pending_migration_ops(
    project_dir: &Path,
    out_dir: &str,
    program: &Program,
) -> Result<Vec<MigrationOp>> {
    let new_plans = plan_tables_with_internal(program);
    match read_snapshot(project_dir, out_dir) {
        Some(snap) => Ok(diff_ops(&snap.tables, &new_plans)),
        None => Ok(diff_ops(&[], &new_plans)),
    }
}

/// 表结构差异 → 带回滚配对的结构化操作序列。
pub fn diff_ops(old: &[TablePlan], new: &[TablePlan]) -> Vec<MigrationOp> {
    let mut ops: Vec<MigrationOp> = Vec::new();
    let old_by_table: std::collections::HashMap<&str, &TablePlan> =
        old.iter().map(|t| (t.table.as_str(), t)).collect();
    let new_by_table: std::collections::HashMap<&str, &TablePlan> =
        new.iter().map(|t| (t.table.as_str(), t)).collect();

    // 1. 新增表
    for t in new {
        if !old_by_table.contains_key(t.table.as_str()) {
            ops.push(MigrationOp {
                kind: "create_table",
                table: t.table.clone(),
                summary: format!("新增表 {}（{} 列）", t.table, t.columns.len()),
                up: vec![render_create_table(t)],
                down: vec![format!("DROP TABLE IF EXISTS {};", t.table)],
            });
        }
    }
    // 2. 删除表
    for t in old.iter().rev() {
        if !new_by_table.contains_key(t.table.as_str()) {
            ops.push(MigrationOp {
                kind: "drop_table",
                table: t.table.clone(),
                summary: format!("删除表 {}", t.table),
                up: vec![format!("DROP TABLE IF EXISTS {};", t.table)],
                down: vec![render_create_table(t)],
            });
        }
    }
    // 3. 共有表的列/索引/外键差异
    // MySQL 安全顺序（每表内）：
    //   ① 先删约束（外键/索引/唯一键）——它们引用即将被删的列，
    //     若先 DROP COLUMN，MySQL 会连带删索引，后续 DROP INDEX 反而报不存在；
    //   ② 再加列/改列——后续新索引要引用新列；
    //   ③ 再加约束——此时新列已存在；
    //   ④ 最后删列。
    // 回滚 = 逆序执行每个 op 的 down，同样自洽。
    for t in new {
        let Some(old_t) = old_by_table.get(t.table.as_str()) else {
            continue;
        };
        ops.extend(
            index_ops(t, old_t)
                .into_iter()
                .filter(|op| matches!(op.kind, "drop_index" | "drop_unique" | "drop_fulltext")),
        );
        ops.extend(
            fk_ops(t, old_t)
                .into_iter()
                .filter(|op| op.kind == "drop_fk"),
        );
        ops.extend(
            column_ops(t, old_t)
                .into_iter()
                .filter(|op| matches!(op.kind, "add_column" | "modify_column")),
        );
        ops.extend(
            index_ops(t, old_t)
                .into_iter()
                .filter(|op| matches!(op.kind, "add_index" | "add_unique" | "add_fulltext")),
        );
        ops.extend(
            fk_ops(t, old_t)
                .into_iter()
                .filter(|op| op.kind == "add_fk"),
        );
        ops.extend(
            column_ops(t, old_t)
                .into_iter()
                .filter(|op| op.kind == "drop_column"),
        );
    }
    ops
}

/// 回滚时重建被删列（列级 @unique 由配对的 drop_unique.down 重建，这里不带 UNIQUE）。
fn column_def(col: &ColumnPlan) -> String {
    let null_sql = if col.nullable { "NULL" } else { "NOT NULL" };
    format!("{} {} {}", col.name, col.sql_type, null_sql)
}

fn column_ops(new: &TablePlan, old: &TablePlan) -> Vec<MigrationOp> {
    let mut out = Vec::new();
    let t = &new.table;
    let old_by_name: std::collections::HashMap<&str, &ColumnPlan> =
        old.columns.iter().map(|c| (c.name.as_str(), c)).collect();

    // 新增列（存量表回填安全：以 NULL 落地）
    for col in &new.columns {
        if !old_by_name.contains_key(col.name.as_str()) {
            let null_sql = if col.nullable {
                "NULL".to_string()
            } else {
                "NULL /* 回填后可收紧为 NOT NULL */".to_string()
            };
            let up_sql = format!(
                "ALTER TABLE {t} ADD COLUMN {} {} {};",
                col.name, col.sql_type, null_sql
            );
            out.push(MigrationOp {
                kind: "add_column",
                table: t.clone(),
                summary: format!(
                    "{t}: 新增列 {} {}（可空，便于存量回填）",
                    col.name, col.sql_type
                ),
                up: vec![up_sql],
                down: vec![format!("ALTER TABLE {t} DROP COLUMN {};", col.name)],
            });
        }
    }
    // 删除列
    for col in &old.columns {
        if !new.columns.iter().any(|c| c.name == col.name) {
            let up_sql = format!("ALTER TABLE {t} DROP COLUMN {};", col.name);
            let down_sql = format!("ALTER TABLE {t} ADD COLUMN {};", column_def(col));
            out.push(MigrationOp {
                kind: "drop_column",
                table: t.clone(),
                summary: format!("{t}: 删除列 {} {}", col.name, col.sql_type),
                up: vec![up_sql],
                down: vec![down_sql],
            });
        }
    }
    // 类型/可空修改
    for col in &new.columns {
        let Some(old_col) = old_by_name.get(col.name.as_str()) else {
            continue;
        };
        if old_col.sql_type != col.sql_type || old_col.nullable != col.nullable {
            let new_null = if col.nullable { "NULL" } else { "NOT NULL" };
            let old_null = if old_col.nullable { "NULL" } else { "NOT NULL" };
            let up_sql = format!(
                "ALTER TABLE {t} MODIFY COLUMN {} {} {};",
                col.name, col.sql_type, new_null
            );
            let down_sql = format!(
                "ALTER TABLE {t} MODIFY COLUMN {} {} {};",
                col.name, old_col.sql_type, old_null
            );
            out.push(MigrationOp {
                kind: "modify_column",
                table: t.clone(),
                summary: format!(
                    "{t}: 列 {} 类型 {} {} → {} {}",
                    col.name, old_col.sql_type, old_null, col.sql_type, new_null
                ),
                up: vec![up_sql],
                down: vec![down_sql],
            });
        }
    }
    out
}

fn index_ops(new: &TablePlan, old: &TablePlan) -> Vec<MigrationOp> {
    let mut out = Vec::new();
    let t = &new.table;
    // 唯一约束（块级 unique [..]）
    for cols in new.uniques.iter() {
        if !old.uniques.contains(cols) {
            let name = super::mysql::key_name("uq", t, cols);
            out.push(MigrationOp {
                kind: "add_unique",
                table: t.clone(),
                summary: format!("{t}: 新增唯一约束 {} ({})", name, cols.join(", ")),
                up: vec![format!(
                    "ALTER TABLE {t} ADD UNIQUE KEY {name} ({});",
                    cols.join(", ")
                )],
                down: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
            });
        }
    }
    for cols in old.uniques.iter() {
        if !new.uniques.contains(cols) {
            let name = super::mysql::key_name("uq", t, cols);
            out.push(MigrationOp {
                kind: "drop_unique",
                table: t.clone(),
                summary: format!("{t}: 删除唯一约束 {} ({})", name, cols.join(", ")),
                up: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
                down: vec![format!(
                    "ALTER TABLE {t} ADD UNIQUE KEY {name} ({});",
                    cols.join(", ")
                )],
            });
        }
    }
    // 普通索引
    for cols in new.indexes.iter() {
        if !old.indexes.contains(cols) {
            let name = super::mysql::key_name("idx", t, cols);
            out.push(MigrationOp {
                kind: "add_index",
                table: t.clone(),
                summary: format!("{t}: 新增索引 {} ({})", name, cols.join(", ")),
                up: vec![format!(
                    "ALTER TABLE {t} ADD INDEX {name} ({});",
                    cols.join(", ")
                )],
                down: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
            });
        }
    }
    for cols in old.indexes.iter() {
        if !new.indexes.contains(cols) {
            let name = super::mysql::key_name("idx", t, cols);
            out.push(MigrationOp {
                kind: "drop_index",
                table: t.clone(),
                summary: format!("{t}: 删除索引 {} ({})", name, cols.join(", ")),
                up: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
                down: vec![format!(
                    "ALTER TABLE {t} ADD INDEX {name} ({});",
                    cols.join(", ")
                )],
            });
        }
    }
    // 列级 @unique 变化
    // 注意：新列（old 中不存在）也可以带 @unique —— 必须补命名唯一键，
    // 否则 ADD COLUMN 后唯一约束丢失（数据完整性回退）。
    let old_by_name: std::collections::HashMap<&str, &ColumnPlan> =
        old.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    for col in &new.columns {
        let had_unique = old_by_name
            .get(col.name.as_str())
            .map(|c| c.unique)
            .unwrap_or(false);
        if col.unique && !had_unique {
            let name = super::mysql::key_name("uq", t, std::slice::from_ref(&col.name));
            out.push(MigrationOp {
                kind: "add_unique",
                table: t.clone(),
                summary: format!("{t}: 列 {} 新增唯一约束 {}", col.name, name),
                up: vec![format!(
                    "ALTER TABLE {t} ADD UNIQUE KEY {name} ({});",
                    col.name
                )],
                down: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
            });
        }
    }
    for col in &old.columns {
        let still_unique_and_present = new.columns.iter().any(|c| c.name == col.name && c.unique);
        if col.unique && !still_unique_and_present {
            let name = super::mysql::key_name("uq", t, std::slice::from_ref(&col.name));
            out.push(MigrationOp {
                kind: "drop_unique",
                table: t.clone(),
                summary: format!("{t}: 列 {} 移除唯一约束 {}", col.name, name),
                up: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
                down: vec![format!(
                    "ALTER TABLE {t} ADD UNIQUE KEY {name} ({});",
                    col.name
                )],
            });
        }
    }
    // FULLTEXT（M10/P1：searchable 声明；ngram 分词，MySQL 8 内置）
    for cols in new.fulltext.iter() {
        if !old.fulltext.contains(cols) {
            let name = super::mysql::key_name("ft", t, cols);
            out.push(MigrationOp {
                kind: "add_fulltext",
                table: t.clone(),
                summary: format!("{t}: 新增全文索引 {} ({})", name, cols.join(", ")),
                up: vec![format!(
                    "ALTER TABLE {t} ADD FULLTEXT INDEX {name} ({}) WITH PARSER ngram;",
                    cols.join(", ")
                )],
                down: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
            });
        }
    }
    for cols in old.fulltext.iter() {
        if !new.fulltext.contains(cols) {
            let name = super::mysql::key_name("ft", t, cols);
            out.push(MigrationOp {
                kind: "drop_fulltext",
                table: t.clone(),
                summary: format!("{t}: 删除全文索引 {} ({})", name, cols.join(", ")),
                up: vec![format!("ALTER TABLE {t} DROP INDEX {name};")],
                down: vec![format!(
                    "ALTER TABLE {t} ADD FULLTEXT INDEX {name} ({}) WITH PARSER ngram;",
                    cols.join(", ")
                )],
            });
        }
    }
    out
}

fn fk_ops(new: &TablePlan, old: &TablePlan) -> Vec<MigrationOp> {
    let mut out = Vec::new();
    let t = &new.table;
    for fk in &new.fks {
        if !old.fks.contains(fk) {
            let name = format!("fk_{t}_{}", fk.column);
            out.push(MigrationOp {
                kind: "add_fk",
                table: t.clone(),
                summary: format!("{t}: 新增外键 {} → {}", fk.column, fk.ref_table),
                up: vec![format!(
                    "ALTER TABLE {t} ADD CONSTRAINT {name} FOREIGN KEY ({}) REFERENCES {} (id);",
                    fk.column, fk.ref_table
                )],
                down: vec![format!("ALTER TABLE {t} DROP FOREIGN KEY {name};")],
            });
        }
    }
    for fk in &old.fks {
        if !new.fks.contains(fk) {
            let name = format!("fk_{t}_{}", fk.column);
            out.push(MigrationOp {
                kind: "drop_fk",
                table: t.clone(),
                summary: format!("{t}: 删除外键 {} → {}", fk.column, fk.ref_table),
                up: vec![format!("ALTER TABLE {t} DROP FOREIGN KEY {name};")],
                down: vec![format!(
                    "ALTER TABLE {t} ADD CONSTRAINT {name} FOREIGN KEY ({}) REFERENCES {} (id);",
                    fk.column, fk.ref_table
                )],
            });
        }
    }
    out
}

/// 表结构差异 → 升级 SQL 语句序列（兼容旧接口）。
pub fn diff_sql(old: &[TablePlan], new: &[TablePlan]) -> String {
    join_stmts(
        diff_ops(old, new)
            .iter()
            .flat_map(|op| op.up.iter().cloned()),
    )
}

/// 表结构差异 → 回滚 SQL（逆序执行反向操作）。
pub fn diff_down_sql(old: &[TablePlan], new: &[TablePlan]) -> String {
    join_stmts(
        diff_ops(old, new)
            .iter()
            .rev()
            .flat_map(|op| op.down.iter().cloned()),
    )
}

fn join_stmts(stmts: impl Iterator<Item = String>) -> String {
    stmts.fold(String::new(), |mut out, s| {
        out.push_str(&s);
        out.push('\n');
        out
    })
}

// ── 迁移文件读写 ──────────────────────────────────────────

const UP_MARKER: &str = "-- UP";
const DOWN_MARKER: &str = "-- DOWN";

fn upgrade_rel(out_dir: &str, index: u32) -> String {
    format!("{out_dir}/migrations/{index:04}_upgrade.sql")
}

fn rollback_rel(out_dir: &str, index: u32) -> String {
    format!("{out_dir}/migrations/{index:04}_rollback.sql")
}

/// 从迁移文件中提取某一段 SQL（`-- UP` / `-- DOWN` 标记之后的内容）。
/// 标记不存在时返回 None（例如 gen-mysql 的基线文件没有标记段）。
fn extract_section(path: &Path, marker: &str) -> Option<String> {
    let text = read_text(path).ok()?;
    let start = text.lines().position(|l| l.trim() == marker)? + 1;
    let body: Vec<&str> = text.lines().skip(start).collect();
    let trimmed = body
        .iter()
        .skip_while(|l| l.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if trimmed.trim().is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

// ── MIGRATIONS.md：让版本链与 SQL 更新可见 ─────────────────

fn render_migrations_md(ledger: &MigrationLedger, migrations_dir: &Path) -> String {
    let mut out = String::from(
        "# Migrations — 数据库版本台账\n\n\
         > 由 Forma `gen-migrate` 自动生成，请勿手工编辑；DSL 变更后 `forma build` 会追加新版本。\n\
         > 应用/回滚：`forma migrate up|down`（详见 `forma migrate --help`）。\n\n",
    );
    out.push_str("| # | 目标版本 | 升级 | 回滚 | 状态 | 变更摘要 |\n");
    out.push_str("|---|----------|------|------|------|----------|\n");
    for e in &ledger.entries {
        let status = if e.applied {
            "✅ applied"
        } else {
            "⏳ pending"
        };
        let rollback = e
            .rollback_file
            .as_deref()
            .map(|p| p.rsplit('/').next().unwrap_or(p))
            .unwrap_or("—");
        let changes = e.changes.join("；");
        let upgrade = e.upgrade_file.rsplit('/').next().unwrap_or(&e.upgrade_file);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            e.index, e.version, upgrade, rollback, status, changes
        ));
    }
    out.push_str("\n---\n\n");

    for e in &ledger.entries {
        let upgrade_abs =
            migrations_dir.join(e.upgrade_file.rsplit('/').next().unwrap_or(&e.upgrade_file));
        out.push_str(&format!("## #{} — upgrade to v{}\n\n", e.index, e.version));
        out.push_str("变更：\n");
        for c in &e.changes {
            out.push_str(&format!("- {c}\n"));
        }
        out.push_str("\n```sql\n-- UP\n");
        if let Some(sql) = extract_section(&upgrade_abs, UP_MARKER) {
            out.push_str(&sql);
        } else if let Ok(text) = read_text(&upgrade_abs) {
            // 基线（gen-mysql 0001_init.sql）没有 UP 标记段，整体即 SQL
            let body = text
                .lines()
                .skip_while(|l| l.starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            out.push_str(body.trim());
        }
        out.push_str("\n```\n");
        if let Some(rollback) = &e.rollback_file {
            let rollback_abs = migrations_dir.join(rollback.rsplit('/').next().unwrap_or(rollback));
            if let Some(sql) = extract_section(&rollback_abs, DOWN_MARKER) {
                out.push_str(&format!("```sql\n-- DOWN（回滚）\n{sql}\n```\n"));
            }
        }
        out.push('\n');
    }
    out
}

fn write_migration_docs(project_dir: &Path, out_dir: &str, ledger: &MigrationLedger) -> Result<()> {
    let migrations_dir = project_dir.join(out_dir).join("migrations");
    let md = render_migrations_md(ledger, &migrations_dir);
    write_text(&migrations_dir.join("MIGRATIONS.md"), &md)?;
    Ok(())
}

// ── 生成入口 ──────────────────────────────────────────────

fn app_version_of(program: &Program) -> String {
    program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| "0.0.0".into())
}

/// 与上次构建的 IR 快照对比；有变化则生成一对 `{index:04}_upgrade.sql` +
/// `{index:04}_rollback.sql`，登记进版本台账并刷新 MIGRATIONS.md。
/// 首次构建（无快照）只登记基线——基线 SQL 由 gen-mysql 的 `0001_init.sql` 承载。
pub fn emit_migration(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Option<crate::generators::GeneratedFile>> {
    let new_plans = plan_tables_with_internal(program);
    let app_version = app_version_of(program);
    let migrations_dir = project_dir.join(out_dir).join("migrations");
    let mut ledger = read_ledger(project_dir);
    // 台账只承载 applied 状态；台账缺失（新克隆）时从**入库的迁移文件**重建，
    // applied 一律 pending（真实是否已执行由数据库决定）。
    if ledger.entries.is_empty() {
        ledger = ledger_from_chain(project_dir, out_dir, &app_version)?;
        save_ledger(project_dir, &ledger)?;
    }

    let rs = read_snapshot(project_dir, out_dir);
    let old_snapshot = rs.or_else(|| rebuild_snapshot_from_chain(project_dir, out_dir));
    if let Some(snap) = old_snapshot.as_ref() {
        // 快照是从迁移链重放重建的（新克隆场景）→ 落盘持久化，
        // 否则每次构建都要重放整条链（且 save_snapshot 分支永远不会执行——踩过）
        let p = snapshot_path(project_dir, out_dir);
        if !p.is_file() {
            let snap2 = IrSnapshot {
                version: SNAPSHOT_VERSION,
                app_version: app_version.clone(),
                tables: snap.tables.clone(),
            };
            write_text(&p, &serde_json::to_string_pretty(&snap2)?)?;
        }
    }
    let Some(old_snapshot) = old_snapshot else {
        // 首次：登记基线（迁移文件由 gen-mysql 的 0001_init.sql 承载）
        save_snapshot(project_dir, out_dir, &app_version, &new_plans)?;
        if ledger.entries.is_empty() {
            let changes = if new_plans.is_empty() {
                vec!["空 schema 基线".to_string()]
            } else {
                vec![format!(
                    "基线建表 {}",
                    new_plans
                        .iter()
                        .map(|t| t.table.as_str())
                        .collect::<Vec<_>>()
                        .join("/")
                )]
            };
            ledger.entries.push(MigrationEntry {
                index: 1,
                version: app_version.clone(),
                upgrade_file: format!("{out_dir}/migrations/0001_init.sql"),
                rollback_file: None,
                changes,
                up_hash: String::new(),
                down_hash: None,
                applied: false,
            });
            save_ledger(project_dir, &ledger)?;
        }
        write_migration_docs(project_dir, out_dir, &ledger)?;
        return Ok(None);
    };

    let ops = diff_ops(&old_snapshot.tables, &new_plans);
    for t in &new_plans {
        if t.table == "users" {
            let oldu = old_snapshot.tables.iter().find(|x| x.table == "users");
            let old_col = oldu.and_then(|o| o.columns.iter().find(|c| c.name == "username"));
            let new_col = t.columns.iter().find(|c| c.name == "username");
            eprintln!("DBG users username old_unique={:?} new_unique={:?} old_uniques={:?} new_uniques={:?} ops={}", old_col.map(|c| c.unique), new_col.map(|c| c.unique), oldu.map(|o| &o.uniques), Some(&t.uniques), ops.len());
        }
    }
    if ops.is_empty() {
        // schema 未变：仅刷新台账文档（applied 状态可能被 CLI 更新过）
        write_migration_docs(project_dir, out_dir, &ledger)?;
        return Ok(None);
    }

    let index = migration_index(&migrations_dir) + 1;
    let upgrade_rel = upgrade_rel(out_dir, index);
    let rollback_rel = rollback_rel(out_dir, index);

    let up_sql = join_stmts(ops.iter().flat_map(|op| op.up.iter().cloned()));
    let down_sql = join_stmts(ops.iter().rev().flat_map(|op| op.down.iter().cloned()));

    let upgrade_content = format!(
        "-- @generated by Forma (gen-migrate) — do not edit manually.\n\
         -- Migration #{index:04} — upgrade to v{app_version}\n\
         -- Rollback: migrations/{index:04}_rollback.sql\n\
         -- 变更:\n{}\
         --\n{UP_MARKER}\n\n{up_sql}",
        ops.iter()
            .map(|op| format!("--   - {}\n", op.summary))
            .collect::<String>()
    );
    let rollback_content = format!(
        "-- @generated by Forma (gen-migrate) — do not edit manually.\n\
         -- Migration #{index:04} — rollback from v{app_version}\n\
         -- Counterpart: migrations/{index:04}_upgrade.sql\n\
         -- 逆序执行反向操作:\n{}\
         --\n{DOWN_MARKER}\n\n{down_sql}",
        ops.iter()
            .rev()
            .map(|op| format!("--   - {}\n", op.summary))
            .collect::<String>()
    );

    write_text(&project_dir.join(&upgrade_rel), &upgrade_content)?;
    write_text(&project_dir.join(&rollback_rel), &rollback_content)?;

    ledger.entries.push(MigrationEntry {
        index,
        version: app_version.clone(),
        upgrade_file: upgrade_rel.clone(),
        rollback_file: Some(rollback_rel.clone()),
        changes: ops.iter().map(|op| op.summary.clone()).collect(),
        // 哈希基于提取后的 SQL 正文（与 verify 的提取规则一致，排除头部注释/尾随空行）
        up_hash: crate::util::sha256_hex(up_sql.trim()),
        down_hash: Some(crate::util::sha256_hex(down_sql.trim())),
        applied: false,
    });
    save_ledger(project_dir, &ledger)?;
    save_snapshot(project_dir, out_dir, &app_version, &new_plans)?;
    write_migration_docs(project_dir, out_dir, &ledger)?;

    Ok(Some(crate::generators::GeneratedFile {
        path: upgrade_rel,
        content: upgrade_content,
        generator: "gen-migrate".into(),
    }))
}

// ── CLI：forma migrate {status|history|up|down} ────────────

/// 台账摘要行（供 status/history 渲染）。
///
/// 台账是 gitignored 本机状态；干净 checkout 上缺失时从**入库迁移链**只读重建，
/// 不写盘（与 `ledger_from_committed_chain` 同一来源）。
pub fn migration_status(project_dir: &Path) -> Result<Vec<String>> {
    let ledger = read_ledger_lenient_public(project_dir)?;
    if ledger.entries.is_empty() {
        return Ok(vec![
            "(no migrations recorded — run `forma build` first)".into()
        ]);
    }
    let mut lines = Vec::new();
    for e in &ledger.entries {
        let file = e.upgrade_file.rsplit('/').next().unwrap_or(&e.upgrade_file);
        let status = if e.applied { "applied" } else { "pending" };
        let rollback = e
            .rollback_file
            .as_ref()
            .map(|p| p.rsplit('/').next().unwrap().to_string())
            .unwrap_or_else(|| "—".into());
        lines.push(format!(
            "  #{:04}  v{:<8} {:<22} {:<22} [{status:7}]  {}",
            e.index,
            e.version,
            file,
            rollback,
            e.changes.first().map(String::as_str).unwrap_or("")
        ));
    }
    Ok(lines)
}

/// `migrate up [--to <version>]`：输出待应用迁移的升级 SQL；apply=true 时标记台账。
/// target 指定时只应用版本 ≤ target 的 pending 迁移（多步前进到指定版本）。
/// 返回 (文件, SQL) 列表。
pub fn migration_up(
    project_dir: &Path,
    out_dir: &str,
    apply: bool,
    target: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let mut ledger = read_ledger(project_dir);
    let selected: Vec<usize> = ledger
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.applied)
        .filter(|(_, e)| target.map(|t| version_le(&e.version, t)).unwrap_or(true))
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    for i in &selected {
        let e = &ledger.entries[*i];
        let abs = project_dir.join(&e.upgrade_file);
        let Some(sql) = extract_section(&abs, UP_MARKER) else {
            // 基线 0001_init.sql 没有 UP 标记，整体即 SQL
            let text = read_text(&abs)?;
            out.push((e.upgrade_file.clone(), text));
            continue;
        };
        out.push((e.upgrade_file.clone(), sql));
    }
    if apply {
        for i in &selected {
            ledger.entries[*i].applied = true;
        }
        save_ledger(project_dir, &ledger)?;
        write_migration_docs(project_dir, out_dir, &ledger)?;
    }
    Ok(out)
}

/// `migrate down [--steps N] [--to <version>]`：回滚最近 N 步（或回滚到目标版本之上）。
/// apply=true 时更新台账。返回按执行顺序的 (文件, SQL) 列表。
pub fn migration_down(
    project_dir: &Path,
    out_dir: &str,
    apply: bool,
    steps: usize,
    target: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let mut ledger = read_ledger(project_dir);
    // 逆序收集要回滚的已应用迁移
    let mut positions: Vec<usize> = Vec::new();
    for (pos, e) in ledger.entries.iter().enumerate().rev() {
        if !e.applied {
            continue;
        }
        if let Some(t) = target {
            if version_le(&e.version, t) {
                break; // 已到目标版本之下
            }
        }
        positions.push(pos);
        if target.is_none() && positions.len() >= steps {
            break;
        }
    }
    let mut out = Vec::new();
    for pos in &positions {
        let entry = ledger.entries[*pos].clone();
        let Some(rollback_file) = &entry.rollback_file else {
            anyhow::bail!("migration #{} 基线没有回滚脚本", entry.index);
        };
        let abs = project_dir.join(rollback_file);
        let sql = extract_section(&abs, DOWN_MARKER).ok_or_else(|| {
            anyhow::anyhow!("rollback file {} missing -- DOWN section", rollback_file)
        })?;
        out.push((rollback_file.clone(), sql));
    }
    if apply {
        for pos in &positions {
            ledger.entries[*pos].applied = false;
        }
        save_ledger(project_dir, &ledger)?;
        write_migration_docs(project_dir, out_dir, &ledger)?;
    }
    Ok(out)
}

/// 简易 semver 比较：a ≤ b（按数字段逐位比较，段数不足补 0）。
fn version_le(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|s| s.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x < y;
        }
    }
    true
}

/// `migrate verify`：链完整性 + 前向重放一致性校验（CI 可用，发现问题返回非空列表）。
///
/// 1. 编号连续（1..N 无缺口）；
/// 2. 每个升级（除基线）都有配对回滚文件；
/// 3. 台账哈希与迁移文件内容一致（检测手工篡改）；
/// 4. 把全部迁移按序重放到模拟 schema，与当前 DSL 派生的 schema 逐项比对（检测基线漂移/链断裂）。
pub fn migration_verify(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Vec<String>> {
    let ledger = read_ledger(project_dir);
    let mut issues: Vec<String> = Vec::new();

    // 链的权威 = 已入库的迁移文件（台账只记录 applied 状态与哈希参考）：
    // 否则新克隆仓库没有台账 → 校验与增量生成都会断链。
    let chain = chain_files(project_dir, out_dir);
    if chain.is_empty() {
        issues.push("迁移目录为空——先运行 `forma build` 生成基线".into());
        return Ok(issues);
    }

    // 1. 编号连续
    for (want, f) in chain.iter().enumerate() {
        if f.index != (want + 1) as u32 {
            issues.push(format!(
                "编号不连续：第 {} 个迁移应为 {}，实际 {}",
                want + 1,
                want + 1,
                f.index
            ));
        }
    }

    // 2/3. 文件存在 + 回滚配对 + 哈希一致（台账有记录时才做哈希比对）
    let mut replay_sql_all = String::new();
    for f in &chain {
        let up_abs = project_dir.join(&f.upgrade_file);
        if !up_abs.is_file() {
            issues.push(format!("升级文件缺失: {}", f.upgrade_file));
            continue;
        }
        let text = read_text(&up_abs)?;
        let section = extract_section(&up_abs, UP_MARKER).unwrap_or_else(|| text.clone());
        replay_sql_all.push_str(&section);
        replay_sql_all.push('\n');
        if let Some(e) = ledger.entries.iter().find(|e| e.index == f.index) {
            // 只对**已应用**的迁移做哈希比对：未应用的文件可随 DSL 演进重新生成
            if e.applied && !e.up_hash.is_empty() && crate::util::sha256_hex(&section) != e.up_hash
            {
                issues.push(format!(
                    "#{} {} 内容哈希与台账不符——迁移文件被手工修改",
                    f.index,
                    f.upgrade_file.rsplit('/').next().unwrap_or("")
                ));
            }
        }
        match &f.rollback_file {
            None if f.index != 1 => {
                issues.push(format!("#{} 缺少配对回滚文件", f.index));
            }
            Some(rb) => {
                let rb_abs = project_dir.join(rb);
                if !rb_abs.is_file() {
                    issues.push(format!("回滚文件缺失: {rb}"));
                }
            }
            _ => {}
        }
    }

    // 4. 前向重放一致性：0001..000N 重放 == 当前 DSL schema
    let mut schema = SimSchema::default();
    if let Err(err) = replay_sql(&mut schema, &replay_sql_all) {
        issues.push(format!("迁移链重放失败（真实 MySQL 也会在此报错）: {err}"));
    } else {
        let expected = schema_from_plans(&plan_tables_with_internal(program));
        let diffs = schema_diff(&schema, &expected);
        if diffs.is_empty() {
            // 一致 ✓
        } else {
            issues.push(format!(
                "迁移链重放结果与当前 DSL schema 不一致（{} 处漂移）：",
                diffs.len()
            ));
            for d in diffs.iter().take(10) {
                issues.push(format!("  - {d}"));
            }
        }
    }
    let _ = out_dir;
    Ok(issues)
}
