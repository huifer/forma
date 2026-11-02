//! JSON Forms 适配器：UI Schema → ui-form / ui-control 节点。
//! 通过 `schema` 字段关联 JSON Schema；实体关联由引擎后置匹配。

use crate::adapters::{AdapterContext, AdapterResult, SpecAdapter};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, Standard, SourceRef};
use crate::identity::{ui_control_uri, ui_form_uri};
use anyhow::Result;

pub struct JsonFormsAdapter;

impl SpecAdapter for JsonFormsAdapter {
    fn standard(&self) -> Standard {
        Standard::JsonForms
    }

    fn parse(&self, ctx: &AdapterContext<'_>) -> Result<AdapterResult> {
        let text = crate::util::read_text(&ctx.absolute_path)?;
        let doc: serde_json::Value = serde_json::from_str(&text)?;
        let mut result = AdapterResult::default();
        let project = ctx.loaded.manifest.project_name.clone();

        let form_id = doc
            .get("$id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                ctx.file
                    .rsplit('/')
                    .next()
                    .unwrap_or(&ctx.file)
                    .trim_end_matches(".ui.json")
                    .trim_end_matches(".json")
                    .to_string()
            });

        let bound_schema_id = doc.get("schema").and_then(|v| v.as_str()).map(String::from);

        let mut controls: Vec<serde_json::Value> = Vec::new();
        collect_controls(&doc, &text, &mut controls);

        let form_uri = ui_form_uri(&project, &form_id);
        result.nodes.push(SemanticNode {
            uri: form_uri.clone(),
            kind: NodeKind::UiForm,
            name: form_id.clone(),
            standard: Standard::JsonForms,
            source: SourceRef { file: ctx.file.clone(), line: None },
            data: serde_json::json!({
                "formId": form_id,
                "boundSchemaId": bound_schema_id,
                "controls": controls,
            }),
        });

        for control in &controls {
            let cid = control.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let control_uri = ui_control_uri(&project, &form_id, cid);
            let _ = &control_uri;
            result.nodes.push(SemanticNode {
                uri: control_uri.clone(),
                kind: NodeKind::UiControl,
                name: format!("{form_id}.{cid}"),
                standard: Standard::JsonForms,
                source: SourceRef {
                    file: ctx.file.clone(),
                    line: control.get("line").and_then(|v| v.as_u64()).map(|v| v as usize),
                },
                data: control.clone(),
            });
            result.edges.push(SemanticEdge {
                from: form_uri.clone(),
                to: control_uri,
                kind: EdgeKind::BelongsTo,
            });
        }

        Ok(result)
    }
}

fn collect_controls(node: &serde_json::Value, text: &str, out: &mut Vec<serde_json::Value>) {
    match node {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_controls(item, text, out);
            }
        }
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|v| v.as_str()) == Some("Control") {
                if let Some(scope) = map.get("scope").and_then(|v| v.as_str()) {
                    let field = scope.rsplit('/').next().unwrap_or(scope).to_string();
                    out.push(serde_json::json!({
                        "id": field,
                        "scope": scope,
                        "label": map.get("label").and_then(|v| v.as_str()),
                        "type": "control",
                        "line": find_scope_line(text, scope),
                    }));
                }
            }
            if let Some(elements) = map.get("elements").and_then(|v| v.as_array()) {
                collect_controls(&serde_json::Value::Array(elements.clone()), text, out);
            }
        }
        _ => {}
    }
}

fn find_scope_line(text: &str, scope: &str) -> Option<u64> {
    for (i, line) in text.lines().enumerate() {
        if line.contains(&format!("\"{scope}\"")) || line.contains(&format!("'{scope}'")) {
            return Some((i + 1) as u64);
        }
    }
    None
}
