//! SQL Server target（轨道 B 重新排期）：Program → SQL Server (T-SQL) DDL。
//!
//! 与 MySQL/PostgreSQL/SQLite 共享 DSL 表拓扑与约束语义，独立渲染 T-SQL 方言：
//! - Uuid → UNIQUEIDENTIFIER；String(n) → NVARCHAR(n)；Text/Json → NVARCHAR(MAX)；
//! - Bool → BIT；Timestamp → DATETIME2；Date → DATE；Decimal(p,s) → DECIMAL(p,s)；
//! - 幂等建表：`IF OBJECT_ID(N'dbo.t', N'U') IS NULL …`（T-SQL 无 IF NOT EXISTS）；
//! - 索引：`IF NOT EXISTS (SELECT 1 FROM sys.indexes …) CREATE INDEX …`。

use super::mysql::{plan_tables, ColumnPlan, TablePlan};
use crate::dsl::{is_enum, is_relation, FieldDecl, Program};
use crate::generators::GeneratedFile;
use crate::identity::to_snake_case;
use std::path::Path;

/// 字段类型参数（String(n) / Decimal(p,s)）。
fn type_args(field: &FieldDecl) -> String {
    field.type_args.join(",")
}

pub fn mssql_type(program: &Program, field: &FieldDecl) -> String {
    match field.type_name.as_str() {
        "Uuid" => "UNIQUEIDENTIFIER".into(),
        "String" => {
            let n = type_args(field);
            if n.is_empty() {
                "NVARCHAR(255)".into()
            } else {
                format!("NVARCHAR({n})")
            }
        }
        "Email" => "NVARCHAR(320)".into(),
        "Text" => "NVARCHAR(MAX)".into(),
        "Int" => "INT".into(),
        "Float" => "FLOAT(53)".into(),
        "Decimal" => {
            let n = type_args(field);
            if n.is_empty() {
                "DECIMAL(18,2)".into()
            } else {
                format!("DECIMAL({n})")
            }
        }
        "Bool" => "BIT".into(),
        "Timestamp" => "DATETIME2".into(),
        "Date" => "DATE".into(),
        "Json" => "NVARCHAR(MAX)".into(),
        other if is_enum(program, other) => "NVARCHAR(100)".into(),
        _ if is_relation(program, &field.type_name) => "UNIQUEIDENTIFIER".into(),
        _ => "NVARCHAR(255)".into(),
    }
}

fn column_sql(program: &Program, entity: &crate::dsl::EntityDecl, col: &ColumnPlan) -> String {
    let field_name = col.name.strip_suffix("_id").unwrap_or(&col.name);
    let ty = entity
        .fields
        .iter()
        .find(|f| to_snake_case(&f.name) == field_name)
        .map(|f| mssql_type(program, f))
        .unwrap_or_else(|| col.sql_type.clone());
    let mut out = format!("[{}] {}", col.name, ty);
    if col.name == table_pk(entity) {
        out.push_str(" NOT NULL PRIMARY KEY");
    } else if !col.nullable {
        out.push_str(" NOT NULL");
    }
    out
}

fn table_pk(entity: &crate::dsl::EntityDecl) -> String {
    entity
        .fields
        .iter()
        .find(|f| crate::dsl::has_attr(f, "id"))
        .map(|f| to_snake_case(&f.name))
        .unwrap_or_else(|| "id".into())
}

fn render_table(program: &Program, entity: &crate::dsl::EntityDecl, table: &TablePlan) -> String {
    let mut out = format!(
        "IF OBJECT_ID(N'dbo.{}', N'U') IS NULL\nBEGIN\n    CREATE TABLE dbo.{} (\n",
        table.table, table.table
    );
    let mut defs: Vec<String> = table
        .columns
        .iter()
        .map(|c| format!("        {}", column_sql(program, entity, c)))
        .collect();
    if table.primary_key.contains(',') {
        defs.push(format!("        PRIMARY KEY ({})", table.primary_key));
    }
    for unique in &table.uniques {
        defs.push(format!(
            "        CONSTRAINT [uq_{}_{}] UNIQUE ({})",
            table.table,
            unique.join("_"),
            unique.join(", ")
        ));
    }
    for fk in &table.fks {
        defs.push(format!(
            "        CONSTRAINT [fk_{}_{}] FOREIGN KEY ({}) REFERENCES dbo.{} ([id])",
            table.table, fk.column, fk.column, fk.ref_table
        ));
    }
    out.push_str(&defs.join(",\n"));
    out.push_str("\n    );\nEND;\nGO\n\n");
    for idx in &table.indexes {
        let name = format!("idx_{}_{}", table.table, idx.join("_"));
        out.push_str(&format!(
            "IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = N'{name}' AND object_id = OBJECT_ID(N'dbo.{}'))\n    CREATE INDEX [{name}] ON dbo.{} ([{}]);\nGO\n\n",
            table.table,
            table.table,
            idx.join("], [")
        ));
    }
    out
}

fn render_all(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    header: &str,
    rel: &str,
) -> GeneratedFile {
    let plans = plan_tables(program);
    let mut sql = String::from(header);
    for table in &plans {
        let entity = program
            .entities
            .iter()
            .find(|e| e.name == table.entity)
            .ok_or_else(|| anyhow::anyhow!("table plan references unknown entity {}", table.entity))
            .expect("entity for table plan");
        sql.push_str(&render_table(program, entity, table));
    }
    let _ = project_dir;
    GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content: sql,
        generator: "gen-mssql".into(),
    }
}

pub fn emit_mssql(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let file = render_all(
        project_dir,
        program,
        out_dir,
        "-- @generated by Forma (gen-mssql) — do not edit manually.\n-- SQL Server (T-SQL)\n\n",
        "0001_init.sql",
    );
    crate::util::write_text(&project_dir.join(&file.path), &file.content)?;
    Ok(vec![file])
}

pub fn emit_mssql_migration(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let file = render_all(
        project_dir,
        program,
        out_dir,
        "-- @generated by Forma (gen-mssql) — upgrade migration\n\n",
        "migrations/0001_upgrade.sql",
    );
    crate::util::write_text(&project_dir.join(&file.path), &file.content)?;
    Ok(vec![file])
}
