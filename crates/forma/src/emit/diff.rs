//! 版本差异比较：两个 Program 的结构化 diff → JSON + 可视化 HTML 报告。

use super::Program;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub summary: DiffSummary,
    pub entities: Vec<EntityDiff>,
    pub enums: Vec<EnumDiff>,
    pub apis: Vec<ApiDiff>,
    pub pages: Vec<PageDiff>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntityDiff {
    pub name: String,
    pub status: String, // added | removed | modified | unchanged
    pub fields_added: Vec<FieldChange>,
    pub fields_removed: Vec<FieldChange>,
    pub fields_modified: Vec<FieldModified>,
    pub indexes_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldChange {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldModified {
    pub name: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumDiff {
    pub name: String,
    pub status: String,
    pub variants_added: Vec<String>,
    pub variants_removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiDiff {
    pub name: String,
    pub status: String,
    pub operations_added: Vec<String>,
    pub operations_removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageDiff {
    pub name: String,
    pub status: String,
    pub changed: bool,
}

pub fn diff_programs(old: &Program, new: &Program) -> DiffReport {
    let mut report = DiffReport {
        summary: Default::default(),
        entities: Vec::new(),
        enums: Vec::new(),
        apis: Vec::new(),
        pages: Vec::new(),
    };

    // 实体
    for new_entity in &new.entities {
        match old.entities.iter().find(|e| e.name == new_entity.name) {
            None => {
                report.entities.push(EntityDiff {
                    name: new_entity.name.clone(),
                    status: "added".into(),
                    fields_added: new_entity
                        .fields
                        .iter()
                        .map(|f| FieldChange {
                            name: f.name.clone(),
                            type_name: f.type_name.clone(),
                        })
                        .collect(),
                    fields_removed: vec![],
                    fields_modified: vec![],
                    indexes_changed: false,
                });
                report.summary.added += 1;
            }
            Some(old_entity) => {
                let mut fields_added = Vec::new();
                let mut fields_removed = Vec::new();
                let mut fields_modified = Vec::new();
                for f in &new_entity.fields {
                    match old_entity.fields.iter().find(|x| x.name == f.name) {
                        None => fields_added.push(FieldChange {
                            name: f.name.clone(),
                            type_name: f.type_name.clone(),
                        }),
                        Some(of) => {
                            let old_sig = field_signature(old_entity, of);
                            let new_sig = field_signature(new_entity, f);
                            if old_sig != new_sig {
                                fields_modified.push(FieldModified {
                                    name: f.name.clone(),
                                    old: old_sig,
                                    new: new_sig,
                                });
                            }
                        }
                    }
                }
                for f in &old_entity.fields {
                    if !new_entity.fields.iter().any(|x| x.name == f.name) {
                        fields_removed.push(FieldChange {
                            name: f.name.clone(),
                            type_name: f.type_name.clone(),
                        });
                    }
                }
                let indexes_changed = old_entity.indexes != new_entity.indexes
                    || old_entity.uniques != new_entity.uniques;
                let status = if fields_added.is_empty()
                    && fields_removed.is_empty()
                    && fields_modified.is_empty()
                    && !indexes_changed
                {
                    "unchanged".to_string()
                } else {
                    "modified".to_string();
                    report.summary.modified += 1;
                    "modified".to_string()
                };
                if status == "modified" {
                    report.entities.push(EntityDiff {
                        name: new_entity.name.clone(),
                        status,
                        fields_added,
                        fields_removed,
                        fields_modified,
                        indexes_changed,
                    });
                }
            }
        }
    }
    for old_entity in &old.entities {
        if !new.entities.iter().any(|e| e.name == old_entity.name) {
            report.entities.push(EntityDiff {
                name: old_entity.name.clone(),
                status: "removed".into(),
                fields_added: vec![],
                fields_removed: old_entity
                    .fields
                    .iter()
                    .map(|f| FieldChange {
                        name: f.name.clone(),
                        type_name: f.type_name.clone(),
                    })
                    .collect(),
                fields_modified: vec![],
                indexes_changed: false,
            });
            report.summary.removed += 1;
        }
    }

    // 枚举
    for new_enum in &new.enums {
        match old.enums.iter().find(|e| e.name == new_enum.name) {
            None => {
                report.enums.push(EnumDiff {
                    name: new_enum.name.clone(),
                    status: "added".into(),
                    variants_added: new_enum.variants.iter().map(|v| v.name.clone()).collect(),
                    variants_removed: vec![],
                });
                report.summary.added += 1;
            }
            Some(old_enum) => {
                let added: Vec<String> = new_enum
                    .variants
                    .iter()
                    .filter(|v| !old_enum.variants.iter().any(|x| x.name == v.name))
                    .map(|v| v.name.clone())
                    .collect();
                let removed: Vec<String> = old_enum
                    .variants
                    .iter()
                    .filter(|v| !new_enum.variants.iter().any(|x| x.name == v.name))
                    .map(|v| v.name.clone())
                    .collect();
                if !added.is_empty() || !removed.is_empty() {
                    report.enums.push(EnumDiff {
                        name: new_enum.name.clone(),
                        status: "modified".into(),
                        variants_added: added,
                        variants_removed: removed,
                    });
                    report.summary.modified += 1;
                }
            }
        }
    }
    for old_enum in &old.enums {
        if !new.enums.iter().any(|e| e.name == old_enum.name) {
            report.enums.push(EnumDiff {
                name: old_enum.name.clone(),
                status: "removed".into(),
                variants_added: vec![],
                variants_removed: old_enum.variants.iter().map(|v| v.name.clone()).collect(),
            });
            report.summary.removed += 1;
        }
    }

    // API
    for new_api in &new.apis {
        match old.apis.iter().find(|a| a.name == new_api.name) {
            None => {
                report.apis.push(ApiDiff {
                    name: new_api.name.clone(),
                    status: "added".into(),
                    operations_added: new_api.operations.clone(),
                    operations_removed: vec![],
                });
                report.summary.added += 1;
            }
            Some(old_api) => {
                let added: Vec<String> = new_api
                    .operations
                    .iter()
                    .filter(|o| !old_api.operations.contains(o))
                    .cloned()
                    .collect();
                let removed: Vec<String> = old_api
                    .operations
                    .iter()
                    .filter(|o| !new_api.operations.contains(o))
                    .cloned()
                    .collect();
                if !added.is_empty() || !removed.is_empty() {
                    report.apis.push(ApiDiff {
                        name: new_api.name.clone(),
                        status: "modified".into(),
                        operations_added: added,
                        operations_removed: removed,
                    });
                    report.summary.modified += 1;
                }
            }
        }
    }
    for old_api in &old.apis {
        if !new.apis.iter().any(|a| a.name == old_api.name) {
            report.apis.push(ApiDiff {
                name: old_api.name.clone(),
                status: "removed".into(),
                operations_added: vec![],
                operations_removed: old_api.operations.clone(),
            });
            report.summary.removed += 1;
        }
    }

    // 页面（按 route + 结构签名）
    for new_page in &new.pages {
        match old.pages.iter().find(|p| p.name == new_page.name) {
            None => {
                report.pages.push(PageDiff {
                    name: new_page.name.clone(),
                    status: "added".into(),
                    changed: false,
                });
                report.summary.added += 1;
            }
            Some(old_page) => {
                if old_page.route != new_page.route {
                    report.pages.push(PageDiff {
                        name: new_page.name.clone(),
                        status: "modified".into(),
                        changed: true,
                    });
                    report.summary.modified += 1;
                }
            }
        }
    }
    for old_page in &old.pages {
        if !new.pages.iter().any(|p| p.name == old_page.name) {
            report.pages.push(PageDiff {
                name: old_page.name.clone(),
                status: "removed".into(),
                changed: false,
            });
            report.summary.removed += 1;
        }
    }

    report.summary.unchanged =
        report.summary.added == 0 && report.summary.removed == 0 && report.summary.modified == 0;
    report
}

fn field_signature(entity: &super::EntityDecl, field: &super::FieldDecl) -> String {
    let mut attrs: Vec<String> = field.attrs.iter().map(|a| a.name.clone()).collect();
    attrs.sort();
    format!(
        "{}{:?}{}{:?}{}",
        field.type_name,
        field.type_args,
        if field.optional { "?" } else { "" },
        attrs,
        if entity.uniques.iter().any(|u| u.contains(&field.name)) {
            " @uq"
        } else {
            ""
        }
    )
}

/// 自包含 HTML 差异报告（可直接浏览器打开）。
pub fn render_html(report: &DiffReport, old_label: &str, new_label: &str) -> String {
    let mut entities_html = String::new();
    for e in &report.entities {
        let (badge, color) = match e.status.as_str() {
            "added" => ("新增", "#dcfce7"),
            "removed" => ("删除", "#fee2e2"),
            _ => ("修改", "#fef9c3"),
        };
        let mut rows = String::new();
        for f in &e.fields_added {
            rows.push_str(&format!(
                r#"<tr class="add"><td>+ {}</td><td>{}</td><td></td></tr>"#,
                escape(&f.name),
                escape(&f.type_name)
            ));
        }
        for f in &e.fields_removed {
            rows.push_str(&format!(
                r#"<tr class="del"><td>- {}</td><td>{}</td><td></td></tr>"#,
                escape(&f.name),
                escape(&f.type_name)
            ));
        }
        for m in &e.fields_modified {
            rows.push_str(&format!(
                r#"<tr class="mod"><td>~ {}</td><td>{}</td><td><code>{}</code> → <code>{}</code></td></tr>"#,
                escape(&m.name), escape(&m.name), escape(&m.old), escape(&m.new)
            ));
        }
        if e.indexes_changed {
            rows.push_str(
                r#"<tr class="mod"><td>~ indexes</td><td></td><td>索引/唯一约束有变化</td></tr>"#,
            );
        }
        entities_html.push_str(&format!(
            r#"<div class="entity">
  <div class="entity-head"><span class="pill" style="background:{color}">{badge}</span> <b>{}</b></div>
  <table><tr><th>字段</th><th>类型</th><th>变更</th></tr>{rows}</table>
</div>"#,
            escape(&e.name)
        ));
    }

    let mut enums_html = String::new();
    for e in &report.enums {
        let (badge, color) = match e.status.as_str() {
            "added" => ("新增", "#dcfce7"),
            "removed" => ("删除", "#fee2e2"),
            _ => ("修改", "#fef9c3"),
        };
        let mut detail = String::new();
        for v in &e.variants_added {
            detail.push_str(&format!(
                r#"<span class="pill" style="background:#dcfce7">+ {}</span> "#,
                escape(v)
            ));
        }
        for v in &e.variants_removed {
            detail.push_str(&format!(
                r#"<span class="pill" style="background:#fee2e2">- {}</span> "#,
                escape(v)
            ));
        }
        enums_html.push_str(&format!(
            r#"<div class="entity"><div class="entity-head"><span class="pill" style="background:{color}">{badge}</span> <b>{}</b> {}</div></div>"#,
            escape(&e.name), detail
        ));
    }

    let mut apis_html = String::new();
    for a in &report.apis {
        let (badge, color) = match a.status.as_str() {
            "added" => ("新增", "#dcfce7"),
            "removed" => ("删除", "#fee2e2"),
            _ => ("修改", "#fef9c3"),
        };
        let mut detail = String::new();
        for o in &a.operations_added {
            detail.push_str(&format!(
                r#"<span class="pill" style="background:#dcfce7">+ {}</span> "#,
                escape(o)
            ));
        }
        for o in &a.operations_removed {
            detail.push_str(&format!(
                r#"<span class="pill" style="background:#fee2e2">- {}</span> "#,
                escape(o)
            ));
        }
        apis_html.push_str(&format!(
            r#"<div class="entity"><div class="entity-head"><span class="pill" style="background:{color}">{badge}</span> <b>{}</b> {}</div></div>"#,
            escape(&a.name), detail
        ));
    }

    let mut pages_html = String::new();
    for p in &report.pages {
        let (badge, color) = match p.status.as_str() {
            "added" => ("新增", "#dcfce7"),
            "removed" => ("删除", "#fee2e2"),
            _ => ("修改", "#fef9c3"),
        };
        pages_html.push_str(&format!(
            r#"<div class="entity"><div class="entity-head"><span class="pill" style="background:{color}">{badge}</span> <b>{}</b></div></div>"#,
            escape(&p.name)
        ));
    }

    let summary = if report.summary.unchanged {
        "两个版本无差异".to_string()
    } else {
        format!(
            "新增 {} · 删除 {} · 修改 {}",
            report.summary.added, report.summary.removed, report.summary.modified
        )
    };

    format!(
        r#"<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8"/>
<title>Forma DSL 版本差异 · {old} → {new}</title>
<style>
body {{ font-family: ui-sans-serif, system-ui, "PingFang SC", "Microsoft YaHei", sans-serif; background:#f8fafc; color:#0f172a; margin:0; padding:32px; }}
h1 {{ font-size:22px; }}
.summary {{ display:inline-block; background:#e2e8f0; padding:6px 14px; border-radius:999px; margin-bottom:24px; }}
.entity {{ background:#fff; border:1px solid #e2e8f0; border-radius:10px; padding:14px 16px; margin-bottom:14px; }}
.entity-head {{ margin-bottom:8px; }}
.pill {{ padding:2px 10px; border-radius:999px; font-size:12px; font-weight:600; }}
table {{ width:100%; border-collapse:collapse; font-size:13px; }}
th, td {{ text-align:left; padding:6px 10px; border-top:1px solid #f1f5f9; }}
tr.add {{ background:#f0fdf4; }}
tr.del {{ background:#fef2f2; }}
tr.mod {{ background:#fefce8; }}
code {{ background:#f1f5f9; padding:1px 6px; border-radius:4px; }}
h2 {{ font-size:16px; margin:28px 0 12px; }}
.muted {{ color:#64748b; font-size:13px; }}
</style>
</head>
<body>
<h1>Forma DSL 版本差异</h1>
<div class="muted">{old} → {new} · 由 Forma (gen-diff) 生成</div>
<p><span class="summary">{summary}</span></p>
<h2>实体（{ent_count}）</h2>
{entities_html}
<h2>枚举</h2>
{enums_html}
<h2>API</h2>
{apis_html}
<h2>页面</h2>
{pages_html}
</body>
</html>
"#,
        old = escape(old_label),
        new = escape(new_label),
        ent_count = report.entities.len(),
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// CLI 入口：比较两个项目目录的 DSL。
pub fn emit_diff(
    dir_old: &Path,
    dir_new: &Path,
    out_html: Option<&Path>,
    out_json: Option<&Path>,
) -> Result<DiffReport> {
    let manifest_old = crate::manifest::load_manifest(dir_old)?;
    let manifest_new = crate::manifest::load_manifest(dir_new)?;

    let source_old = manifest_source(&manifest_old)?;
    let source_new = manifest_source(&manifest_new)?;

    let (old, old_diags) = super::load_program(dir_old, &source_old)?;
    let (new, new_diags) = super::load_program(dir_new, &source_new)?;
    let errors: Vec<_> = old_diags
        .iter()
        .chain(new_diags.iter())
        .filter(|d| d.severity == "error")
        .collect();
    if !errors.is_empty() {
        anyhow::bail!("DSL has errors; fix them before diffing");
    }

    let report = diff_programs(&old, &new);

    if let Some(html_path) = out_html {
        let old_label = dir_old
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
            .to_string();
        let new_label = dir_new
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
            .to_string();
        crate::util::write_text(html_path, &render_html(&report, &old_label, &new_label))?;
    }
    if let Some(json_path) = out_json {
        crate::util::write_text(json_path, &serde_json::to_string_pretty(&report)?)?;
    }
    Ok(report)
}

fn manifest_source(loaded: &crate::manifest::LoadedManifest) -> Result<Vec<String>> {
    let source = loaded
        .manifest
        .source
        .clone()
        .ok_or_else(|| anyhow::anyhow!("project has no `source` globs (not a DSL project)"))?;
    Ok(source)
}
