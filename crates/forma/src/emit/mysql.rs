//! db-mysql 适配层：Program → MySQL 8 DDL + 增量迁移计划。

use super::{pluralize, EntityDecl, FieldDecl, Program};
use crate::identity::to_snake_case;
use serde::{Deserialize, Serialize};

/// 内容派生的索引/唯一约束名：`idx_{表}_{列1}_{列2}` / `uq_{表}_{列1}_{列2}`。
/// 必须由列签名派生而非位置序号——否则多版本链里先建 (a,b) 再加 (c) 会重名，
/// 真实 MySQL 重放直接报 Duplicate key name。内容派生还保证回滚时可恢复名字。
pub fn key_name(kind: &str, table: &str, cols: &[String]) -> String {
    format!("{kind}_{table}_{}", cols.join("_"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnPlan {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FkPlan {
    pub column: String,
    pub ref_table: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TablePlan {
    pub entity: String,
    pub table: String,
    pub columns: Vec<ColumnPlan>,
    pub primary_key: String,
    pub uniques: Vec<Vec<String>>,
    pub indexes: Vec<Vec<String>>,
    /// FULLTEXT 索引（M10/P1：实体 `searchable [...]` 声明；渲染为
    /// FULLTEXT KEY ft_... WITH PARSER ngram——MySQL 8 内置 ngram，中文 bigram 可用）。
    /// `#[serde(default)]`：旧快照无该字段时反序列化为空——否则 read_snapshot 失败
    /// 会静默回退到迁移链重放路径，产生伪 diff（踩过：users 唯一约束 DROP+ADD 同名）。
    #[serde(default)]
    pub fulltext: Vec<Vec<String>>,
    pub fks: Vec<FkPlan>,
}

pub fn mysql_type(program: &Program, field: &FieldDecl) -> String {
    match field.type_name.as_str() {
        "Uuid" => "CHAR(36)".into(),
        "String" => {
            let n = field
                .type_args
                .first()
                .and_then(|a| a.parse::<usize>().ok())
                .unwrap_or(255);
            format!("VARCHAR({n})")
        }
        "Email" => "VARCHAR(320)".into(),
        "Text" => "TEXT".into(),
        "Int" => "BIGINT".into(),
        "Float" => "DOUBLE".into(),
        "Decimal" => {
            let p = field
                .type_args
                .first()
                .and_then(|a| a.parse::<usize>().ok())
                .unwrap_or(12);
            let s = field
                .type_args
                .get(1)
                .and_then(|a| a.parse::<usize>().ok())
                .unwrap_or(2);
            format!("DECIMAL({p},{s})")
        }
        "Bool" => "TINYINT(1)".into(),
        "Timestamp" => "DATETIME(3)".into(),
        "Date" => "DATE".into(),
        "Json" => "JSON".into(),
        "Attachment" => "VARCHAR(500)".into(),
        other => {
            if super::is_enum(program, other) {
                "VARCHAR(64)".into()
            } else {
                "CHAR(36)".into() // 实体引用 → FK 列
            }
        }
    }
}

/// users 表列计划（`auth module` 存在时自动附加：密码仅存哈希）。
fn users_plan() -> TablePlan {
    let col = |name: &str, sql_type: &str, nullable: bool, unique: bool| ColumnPlan {
        name: name.into(),
        sql_type: sql_type.into(),
        nullable,
        unique,
    };
    TablePlan {
        entity: "User".into(),
        table: "users".into(),
        fulltext: vec![],
        columns: vec![
            col("id", "CHAR(36)", false, false),
            // 单列唯一约束用列级 unique 表示：与迁移链重放基线（sim_to_plans）一致。
            // 若放表级 uniques，diff 会同时产生 DROP+ADD 同名伪迁移（真库实测）。
            col("username", "VARCHAR(64)", false, true),
            col("password_hash", "VARCHAR(255)", false, false),
            col("role", "VARCHAR(64)", false, false),
            col("created_at", "DATETIME(3)", false, false),
        ],
        primary_key: "id".into(),
        uniques: vec![],
        indexes: vec![],
        fks: vec![],
    }
}

/// 审计表列计划（`audit { action }` 命令存在时自动附加，actor 由 auth 模块提供）。
fn audit_log_plan() -> TablePlan {
    let col = |name: &str, sql_type: &str, nullable: bool| ColumnPlan {
        name: name.into(),
        sql_type: sql_type.into(),
        nullable,
        unique: false,
    };
    TablePlan {
        entity: "AuditLog".into(),
        table: "audit_logs".into(),
        columns: vec![
            col("id", "CHAR(36)", false),
            col("action", "VARCHAR(191)", false),
            col("entity", "VARCHAR(191)", false),
            col("entity_id", "CHAR(36)", false),
            col("actor", "VARCHAR(191)", true),
            // 命令幂等键（spec:protocols §10.5）：客户端 Idempotency-Key 头落库，供对账识别重复提交
            col("idempotency_key", "CHAR(36)", true),
            col("payload", "JSON", true),
            col("created_at", "DATETIME(3)", false),
        ],
        primary_key: "id".into(),
        uniques: vec![],
        indexes: vec![
            vec!["created_at".into()],
            vec!["entity".into(), "entity_id".into()],
        ],
        fulltext: vec![],
        fks: vec![],
    }
}

/// Program → 表结构计划（拓扑排序：被引用表在前；环回退声明顺序）。
/// 含内部表：存在 `audit { action }` 命令时附加 audit_logs（走同一套迁移/verify 管线）。
pub fn plan_tables_with_internal(program: &Program) -> Vec<TablePlan> {
    let mut plans = plan_tables(program);
    if program.auth.is_some() && !plans.iter().any(|p| p.table == "users") {
        let mut users = users_plan();
        // 租户维度（C4/M10 收尾）：@tenant 字段存在 → users 表带 tenant 列（JWT claim 来源）+
        // disabled 列（租户吊销：停用租户全部用户；nullable=true 兼容存量不加租户列的已有表，
        // 存量行 tenant=NULL 语义上不属于任何租户——租户隔离查询只命中显式绑定的行）
        if crate::dsl::tenant_enabled(program) {
            users.columns.push(ColumnPlan {
                name: "tenant".into(),
                sql_type: "VARCHAR(64)".into(),
                nullable: true,
                unique: false,
            });
            users.indexes.push(vec!["tenant".into()]);
            users.columns.push(ColumnPlan {
                name: "disabled".into(),
                sql_type: "TINYINT(1)".into(),
                nullable: false,
                unique: false,
            });
        }
        plans.insert(0, users);
    }
    let has_audit = program.commands.iter().any(|c| c.audit.is_some());
    if has_audit && !plans.iter().any(|p| p.table == "audit_logs") {
        plans.push(audit_log_plan());
    }
    // 定时任务（M10/P1）：jobs 非空时建 forma_jobs 内部表
    if !program.jobs.is_empty() && !plans.iter().any(|p| p.table == "forma_jobs") {
        plans.push(job_table_plan());
    }
    plans
}

fn job_table_plan() -> TablePlan {
    let col = |name: &str, sql_type: &str, nullable: bool, unique: bool| ColumnPlan {
        name: name.into(),
        sql_type: sql_type.into(),
        nullable,
        unique,
    };
    TablePlan {
        entity: "FormaJob".into(),
        table: "forma_jobs".into(),
        fulltext: vec![],
        columns: vec![
            col("id", "CHAR(36)", false, false),
            // 同 users_plan：单列唯一用列级标记，避免重放基线产生伪 diff
            col("name", "VARCHAR(100)", false, true),
            col("cron", "VARCHAR(50)", false, false),
            col("label", "VARCHAR(200)", true, false),
            col("op", "VARCHAR(100)", false, false),
            col("status", "VARCHAR(20)", false, false),
            col("last_run_at", "DATETIME", true, false),
            col("next_run_at", "DATETIME", true, false),
        ],
        primary_key: "id".into(),
        uniques: vec![],
        indexes: vec![vec!["status".into()]],
        fks: vec![],
    }
}

/// 语义校验/展示用：仅 DSL 声明的表。
pub fn plan_tables(program: &Program) -> Vec<TablePlan> {
    let mut emitted: Vec<String> = Vec::new();
    let mut ordered: Vec<TablePlan> = Vec::new();
    let mut remaining: Vec<&EntityDecl> = program.entities.iter().collect();

    loop {
        let mut progressed = false;
        remaining.retain(|entity| {
            let deps_ready = entity.fields.iter().all(|f| {
                !super::is_relation(program, &f.type_name) || emitted.contains(&f.type_name)
            });
            if deps_ready {
                emitted.push(entity.name.clone());
                ordered.push(plan_table(program, entity));
                progressed = true;
                false
            } else {
                true
            }
        });
        if remaining.is_empty() || !progressed {
            for e in remaining.drain(..) {
                ordered.push(plan_table(program, e));
            }
            break;
        }
    }
    ordered
}

fn plan_table(program: &Program, entity: &EntityDecl) -> TablePlan {
    let table = to_snake_case(&pluralize(&entity.name));
    let mut columns: Vec<ColumnPlan> = Vec::new();
    let mut fks: Vec<FkPlan> = Vec::new();

    for field in &entity.fields {
        let col = to_snake_case(&field.name);
        if super::is_relation(program, &field.type_name) {
            let fk_col = format!("{col}_id");
            let nullable = field.optional || !super::has_attr(field, "required");
            columns.push(ColumnPlan {
                name: fk_col.clone(),
                sql_type: "CHAR(36)".into(),
                nullable,
                unique: false,
            });
            fks.push(FkPlan {
                column: fk_col,
                ref_table: to_snake_case(&pluralize(&field.type_name)),
            });
            continue;
        }
        columns.push(ColumnPlan {
            name: col,
            sql_type: mysql_type(program, field),
            nullable: field.optional,
            unique: super::has_attr(field, "unique"),
        });
    }

    let primary_key = entity
        .fields
        .iter()
        .find(|f| super::has_attr(f, "id"))
        .map(|f| to_snake_case(&f.name))
        .unwrap_or_else(|| "id".into());

    let resolve = |col: &str| -> String {
        if let Some(field) = entity.fields.iter().find(|f| f.name == col) {
            if super::is_relation(program, &field.type_name) {
                return format!("{}_id", to_snake_case(col));
            }
        }
        to_snake_case(col)
    };

    // 全文搜索（M10/P1）：searchable 声明 → FULLTEXT 索引列集合
    // （仅 String/Text/Email——FULLTEXT 只支持 CHAR/VARCHAR/TEXT 家族）
    let fulltext = if entity.searchable.is_empty() {
        vec![]
    } else {
        let cols: Vec<String> = entity
            .searchable
            .iter()
            .filter_map(|name| {
                entity
                    .fields
                    .iter()
                    .find(|f| &f.name == name)
                    .map(|f| to_snake_case(&f.name))
            })
            .collect();
        if cols.is_empty() {
            vec![]
        } else {
            vec![cols]
        }
    };
    TablePlan {
        fulltext,
        entity: entity.name.clone(),
        table,
        columns,
        primary_key,
        uniques: entity
            .uniques
            .iter()
            .map(|cols| cols.iter().map(|c| resolve(c)).collect())
            .collect(),
        indexes: entity
            .indexes
            .iter()
            .map(|cols| cols.iter().map(|c| resolve(c)).collect())
            .collect(),
        fks,
    }
}

pub fn render_create_table(plan: &TablePlan) -> String {
    let mut lines: Vec<String> = Vec::new();
    for col in &plan.columns {
        let mut def = format!("  {} {}", col.name, col.sql_type);
        if col.nullable {
            def.push_str(" NULL");
        } else {
            def.push_str(" NOT NULL");
        }
        // 列级 @unique 用命名唯一键（与 ALTER 路径一致），不再用内联 UNIQUE——
        // 否则 CREATE 与 ALTER 两条路径产出的约束名不同，重放比对无法对齐。
        lines.push(def);
    }
    lines.push(format!("  PRIMARY KEY ({})", plan.primary_key));
    for cols in plan.uniques.iter() {
        lines.push(format!(
            "  UNIQUE KEY {} ({})",
            key_name("uq", &plan.table, cols),
            cols.join(", ")
        ));
    }
    // 列级 @unique → 命名唯一键
    for col in plan.columns.iter().filter(|c| c.unique) {
        let cols = vec![col.name.clone()];
        lines.push(format!(
            "  UNIQUE KEY {} ({})",
            key_name("uq", &plan.table, &cols),
            col.name
        ));
    }
    for cols in plan.indexes.iter() {
        lines.push(format!(
            "  KEY {} ({})",
            key_name("idx", &plan.table, cols),
            cols.join(", ")
        ));
    }
    // FULLTEXT（M10/P1）：searchable 声明；ngram 分词器（MySQL 8 内置，中文 bigram 可用）
    for cols in plan.fulltext.iter() {
        lines.push(format!(
            "  FULLTEXT KEY {} ({}) WITH PARSER ngram",
            key_name("ft", &plan.table, cols),
            cols.join(", ")
        ));
    }
    for fk in &plan.fks {
        lines.push(format!(
            "  CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} (id)",
            plan.table, fk.column, fk.column, fk.ref_table
        ));
    }
    format!(
        "CREATE TABLE {} (\n{}\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;",
        plan.table,
        lines.join(",\n")
    )
}

pub fn emit_mysql(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Vec<crate::generators::GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let plans = plan_tables_with_internal(program);
    let mut sql = "-- @generated by Forma (gen-mysql) — do not edit manually.\n-- Target: MySQL 8.x, InnoDB, utf8mb4\n\n".to_string();
    for plan in &plans {
        sql.push_str(&render_create_table(plan));
        sql.push_str("\n\n");
    }

    // 基线不可变：`0001_init.sql` 只在首次构建时生成；之后的 schema 演进
    // 全部由 gen-migrate 的增量 upgrade/rollback 迁移承载（与真实工程一致）。
    let rel = format!("{out_dir}/migrations/0001_init.sql");
    let abs = project_dir.join(&rel);
    if abs.is_file() {
        return Ok(vec![]);
    }
    crate::util::write_text(&abs, &sql)?;

    Ok(vec![crate::generators::GeneratedFile {
        path: rel,
        content: sql,
        generator: "gen-mysql".into(),
    }])
}

use anyhow::Result;
use std::path::Path;
