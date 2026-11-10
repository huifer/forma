//! SQL DDL → Forma DSL（`.fm`）逆向工程生成视图（roadmap M7 / 轨道 A-8）。
//!
//! 设计立场与 [openapi] 导入一致：**导入是生成视图，不是语义等价保证**。
//! 表→实体、列→字段、外键/REFERENCES→关系字段、主键/非空/唯一→属性；
//! 数据库专有构造（触发器、存储过程、分区、CHECK、部分索引等）**显式跳过并在
//! 生成注释里提示人工校对**，绝不静默编造 DSL 语义。
//!
//! 产物保证通过 `check_program`（不引用不存在的实体、有且仅有一个 @id 主键）。

use crate::adapters::sql::parse_sql_ddl;
use crate::adapters::{LossItem, LossReport};
use crate::identity::{singularize, to_pascal_case};
use anyhow::{bail, Result};
use std::collections::BTreeMap;

struct Field {
    name: String,
    ty: String,
    attrs: Vec<String>,
}

fn norm(s: &str) -> String {
    s.trim().trim_matches('`').trim_matches('"').to_lowercase()
}

/// 任意 snake_case 列名 → lowerCamelCase DSL 标识符。
fn to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in s.chars() {
        if c == '_' || c == '-' {
            upper = true;
            continue;
        }
        if upper && !out.is_empty() {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
        format!("f{out}")
    } else {
        out
    }
}

/// SQL 类型 → DSL 类型。返回 `None` = 无法保守映射（显式跳过，不降级编造）。
fn map_type(base: &str, type_text: &str) -> Option<String> {
    let b = base.to_uppercase();
    match b.as_str() {
        "VARCHAR" | "NVARCHAR" | "CHARACTER VARYING" => {
            let n = extract_first_num(type_text).unwrap_or(255);
            Some(format!("String({n})"))
        }
        "CHAR" | "NCHAR" => {
            // CHAR(36) 通常是文本 UUID（生成的 MySQL schema 契约）
            if extract_first_num(type_text) == Some(36) {
                Some("Uuid".into())
            } else {
                let n = extract_first_num(type_text).unwrap_or(255);
                Some(format!("String({n})"))
            }
        }
        "UUID" | "UNIQUEIDENTIFIER" => Some("Uuid".into()),
        "TEXT" | "LONGTEXT" | "MEDIUMTEXT" | "TINYTEXT" | "CLOB" => Some("Text".into()),
        "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "MEDIUMINT" => Some("Int".into()),
        "DECIMAL" | "NUMERIC" => {
            let nums = extract_nums(type_text);
            match nums.as_slice() {
                [p, s] => Some(format!("Decimal({p},{s})")),
                [p] => Some(format!("Decimal({p},0)")),
                _ => Some("Decimal(10,2)".into()),
            }
        }
        "FLOAT" | "DOUBLE" | "REAL" | "DOUBLE PRECISION" => Some("Float".into()),
        "BOOL" | "BOOLEAN" | "TINYINT" => Some("Bool".into()),
        "DATE" => Some("Date".into()),
        "DATETIME" | "TIMESTAMP" | "TIMESTAMPTZ" | "TIME" => Some("Timestamp".into()),
        "JSON" | "JSONB" => Some("Json".into()),
        _ => None,
    }
}

fn extract_nums(s: &str) -> Vec<u32> {
    let inner = s
        .find('(')
        .and_then(|i| s[i + 1..].find(')').map(|j| &s[i + 1..i + 1 + j]));
    inner
        .unwrap_or("")
        .split(',')
        .filter_map(|x| x.trim().parse::<u32>().ok())
        .filter(|n| *n > 0 && *n <= 65535)
        .collect()
}

fn extract_first_num(s: &str) -> Option<u32> {
    extract_nums(s).first().copied()
}

/// 生成 `.fm` 骨架；`file` 仅用于生成文件头注释（来源追踪）。
pub fn sql_ddl_to_dsl(sql_text: &str, file: &str) -> Result<String> {
    Ok(sql_ddl_to_dsl_with_report(sql_text, file)?.0)
}

/// RFC-0002 §14 S2：返回骨架 + 机读损失清单。
pub fn sql_ddl_to_dsl_with_report(sql_text: &str, file: &str) -> Result<(String, LossReport)> {
    let parsed = parse_sql_ddl(sql_text)?;
    if parsed.tables.is_empty() {
        bail!("no CREATE TABLE found in {file} — nothing to import");
    }

    // 第一遍：表名 → 实体名（关系解析需要完整名单）
    let mut entity_by_table: BTreeMap<String, String> = BTreeMap::new();
    for t in &parsed.tables {
        entity_by_table.insert(norm(&t.name), to_pascal_case(&singularize(&t.name)));
    }
    // 同名冲突（不同表归一化后同名）：显式拒绝，避免静默覆盖
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (table, entity) in &entity_by_table {
        if let Some(prev) = seen.insert(entity.clone(), table.clone()) {
            bail!("table '{table}' and '{prev}' both map to entity '{entity}' — rename manually");
        }
    }

    let mut skipped: Vec<LossItem> = Vec::new();
    let mut entities: Vec<(String, Vec<Field>)> = Vec::new();
    let mut indexes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut searchables_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for t in &parsed.tables {
        let entity = entity_by_table[&norm(&t.name)].clone();
        // 表级 FOREIGN KEY 约束 → 列 → 目标表（列级 REFERENCES 优先）
        let mut fk_by_col: BTreeMap<String, String> = BTreeMap::new();
        // 表级 UNIQUE KEY / KEY / FULLTEXT KEY（Forma 自产 MySQL DDL 的常态）
        let mut table_unique_cols: Vec<Vec<String>> = Vec::new();
        let mut table_index_cols: Vec<Vec<String>> = Vec::new();
        let mut table_fulltext_cols: Vec<Vec<String>> = Vec::new();
        for c in &t.constraints {
            let kind: &str = c.kind;
            match kind {
                "foreign-key" => {
                    if let Some((target, _)) = &c.references {
                        for col in &c.columns {
                            fk_by_col.insert(norm(col), target.clone());
                        }
                    }
                }
                "unique" => table_unique_cols.push(c.columns.clone()),
                "index" => table_index_cols.push(c.columns.clone()),
                "fulltext" => table_fulltext_cols.push(c.columns.clone()),
                _ => {}
            }
        }
        let mut fields: Vec<Field> = Vec::new();
        for col in &t.columns {
            let target = col
                .references
                .as_ref()
                .map(|(tbl, _)| tbl.clone())
                .or_else(|| fk_by_col.get(&norm(&col.name)).cloned());
            if let Some(target_table) = target {
                if let Some(target_entity) = entity_by_table.get(&norm(&target_table)) {
                    // 关系字段：外键列 xxx_id → DSL 字段名 xxx，类型 = 目标实体名
                    let raw = col
                        .name
                        .strip_suffix("_id")
                        .unwrap_or(&col.name)
                        .to_string();
                    let mut attrs = Vec::new();
                    if col.not_null {
                        attrs.push("@required".into());
                    }
                    // 可空外键 → DSL 可选关系（Rust 侧 Option<String>）
                    let ty = if col.not_null {
                        target_entity.clone()
                    } else {
                        format!("{target_entity}?")
                    };
                    fields.push(Field {
                        name: to_camel(&raw),
                        ty,
                        attrs,
                    });
                    continue;
                }
            }
            let Some(ty) = map_type(&col.base_type, &col.type_text) else {
                skipped.push(LossItem {
                    construct: format!("{}.{} ({})", t.name, col.name, col.type_text),
                    reason: "unsupported SQL column type; no conservative DSL mapping".into(),
                });
                continue;
            };
            let mut attrs = Vec::new();
            if col.primary_key {
                attrs.push("@id".into());
                if ty == "Uuid" || ty.starts_with("String(") {
                    attrs.push("@default(uuid)".into());
                }
            }
            if col.not_null && !col.primary_key {
                attrs.push("@required".into());
            }
            if col.unique && !col.primary_key {
                attrs.push("@unique".into());
            }
            // 可空列 → DSL 可选类型（`?` 决定 Rust 侧 Option<T>）
            let ty = if !col.not_null && !col.primary_key {
                format!("{ty}?")
            } else {
                ty
            };
            // SQL DEFAULT 不自动翻译（now()/字面量/表达式跨端语义不一致）——人工校对
            fields.push(Field {
                name: to_camel(&col.name),
                ty,
                attrs,
            });
        }
        // 保证有且仅有一个 @id（关系目标必须满足；无主键的导入表补 Uuid id）
        if !fields.iter().any(|f| f.attrs.iter().any(|a| a == "@id")) {
            fields.insert(
                0,
                Field {
                    name: "id".into(),
                    ty: "Uuid".into(),
                    attrs: vec!["@id".into(), "@default(uuid)".into()],
                },
            );
            skipped.push(LossItem {
                construct: format!("{}.id", t.name),
                reason: "table had no primary key; synthesized Uuid @id".into(),
            });
        }
        // CREATE INDEX / 表内 KEY·INDEX → DSL index；表级 UNIQUE KEY → @unique；
        // FULLTEXT KEY → searchable（MySQL ngram 语义与 Forma FULLTEXT 契约同形）
        let mut ix: Vec<String> = Vec::new();
        let column_to_field = |raw: &str, fields: &[Field]| -> Option<String> {
            let camel = to_camel(raw.strip_suffix("_id").unwrap_or(raw));
            fields
                .iter()
                .find(|f| f.name == camel)
                .map(|f| f.name.clone())
        };
        let mapped_index = |cols: &[String],
                            fields: &[Field],
                            skipped: &mut Vec<LossItem>,
                            table: &str|
         -> Vec<String> {
            let mapped: Vec<String> = cols
                .iter()
                .filter_map(|c| column_to_field(c, fields))
                .collect();
            if mapped.len() != cols.len() {
                skipped.push(LossItem {
                    construct: format!("{table}.index({})", cols.join(", ")),
                    reason: "index column has no matching DSL field".into(),
                });
            }
            mapped
        };
        for cols in &table_index_cols {
            let mapped = mapped_index(cols, &fields, &mut skipped, &t.name);
            if !mapped.is_empty() {
                ix.push(format!("index [{}]", mapped.join(", ")));
            }
        }
        let mut searchables: Vec<String> = Vec::new();
        for cols in &table_fulltext_cols {
            let mapped = mapped_index(cols, &fields, &mut skipped, &t.name);
            if mapped.len() == cols.len() && !mapped.is_empty() {
                searchables.push(format!("searchable [{}]", mapped.join(", ")));
            }
        }
        for cols in &table_unique_cols {
            if cols.len() == 1 {
                if let Some(name) = column_to_field(&cols[0], &fields) {
                    for f in fields.iter_mut() {
                        if f.name == name && !f.attrs.iter().any(|a| a == "@id" || a == "@unique") {
                            f.attrs.push("@unique".into());
                        }
                    }
                }
            } else if !cols.is_empty() {
                skipped.push(LossItem {
                    construct: format!("{}.UNIQUE({})", t.name, cols.join(", ")),
                    reason: "composite unique is not expressible as column @unique".into(),
                });
            }
        }
        for i in &parsed.indexes {
            if !i.unique && norm(&i.table) == norm(&t.name) {
                let cols: Vec<String> = i
                    .columns
                    .iter()
                    .map(|c| {
                        let raw = c.strip_suffix("_id").unwrap_or(c);
                        to_camel(raw)
                    })
                    .collect();
                let cols: Vec<String> = cols
                    .into_iter()
                    .filter(|c| fields.iter().any(|f| f.name == *c))
                    .collect();
                if !cols.is_empty() {
                    ix.push(format!("index [{}]", cols.join(", ")));
                }
            }
        }
        indexes.insert(entity.clone(), ix);
        if !searchables.is_empty() {
            searchables_map.insert(entity.clone(), searchables);
        }
        entities.push((entity, fields));
    }

    let mut out = String::new();
    out.push_str(&format!(
        "// Generated by Forma from {file} (SQL DDL import)\n"
    ));
    out.push_str(
        "// 逆向工程骨架：表→实体，外键→关系，索引/约束按可表达范围映射；请人工校对业务规则。\n",
    );
    if !skipped.is_empty() {
        out.push_str("//\n// 以下构造未映射（需人工处理）：\n");
        for s in &skipped {
            out.push_str(&format!("//   - {} ({})\n", s.construct, s.reason));
        }
    }
    out.push_str("\napp ImportedApp {\n  version = \"0.1.0\"\n  locale = [\"en\"]\n}\n\n");
    for (name, fields) in &entities {
        out.push_str(&format!("entity {name} {{\n"));
        for f in fields {
            let attrs = if f.attrs.is_empty() {
                String::new()
            } else {
                format!(" {}", f.attrs.join(" "))
            };
            out.push_str(&format!("  {} {}{}\n", f.name, f.ty, attrs));
        }
        for line in indexes.get(name).into_iter().flatten() {
            out.push_str(&format!("\n  {line}\n"));
        }
        if let Some(searchable) = searchables_map.get(name) {
            for line in searchable {
                out.push_str(&format!("\n  {line}\n"));
            }
        }
        out.push_str("}\n\n");
    }
    for (name, _) in &entities {
        out.push_str(&format!(
            "api {name}s {{\n  entity {name}\n  operations [list get create update delete]\n}}\n\n"
        ));
    }
    let report = LossReport {
        format: "sql".into(),
        source: file.to_string(),
        entities: entities.len(),
        items: skipped,
    };
    Ok((out, report))
}
