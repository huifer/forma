//! OpenAPI 3.1 导出视图：`api` 块 → openapi.json + Swagger UI 静态页。

use super::Program;
use crate::util::{sha256_hex, write_text};
use anyhow::Result;
use std::path::Path;

pub fn emit_openapi(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Vec<crate::generators::GeneratedFile>> {
    if program.apis.is_empty() {
        return Ok(vec![]);
    }
    let title = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "forma-app".into());
    let version = program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| "0.1.0".into());

    let mut paths = serde_json::Map::new();
    let mut schemas = serde_json::Map::new();

    // 实体 → schema
    for entity in &program.entities {
        let mut props = serde_json::Map::new();
        let mut required = Vec::new();
        for field in &entity.fields {
            let (schema, is_required) = field_schema(program, field);
            props.insert(crate::identity::to_snake_case(&field.name), schema);
            if is_required {
                required.push(serde_json::Value::String(crate::identity::to_snake_case(
                    &field.name,
                )));
            }
        }
        let mut schema_obj = serde_json::json!({ "type": "object", "properties": props });
        if !required.is_empty() {
            schema_obj["required"] = serde_json::Value::Array(required.into_iter().collect());
        }
        schemas.insert(entity.name.clone(), schema_obj);
    }

    // api → paths
    for api in &program.apis {
        let entity = api.entity.as_str();
        let Some(decl) = super::find_entity(program, entity) else {
            continue;
        };
        let _table = super::pluralize(entity);
        let base = format!(
            "/api/{}",
            super::pluralize(&crate::identity::to_snake_case(entity))
        );
        let tag = api.name.clone();

        for op in &api.operations {
            match op.as_str() {
                "list" => {
                    let mut parameters = Vec::new();
                    if let Some(list) = &api.list {
                        for (f, contains) in &list.filters {
                            parameters.push(serde_json::json!({
                                "name": format!("{}{}", crate::identity::to_snake_case(f), if *contains { "_contains" } else { "" }),
                                "in": "query",
                                "schema": { "type": "string" }
                            }));
                        }
                        for s in &list.search {
                            parameters.push(serde_json::json!({
                                "name": format!("{}_search", crate::identity::to_snake_case(s)),
                                "in": "query",
                                "schema": { "type": "string" }
                            }));
                        }
                        parameters.push(serde_json::json!({
                            "name": "page", "in": "query", "schema": { "type": "integer", "default": 1 }
                        }));
                        parameters.push(serde_json::json!({
                            "name": "pageSize", "in": "query", "schema": { "type": "integer", "default": 20 }
                        }));
                    }
                    paths.insert(
                        base.clone(),
                        serde_json::json!({
                            "get": {
                                "tags": [tag],
                                "summary": format!("List {}", entity),
                                "parameters": parameters,
                                "responses": { "200": { "description": "OK", "content": { "application/json": { "schema": { "type": "array", "items": { "$ref": format!("#/components/schemas/{entity}") } } } } } }
                            }
                        }),
                    );
                }
                "get" => {
                    paths.insert(
                        format!("{base}/{{id}}"),
                        serde_json::json!({
                            "get": {
                                "tags": [tag],
                                "summary": format!("Get {entity} by id"),
                                "parameters": [ { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } } ],
                                "responses": { "200": { "description": "OK", "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{entity}") } } } } }
                            }
                        }),
                    );
                }
                "create" => {
                    paths
                        .entry(base.clone())
                        .or_insert_with(|| serde_json::json!({}))
                        .as_object_mut()
                        .unwrap()
                        .insert(
                            "post".to_string(),
                            serde_json::json!({
                                "tags": [tag],
                                "summary": format!("Create {entity}"),
                                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/Create{entity}") } } } },
                                "responses": { "201": { "description": "Created" } }
                            }),
                        );
                    // Create schema
                    let mut props = serde_json::Map::new();
                    for field in &decl.fields {
                        if super::has_attr(field, "id") || super::has_attr(field, "readonly") {
                            continue;
                        }
                        let (schema, is_required) = input_field_schema(program, field);
                        props.insert(crate::identity::to_snake_case(&field.name), schema);
                        if is_required {
                            required_entry(
                                &mut props,
                                &crate::identity::to_snake_case(&field.name),
                            );
                        }
                    }
                    schemas.insert(
                        format!("Create{entity}"),
                        serde_json::json!({ "type": "object", "properties": props }),
                    );
                }
                "update" => {
                    paths.insert(
                        format!("{base}/{{id}}"),
                        serde_json::json!({
                            "put": {
                                "tags": [tag],
                                "summary": format!("Update {entity}"),
                                "parameters": [ { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } } ],
                                "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/Update{entity}") } } } },
                                "responses": { "200": { "description": "OK" } }
                            }
                        }),
                    );
                    let mut props = serde_json::Map::new();
                    for field in &decl.fields {
                        if super::has_attr(field, "id") {
                            continue;
                        }
                        let (schema, _) = input_field_schema(program, field);
                        props.insert(crate::identity::to_snake_case(&field.name), schema);
                    }
                    schemas.insert(
                        format!("Update{entity}"),
                        serde_json::json!({ "type": "object", "properties": props }),
                    );
                }
                "delete" => {
                    paths
                        .entry(format!("{base}/{{id}}"))
                        .or_insert_with(|| serde_json::json!({}))
                        .as_object_mut()
                        .unwrap()
                        .insert(
                            "delete".to_string(),
                            serde_json::json!({
                                "tags": [tag],
                                "summary": format!("Delete {entity}"),
                                "parameters": [ { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } } ],
                                "responses": { "200": { "description": "OK" } }
                            }),
                        );
                }
                _ => {}
            }
        }
    }

    let doc = serde_json::json!({
        "openapi": "3.1.0",
        "info": { "title": format!("{title} API"), "version": version, "description": "Generated by Forma (gen-openapi)" },
        "paths": paths,
        "components": { "schemas": schemas }
    });
    let json_text = serde_json::to_string_pretty(&doc)?;

    let swagger_html = format!(
        r##"<!doctype html>
<html>
  <head>
    <title>{title} API — Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
      window.onload = () => SwaggerUIBundle({{ url: "openapi.json", dom_id: "#swagger-ui" }});
    </script>
  </body>
</html>
"##
    );

    let mut files = Vec::new();
    for (rel, content, gen) in [
        (format!("{out_dir}/openapi.json"), json_text, "gen-openapi"),
        (
            format!("{out_dir}/swagger-ui.html"),
            swagger_html,
            "gen-openapi",
        ),
    ] {
        let abs = project_dir.join(&rel);
        let hash = sha256_hex(&content);
        let unchanged = abs.is_file()
            && crate::util::read_text(&abs)
                .map(|c| sha256_hex(&c) == hash)
                .unwrap_or(false);
        if !unchanged {
            write_text(&abs, &content)?;
        }
        files.push(crate::generators::GeneratedFile {
            path: rel,
            content,
            generator: gen.into(),
        });
    }
    Ok(files)
}

fn required_entry(props: &mut serde_json::Map<String, serde_json::Value>, name: &str) {
    if let Some(obj) = props.get_mut(name).and_then(|v| v.as_object_mut()) {
        obj.insert("__required".into(), serde_json::Value::Bool(true));
    }
}

fn field_schema(program: &Program, field: &crate::dsl::FieldDecl) -> (serde_json::Value, bool) {
    let required = !field.optional && super::has_attr(field, "required");
    if super::is_relation(program, &field.type_name) {
        return (
            serde_json::json!({ "type": "string", "format": "uuid", "description": format!("FK → {}", field.type_name) }),
            required,
        );
    }
    let schema = match field.type_name.as_str() {
        "Uuid" => serde_json::json!({ "type": "string", "format": "uuid" }),
        "String" | "Text" => serde_json::json!({ "type": "string" }),
        "Email" => serde_json::json!({ "type": "string", "format": "email" }),
        "Int" => serde_json::json!({ "type": "integer", "format": "int64" }),
        "Float" => serde_json::json!({ "type": "number" }),
        "Decimal" => serde_json::json!({ "type": "string", "description": "decimal" }),
        "Bool" => serde_json::json!({ "type": "boolean" }),
        "Timestamp" => serde_json::json!({ "type": "string", "format": "date-time" }),
        "Date" => serde_json::json!({ "type": "string", "format": "date" }),
        "Json" => serde_json::json!({}),
        other => {
            if super::is_enum(program, other) {
                let Some(e) = program.enums.iter().find(|e| e.name == other) else {
                    return (serde_json::json!({ "type": "string" }), required);
                };
                let variants: Vec<serde_json::Value> = e
                    .variants
                    .iter()
                    .map(|v| serde_json::Value::String(v.name.clone()))
                    .collect();
                serde_json::json!({ "type": "string", "enum": variants })
            } else {
                serde_json::json!({ "type": "string", "format": "uuid", "description": format!("FK → {other}") })
            }
        }
    };
    (schema, required)
}

fn input_field_schema(
    program: &Program,
    field: &crate::dsl::FieldDecl,
) -> (serde_json::Value, bool) {
    if super::is_relation(program, &field.type_name) {
        return (
            serde_json::json!({ "type": "string", "format": "uuid" }),
            !field.optional && super::has_attr(field, "required"),
        );
    }
    field_schema(program, field)
}
