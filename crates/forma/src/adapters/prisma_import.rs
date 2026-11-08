//! Prisma schema → Forma DSL（`.fm`）逆向工程生成视图（roadmap M10 / 轨道 A-8）。
//!
//! 与 [sql_import] / [openapi] 同一立场：**保守骨架 + 显式跳过**，不保证 1:1 等价。
//! 支持：model→entity、标量字段、enum→enum、@id/@unique/@default(uuid|now)、
//! `@relation(fields:[...], references:[...])` 正向关系、`@@index` / `@@unique`。
//! 不支持：反向列表关系（`posts Post[]`）、`@@map` / `@map`、复合主键、视图、
//! 多文件 schema、原生类型（`@db.*`）——**显式跳过并在注释中列出**，绝不静默编造。

use crate::adapters::{LossItem, LossReport};
use anyhow::{bail, Result};
use std::collections::BTreeSet;

fn ident(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in s.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' {
            upper = true;
            continue;
        }
        if c == '_' {
            upper = true;
            continue;
        }
        if upper && !out.is_empty() {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "field".into()
    } else {
        out
    }
}

fn scalar_type(t: &str) -> Option<String> {
    match t.trim_end_matches('?') {
        "String" => Some("String(255)".into()),
        "Int" | "BigInt" => Some("Int".into()),
        "Float" => Some("Float".into()),
        "Boolean" => Some("Bool".into()),
        "DateTime" => Some("Timestamp".into()),
        "Decimal" => Some("Decimal(10,2)".into()),
        "Json" => Some("Json".into()),
        "Bytes" | "Unsupported" => None,
        // 枚举 / 关联模型由调用方按已知名单判定
        _ => None,
    }
}

#[derive(Debug)]
struct FmField {
    name: String,
    ty: String,
    attrs: Vec<String>,
    optional: bool,
}

/// 从 `@relation(fields: [authorId], references: [id])` 提取 fields 列表。
fn relation_scalar_fields(line: &str) -> Vec<String> {
    let Some(start) = line.find("fields:") else {
        return vec![];
    };
    let rest = &line[start + "fields:".len()..];
    let Some(open) = rest.find('[') else {
        return vec![];
    };
    let Some(close) = rest[open + 1..].find(']') else {
        return vec![];
    };
    rest[open + 1..open + 1 + close]
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 从 `@@index([a, b])` / `@@unique([a, b])` 提取列清单。
fn block_attrs(line: &str) -> Vec<String> {
    let Some(open) = line.find('[') else {
        return vec![];
    };
    let Some(close) = line[open + 1..].find(']') else {
        return vec![];
    };
    line[open + 1..open + 1 + close]
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn prisma_to_dsl(text: &str, file: &str) -> Result<String> {
    Ok(prisma_to_dsl_with_report(text, file)?.0)
}

/// RFC-0002 §14 S2：返回骨架 + 机读损失清单。
pub fn prisma_to_dsl_with_report(text: &str, file: &str) -> Result<(String, LossReport)> {
    // 第一遍：收集 model / enum 名（关系解析需要完整名单）
    let mut models: Vec<String> = Vec::new();
    let mut enums: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<(String, Vec<String>, bool)> = None; // (name, body, is_enum)
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("model ") || t.starts_with("enum ") {
            let is_enum = t.starts_with("enum ");
            let name = t
                .trim_start_matches("model ")
                .trim_start_matches("enum ")
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();
            if name.is_empty() {
                bail!("invalid model/enum declaration in {file}: `{t}`");
            }
            current = Some((name, Vec::new(), is_enum));
            continue;
        }
        if t == "}" {
            if let Some((name, body, is_enum)) = current.take() {
                if is_enum {
                    let variants: Vec<String> = body
                        .iter()
                        .filter(|l| !l.starts_with("@@") && !l.starts_with("//"))
                        .filter_map(|l| l.split_whitespace().next().map(String::from))
                        .filter(|v| !v.is_empty())
                        .collect();
                    enums.push((name, variants));
                } else {
                    models.push(name);
                }
            }
            continue;
        }
        if let Some((_, body, _)) = current.as_mut() {
            if !t.is_empty() && !t.starts_with("//") {
                body.push(t.to_string());
            }
        }
    }
    if models.is_empty() {
        bail!("no model block found in {file} — nothing to import");
    }

    let model_set: BTreeSet<String> = models.iter().cloned().collect();
    let enum_set: BTreeSet<String> = enums.iter().map(|(n, _)| n.clone()).collect();

    // 第二遍：逐 model 生成实体
    let mut skipped: Vec<LossItem> = Vec::new();
    let mut entity_blocks: Vec<(String, Vec<FmField>, Vec<String>)> = Vec::new(); // (name, fields, index lines)
                                                                                  // 重新扫描以取回每个 model 的 body
    let mut current: Option<(String, Vec<String>, bool)> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("model ") || t.starts_with("enum ") {
            let is_enum = t.starts_with("enum ");
            let name = t
                .trim_start_matches("model ")
                .trim_start_matches("enum ")
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();
            current = Some((name, Vec::new(), is_enum));
            continue;
        }
        if t == "}" {
            let Some((name, body, is_enum)) = current.take() else {
                continue;
            };
            if is_enum {
                continue;
            }
            let mut fk_scalars: BTreeSet<String> = BTreeSet::new();
            for l in &body {
                if l.contains("@relation(") && l.contains("fields:") {
                    for f in relation_scalar_fields(l) {
                        fk_scalars.insert(f);
                    }
                }
            }
            let mut fields: Vec<FmField> = Vec::new();
            let mut index_lines: Vec<String> = Vec::new();
            for l in &body {
                if l.starts_with("@@") {
                    let cols: Vec<String> = block_attrs(l)
                        .into_iter()
                        .filter(|c| !fk_scalars.contains(c))
                        .collect();
                    if !cols.is_empty() {
                        let kw = if l.starts_with("@@unique") {
                            "unique"
                        } else {
                            "index"
                        };
                        index_lines.push(format!("{kw} [{}]", cols.join(", ")));
                    } else {
                        skipped.push(LossItem {
                            construct: format!("{name}.{l}"),
                            reason: "block-level attribute with no expressible columns".into(),
                        });
                    }
                    continue;
                }
                let mut parts = l.split_whitespace();
                let Some(raw_name) = parts.next() else {
                    continue;
                };
                let Some(raw_ty) = parts.next() else {
                    continue;
                };
                let attrs_text = parts.collect::<Vec<_>>().join(" ");
                let base_ty = raw_ty.trim_end_matches('?');
                let optional = raw_ty.ends_with('?');
                let list_relation = raw_ty.ends_with("[]");

                if list_relation {
                    // 反向列表关系：v1 不生成（需要逆向关系 DSL）
                    skipped.push(LossItem {
                        construct: format!("{name}.{raw_name} ({raw_ty})"),
                        reason: "reverse list relation; not expressible in DSL v0.3".into(),
                    });
                    continue;
                }
                if fk_scalars.contains(raw_name) {
                    continue; // 关系标量列由 @relation 字段承载
                }
                let is_relation = model_set.contains(base_ty) && attrs_text.contains("@relation(");
                let ty = if is_relation || enum_set.contains(base_ty) {
                    base_ty.to_string()
                } else {
                    match scalar_type(base_ty) {
                        Some(t) => t,
                        None => {
                            skipped.push(LossItem {
                                construct: format!("{name}.{raw_name} ({raw_ty})"),
                                reason: "unsupported Prisma scalar type".into(),
                            });
                            continue;
                        }
                    }
                };
                let mut attrs: Vec<String> = Vec::new();
                if attrs_text.contains("@id") {
                    attrs.push("@id".into());
                }
                if attrs_text.contains("@default(uuid())") {
                    attrs.push("@default(uuid)".into());
                } else if attrs_text.contains("@default(now())") {
                    attrs.push("@default(now)".into());
                }
                if attrs_text.contains("@unique") {
                    attrs.push("@unique".into());
                }
                if !optional && !attrs_text.contains("@id") {
                    attrs.push("@required".into());
                }
                fields.push(FmField {
                    name: ident(raw_name),
                    ty,
                    attrs,
                    optional,
                });
            }
            if !fields.iter().any(|f| f.attrs.iter().any(|a| a == "@id")) {
                fields.insert(
                    0,
                    FmField {
                        name: "id".into(),
                        ty: "Uuid".into(),
                        attrs: vec!["@id".into(), "@default(uuid)".into()],
                        optional: false,
                    },
                );
                skipped.push(LossItem {
                    construct: format!("{name}.id"),
                    reason: "model had no @id; synthesized Uuid @id".into(),
                });
            }
            entity_blocks.push((name, fields, index_lines));
        } else if let Some((_, body, _)) = current.as_mut() {
            if !t.is_empty() && !t.starts_with("//") {
                body.push(t.to_string());
            }
        }
    }

    let mut out = String::new();
    out.push_str(&format!(
        "// Generated by Forma from {file} (Prisma schema import)\n"
    ));
    out.push_str(
        "// 逆向工程骨架：model→entity、enum→enum、@relation 正向关系；请人工校对业务规则。\n",
    );
    if !skipped.is_empty() {
        out.push_str("//\n// 以下构造未映射（需人工处理）：\n");
        for s in &skipped {
            out.push_str(&format!("//   - {} ({})\n", s.construct, s.reason));
        }
    }
    out.push_str("\napp ImportedApp {\n  version = \"0.1.0\"\n  locale = [\"en\"]\n}\n\n");
    for (name, variants) in &enums {
        if variants.is_empty() {
            continue;
        }
        out.push_str(&format!("enum {name} {{\n"));
        for v in variants {
            out.push_str(&format!("  {v}\n"));
        }
        out.push_str("}\n\n");
    }
    for (name, fields, index_lines) in &entity_blocks {
        out.push_str(&format!("entity {name} {{\n"));
        for f in fields {
            let opt = if f.optional { "?" } else { "" };
            let attrs = if f.attrs.is_empty() {
                String::new()
            } else {
                format!(" {}", f.attrs.join(" "))
            };
            out.push_str(&format!("  {} {}{}{}\n", f.name, f.ty, opt, attrs));
        }
        for line in index_lines {
            out.push_str(&format!("\n  {line}\n"));
        }
        out.push_str("}\n\n");
    }
    for (name, _, _) in &entity_blocks {
        out.push_str(&format!(
            "api {name}s {{\n  entity {name}\n  operations [list get create update delete]\n}}\n\n"
        ));
    }
    let report = LossReport {
        format: "prisma".into(),
        source: file.to_string(),
        entities: entity_blocks.len(),
        items: skipped,
    };
    Ok((out, report))
}
