//! 跨标准一致性校验（forma doctor）。规则清单见 ARCHITECTURE.md §7。

use crate::graph::{EdgeKind, NodeKind, Standard, UnifiedSemanticGraph};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: String, // error | warning | info
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<String>>,
}

pub fn format_diagnostic(d: &Diagnostic) -> String {
    let loc = d
        .file
        .as_ref()
        .map(|f| match d.line {
            Some(l) => format!("{f}:{l} "),
            None => format!("{f} "),
        })
        .unwrap_or_default();
    format!("[{}] {} {}— {}", d.severity.to_uppercase(), d.code, loc, d.message)
}

pub fn run_doctor(graph: &UnifiedSemanticGraph) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    out.extend(check_field_consistency(graph));
    out.extend(check_form_bindings(graph));
    out.extend(check_workflow_references(graph));
    out
}

/// SQL 列 ↔ JSON Schema 属性一致性
fn check_field_consistency(graph: &UnifiedSemanticGraph) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for entity in graph.entities() {
        let schema_type = graph
            .edges_from(&entity.uri)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Defines)
            .filter_map(|e| graph.get(&e.to))
            .find(|n| n.kind == NodeKind::Type && n.standard == Standard::JsonSchema);
        let tables: Vec<_> = graph
            .edges_from(&entity.uri)
            .into_iter()
            .filter(|e| e.kind == EdgeKind::Defines)
            .filter_map(|e| graph.get(&e.to))
            .filter(|n| n.kind == NodeKind::Table)
            .collect();
        let (Some(schema_type), Some(table)) = (schema_type, tables.first()) else {
            continue;
        };

        let schema_props: Vec<serde_json::Value> = schema_type
            .data
            .get("properties")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let columns = table
            .data
            .get("columns")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for column in &columns {
            let cname = column.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let line = column.get("line").and_then(|v| v.as_u64()).map(|v| v as usize);
            let not_null = column.get("notNull").and_then(|v| v.as_bool()).unwrap_or(false);
            let primary = column.get("primaryKey").and_then(|v| v.as_bool()).unwrap_or(false);
            let sql_type = column.get("type").and_then(|v| v.as_str()).unwrap_or_default();

            let prop = schema_props
                .iter()
                .find(|p| crate::identity::normalize_name(p.get("name").and_then(|v| v.as_str()).unwrap_or_default())
                    == crate::identity::normalize_name(cname));

            let Some(prop) = prop else {
                out.push(Diagnostic {
                    severity: "warning".into(),
                    code: "field-schema-missing".into(),
                    message: format!("列 {cname} 在 JSON Schema \"{}\" 中没有对应属性", schema_type.name),
                    file: Some(table.source.file.clone()),
                    line,
                    nodes: Some(vec![entity.uri.clone()]),
                });
                continue;
            };

            let pname = prop.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let prop_required = prop.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
            let prop_type = prop.get("type").and_then(|v| v.as_str()).unwrap_or_default();

            // 命名风格
            if cname != crate::identity::to_snake_case(pname) {
                out.push(Diagnostic {
                    severity: "info".into(),
                    code: "field-name-style".into(),
                    message: format!("列 \"{cname}\" 与属性 \"{pname}\" 命名风格不同（建议 {}）", crate::identity::to_snake_case(pname)),
                    file: Some(table.source.file.clone()),
                    line,
                    nodes: Some(vec![entity.uri.clone()]),
                });
            }
            // 必填一致性：NOT NULL ↔ required
            if not_null && !primary && !prop_required {
                out.push(Diagnostic {
                    severity: "warning".into(),
                    code: "field-required-mismatch".into(),
                    message: format!("{cname} 是 NOT NULL，但属性 \"{pname}\" 不在 required 中"),
                    file: Some(table.source.file.clone()),
                    line,
                    nodes: Some(vec![entity.uri.clone()]),
                });
            }
            // 类型兼容性
            if !crate::adapters::sql::sql_type_compatible(sql_type, prop_type) {
                out.push(Diagnostic {
                    severity: "error".into(),
                    code: "field-type-mismatch".into(),
                    message: format!("{cname} 类型 {sql_type} 与 JSON Schema 属性 \"{pname}\" 类型 {prop_type} 不兼容"),
                    file: Some(table.source.file.clone()),
                    line,
                    nodes: Some(vec![entity.uri.clone()]),
                });
            }
        }

        // 反向：schema 属性缺列
        let col_names: Vec<String> = columns
            .iter()
            .map(|c| crate::identity::normalize_name(c.get("name").and_then(|v| v.as_str()).unwrap_or_default()))
            .collect();
        for prop in &schema_props {
            let pname = prop.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if !col_names.contains(&crate::identity::normalize_name(pname)) {
                out.push(Diagnostic {
                    severity: "warning".into(),
                    code: "field-column-missing".into(),
                    message: format!("JSON Schema 属性 \"{pname}\" 在表 {} 中没有对应列", table.name),
                    file: Some(schema_type.source.file.clone()),
                    line: None,
                    nodes: Some(vec![entity.uri.clone()]),
                });
            }
        }
    }
    out
}

/// JSON Forms 绑定的 schema 必须存在于项目中
fn check_form_bindings(graph: &UnifiedSemanticGraph) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let known: Vec<String> = graph
        .nodes_by_kind(NodeKind::Type)
        .into_iter()
        .filter_map(|n| n.data.get("$id").and_then(|v| v.as_str()).map(crate::identity::normalize_name))
        .collect();
    for form in graph.nodes_by_kind(NodeKind::UiForm) {
        let bound = form.data.get("boundSchemaId").and_then(|v| v.as_str()).unwrap_or_default();
        if !bound.is_empty() && !known.contains(&crate::identity::normalize_name(bound)) {
            out.push(Diagnostic {
                severity: "error".into(),
                code: "form-binding-missing".into(),
                message: format!("表单 \"{}\" 绑定的 JSON Schema \"{bound}\" 不存在于项目规格中", form.name),
                file: Some(form.source.file.clone()),
                line: None,
                nodes: Some(vec![form.uri.clone()]),
            });
        }
    }
    out
}

/// BPMN 条件/变量对实体字段的软引用提示
fn check_workflow_references(graph: &UnifiedSemanticGraph) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let field_names: Vec<String> = graph
        .nodes_by_kind(NodeKind::EntityField)
        .into_iter()
        .map(|f| crate::identity::normalize_name(f.name.rsplit('.').next().unwrap_or("")))
        .collect();
    for wf in graph.nodes_by_kind(NodeKind::Workflow) {
        let steps = wf.data.get("steps").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for step in &steps {
            let condition = step.get("condition").and_then(|v| v.as_str()).unwrap_or_default();
            if condition.is_empty() {
                continue;
            }
            let mentioned: Vec<&String> = field_names.iter().filter(|f| condition.contains(f.as_str())).collect();
            if !mentioned.is_empty() {
                let list = mentioned.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                out.push(Diagnostic {
                    severity: "info".into(),
                    code: "workflow-reference-soft".into(),
                    message: format!("BPMN 条件引用了实体字段（{list}）— 建议通过 bindings 建立正式关联"),
                    file: Some(wf.source.file.clone()),
                    line: None,
                    nodes: Some(vec![wf.uri.clone()]),
                });
            }
        }
    }
    out
}
