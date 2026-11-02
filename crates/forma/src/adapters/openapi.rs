//! OpenAPI 3.x 适配器：paths/operations → api-operation 节点；
//! components.schemas → api-schema 节点（该实体在 OpenAPI 视图下的 schema 定义）；
//! x-forma-entity / $ref 关联实体。

use crate::adapters::{AdapterContext, AdapterResult, SpecAdapter};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, Standard, SourceRef};
use crate::identity::{api_operation_uri, api_schema_uri, entity_uri, normalize_name};
use anyhow::Result;

pub struct OpenApiAdapter;

impl SpecAdapter for OpenApiAdapter {
    fn standard(&self) -> Standard {
        Standard::Openapi
    }

    fn parse(&self, ctx: &AdapterContext<'_>) -> Result<AdapterResult> {
        let text = crate::util::read_text(&ctx.absolute_path)?;
        let doc: serde_yaml::Value = serde_yaml::from_str(&text)?;
        let mut result = AdapterResult::default();
        let project = ctx.loaded.manifest.project_name.clone();

        let schemas = doc
            .get("components")
            .and_then(|c| c.get("schemas"))
            .and_then(|s| s.as_mapping())
            .cloned()
            .unwrap_or_default();

        // components.schemas → api-schema 节点 + entity 认领
        for (key, schema) in &schemas {
            let name = key.as_str().unwrap_or_default().to_string();
            if name.is_empty() {
                continue;
            }
            let api_schema_uri = api_schema_uri(&project, &name);
            let required: Vec<String> = schema
                .get("required")
                .and_then(|v| v.as_sequence())
                .map(|s| s.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let mut properties: Vec<serde_json::Value> = Vec::new();
            if let Some(props) = schema.get("properties").and_then(|v| v.as_mapping()) {
                for (pk, pv) in props {
                    let pname = pk.as_str().unwrap_or_default();
                    properties.push(serde_json::json!({
                        "name": pname,
                        "type": pv.get("type").and_then(|v| v.as_str()),
                        "$ref": pv.get("$ref").and_then(|v| v.as_str()),
                        "required": required.iter().any(|r| r == pname),
                    }));
                }
            }

            let entity = match schema.get("x-forma-entity").and_then(|v| v.as_str()) {
                Some(e) => Some(e.to_string()),
                None => ctx
                    .loaded
                    .manifest
                    .bindings
                    .iter()
                    .find(|b| {
                        normalize_name(b.uri.rsplit('/').next().unwrap_or(""))
                            == normalize_name(&name)
                    })
                    .map(|b| b.uri.clone())
                    .or_else(|| Some(entity_uri(&project, &name))), // 约定：组件 schema 即实体
            };

            if let Some(entity_uri) = &entity {
                if !result.nodes.iter().any(|n| n.uri == *entity_uri) {
                    result.nodes.push(SemanticNode {
                        uri: entity_uri.clone(),
                        kind: NodeKind::Entity,
                        name: name.clone(),
                        standard: Standard::Core,
                        source: SourceRef {
                            file: ctx.file.clone(),
                            line: find_key_line(&text, &name),
                        },
                        data: serde_json::json!({ "from": "openapi", "schema": name }),
                    });
                }
                result.edges.push(SemanticEdge {
                    from: entity_uri.clone(),
                    to: api_schema_uri.clone(),
                    kind: EdgeKind::Defines,
                });
            }

            result.nodes.push(SemanticNode {
                uri: api_schema_uri,
                kind: NodeKind::Type,
                name: name.clone(),
                standard: Standard::Openapi,
                source: SourceRef {
                    file: ctx.file.clone(),
                    line: find_key_line(&text, &name),
                },
                data: serde_json::json!({
                    "name": name,
                    "entityUri": entity,
                    "type": schema.get("type").and_then(|v| v.as_str()).unwrap_or("object"),
                    "properties": properties,
                    "required": required,
                }),
            });
        }

        // paths → operations
        let paths = doc.get("paths").and_then(|v| v.as_mapping()).cloned().unwrap_or_default();
        for (path_key, path_item) in &paths {
            let p = path_key.as_str().unwrap_or_default().to_string();
            for method in ["get", "post", "put", "patch", "delete"] {
                let Some(op) = path_item.get(method) else { continue };
                let operation_id = op
                    .get("operationId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!("{}{}", method, p.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>())
                    });

                let request_ref = op
                    .get("requestBody")
                    .and_then(|rb| rb.get("content"))
                    .and_then(|c| c.as_mapping())
                    .and_then(|c| c.values().next())
                    .and_then(|media| media.get("schema"))
                    .and_then(|s| s.get("$ref"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let mut response_refs: Vec<String> = Vec::new();
                if let Some(responses) = op.get("responses").and_then(|v| v.as_mapping()) {
                    for (_, resp) in responses {
                        if let Some(content) = resp.get("content").and_then(|v| v.as_mapping()) {
                            for (_, media) in content {
                                if let Some(r) = media
                                    .get("schema")
                                    .and_then(|s| s.get("$ref"))
                                    .and_then(|v| v.as_str())
                                {
                                    response_refs.push(r.to_string());
                                }
                            }
                        }
                    }
                }

                let mut parameters: Vec<serde_json::Value> = Vec::new();
                if let Some(params) = op.get("parameters").and_then(|v| v.as_sequence()) {
                    for param in params {
                        parameters.push(serde_json::json!({
                            "name": param.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                            "in": param.get("in").and_then(|v| v.as_str()).unwrap_or("query"),
                            "required": param.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                        }));
                    }
                }

                let op_uri = api_operation_uri(&project, &operation_id);
                result.nodes.push(SemanticNode {
                    uri: op_uri.clone(),
                    kind: NodeKind::ApiOperation,
                    name: format!("{} {}", method.to_uppercase(), p),
                    standard: Standard::Openapi,
                    source: SourceRef {
                        file: ctx.file.clone(),
                        line: Some(find_operation_line(&text, method, &p)),
                    },
                    data: serde_json::json!({
                        "operationId": operation_id,
                        "method": method,
                        "path": p,
                        "summary": op.get("summary").and_then(|v| v.as_str()),
                        "requestSchemaRef": request_ref,
                        "responseSchemaRefs": response_refs,
                        "parameters": parameters,
                    }),
                });

                // operation ← 实体（request/response $ref）
                let mut refs: Vec<Option<&String>> = Vec::new();
                if let Some(r) = op.get("requestBody").is_some().then_some(&request_ref) {
                    refs.push(r.as_ref());
                }
                for r in &response_refs {
                    refs.push(Some(r));
                }
                for r in refs.into_iter().flatten() {
                    if let Some(name) = r.strip_prefix("#/components/schemas/") {
                        let e = entity_uri(&project, name);
                        if result.nodes.iter().any(|n| n.uri == e) {
                            result.edges.push(SemanticEdge {
                                from: e,
                                to: op_uri.clone(),
                                kind: EdgeKind::References,
                            });
                        }
                    }
                }
            }
        }

        Ok(result)
    }
}

fn find_key_line(text: &str, needle: &str) -> Option<usize> {
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.starts_with(&format!("{needle}:"))
            || t.starts_with(&format!("{needle}: "))
            || t.starts_with(&format!("\"{needle}\":"))
        {
            return Some(i + 1);
        }
    }
    None
}

fn find_operation_line(text: &str, method: &str, p: &str) -> usize {
    let path_line = text
        .lines()
        .position(|l| l.trim().starts_with(&format!("{p}:")))
        .unwrap_or(0);
    // 在路径块内找 method（简化：路径行之后 40 行内）
    for (i, line) in text.lines().enumerate().skip(path_line) {
        if i > path_line + 40 {
            break;
        }
        if line.trim().starts_with(&format!("{method}:")) {
            return i + 1;
        }
    }
    path_line + 1
}
