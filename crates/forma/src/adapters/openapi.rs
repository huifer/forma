//! OpenAPI 3.x 适配器：paths/operations → api-operation 节点；
//! components.schemas → api-schema 节点（该实体在 OpenAPI 视图下的 schema 定义）；
//! x-forma-entity / $ref 关联实体。

use crate::adapters::{AdapterContext, AdapterResult, SpecAdapter};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, SourceRef, Standard};
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
                .map(|s| {
                    s.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
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
        let paths = doc
            .get("paths")
            .and_then(|v| v.as_mapping())
            .cloned()
            .unwrap_or_default();
        for (path_key, path_item) in &paths {
            let p = path_key.as_str().unwrap_or_default().to_string();
            for method in ["get", "post", "put", "patch", "delete"] {
                let Some(op) = path_item.get(method) else {
                    continue;
                };
                let operation_id = op
                    .get("operationId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!(
                            "{}{}",
                            method,
                            p.chars()
                                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                                .collect::<String>()
                        )
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

// ── OpenAPI 3.x → Forma DSL (.fm) 逆向工程 ────────────────────────────────────

/// 从 OpenAPI 3.x 规范逆向生成 Forma DSL (`.fm`) 源文件骨架。
///
/// 设计立场（与 RFC-0002 标准互通一致）：导入是**生成视图**——实体来自
/// `components.schemas`，API 来自 `paths`；关系由 `$ref` / tag 推断。生成的
/// `.fm` 是**可人工校对的骨架**，不保证 1:1 语义等价：枚举、数组、嵌套对象等
/// 在 v1 被保守降级为 `String`/`Text` 或跳过，应在生成后人工核对。
///
/// 生成的 `.fm` 保证通过 `forma check`（不引用不存在的实体、操作名均为合法 token），
/// 但不保证通过 `lint`（可能缺业务规则 / 页面）。
pub fn openapi_to_dsl(spec_text: &str, file: &str) -> Result<String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(spec_text)?;

    let info = doc.get("info");
    let title = info
        .and_then(|i| i.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("ImportedApp");
    let version = info
        .and_then(|i| i.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0");
    let app_name = sanitize_ident(title, true);

    // components.schemas → 实体
    let schemas = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_mapping())
        .cloned()
        .unwrap_or_default();

    // 第一遍：收集全部实体名（关系解析需要完整名单，避免前向引用漏判）
    let mut entity_names: Vec<String> = Vec::new();
    for (key, _) in &schemas {
        let name = key.as_str().unwrap_or_default().to_string();
        if !name.is_empty() {
            entity_names.push(name);
        }
    }
    // 第二遍：构建实体与字段
    let mut entities: Vec<(String, Vec<Field>)> = Vec::new();
    for (key, schema) in &schemas {
        let name = key.as_str().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let required: Vec<String> = schema
            .get("required")
            .and_then(|v| v.as_sequence())
            .map(|s| s.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let mut fields: Vec<Field> = Vec::new();
        if let Some(props) = schema.get("properties").and_then(|v| v.as_mapping()) {
            for (pk, pv) in props {
                let pname = pk.as_str().unwrap_or_default();
                if pname.is_empty() {
                    continue;
                }
                let fname = sanitize_ident(pname, false);
                if let Some((ty, mut attrs)) = map_property(pv, &required, pname, &entity_names) {
                    // OpenAPI 不显式声明主键：把名为 id 的字段当作 @id 主键，
                    // Uuid/String 类型补 @default(uuid)，以满足关系目标的 PK 约束。
                    if fname == "id" {
                        if !attrs.iter().any(|a| a == "@id") {
                            attrs.push("@id".to_string());
                        }
                        if (ty == "Uuid" || ty == "String")
                            && !attrs.iter().any(|a| a.starts_with("@default"))
                        {
                            attrs.push("@default(uuid)".to_string());
                        }
                    }
                    fields.push(Field { name: fname, ty, attrs });
                }
            }
        }
        // 保证实体有且仅有一个 @id 主键（关系目标必须满足）
        if !fields
            .iter()
            .any(|f| f.attrs.iter().any(|a| a == "@id"))
        {
            fields.insert(
                0,
                Field {
                    name: "id".to_string(),
                    ty: "Uuid".to_string(),
                    attrs: vec!["@id".to_string(), "@default(uuid)".to_string()],
                },
            );
        }
        entities.push((name.clone(), fields));
    }

    // paths → 每实体的操作集合
    let mut ops: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
        entity_names
            .iter()
            .map(|e| (e.clone(), std::collections::BTreeSet::new()))
            .collect();
    let paths = doc
        .get("paths")
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();
    for (path_key, path_item) in &paths {
        let p = path_key.as_str().unwrap_or_default();
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(op) = path_item.get(method) else {
                continue;
            };
            if let Some(entity) = resolve_entity(op, path_item, &entity_names) {
                if let Some(set) = ops.get_mut(&entity) {
                    set.insert(op_token(method, p));
                }
            }
        }
    }

    // 组装 .fm
    let mut out = String::new();
    out.push_str(&format!(
        "// Generated by Forma from {file} (OpenAPI 3.x import)\n"
    ));
    out.push_str(
        "// 逆向工程骨架：实体来自 components.schemas，API 来自 paths。请人工校对关系与业务规则。\n\n",
    );
    out.push_str(&format!(
        "app {app_name} {{\n  version = \"{version}\"\n  locale = [\"en\"]\n}}\n\n"
    ));
    for (name, fields) in &entities {
        out.push_str(&format!("entity {name} {{\n"));
        for f in fields {
            let attr = if f.attrs.is_empty() {
                String::new()
            } else {
                format!(" {}", f.attrs.join(" "))
            };
            out.push_str(&format!("  {} {}{}\n", f.name, f.ty, attr));
        }
        out.push_str("}\n\n");
    }
    for name in &entity_names {
        let set = &ops[name];
        let operations: Vec<&str> = if set.is_empty() {
            vec!["list", "get", "create", "update", "delete"]
        } else {
            ["list", "get", "create", "update", "delete"]
                .iter()
                .filter(|o| set.contains(**o))
                .copied()
                .collect()
        };
        out.push_str(&format!(
            "api {name} {{\n  entity {name}\n  operations [{}]\n}}\n\n",
            operations.join(" ")
        ));
    }
    Ok(out)
}

struct Field {
    name: String,
    ty: String,
    attrs: Vec<String>,
}

/// 把任意字符串清洗为合法 DSL 标识符。`pascal=true` 时首字母大写（用于 app / 实体名）。
fn sanitize_ident(s: &str, pascal: bool) -> String {
    let mut base = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c);
        } else {
            base.push('_');
        }
    }
    while base.contains("__") {
        base = base.replace("__", "_");
    }
    base = base.trim_matches('_').to_string();
    if base.is_empty() {
        base = "Field".to_string();
    } else if base.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        base = format!("_{base}");
    }
    if pascal {
        // CamelCase：把分隔符（已替换为 `_`）后的首字母大写后拼接
        let mut out = String::new();
        let mut capitalize_next = true;
        for c in base.chars() {
            if c == '_' {
                capitalize_next = true;
            } else if c.is_ascii_alphabetic() {
                if capitalize_next {
                    out.extend(c.to_uppercase());
                    capitalize_next = false;
                } else {
                    out.push(c);
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        base
    }
}

/// 把 OpenAPI property 映射为 (DSL 类型, 属性列表)。不支持的类型（数组 / 裸 object）返回 None 跳过。
fn map_property(
    pv: &serde_yaml::Value,
    required: &[String],
    pname: &str,
    entity_names: &[String],
) -> Option<(String, Vec<String>)> {
    let mut attrs = Vec::new();
    if required.iter().any(|r| r == pname) {
        attrs.push("@required".to_string());
    }
    // $ref → 关系
    if let Some(r) = pv.get("$ref").and_then(|v| v.as_str()) {
        let target = r.rsplit('/').next().unwrap_or(r).to_string();
        if entity_names.iter().any(|e| e == &target) {
            return Some((target, attrs));
        }
        // 指向未知 schema：保守降级为 Text（保留结构信息有限）
        return Some(("Text".to_string(), attrs));
    }
    let ty = pv.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let format = pv.get("format").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "string" => {
            let t = match format {
                "email" => "Email".to_string(),
                "uuid" => "Uuid".to_string(),
                "date-time" => "Timestamp".to_string(),
                "date" => "Date".to_string(),
                _ => {
                    if let Some(n) = pv.get("maxLength").and_then(|v| v.as_u64()) {
                        format!("String({n})")
                    } else {
                        "String".to_string()
                    }
                }
            };
            Some((t, attrs))
        }
        "integer" => {
            push_min_max(&mut attrs, pv);
            Some(("Int".to_string(), attrs))
        }
        "number" => {
            push_min_max(&mut attrs, pv);
            Some(("Decimal".to_string(), attrs))
        }
        "boolean" => Some(("Bool".to_string(), attrs)),
        _ => {
            // array / object / enum：v1 保守降级
            if pv.get("enum").is_some() {
                Some(("String".to_string(), attrs))
            } else if ty == "array" {
                None
            } else {
                Some(("Text".to_string(), attrs))
            }
        }
    }
}

fn push_min_max(attrs: &mut Vec<String>, pv: &serde_yaml::Value) {
    if let Some(n) = pv.get("minimum").and_then(|v| v.as_i64()) {
        attrs.push(format!("@min({n})"));
    }
    if let Some(n) = pv.get("maximum").and_then(|v| v.as_i64()) {
        attrs.push(format!("@max({n})"));
    }
}

/// 从一次 operation 推断所属实体（按 tag / requestBody $ref / response $ref / path 级 tag 顺序）。
fn resolve_entity(
    op: &serde_yaml::Value,
    path_item: &serde_yaml::Value,
    entity_names: &[String],
) -> Option<String> {
    let ref_target = |v: &serde_yaml::Value| -> Option<String> {
        let r = v.as_str()?;
        let target = r.rsplit('/').next().unwrap_or(r).to_string();
        entity_names.iter().any(|e| e == &target).then_some(target)
    };
    // 1. operation 级 tag
    if let Some(t) = op
        .get("tags")
        .and_then(|v| v.as_sequence())
        .and_then(|s| s.first())
        .and_then(|x| x.as_str())
    {
        if entity_names.iter().any(|e| e == t) {
            return Some(t.to_string());
        }
    }
    // 2. requestBody $ref
    if let Some(r) = op
        .get("requestBody")
        .and_then(|rb| rb.get("content"))
        .and_then(|c| c.as_mapping())
        .and_then(|c| c.values().next())
        .and_then(|media| media.get("schema"))
        .and_then(|s| s.get("$ref"))
        .and_then(ref_target)
    {
        return Some(r);
    }
    // 3. response $ref
    if let Some(responses) = op.get("responses").and_then(|v| v.as_mapping()) {
        for (_, resp) in responses {
            if let Some(r) = resp
                .get("content")
                .and_then(|c| c.as_mapping())
                .and_then(|c| c.values().next())
                .and_then(|media| media.get("schema"))
                .and_then(|s| s.get("$ref"))
                .and_then(ref_target)
            {
                return Some(r);
            }
        }
    }
    // 4. path 级 tag（部分规范把 tag 放在 path 级）
    if let Some(t) = path_item
        .get("tags")
        .and_then(|v| v.as_sequence())
        .and_then(|s| s.first())
        .and_then(|x| x.as_str())
    {
        if entity_names.iter().any(|e| e == t) {
            return Some(t.to_string());
        }
    }
    None
}

/// HTTP method + 路径 → DSL 操作 token。
fn op_token(method: &str, path: &str) -> String {
    let token: &str = match method {
        "post" => "create",
        "put" | "patch" => "update",
        "delete" => "delete",
        "get" => {
            if path.trim_end_matches('/').ends_with('}') {
                "get"
            } else {
                "list"
            }
        }
        _ => "list",
    };
    token.to_string()
}
