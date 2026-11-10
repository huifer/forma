//! Oracle target（轨道 B 重新排期）：Program → Oracle SQL DDL。
//!
//! 与 MySQL/PostgreSQL/SQLite/SQL Server 共享 DSL 表拓扑与约束语义，独立渲染 Oracle 方言：
//! Uuid → VARCHAR2(36)、String(n) → VARCHAR2(n)、Text/Json → CLOB、
//! Int → NUMBER(10)、Float → BINARY_DOUBLE、Decimal(p,s) → NUMBER(p,s)、Bool → NUMBER(1)、
//! Timestamp → TIMESTAMP、Date → DATE、枚举 → VARCHAR2(100)。
//! 幂等建表：`BEGIN EXECUTE IMMEDIATE … EXCEPTION WHEN OTHERS THEN IF SQLCODE != -955 …`（Oracle 无 IF NOT EXISTS）。

use super::mysql::{plan_tables, ColumnPlan, TablePlan};
use crate::dsl::{is_enum, is_relation, FieldDecl, Program};
use crate::generators::GeneratedFile;
use crate::identity::to_snake_case;
use std::path::Path;

fn type_args(field: &FieldDecl) -> String {
    field.type_args.join(",")
}

pub fn oracle_type(program: &Program, field: &FieldDecl) -> String {
    match field.type_name.as_str() {
        "Uuid" => "VARCHAR2(36)".into(),
        "String" => {
            let n = type_args(field);
            if n.is_empty() {
                "VARCHAR2(255)".into()
            } else {
                format!("VARCHAR2({n})")
            }
        }
        "Email" => "VARCHAR2(320)".into(),
        "Text" | "Json" => "CLOB".into(),
        "Int" => "NUMBER(10)".into(),
        "Float" => "BINARY_DOUBLE".into(),
        "Decimal" => {
            let n = type_args(field);
            if n.is_empty() {
                "NUMBER(18,2)".into()
            } else {
                format!("NUMBER({n})")
            }
        }
        "Bool" => "NUMBER(1)".into(),
        "Timestamp" => "TIMESTAMP".into(),
        "Date" => "DATE".into(),
        other if is_enum(program, other) => "VARCHAR2(100)".into(),
        _ if is_relation(program, &field.type_name) => "VARCHAR2(36)".into(),
        _ => "VARCHAR2(255)".into(),
    }
}

fn column_sql(program: &Program, entity: &crate::dsl::EntityDecl, col: &ColumnPlan) -> String {
    let field_name = col.name.strip_suffix("_id").unwrap_or(&col.name);
    let ty = entity
        .fields
        .iter()
        .find(|f| to_snake_case(&f.name) == field_name)
        .map(|f| oracle_type(program, f))
        .unwrap_or_else(|| col.sql_type.clone());
    let mut out = format!("{} {}", col.name, ty);
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
    let mut defs: Vec<String> = table
        .columns
        .iter()
        .map(|c| column_sql(program, entity, c))
        .collect();
    if table.primary_key.contains(',') {
        defs.push(format!("PRIMARY KEY ({})", table.primary_key));
    }
    for unique in &table.uniques {
        defs.push(format!(
            "CONSTRAINT uq_{}_{} UNIQUE ({})",
            table.table,
            unique.join("_"),
            unique.join(", ")
        ));
    }
    for fk in &table.fks {
        defs.push(format!(
            "CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} (id)",
            table.table, fk.column, fk.column, fk.ref_table
        ));
    }
    // 单引号转义（DDL 内嵌于 EXECUTE IMMEDIATE 字符串）
    let ddl = format!(
        "CREATE TABLE {} (\n    {}\n)",
        table.table,
        defs.join(",\n    ")
    )
    .replace('\'', "''");
    let mut out = format!(
        "BEGIN\n    EXECUTE IMMEDIATE '{ddl}';\nEXCEPTION\n    WHEN OTHERS THEN\n        IF SQLCODE != -955 THEN RAISE; END IF;\nEND;\n/\n\n",
    );
    for idx in &table.indexes {
        let name = format!("idx_{}_{}", table.table, idx.join("_"));
        let sql = format!(
            "CREATE INDEX {name} ON {} ({})",
            table.table,
            idx.join(", ")
        );
        out.push_str(&format!(
            "BEGIN\n    EXECUTE IMMEDIATE '{sql}';\nEXCEPTION\n    WHEN OTHERS THEN\n        IF SQLCODE != -955 THEN RAISE; END IF;\nEND;\n/\n\n"
        ));
    }
    out
}

fn render_all(program: &Program, out_dir: &str, header: &str, rel: &str) -> GeneratedFile {
    let plans = plan_tables(program);
    let mut sql = String::from(header);
    for table in &plans {
        let entity = program
            .entities
            .iter()
            .find(|e| e.name == table.entity)
            .unwrap_or_else(|| panic!("table plan references unknown entity {}", table.entity));
        sql.push_str(&render_table(program, entity, table));
    }
    GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content: sql,
        generator: "gen-oracle".into(),
    }
}

pub fn emit_oracle(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let file = render_all(
        program,
        out_dir,
        "-- @generated by Forma (gen-oracle) — do not edit manually.\n-- Oracle SQL\n\n",
        "0001_init.sql",
    );
    crate::util::write_text(&project_dir.join(&file.path), &file.content)?;
    Ok(vec![file])
}

pub fn emit_oracle_migration(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let file = render_all(
        program,
        out_dir,
        "-- @generated by Forma (gen-oracle) — upgrade migration\n\n",
        "migrations/0001_upgrade.sql",
    );
    crate::util::write_text(&project_dir.join(&file.path), &file.content)?;
    Ok(vec![file])
}
