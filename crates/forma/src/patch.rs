//! Semantic Patch — AI 与人修改软件的唯一语义入口。
//! patch → 影响闭包 → 跨标准编辑计划 → 原位修改规格文件 → 重校验 → 增量编译。

use crate::engine::{build_project, BuildOptions};
use crate::graph::{EdgeKind, NodeKind, Standard};
use crate::validate::Diagnostic;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Patch 文档模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PatchFieldSpec {
    pub name: String,
    pub r#type: String, // string | integer | number | boolean
    #[serde(default)]
    pub r#enum: Vec<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub sql: Option<SqlFieldSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SqlFieldSpec {
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum PatchOperation {
    AddEntityField {
        entity: String,
        field: PatchFieldSpec,
    },
    RenameEntityField {
        entity: String,
        from: String,
        to: String,
    },
    SetEntityFieldRequired {
        entity: String,
        field: String,
        required: bool,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchDocument {
    pub operations: Vec<PatchOperation>,
}

// ---------------------------------------------------------------------------
// 编辑指令（类型化，非字符串解析）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum EditKind {
    AddColumn {
        table: String,
        column: String,
        sql_type: String,
        not_null: bool,
    },
    AddJsonProperty {
        field: PatchFieldSpec,
    },
    AddOpenApiProperty {
        schema: String,
        field: PatchFieldSpec,
    },
    AddFormControl {
        field: String,
    },
    RenameColumn {
        from: String,
        to: String,
    },
    RenameProperty {
        from: String,
        to: String,
    },
    RenameControl {
        from: String,
        to: String,
    },
    ToggleNotNull {
        table: String,
        column: String,
        not_null: bool,
    },
    ToggleRequired {
        field: String,
        required: bool,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlannedEdit {
    pub file: String,
    pub standard: String,
    pub action: String,
    pub detail: String,
    #[serde(skip)]
    pub kind: EditKind,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OperationReport {
    pub op: String,
    pub entity: String,
    pub status: String, // applied | not_implemented | failed
    pub edits: Vec<PlannedEdit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct PatchReport {
    pub operations: Vec<OperationReport>,
    pub diagnostics: Vec<Diagnostic>,
    pub build: Option<BuildSummary>,
}

#[derive(Debug, serde::Serialize)]
pub struct BuildSummary {
    pub files: Vec<String>,
    pub skipped: usize,
}

// ---------------------------------------------------------------------------
// 执行器（事务式）
// ---------------------------------------------------------------------------

/// 补丁入口分发（RFC-0006 P2）：DSL 工程 → `dsl::patch`（加字段 / 加规则 / 加页面），
/// 联邦工程 → 适配器规格的原位编辑。两条路径共享「快照 → 编辑 → 重校验 → 重建」的事务形状。
///
/// 调用方（CLI `forma patch` / MCP `forma_patch`）按工程类型解析对应的补丁文档后进入本函数。
pub enum PatchInput {
    /// 联邦补丁文档（规格导入侧）
    Federal(PatchDocument),
    /// DSL 主线补丁文档（`dsl::patch` 操作集）
    Dsl(crate::dsl::DslPatchDocument),
}

pub fn apply_patch_input(project_dir: &Path, input: PatchInput) -> Result<serde_json::Value> {
    match input {
        PatchInput::Federal(patch) => {
            let report = apply_patch(project_dir, &patch)?;
            Ok(serde_json::to_value(&report)?)
        }
        PatchInput::Dsl(patch) => {
            let report = crate::dsl::patch::apply_dsl_patch(project_dir, &patch)?;
            Ok(serde_json::to_value(&report)?)
        }
    }
}

pub fn apply_patch(project_dir: &Path, patch: &PatchDocument) -> Result<PatchReport> {
    let load = crate::engine::load_project(project_dir)?;
    let mut report = PatchReport {
        operations: Vec::new(),
        diagnostics: Vec::new(),
        build: None,
    };
    let mut snapshots: HashMap<PathBuf, String> = HashMap::new();
    let rollback = |snapshots: &HashMap<PathBuf, String>| -> Result<()> {
        for (abs, content) in snapshots {
            crate::util::write_text(abs, content)?;
        }
        Ok(())
    };

    for operation in &patch.operations {
        let (op_name, entity_name) = match operation {
            PatchOperation::AddEntityField { entity, .. } => ("add-entity-field", entity),
            PatchOperation::RenameEntityField { entity, .. } => ("rename-entity-field", entity),
            PatchOperation::SetEntityFieldRequired { entity, .. } => {
                ("set-entity-field-required", entity)
            }
        };

        let Some(entity_uri) = load
            .graph
            .entities()
            .into_iter()
            .find(|e| e.name.eq_ignore_ascii_case(entity_name))
            .map(|e| e.uri.clone())
        else {
            report.operations.push(OperationReport {
                op: op_name.into(),
                entity: entity_name.clone(),
                status: "failed".into(),
                edits: vec![],
                message: Some(format!(
                    "entity \"{entity_name}\" not found in project specifications"
                )),
            });
            continue;
        };

        let edits: Vec<PlannedEdit> = match operation {
            PatchOperation::AddEntityField { field, .. } => {
                plan_add_field(&load, &entity_uri, field)
            }
            PatchOperation::RenameEntityField { from, to, .. } => {
                plan_rename_field(&load, &entity_uri, from, to)
            }
            PatchOperation::SetEntityFieldRequired {
                field, required, ..
            } => plan_set_required(&load, &entity_uri, field, *required),
        };

        if edits.is_empty() {
            report.operations.push(OperationReport {
                op: op_name.into(),
                entity: entity_name.clone(),
                status: "not_implemented".into(),
                edits: vec![],
                message: Some(
                    "operation recognized but no edit plan produced (unsupported combination)"
                        .into(),
                ),
            });
            continue;
        }

        let mut summaries: Vec<PlannedEdit> = Vec::new();
        let mut failure: Option<String> = None;

        for edit in &edits {
            let abs = load.loaded.dir.join(&edit.file);
            if !snapshots.contains_key(&abs) {
                let before = if abs.is_file() {
                    crate::util::read_text(&abs)?
                } else {
                    String::new()
                };
                snapshots.insert(abs.clone(), before);
            }
            let before = snapshots.get(&abs).unwrap().clone();

            match apply_edit(&before, edit) {
                Ok(after) => {
                    if after == before {
                        failure = Some(format!("edit produced no change in {}", edit.file));
                        break;
                    }
                    crate::util::write_text(&abs, &after)?;
                    summaries.push(edit.clone());
                }
                Err(err) => {
                    failure = Some(format!("edit failed on {}: {err}", edit.file));
                    break;
                }
            }
        }

        if let Some(msg) = failure {
            rollback(&snapshots)?;
            report.operations.push(OperationReport {
                op: op_name.into(),
                entity: entity_name.clone(),
                status: "failed".into(),
                edits: summaries,
                message: Some(msg),
            });
            report.diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "patch-rollback".into(),
                message: "patch failed and rolled back to original state".into(),
                file: None,
                line: None,
                nodes: None,
            });
            return Ok(report);
        }

        report.operations.push(OperationReport {
            op: op_name.into(),
            entity: entity_name.clone(),
            status: "applied".into(),
            edits: summaries,
            message: None,
        });
    }

    // 重校验 + 增量编译受影响目标
    if !report.operations.is_empty() && report.operations.iter().all(|o| o.status != "failed") {
        let affected: Vec<String> = report
            .operations
            .iter()
            .map(|o| o.entity.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let build = build_project(&BuildOptions {
            project_dir: project_dir.to_path_buf(),
            force: true,
            entities: Some(affected),
            dry_run: false,
            ownership: crate::build::OwnershipPolicy::default(),
        })?;
        report.build = Some(BuildSummary {
            files: build.files.iter().map(|f| f.path.clone()).collect(),
            skipped: build.skipped,
        });
        report.diagnostics = build.diagnostics;
    }

    Ok(report)
}

fn apply_edit(src: &str, edit: &PlannedEdit) -> Result<String> {
    match &edit.kind {
        EditKind::AddColumn {
            table,
            column,
            sql_type,
            not_null,
        } => insert_column_into_create_table(src, table, column, sql_type, *not_null),
        EditKind::AddJsonProperty { field } => add_property_to_json_schema(src, field),
        EditKind::AddOpenApiProperty { schema, field } => {
            add_property_to_openapi_schema(src, schema, field)
        }
        EditKind::AddFormControl { field } => add_control_to_json_forms(src, field),
        EditKind::RenameColumn { from, to } => replace_word(src, &to_snake(from), &to_snake(to)),
        EditKind::RenameProperty { from, to } => rename_json_property(src, from, to),
        EditKind::RenameControl { from, to } => {
            let old = format!("#/properties/{from}");
            let new = format!("#/properties/{to}");
            if !src.contains(&old) {
                bail!("scope \"{old}\" not found");
            }
            Ok(src.replace(&old, &new))
        }
        EditKind::ToggleNotNull {
            table,
            column,
            not_null,
        } => toggle_not_null(src, table, column, *not_null),
        EditKind::ToggleRequired { field, required } => toggle_required(src, field, *required),
    }
}

// ---------------------------------------------------------------------------
// 编辑计划
// ---------------------------------------------------------------------------

fn find_entity_table(
    load: &crate::engine::LoadOutcome,
    entity_uri: &str,
) -> Option<crate::graph::SemanticNode> {
    load.graph
        .edges_from(entity_uri)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Defines)
        .filter_map(|e| load.graph.get(&e.to))
        .find(|n| n.kind == NodeKind::Table)
        .cloned()
}

fn find_entity_schema_type(
    load: &crate::engine::LoadOutcome,
    entity_uri: &str,
    standard: Standard,
) -> Option<crate::graph::SemanticNode> {
    load.graph
        .edges_from(entity_uri)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Defines)
        .filter_map(|e| load.graph.get(&e.to))
        .find(|n| n.kind == NodeKind::Type && n.standard == standard)
        .cloned()
}

fn find_entity_form(
    load: &crate::engine::LoadOutcome,
    entity_uri: &str,
) -> Option<crate::graph::SemanticNode> {
    load.graph
        .nodes_by_kind(NodeKind::UiForm)
        .into_iter()
        .find(|f| {
            load.graph
                .edges_to(&f.uri)
                .iter()
                .any(|e| e.kind == EdgeKind::References && e.from == entity_uri)
        })
        .cloned()
}

fn plan_add_field(
    load: &crate::engine::LoadOutcome,
    entity_uri: &str,
    field: &PatchFieldSpec,
) -> Vec<PlannedEdit> {
    let mut edits = Vec::new();
    let sql_type_default = match field.r#type.as_str() {
        "integer" => "INTEGER",
        "number" => "REAL",
        "boolean" => "INTEGER",
        _ => "TEXT",
    };

    if let Some(table) = find_entity_table(load, entity_uri) {
        let column = field
            .sql
            .as_ref()
            .and_then(|s| s.column.clone())
            .unwrap_or_else(|| to_snake(&field.name));
        let sql_type = field
            .sql
            .as_ref()
            .and_then(|s| s.r#type.clone())
            .unwrap_or_else(|| sql_type_default.to_string());
        edits.push(PlannedEdit {
            file: table.source.file.clone(),
            standard: "sql".into(),
            action: "add-column".into(),
            detail: format!(
                "{}.{} {}{}",
                table.name,
                column,
                sql_type,
                if field.required { " NOT NULL" } else { "" }
            ),
            kind: EditKind::AddColumn {
                table: table.name.clone(),
                column,
                sql_type,
                not_null: field.required,
            },
        });
    }

    if let Some(schema_type) = find_entity_schema_type(load, entity_uri, Standard::JsonSchema) {
        edits.push(PlannedEdit {
            file: schema_type.source.file.clone(),
            standard: "json-schema".into(),
            action: "add-property".into(),
            detail: format!(
                "{}: {}{}",
                field.name,
                field.r#type,
                if field.r#enum.is_empty() {
                    String::new()
                } else {
                    format!(" enum[{}]", field.r#enum.len())
                }
            ),
            kind: EditKind::AddJsonProperty {
                field: field.clone(),
            },
        });
    }

    if let Some(api_type) = find_entity_schema_type(load, entity_uri, Standard::Openapi) {
        edits.push(PlannedEdit {
            file: api_type.source.file.clone(),
            standard: "openapi".into(),
            action: "add-schema-property".into(),
            detail: format!("components.schemas.{}.{}", api_type.name, field.name),
            kind: EditKind::AddOpenApiProperty {
                schema: api_type.name.clone(),
                field: field.clone(),
            },
        });
    }

    if let Some(form) = find_entity_form(load, entity_uri) {
        edits.push(PlannedEdit {
            file: form.source.file.clone(),
            standard: "json-forms".into(),
            action: "add-control".into(),
            detail: format!("control #/properties/{}", field.name),
            kind: EditKind::AddFormControl {
                field: field.name.clone(),
            },
        });
    }

    edits
}

fn plan_rename_field(
    load: &crate::engine::LoadOutcome,
    entity_uri: &str,
    from: &str,
    to: &str,
) -> Vec<PlannedEdit> {
    let mut edits = Vec::new();
    if let Some(table) = find_entity_table(load, entity_uri) {
        edits.push(PlannedEdit {
            file: table.source.file.clone(),
            standard: "sql".into(),
            action: "rename-column".into(),
            detail: format!("{}: {} → {}", table.name, to_snake(from), to_snake(to)),
            kind: EditKind::RenameColumn {
                from: from.into(),
                to: to.into(),
            },
        });
    }
    if let Some(schema_type) = find_entity_schema_type(load, entity_uri, Standard::JsonSchema) {
        edits.push(PlannedEdit {
            file: schema_type.source.file.clone(),
            standard: "json-schema".into(),
            action: "rename-property".into(),
            detail: format!("{}: {from} → {to}", schema_type.name),
            kind: EditKind::RenameProperty {
                from: from.into(),
                to: to.into(),
            },
        });
    }
    if let Some(api_type) = find_entity_schema_type(load, entity_uri, Standard::Openapi) {
        edits.push(PlannedEdit {
            file: api_type.source.file.clone(),
            standard: "openapi".into(),
            action: "rename-schema-property".into(),
            detail: format!("components.schemas.{}: {from} → {to}", api_type.name),
            kind: EditKind::RenameProperty {
                from: from.into(),
                to: to.into(),
            },
        });
    }
    if let Some(form) = find_entity_form(load, entity_uri) {
        edits.push(PlannedEdit {
            file: form.source.file.clone(),
            standard: "json-forms".into(),
            action: "rename-control".into(),
            detail: format!("{}: #/properties/{from} → #/properties/{to}", form.name),
            kind: EditKind::RenameControl {
                from: from.into(),
                to: to.into(),
            },
        });
    }
    edits
}

fn plan_set_required(
    load: &crate::engine::LoadOutcome,
    entity_uri: &str,
    field: &str,
    required: bool,
) -> Vec<PlannedEdit> {
    let mut edits = Vec::new();
    if let Some(table) = find_entity_table(load, entity_uri) {
        edits.push(PlannedEdit {
            file: table.source.file.clone(),
            standard: "sql".into(),
            action: if required {
                "set-not-null".into()
            } else {
                "drop-not-null".into()
            },
            detail: format!(
                "{}.{} {}",
                table.name,
                to_snake(field),
                if required { "NOT NULL" } else { "nullable" }
            ),
            kind: EditKind::ToggleNotNull {
                table: table.name.clone(),
                column: to_snake(field),
                not_null: required,
            },
        });
    }
    if let Some(schema_type) = find_entity_schema_type(load, entity_uri, Standard::JsonSchema) {
        edits.push(PlannedEdit {
            file: schema_type.source.file.clone(),
            standard: "json-schema".into(),
            action: if required {
                "add-to-required".into()
            } else {
                "remove-from-required".into()
            },
            detail: format!(
                "{}.required {} {}",
                schema_type.name,
                if required { "+=" } else { "-=" },
                field
            ),
            kind: EditKind::ToggleRequired {
                field: field.into(),
                required,
            },
        });
    }
    edits
}

// ---------------------------------------------------------------------------
// 文本编辑原语（确定性、原位修改）
// ---------------------------------------------------------------------------

fn find_create_table_start(sql: &str, table: &str) -> Option<usize> {
    let lower = sql.to_lowercase();
    let mut search = 0usize;
    loop {
        let idx = lower[search..].find("create table")? + search;
        let mut p = idx + "create table".len();
        while p < sql.len() && sql[p..].starts_with(char::is_whitespace) {
            p += 1;
        }
        if sql[p..].to_lowercase().starts_with("if not exists") {
            p += "if not exists".len();
            while p < sql.len() && sql[p..].starts_with(char::is_whitespace) {
                p += 1;
            }
        }
        let rest = &sql[p..];
        let token: String = rest
            .chars()
            .take_while(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '"' | '`' | '[' | ']' | '.')
            })
            .collect();
        let bare = token.replace(['"', '`', '[', ']'], "");
        let bare_last = bare.rsplit('.').next().unwrap_or("").to_string();
        if bare_last.eq_ignore_ascii_case(table) {
            return Some(idx);
        }
        search = idx + 12;
    }
}

fn matching_paren(sql: &str, open: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn matching_bracket(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn insert_column_into_create_table(
    sql: &str,
    table: &str,
    column: &str,
    sql_type: &str,
    not_null: bool,
) -> Result<String> {
    let create = find_create_table_start(sql, table)
        .ok_or_else(|| anyhow::anyhow!("CREATE TABLE {table} not found"))?;
    let open = sql[create..]
        .find('(')
        .with_context(|| format!("malformed CREATE TABLE {table}"))?
        + create;
    let close = matching_paren(sql, open)
        .ok_or_else(|| anyhow::anyhow!("unbalanced parens in CREATE TABLE {table}"))?;

    let mut insert_at = close;
    while insert_at > open + 1 && sql[..insert_at].ends_with(char::is_whitespace) {
        insert_at -= 1;
    }
    let prev_char = sql[..insert_at].chars().last().unwrap_or('(');
    let needs_comma = prev_char != ',' && prev_char != '(';
    let col_def = format!(
        "  {column} {sql_type}{}",
        if not_null { " NOT NULL" } else { "" }
    );
    let tail_inside = &sql[insert_at..close];

    let mut out = String::with_capacity(sql.len() + col_def.len());
    out.push_str(&sql[..insert_at]);
    out.push_str(if needs_comma { ",\n" } else { "\n" });
    out.push_str(&col_def);
    out.push_str(if tail_inside.contains('\n') { "" } else { "\n" });
    out.push_str(tail_inside);
    out.push_str(&sql[close..]);
    Ok(out)
}

fn add_property_to_json_schema(src: &str, field: &PatchFieldSpec) -> Result<String> {
    // 属性片段：各属性行不带尾逗号，join 时统一加
    let mut attrs: Vec<String> = vec![format!("        \"type\": \"{}\"", field.r#type)];
    if !field.r#enum.is_empty() {
        let vals: Vec<String> = field
            .r#enum
            .iter()
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .collect();
        attrs.push(format!("        \"enum\": [{}]", vals.join(", ")));
    }
    if let Some(f) = &field.format {
        attrs.push(format!("        \"format\": \"{f}\""));
    }
    if let Some(d) = &field.description {
        attrs.push(format!(
            "        \"description\": {}",
            serde_json::to_string(d)?
        ));
    }
    let prop_text = format!(
        "      \"{}\": {{\n{}\n      }}",
        field.name,
        attrs.join(",\n")
    );

    let props_idx = src
        .find("\"properties\"")
        .ok_or_else(|| anyhow::anyhow!("\"properties\" not found"))?;
    let open = src[props_idx..].find('{').context("malformed properties")? + props_idx;
    let close = matching_brace(src, open)
        .ok_or_else(|| anyhow::anyhow!("unbalanced braces in properties"))?;

    let mut insert_at = close;
    while insert_at > open + 1 && src[..insert_at].ends_with(char::is_whitespace) {
        insert_at -= 1;
    }
    let prev_char = src[..insert_at].chars().last().unwrap_or('{');
    let prefix = if prev_char == '{' { "\n" } else { ",\n" };
    // 保留原 properties 收尾空白，保证缩进正确
    let tail = &src[insert_at..close];

    Ok(format!(
        "{}{}{}{}{}",
        &src[..insert_at],
        prefix,
        prop_text,
        tail,
        &src[close..]
    ))
}

fn add_property_to_openapi_schema(
    src: &str,
    schema: &str,
    field: &PatchFieldSpec,
) -> Result<String> {
    let lines: Vec<&str> = src.lines().collect();
    // 定位 components.schemas 下的 4 空格缩进 schema 键
    let mut in_schemas = false;
    let mut schema_line = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim_end() == "  schemas:" {
            in_schemas = true;
            continue;
        }
        if in_schemas {
            if !line.starts_with("    ") && !line.trim().is_empty() {
                in_schemas = false;
                continue;
            }
            if line.starts_with(&format!("    {schema}:")) {
                schema_line = Some(i);
                break;
            }
        }
    }
    let schema_line =
        schema_line.ok_or_else(|| anyhow::anyhow!("schema {schema} not found in openapi yaml"))?;

    // 在该 schema 块内找 6 空格缩进的 properties:
    let mut props_line = None;
    for (i, l) in lines.iter().enumerate().skip(schema_line + 1) {
        if l.trim_end() == "      properties:" {
            props_line = Some(i);
            break;
        }
        if !l.trim().is_empty() && l.starts_with("    ") && !l.starts_with("      ") {
            break; // 下一个 schema
        }
    }
    let props_line =
        props_line.ok_or_else(|| anyhow::anyhow!("properties not found under schema {schema}"))?;

    // properties 块结束 = 下一个缩进 ≤ 6 的非空行
    let mut end_line = lines.len();
    for (i, l) in lines.iter().enumerate().skip(props_line + 1) {
        if l.trim().is_empty() {
            continue;
        }
        let indent = l.len() - l.trim_start().len();
        if indent <= 6 {
            end_line = i;
            break;
        }
    }

    let mut out: Vec<String> = lines[..end_line].iter().map(|s| s.to_string()).collect();
    let fi = "        ";
    out.push(format!("{}{}:", fi, field.name));
    out.push(format!("{}  type: {}", fi, field.r#type));
    if !field.r#enum.is_empty() {
        out.push(format!("{fi}  enum:"));
        for v in &field.r#enum {
            out.push(format!("{fi}    - {v}"));
        }
    }
    out.extend(lines[end_line..].iter().map(|s| s.to_string()));
    let mut joined = out.join("\n");
    if src.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    Ok(joined)
}

fn add_control_to_json_forms(src: &str, field_name: &str) -> Result<String> {
    let elements_idx = src
        .find("\"elements\"")
        .ok_or_else(|| anyhow::anyhow!("\"elements\" not found"))?;
    let open = src[elements_idx..]
        .find('[')
        .context("malformed elements")?
        + elements_idx;
    let close = matching_bracket(src, open)
        .ok_or_else(|| anyhow::anyhow!("unbalanced brackets in elements"))?;

    let mut insert_at = close;
    while insert_at > open + 1 && src[..insert_at].ends_with(char::is_whitespace) {
        insert_at -= 1;
    }
    let prev_char = src[..insert_at].chars().last().unwrap_or('[');
    let needs_comma = prev_char != '[' && prev_char != ',';
    let control = format!(
        "      {{\n        \"type\": \"Control\",\n        \"scope\": \"#/properties/{field_name}\"\n      }}"
    );
    let tail = &src[insert_at..close];
    Ok(format!(
        "{}{}{}{}{}",
        &src[..insert_at],
        if needs_comma { ",\n" } else { "" },
        control,
        tail,
        &src[close..]
    ))
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn replace_word(src: &str, from: &str, to: &str) -> Result<String> {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut found = false;
    while i < src.len() {
        if src[i..].starts_with(from) {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after = i + from.len();
            let after_ok = after >= src.len() || !is_ident_char(bytes[after]);
            if before_ok && after_ok {
                out.push_str(to);
                i = after;
                found = true;
                continue;
            }
        }
        let ch_len = src[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&src[i..i + ch_len]);
        i += ch_len;
    }
    if !found {
        bail!("pattern \"{from}\" not found");
    }
    Ok(out)
}

fn rename_json_property(src: &str, from: &str, to: &str) -> Result<String> {
    let mut out = src.to_string();
    let mut renamed = false;
    // JSON 风格
    for pattern in [format!("\"{from}\":"), format!("\"{from}\" :")] {
        let replacement = pattern.replace(from, to);
        if out.contains(&pattern) {
            out = out.replace(&pattern, &replacement);
            renamed = true;
        }
    }
    // YAML 风格（行首缩进 + from:）
    let mut yaml_lines: Vec<String> = Vec::new();
    let mut yaml_hit = false;
    for line in out.lines() {
        let t = line.trim_start();
        if t == format!("{from}:") || t.starts_with(&format!("{from}: ")) {
            let indent = &line[..line.len() - t.len()];
            let rest = t.repl_once_prefix(from);
            yaml_lines.push(format!("{indent}{to}{rest}"));
            yaml_hit = true;
        } else {
            yaml_lines.push(line.to_string());
        }
    }
    if yaml_hit {
        out = yaml_lines.join("\n");
        if !src.ends_with('\n') && out.ends_with('\n') {
            out.pop();
        }
        renamed = true;
    }
    if !renamed {
        bail!("property \"{from}\" not found");
    }
    // required 数组引用
    out = rename_in_required_arrays(&out, from, to);
    Ok(out)
}

trait ReplOncePrefix {
    fn repl_once_prefix(&self, prefix: &str) -> String;
}

impl ReplOncePrefix for str {
    fn repl_once_prefix(&self, prefix: &str) -> String {
        self.strip_prefix(prefix).unwrap_or(self).to_string()
    }
}

fn rename_in_required_arrays(src: &str, from: &str, to: &str) -> String {
    // required 数组有两种排版：单行（"required": ["a", "b"]）与多行（项各占一行）。
    // 旧实现只重写 `"required"` 起始行 → 多行数组里的项被漏掉（字段改名后遗留旧名）。
    let mut out = String::new();
    let mut in_required = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("\"required\"") || t.starts_with("required:") {
            out.push_str(&replace_word_loose(line, from, to));
            in_required = !t.contains(']');
        } else if in_required {
            if t.starts_with(']') {
                in_required = false;
                out.push_str(line);
            } else {
                out.push_str(&replace_word_loose(line, from, to));
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !src.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}

fn replace_word_loose(line: &str, from: &str, to: &str) -> String {
    let mut result = String::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < line.len() {
        if line[i..].starts_with(from) {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_idx = i + from.len();
            let after_ok = after_idx >= line.len() || !is_ident_char(bytes[after_idx]);
            if before_ok && after_ok {
                result.push_str(to);
                i = after_idx;
                continue;
            }
        }
        let ch_len = line[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        result.push_str(&line[i..i + ch_len]);
        i += ch_len;
    }
    result
}

fn toggle_not_null(src: &str, table: &str, column: &str, not_null: bool) -> Result<String> {
    let create = find_create_table_start(src, table)
        .ok_or_else(|| anyhow::anyhow!("CREATE TABLE {table} not found"))?;
    let open = src[create..].find('(').context("malformed CREATE TABLE")? + create;
    let close = matching_paren(src, open).context("unbalanced parens")?;
    let block = &src[create..=close];

    let mut new_block = String::new();
    let mut replaced = false;
    for line in block.lines() {
        let t = line.trim_start();
        if t.starts_with(&format!("{column} ")) || t.starts_with(&format!("{column}\t")) {
            let new_line = if not_null {
                let mut nl = line.to_string();
                if !t.to_uppercase().contains("NOT NULL")
                    && !t.to_uppercase().contains("PRIMARY KEY")
                {
                    let trimmed = nl.trim_end();
                    let had_comma = trimmed.ends_with(',');
                    let base = trimmed.trim_end_matches(',');
                    nl = format!("{base} NOT NULL{}", if had_comma { "," } else { "" });
                }
                nl
            } else {
                remove_not_null(line)
            };
            new_block.push_str(&new_line);
            new_block.push('\n');
            replaced = true;
            continue;
        }
        new_block.push_str(line);
        new_block.push('\n');
    }
    if !replaced {
        bail!("column {column} not found in {table}");
    }
    let mut rebuilt = String::from(&src[..create]);
    rebuilt.push_str(new_block.trim_end_matches('\n'));
    rebuilt.push_str(&src[close + 1..]);
    Ok(rebuilt)
}

fn remove_not_null(line: &str) -> String {
    let upper = line.to_uppercase();
    if let Some(pos) = upper.find(" NOT NULL") {
        let mut out = String::from(&line[..pos]);
        out.push_str(&line[pos + " NOT NULL".len()..]);
        out.replace("  ", " ")
    } else if let Some(pos) = upper.find("NOT NULL ") {
        let mut out = String::from(&line[..pos]);
        out.push_str(&line[pos + "NOT NULL ".len()..]);
        out
    } else {
        line.to_string()
    }
}

fn toggle_required(src: &str, field: &str, required: bool) -> Result<String> {
    let Some(req_pos) = src.find("\"required\"") else {
        if required {
            if let Some(t) = src.find("\"type\": \"object\"") {
                let line_end = src[t..].find('\n').map(|p| t + p).unwrap_or(src.len());
                return Ok(format!(
                    "{}\n  \"required\": [\"{}\"],{}",
                    &src[..line_end],
                    field,
                    &src[line_end..]
                ));
            }
            bail!("cannot place required array");
        }
        return Ok(src.to_string());
    };
    let open = src[req_pos..]
        .find('[')
        .ok_or_else(|| anyhow::anyhow!("malformed required array"))?
        + req_pos;
    let close =
        matching_bracket(src, open).ok_or_else(|| anyhow::anyhow!("unbalanced required array"))?;
    let inner = &src[open + 1..close];

    let items: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut new_items: Vec<String> = Vec::new();
    for item in items {
        let bare = item.trim_matches(['"', '\'']).to_string();
        if bare == field {
            if required {
                new_items.push(item);
            }
        } else {
            new_items.push(item);
        }
    }
    if required
        && !new_items
            .iter()
            .any(|i| i.trim_matches(['"', '\'']) == field)
    {
        new_items.push(format!("\"{field}\""));
    }

    Ok(format!(
        "{}{}{}",
        &src[..open + 1],
        new_items.join(", "),
        &src[close..]
    ))
}

fn to_snake(name: &str) -> String {
    crate::identity::to_snake_case(name)
}
