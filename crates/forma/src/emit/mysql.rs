//! db-mysql 适配层：Program → MySQL 8 DDL + 增量迁移计划。

use super::{pluralize, EntityDecl, FieldDecl, Program};
use crate::identity::to_snake_case;
use serde::{Deserialize, Serialize};

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
        other => {
            if super::is_enum(program, other) {
                "VARCHAR(64)".into()
            } else {
                "CHAR(36)".into() // 实体引用 → FK 列
            }
        }
    }
}

/// Program → 表结构计划（拓扑排序：被引用表在前；环回退声明顺序）。
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

    TablePlan {
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
        if col.unique {
            def.push_str(" UNIQUE");
        }
        lines.push(def);
    }
    lines.push(format!("  PRIMARY KEY ({})", plan.primary_key));
    for (i, cols) in plan.uniques.iter().enumerate() {
        lines.push(format!(
            "  UNIQUE KEY uq_{}_{} ({})",
            plan.table,
            i,
            cols.join(", ")
        ));
    }
    for (i, cols) in plan.indexes.iter().enumerate() {
        lines.push(format!(
            "  KEY idx_{}_{} ({})",
            plan.table,
            i,
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
    let plans = plan_tables(program);
    let mut sql = "-- @generated by Forma (gen-mysql) — do not edit manually.\n-- Target: MySQL 8.x, InnoDB, utf8mb4\n\n".to_string();
    for plan in &plans {
        sql.push_str(&render_create_table(plan));
        sql.push_str("\n\n");
    }

    let rel = format!("{out_dir}/migrations/0001_init.sql");
    let abs = project_dir.join(&rel);
    let content_hash = crate::util::sha256_hex(&sql);
    let unchanged = abs.is_file()
        && crate::util::read_text(&abs)
            .map(|c| crate::util::sha256_hex(&c) == content_hash)
            .unwrap_or(false);
    if !unchanged {
        crate::util::write_text(&abs, &sql)?;
    }

    Ok(vec![crate::generators::GeneratedFile {
        path: rel,
        content: sql,
        generator: "gen-mysql".into(),
    }])
}

use anyhow::Result;
use std::path::Path;
