//! Forma 实体身份协议（Entity Identity Protocol）。
//!
//! 跨标准的业务实体拥有稳定的 forma:// URI。

pub fn project_root_uri(project: &str) -> String {
    format!("forma://project/{project}")
}

pub fn entity_uri(project: &str, entity: &str) -> String {
    format!("{}/entity/{entity}", project_root_uri(project))
}

pub fn entity_field_uri(project: &str, entity: &str, field: &str) -> String {
    format!("{}/field/{field}", entity_uri(project, entity))
}

pub fn type_uri(project: &str, schema_id: &str) -> String {
    format!("{}/type/{schema_id}", project_root_uri(project))
}

/// OpenAPI components.schemas 的独立视图 URI（同一实体可有多个标准视图）。
pub fn api_schema_uri(project: &str, schema_name: &str) -> String {
    format!("{}/api-schema/{schema_name}", project_root_uri(project))
}

pub fn api_operation_uri(project: &str, operation_id: &str) -> String {
    format!("{}/api/{operation_id}", project_root_uri(project))
}

pub fn workflow_uri(project: &str, process_id: &str) -> String {
    format!("{}/workflow/{process_id}", project_root_uri(project))
}

pub fn workflow_step_uri(project: &str, process_id: &str, step_id: &str) -> String {
    format!("{}/step/{step_id}", workflow_uri(project, process_id))
}

pub fn ui_form_uri(project: &str, form_id: &str) -> String {
    format!("{}/ui/{form_id}", project_root_uri(project))
}

pub fn ui_control_uri(project: &str, form_id: &str, control_id: &str) -> String {
    format!("{}/control/{control_id}", ui_form_uri(project, form_id))
}

/// 可视化图表（spec:ir §3.1 可视化域 `ChartNode`）。
pub fn chart_uri(project: &str, chart_name: &str) -> String {
    format!("{}/chart/{chart_name}", project_root_uri(project))
}

/// 定时任务（M10/P1：`job` 块；调度目标域）。
pub fn job_uri(project: &str, job_name: &str) -> String {
    format!("{}/job/{job_name}", project_root_uri(project))
}

/// 版本化迁移链（schema 变更的物化载体；spec:compatibility §6.1）。
pub fn migration_uri(project: &str, migration_id: &str) -> String {
    format!("{}/migration/{migration_id}", project_root_uri(project))
}

/// 内部节点 URI（SQL 列、索引等标准局部对象）。
pub fn local_uri(standard: &str, file: &str, kind: &str, name: &str) -> String {
    format!("forma://local/{standard}/{}/{kind}/{name}", sanitize(file))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// 名称归一化：snake/kebab/PascalCase → 小写无分隔（约定匹配用）。
pub fn normalize_name(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && i > 0
            && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit())
        {
            out.push('-');
        }
        if *c == '_' || *c == ' ' || *c == '-' {
            continue;
        }
        out.push(c.to_ascii_lowercase());
    }
    // 与 TS 版一致：移除大小写边界插入的 '-'，结果为纯小写无分隔
    out.chars().filter(|ch| *ch != '-').collect()
}

pub fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && i > 0
            && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit())
        {
            out.push('_');
        }
        if *c == '-' || *c == ' ' {
            out.push('_');
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

pub fn to_camel_case(name: &str) -> String {
    let pascal = to_pascal_case(name);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(c) => c.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

pub fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// 极简英文复数还原（表名 → 实体名约定匹配用）。
pub fn singularize(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with("ies") && lower.len() > 3 {
        return format!("{}y", &name[..name.len() - 3]);
    }
    if ["ses", "xes", "zes", "ches", "shes"]
        .iter()
        .any(|s| lower.ends_with(s))
        && lower.len() > 3
    {
        return name[..name.len() - 2].to_string();
    }
    if lower.ends_with("ss") {
        return name.to_string();
    }
    if lower.ends_with('s') && lower.len() > 1 {
        return name[..name.len() - 1].to_string();
    }
    name.to_string()
}
