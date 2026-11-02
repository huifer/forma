//! JSON Schema 适配器（draft 2020-12 子集）。
//! 顶层对象定义（$id）→ type 节点；x-forma-entity 绑定 → entity 节点。

use crate::adapters::{AdapterContext, AdapterResult, SpecAdapter};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, SourceRef, Standard};
use crate::identity::{entity_field_uri, entity_uri, normalize_name, type_uri};
use anyhow::Result;

pub struct JsonSchemaAdapter;

impl SpecAdapter for JsonSchemaAdapter {
    fn standard(&self) -> Standard {
        Standard::JsonSchema
    }

    fn parse(&self, ctx: &AdapterContext<'_>) -> Result<AdapterResult> {
        let text = crate::util::read_text(&ctx.absolute_path)?;
        let raw: serde_json::Value = serde_json::from_str(&text)?;
        let mut result = AdapterResult::default();

        let id = raw
            .get("$id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                ctx.file
                    .trim_end_matches(|c: char| c != '.' && c != '/')
                    .to_string()
            });
        let project = &ctx.loaded.manifest.project_name;
        let type_uri = type_uri(project, &id);

        let explicit_entity = raw
            .get("x-forma-entity")
            .and_then(|v| v.as_str())
            .map(String::from);

        // 实体身份：x-forma-entity > bindings > $id 本身作为实体名
        let binding_match = ctx
            .loaded
            .manifest
            .bindings
            .iter()
            .find(|b| normalize_name(b.uri.rsplit('/').next().unwrap_or("")) == normalize_name(&id))
            .map(|b| b.uri.clone());

        let entity_name = match (&explicit_entity, &binding_match) {
            (Some(e), _) => last_segment(e),
            (None, Some(b)) => last_segment(b),
            (None, None) => to_entity_name(&id),
        };
        let entity_uri = entity_uri(project, &entity_name);

        let required: Vec<String> = raw
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let props = raw
            .get("properties")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut properties: Vec<serde_json::Value> = Vec::new();
        for (name, prop) in &props {
            properties.push(serde_json::json!({
                "name": name,
                "type": prop.get("type").and_then(|v| v.as_str()).unwrap_or_else(|| infer_type(prop)),
                "format": prop.get("format").and_then(|v| v.as_str()),
                "enum": prop.get("enum"),
                "required": required.iter().any(|r| r == name),
                "description": prop.get("description").and_then(|v| v.as_str()),
                "minLength": prop.get("minLength").and_then(|v| v.as_u64()),
                "maxLength": prop.get("maxLength").and_then(|v| v.as_u64()),
                "pattern": prop.get("pattern").and_then(|v| v.as_str()),
                "itemsType": prop.get("items").and_then(|v| v.get("type")).and_then(|v| v.as_str()),
                "line": find_property_line(&text, name),
            }));
        }

        result.nodes.push(SemanticNode {
            uri: entity_uri.clone(),
            kind: NodeKind::Entity,
            name: entity_name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: ctx.file.clone(),
                line: Some(find_key_line(&text, "\"$id\"").unwrap_or(1)),
            },
            data: serde_json::json!({ "from": "json-schema", "$id": id }),
        });
        result.nodes.push(SemanticNode {
            uri: type_uri.clone(),
            kind: NodeKind::Type,
            name: id.clone(),
            standard: Standard::JsonSchema,
            source: SourceRef {
                file: ctx.file.clone(),
                line: None,
            },
            data: serde_json::json!({
                "$id": id,
                "entityUri": entity_uri,
                "type": raw.get("type").and_then(|v| v.as_str()).unwrap_or("object"),
                "properties": properties,
                "required": required,
            }),
        });
        result.edges.push(SemanticEdge {
            from: entity_uri.clone(),
            to: type_uri.clone(),
            kind: EdgeKind::Defines,
        });
        result.edges.push(SemanticEdge {
            from: entity_uri.clone(),
            to: type_uri,
            kind: EdgeKind::BelongsTo,
        });

        for prop in &properties {
            let prop_name = prop
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let field_uri = entity_field_uri(project, &entity_name, prop_name);
            result.nodes.push(SemanticNode {
                uri: field_uri.clone(),
                kind: NodeKind::EntityField,
                name: format!("{entity_name}.{prop_name}"),
                standard: Standard::Core,
                source: SourceRef {
                    file: ctx.file.clone(),
                    line: prop
                        .get("line")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize),
                },
                data: serde_json::json!({ "from": "json-schema", "property": prop }),
            });
            result.edges.push(SemanticEdge {
                from: entity_uri.clone(),
                to: field_uri,
                kind: EdgeKind::BelongsTo,
            });
        }

        Ok(result)
    }
}

fn to_entity_name(id: &str) -> String {
    let base = id.rsplit(['/', ':']).find(|s| !s.is_empty()).unwrap_or(id);
    base.trim_end_matches(".schema")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

fn last_segment(uri: &str) -> String {
    uri.rsplit('/').next().unwrap_or(uri).to_string()
}

fn infer_type(prop: &serde_json::Value) -> &'static str {
    if prop.get("enum").is_some() {
        "string"
    } else if prop.get("properties").is_some() {
        "object"
    } else if prop.get("items").is_some() {
        "array"
    } else {
        "unknown"
    }
}

fn find_key_line(text: &str, needle: &str) -> Option<usize> {
    text.find(needle).map(|idx| crate::util::line_of(text, idx))
}

fn find_property_line(text: &str, name: &str) -> Option<usize> {
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with(&format!("\"{name}\":"))
            || t.starts_with(&format!("\"{name}\" :"))
            || t.starts_with(&format!("'{name}':"))
        {
            return Some(i + 1);
        }
    }
    None
}
