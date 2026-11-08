//! 可视化图表适配层：Program → ER 图 / 数据流图 / 业务流程图（Mermaid）。

use super::Program;
use crate::util::{sha256_hex, write_text};
use anyhow::Result;
use std::path::Path;

fn sanitize_id(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

fn sanitize_type(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_uppercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

// ── ER 图 ───────────────────────────────────────────────

pub fn er_mermaid(program: &Program) -> String {
    let mut out = String::from("erDiagram\n");
    for entity in &program.entities {
        out.push_str(&format!("    {} {{\n", entity.name));
        for field in &entity.fields {
            let type_name = sanitize_type(&field.type_name);
            let name = crate::identity::to_snake_case(&field.name);
            let key = if super::has_attr(field, "id") {
                " PK"
            } else if super::is_relation(program, &field.type_name) {
                " FK"
            } else if super::has_attr(field, "unique") {
                " UK"
            } else {
                ""
            };
            out.push_str(&format!("        {} {}{}\n", type_name, name, key));
        }
        out.push_str("    }\n");
    }
    out.push('\n');
    // 关系线
    let mut seen = std::collections::HashSet::new();
    for entity in &program.entities {
        for field in &entity.fields {
            if !super::is_relation(program, &field.type_name) {
                continue;
            }
            let key = format!("{}->{}", entity.name, field.type_name);
            if seen.insert(key) {
                out.push_str(&format!(
                    "    {} }}o--|| {} : {}\n",
                    entity.name,
                    field.type_name,
                    crate::identity::to_snake_case(&field.name)
                ));
            }
        }
    }
    out
}

// ── 数据流图 ────────────────────────────────────────────

pub fn dataflow_mermaid(program: &Program) -> String {
    let mut out = String::from("flowchart LR\n");
    out.push_str("    USER([\"用户 / 浏览器\"])\n");

    // 规则节点（横切）
    for rules in &program.rules {
        let id = sanitize_id(&format!("R_{}", rules.name));
        out.push_str(&format!("    {id}[\"规则 {}\"]\n", rules.name));
    }

    for page in &program.pages {
        let pid = sanitize_id(&format!("P_{}", page.name));
        let title = super::pick_message(&page.title, &page.name);
        let _route = &page.route;
        out.push_str(&format!(
            "    {pid}[\"页面 {} · {}\"]\n",
            page.name,
            escape(&title)
        ));
        out.push_str(&format!("    USER --> {pid}\n"));

        match &page.body {
            crate::dsl::PageBody::Table(t) => {
                if let Some(api) = program.apis.iter().find(|a| a.name == t.query_api) {
                    let qid = sanitize_id(&format!("Q_{}", api.name));
                    out.push_str(&format!("    {qid}[\"API {} · list\"]\n", api.name));
                    out.push_str(&format!("    {pid} --> {qid}\n"));
                    link_api_to_entity(program, &mut out, &qid, &api.entity);
                }
            }
            crate::dsl::PageBody::Form(f) => {
                let fid = sanitize_id(&format!("F_{}", page.name));
                out.push_str(&format!("    {fid}[\"表单 {}\"]\n", page.name));
                out.push_str(&format!("    {pid} --> {fid}\n"));
                if let Some(entity) = program.entities.iter().find(|e| e.name == f.entity) {
                    let eid = sanitize_id(&format!("E_{}", entity.name));
                    out.push_str(&format!("    {eid}[\"实体 {}\"]\n", entity.name));
                    for s in &f.submits {
                        let api_name = s.api_op.split('.').next().unwrap_or("").to_string();
                        let op = s.api_op.split('.').nth(1).unwrap_or("").to_string();
                        if let Some(api) = program.apis.iter().find(|a| a.name == api_name) {
                            let qid = sanitize_id(&format!("Q_{}_{}", api.name, op));
                            out.push_str(&format!("    {qid}[\"API {} · {}\"]\n", api.name, op));
                            out.push_str(&format!("    {fid} --> {qid}\n"));
                            out.push_str(&format!("    {qid} --> {eid}\n"));
                        }
                    }
                    for rules in &program.rules {
                        let hits = rules.field_rules.iter().any(|fr| fr.entity == entity.name)
                            || rules.checks.iter().any(|c| c.entity == entity.name);
                        if hits {
                            let rid = sanitize_id(&format!("R_{}", rules.name));
                            out.push_str(&format!("    {rid} -. 校验 .-> {fid}\n"));
                        }
                    }
                }
            }
            crate::dsl::PageBody::Detail(d) => {
                if let Some(api) = program.apis.iter().find(|a| a.entity == d.entity) {
                    let qid = sanitize_id(&format!("Q_{}_get", api.name));
                    out.push_str(&format!("    {qid}[\"API {} · get\"]\n", api.name));
                    out.push_str(&format!("    {pid} --> {qid}\n"));
                    link_api_to_entity(program, &mut out, &qid, &api.entity);
                }
            }
            crate::dsl::PageBody::Chart(c) => {
                let cid = sanitize_id(&format!("C_{}", c.chart));
                out.push_str(&format!("    {cid}[\"图表 {}\"]\n", c.chart));
                out.push_str(&format!("    {pid} --> {cid}\n"));
                if let Some(chart) = program.charts.iter().find(|ch| ch.name == c.chart) {
                    if let Some((api_name, _op)) = &chart.source {
                        if let Some(api) = program.apis.iter().find(|a| a.name == *api_name) {
                            let qid = sanitize_id(&format!("Q_{}", api.name));
                            out.push_str(&format!("    {cid} --> {qid}\n"));
                            link_api_to_entity(program, &mut out, &qid, &api.entity);
                        }
                    }
                }
            }
            crate::dsl::PageBody::Dashboard(dash) => {
                // V0.3：看板→每个条目图表一条引用边
                for item in &dash.items {
                    let cid = sanitize_id(&format!("C_{}", item.chart));
                    out.push_str(&format!("    {cid}[\"图表 {}\"]\n", item.chart));
                    out.push_str(&format!("    {pid} --> {cid}\n"));
                    if let Some(chart) = program.charts.iter().find(|ch| ch.name == item.chart) {
                        if let Some((api_name, _op)) = &chart.source {
                            if let Some(api) = program.apis.iter().find(|a| a.name == *api_name) {
                                let qid = sanitize_id(&format!("Q_{}", api.name));
                                out.push_str(&format!("    {cid} --> {qid}\n"));
                                link_api_to_entity(program, &mut out, &qid, &api.entity);
                            }
                        }
                    }
                }
            }
        }
    }

    // API → 实体 → 表（补齐未被页面引用的 API）
    for api in &program.apis {
        let qid = sanitize_id(&format!("Q_{}", api.name));
        if !out.contains(&format!("{qid}[\"")) {
            out.push_str(&format!("    {qid}[\"API {}\"]\n", api.name));
            if let Some(entity) = program.entities.iter().find(|e| e.name == api.entity) {
                let eid = sanitize_id(&format!("E_{}", entity.name));
                out.push_str(&format!("    {eid}[\"实体 {}\"]\n", entity.name));
                out.push_str(&format!("    {eid} --> {qid}\n"));
            }
        }
    }
    for entity in &program.entities {
        let eid = sanitize_id(&format!("E_{}", entity.name));
        if !out.contains(&format!("{eid}[\"")) {
            out.push_str(&format!("    {eid}[\"实体 {}\"]\n", entity.name));
        }
        let table = crate::identity::to_snake_case(&super::pluralize(&entity.name));
        let tid = sanitize_id(&format!("T_{}", table));
        out.push_str(&format!("    {tid}[(\"MySQL {}\")]\n", table));
        out.push_str(&format!("    {eid} --> {tid}\n"));
    }
    // 规则 → 实体（未被表单引用的）
    for rules in &program.rules {
        let rid = sanitize_id(&format!("R_{}", rules.name));
        for fr in &rules.field_rules {
            let eid = sanitize_id(&format!("E_{}", fr.entity));
            if out.contains(&format!("{eid}[\""))
                && !out.contains(&format!("{rid} -. 校验 .-> {eid}"))
            {
                out.push_str(&format!("    {rid} -. 校验 .-> {eid}\n"));
            }
        }
    }
    out
}

fn link_api_to_entity(program: &Program, out: &mut String, qid: &str, entity_name: &str) {
    if let Some(entity) = program.entities.iter().find(|e| e.name == entity_name) {
        let eid = sanitize_id(&format!("E_{}", entity.name));
        if !out.contains(&format!("{eid}[\"")) {
            out.push_str(&format!("    {eid}[\"实体 {}\"]\n", entity.name));
        }
        out.push_str(&format!("    {eid} --> {qid}\n"));
    }
}

// ── 业务流程图（状态生命周期） ──────────────────────────

/// 每个被实体引用的枚举 → 独立一份 stateDiagram。
/// Mermaid 一个渲染块只能含一份 diagram 文档；多个枚举若拼接在同一块内
/// 会直接报 Syntax error，因此必须按枚举分块。
pub fn status_mermaid(program: &Program) -> Vec<String> {
    let mut diagrams = Vec::new();
    for e in &program.enums {
        // 仅生成被实体字段引用的枚举
        let used = program
            .entities
            .iter()
            .any(|entity| entity.fields.iter().any(|f| f.type_name == e.name));
        if !used {
            continue;
        }
        let mut out = String::from("stateDiagram-v2\n");
        out.push_str(&format!(
            "    state \"{} 状态生命周期\" as {}_title\n",
            e.name,
            sanitize_id(&e.name)
        ));
        if e.variants.is_empty() {
            diagrams.push(out);
            continue;
        }
        // 枚举变体名直接当状态节点，可能撞 Mermaid 关键字（note、state…）——
        // 统一加命名空间前缀，语义由 title state 承载。
        let sid = |v: &str| format!("{}_{}", sanitize_id(&e.name), sanitize_id(v));
        out.push_str(&format!("    [*] --> {}\n", sid(&e.variants[0].name)));
        for w in e.variants.windows(2) {
            out.push_str(&format!(
                "    {} --> {}\n",
                sid(&w[0].name),
                sid(&w[1].name)
            ));
        }
        // 守卫注记：deny 引用该枚举字段具体取值
        for entity in &program.entities {
            let has_field = entity.fields.iter().any(|f| f.type_name == e.name);
            if !has_field {
                continue;
            }
            for rules in &program.rules {
                for guard in &rules.guards {
                    for deny in &guard.denies {
                        if let Some(value) = deny_of_status(&deny.expr) {
                            if e.variants.iter().any(|v| v.name == value) {
                                out.push_str(&format!(
                                    "    note right of {} : {} 时不可 {}\n",
                                    sid(&value),
                                    value,
                                    guard_op_zh(&guard.op)
                                ));
                            }
                        }
                    }
                }
            }
        }
        diagrams.push(out);
    }
    diagrams
}

fn deny_of_status(expr: &crate::dsl::Expr) -> Option<String> {
    use crate::dsl::{BinOp, Expr};
    match expr {
        Expr::Bin(BinOp::Eq, a, b) => match (a.as_ref(), b.as_ref()) {
            (Expr::Ref { path, old: false }, Expr::Str(v)) if path.len() == 1 => Some(v.clone()),
            (Expr::Str(v), Expr::Ref { path, old: false }) if path.len() == 1 => Some(v.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn guard_op_zh(op: &str) -> &'static str {
    match op {
        "delete" => "删除",
        "update" => "修改",
        "create" => "创建",
        _ => "操作",
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Mermaid 标签 / HTML 属性内的引号转义（不碰 &<>，避免破坏 Mermaid 语法）。
fn escape(s: &str) -> String {
    s.replace('"', "\\\"")
}

// ── 输出 ────────────────────────────────────────────────

pub fn emit_diagrams(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Vec<crate::generators::GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let er = er_mermaid(program);
    let dataflow = dataflow_mermaid(program);
    let status_diagrams = status_mermaid(program);
    let status = status_diagrams.join("\n\n");
    // 每个枚举的状态图必须是独立渲染块：一个 <pre class="mermaid"> 只能承载一份 diagram
    let status_cards = if status_diagrams.is_empty() {
        "<div class=\"card\"><p class=\"muted\">无状态枚举（enum 未被实体字段引用）</p></div>"
            .to_string()
    } else {
        status_diagrams
            .iter()
            .map(|d| {
                format!(
                    "<div class=\"card\"><pre class=\"mermaid\">\n{}\n</pre></div>",
                    escape_html(d)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let app_name = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "forma-app".into());

    let html = format!(
        r#"<!-- @generated by Forma (gen-diagrams) — do not edit manually. Regenerate with: forma build -->
<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8"/>
<title>{app} · 可视化</title>
<script src="https://unpkg.com/mermaid@10.9.1/dist/mermaid.min.js"></script>
<style>
body {{ font-family: ui-sans-serif, system-ui, "PingFang SC", "Microsoft YaHei", sans-serif; margin: 24px; color: #0f172a; background: #f8fafc; }}
h1 {{ font-size: 22px; }} h2 {{ font-size: 16px; margin: 28px 0 8px; }}
.card {{ background: #fff; border: 1px solid #e2e8f0; border-radius: 10px; padding: 16px; margin-bottom: 16px; }}
.muted {{ color: #64748b; font-size: 13px; }}
pre.mermaid {{ background: #fff; }}
pre.src {{ background: #0f172a; color: #e2e8f0; padding: 12px; border-radius: 8px; font-size: 12px; overflow: auto; }}
</style>
</head>
<body>
<h1>{app} · 系统可视化</h1>
<p class="muted">由 Forma (gen-diagrams) 从 DSL 生成 · mermaid 渲染 · {sha}</p>

<h2>① 实体关系图（数据模型）</h2>
<div class="card"><pre class="mermaid">
{er}
</pre></div>

<h2>② 数据流图（页面 → API → 实体 → 数据库）</h2>
<div class="card"><pre class="mermaid">
{dataflow}
</pre></div>

<h2>③ 业务流程图（状态生命周期 + 守卫）</h2>
{status_cards}

<h2>图表源码</h2>
<pre class="src">ER:
{er_src}

DATAFLOW:
{dataflow_src}

STATUS:
{status_src}
</pre>
<script>mermaid.initialize({{ startOnLoad: true, theme: "neutral" }});</script>
</body>
</html>
"#,
        app = escape(&app_name),
        sha = &sha256_hex(&format!("{er}{dataflow}{status}"))[..12],
        er = escape_html(&er),
        dataflow = escape_html(&dataflow),
        er_src = escape_html(&er),
        dataflow_src = escape_html(&dataflow),
        status_src = escape_html(&status),
    );

    let mut files = Vec::new();
    // RFC-0007 §16：所有图表产物带 @generated 标记（Mermaid 注释 %%）
    let gen_note = "%% @generated by Forma (gen-diagrams) — do not edit manually.\n";
    let marked = |c: &str| format!("{gen_note}{c}");
    for (rel, content, gen) in [
        (format!("{out_dir}/er.mmd"), marked(&er), "gen-diagrams"),
        (
            format!("{out_dir}/dataflow.mmd"),
            marked(&dataflow),
            "gen-diagrams",
        ),
        (
            format!("{out_dir}/status.mmd"),
            marked(&status),
            "gen-diagrams",
        ),
        (format!("{out_dir}/diagrams.html"), html, "gen-diagrams"),
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
