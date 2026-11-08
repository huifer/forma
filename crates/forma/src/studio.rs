//! Studio（M6/M9 P1）：`forma studio [dir] [--port N]` 本地审阅服务。
//!
//! v1（只读）：ER 图 / 实体 / API / 页面 / 图表 的语义模型 API + 最小 Web UI
//! （同一渲染源，供 v2 双向编辑在其上叠加写回）。
//! v2（写回）：`POST /api/patch` 走 `dsl::patch::apply_dsl_patch` 事务
//! （快照 → 编辑 → check_program → 重建，错误即回滚）。
//!
//! 实现约束：零新增依赖——手写 HTTP/1.1（TcpListener + thread-per-conn）、
//! 无框架、无 TLS；仅用于本地审阅（绑定 127.0.0.1）。

use crate::dsl::{load_program, Program};
use crate::manifest::load_manifest;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

const DEFAULT_PORT: u16 = 4747;

/// 语义模型（Studio 前端消费）。
pub fn studio_model(program: &Program) -> serde_json::Value {
    let project = program.app.as_ref();
    let entities: Vec<serde_json::Value> = program
        .entities
        .iter()
        .map(|e| {
            let fields: Vec<serde_json::Value> = e
                .fields
                .iter()
                .map(|f| {
                    json!({
                        "name": f.name,
                        "type": f.type_name,
                        "args": f.type_args,
                        "optional": f.optional,
                        "attrs": f.attrs.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let relations: Vec<serde_json::Value> = e
                .fields
                .iter()
                .filter(|f| crate::dsl::is_relation(program, &f.type_name))
                .map(|f| json!({ "field": f.name, "target": f.type_name }))
                .collect();
            json!({
                "name": e.name,
                "fields": fields,
                "relations": relations,
                "indexes": e.indexes,
                "uniques": e.uniques,
                "searchable": e.searchable,
            })
        })
        .collect();
    let enums: Vec<serde_json::Value> = program
        .enums
        .iter()
        .map(|e| json!({ "name": e.name, "variants": e.variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>() }))
        .collect();
    let apis: Vec<serde_json::Value> = program
        .apis
        .iter()
        .map(|a| json!({ "name": a.name, "entity": a.entity, "operations": a.operations }))
        .collect();
    let commands: Vec<serde_json::Value> = program
        .commands
        .iter()
        .map(|c| json!({ "name": c.name, "method": c.method, "path": c.path }))
        .collect();
    let pages: Vec<serde_json::Value> = program
        .pages
        .iter()
        .map(|p| {
            let kind = match &p.body {
                crate::dsl::PageBody::Table(_) => "table",
                crate::dsl::PageBody::Form(_) => "form",
                crate::dsl::PageBody::Detail(_) => "detail",
                crate::dsl::PageBody::Chart(_) => "chart",
                crate::dsl::PageBody::Dashboard(_) => "dashboard",
            };
            json!({ "name": p.name, "route": p.route, "kind": kind })
        })
        .collect();
    let charts: Vec<serde_json::Value> = program
        .charts
        .iter()
        .map(|c| {
            json!({ "name": c.name, "chartType": c.chart_type, "source": c.source.as_ref().map(|(a, op)| format!("{a}.{op}")) })
        })
        .collect();
    json!({
        "project": {
            "name": project.map(|a| a.name.clone()).unwrap_or_default(),
            "version": project.map(|a| a.version.clone()).unwrap_or_default(),
            "locales": project.map(|a| a.locales.clone()).unwrap_or_default(),
        },
        "entities": entities,
        "enums": enums,
        "apis": apis,
        "commands": commands,
        "pages": pages,
        "charts": charts,
        "auth": program.auth.as_ref().map(|a| json!({ "provider": a.provider, "roles": a.roles })),
        "irVersion": program.ir_version,
    })
}

fn html_ui() -> String {
    String::from(
        r#"<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8" />
<title>Forma Studio</title>
<style>
  :root { color-scheme: light dark; }
  body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; background: #f8fafc; color: #0f172a; }
  .dark body { background: #0f172a; color: #e2e8f0; }
  header { padding: 12px 20px; border-bottom: 1px solid #e2e8f0; display: flex; align-items: baseline; gap: 12px; }
  header h1 { font-size: 18px; margin: 0; }
  header .meta { font-size: 12px; color: #64748b; }
  main { display: grid; grid-template-columns: 260px 1fr; gap: 16px; padding: 16px 20px; }
  .nav { display: flex; flex-direction: column; gap: 4px; }
  .nav button { text-align: left; padding: 8px 10px; border-radius: 8px; border: 1px solid #e2e8f0; background: #fff; cursor: pointer; font-size: 13px; }
  .nav button:hover { border-color: #94a3b8; }
  .nav .kind { color: #64748b; font-size: 11px; padding-left: 4px; }
  .detail { background: #fff; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
  table { border-collapse: collapse; width: 100%; font-size: 13px; }
  th, td { text-align: left; padding: 5px 8px; border-bottom: 1px solid #f1f5f9; }
  .tag { font-size: 11px; padding: 1px 6px; border-radius: 999px; background: #eff6ff; color: #2563eb; }
  .rel { color: #16a34a; }
  h2 { font-size: 15px; margin: 0 0 8px; }
  h3 { font-size: 13px; color: #64748b; margin: 16px 0 4px; }
  .empty { color: #94a3b8; font-size: 13px; }
  .edit { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; margin: 6px 0; }
  .edit input, .edit select { padding: 6px 8px; border-radius: 8px; border: 1px solid #e2e8f0; font-size: 13px; }
  .mini { font-size: 12px; padding: 4px 10px; border-radius: 8px; border: 1px solid #e2e8f0; background: #fff; cursor: pointer; }
  .result { font-size: 11px; white-space: pre-wrap; background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 8px; padding: 8px; max-height: 220px; overflow: auto; }
</style>
</head>
<body>
<header>
  <h1>Forma Studio</h1>
  <span class="meta" id="meta">loading…</span>
</header>
<main>
  <nav class="nav" id="nav"></nav>
  <section class="detail" id="detail"><p class="empty">选择左侧实体查看字段与关系。</p></section>
</main>
<script>
const nav = document.getElementById('nav');
const detail = document.getElementById('detail');
let model = null;

function show(e) {
  let h = '<h2>' + e.name + '</h2><table><tr><th>字段</th><th>类型</th><th>属性</th><th>编辑</th></tr>';
  for (const f of e.fields) {
    const attrs = f.attrs.length ? '<span class="tag">' + f.attrs.join(' ') + '</span>' : '';
    const rel = e.relations.find(r => r.field === f.name) ? '<span class="rel">→ ' + e.relations.find(r => r.field === f.name).target + '</span>' : '';
    const req = f.attrs.includes('required');
    h += '<tr><td>' + f.name + '</td><td>' + f.type + (rel ? ' ' + rel : '') + '</td><td>' + attrs + '</td>'
       + '<td><button class="mini" onclick="toggleRequired(\'' + e.name + '\',\'' + f.name + '\',' + !req + ')">'
       + (req ? '取消必填' : '设为必填') + '</button></td></tr>';
  }
  h += '</table>';
  if (e.indexes.length) h += '<h3>索引</h3><p>' + e.indexes.map(i => i.join(', ')).join(' · ') + '</p>';
  if (e.uniques.length) h += '<h3>唯一约束</h3><p>' + e.uniques.map(u => u.join(', ')).join(' · ') + '</p>';
  h += '<h3>编辑（Studio v2 双向编辑）</h3>'
    + '<div class="edit">'
    + '<input id="f-name" placeholder="字段名" />'
    + '<select id="f-type"><option>String(50)</option><option>Text</option><option>Int</option>'
    + '<option>Decimal(12,2)</option><option>Bool</option><option>Timestamp</option></select>'
    + '<label><input type="checkbox" id="f-required" /> 必填</label>'
    + '<button class="mini" onclick="addField(\'' + e.name + '\')">添加字段</button>'
    + '</div>'
    + '<h3>页面</h3><div class="edit">'
    + '<input id="p-name" placeholder="页面名" />'
    + '<select id="p-api">' + model.apis.filter(a => a.operations.includes('list')).map(a => '<option value="' + a.name + '">' + a.name + '</option>').join('') + '</select>'
    + '<button class="mini" onclick="addPage()">添加表格页</button>'
    + '</div><pre id="result" class="result"></pre>';
  detail.innerHTML = h;
}

async function sendPatch(doc) {
  const res = await fetch('/api/patch', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(doc),
  });
  const data = await res.json();
  document.getElementById('result').textContent =
    (res.ok ? '✓ ' + res.status : '✗ ' + res.status) + ' ' + JSON.stringify(data, null, 2);
  const fresh = await fetch('/api/model').then(r => r.json());
  rebuildNav(fresh);
}

function addField(entity) {
  const name = document.getElementById('f-name').value.trim();
  if (!name) { alert('字段名不能为空'); return; }
  const type = document.getElementById('f-type').value;
  const required = document.getElementById('f-required').checked;
  sendPatch({ operations: [{ op: 'add-entity-field', entity: entity, field: { name: name, type: type }, required: required }] });
}

function toggleRequired(entity, field, required) {
  sendPatch({ operations: [{ op: 'set-entity-field-required', entity: entity, field: field, required: required }] });
}

function addPage() {
  const name = document.getElementById('p-name').value.trim();
  if (!name) { alert('页面名不能为空'); return; }
  const apiName = document.getElementById('p-api').value;
  const api = model.apis.find(a => a.name === apiName);
  const entity = model.entities.find(e => e.name === api.entity);
  const columns = entity.fields.filter(f => !entity.relations.some(r => r.field === f.name)).map(f => f.name);
  sendPatch({ operations: [{ op: 'add-table-page', name: name, title: name, queryApi: apiName, columns: columns }] });
}

function rebuildNav(m) {
  nav.innerHTML = '';
  for (const e of m.entities) {
    const b = document.createElement('button');
    b.textContent = e.name + ' (' + e.fields.length + ')';
    b.onclick = () => show(e);
    nav.appendChild(b);
  }
}

(async () => {
  model = await fetch('/api/model').then(r => r.json());
  document.getElementById('meta').textContent =
    model.project.name + ' · ' + model.project.version + ' · IR ' + model.irVersion +
    (model.auth ? ' · auth(' + model.auth.provider + ')' : '');
  rebuildNav(model);
})();
</script>
</body>
</html>
"#,
    )
}

fn response(status: &str, content_type: &str, body: String) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn handle(mut conn: TcpStream, dir: &Path) {
    // 读完整请求：headers 以 \r\n\r\n 结束，随后 Content-Length 字节 body
    let mut data = Vec::new();
    let mut chunk = [0u8; 2048];
    let header_end;
    loop {
        match conn.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            header_end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
            break;
        }
        if data.len() > 64 * 1024 {
            return;
        }
    }
    let header_text = String::from_utf8_lossy(&data[..header_end]).to_string();
    let content_length: usize = header_text
        .lines()
        .skip(1)
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while data.len() < header_end + 4 + content_length {
        match conn.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    let body = String::from_utf8_lossy(
        &data[(header_end + 4).min(data.len())..(header_end + 4 + content_length).min(data.len())],
    )
    .to_string();
    let first_line = header_text.lines().next().unwrap_or("");
    let method = first_line.split_whitespace().next().unwrap_or("");
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();

    let resp = match (method, path.as_str()) {
        ("POST", "/api/patch") => {
            // V2 双向编辑：patch 事务（快照 → 编辑 → check_program → 重建，错误即回滚）
            let doc: crate::dsl::DslPatchDocument = match serde_json::from_str(&body) {
                Ok(d) => d,
                Err(e) => {
                    let msg =
                        json!({ "error": format!("invalid patch document: {e}") }).to_string();
                    write_resp(
                        &mut conn,
                        &response("400 Bad Request", "application/json", msg),
                    );
                    return;
                }
            };
            match crate::dsl::patch::apply_dsl_patch(dir, &doc) {
                Ok(report) => {
                    let ok = report.diagnostics.iter().all(|d| d.severity != "error");
                    let status = if ok { "200 OK" } else { "409 Conflict" };
                    response(
                        status,
                        "application/json",
                        serde_json::to_string(&report).unwrap_or_default(),
                    )
                }
                Err(e) => response(
                    "400 Bad Request",
                    "application/json",
                    json!({ "error": e.to_string() }).to_string(),
                ),
            }
        }
        ("GET", "/") => response("200 OK", "text/html; charset=utf-8", html_ui()),
        ("GET", "/api/model") => {
            let out = (|| -> anyhow::Result<String> {
                let loaded = load_manifest(dir)?;
                let source = loaded.manifest.source.clone().unwrap_or_default();
                let (program, _) = load_program(dir, &source)?;
                Ok(serde_json::to_string(&studio_model(&program))?)
            })();
            match out {
                Ok(body) => response("200 OK", "application/json", body),
                Err(e) => response(
                    "500 Internal Server Error",
                    "application/json",
                    json!({ "error": e.to_string() }).to_string(),
                ),
            }
        }
        ("GET", "/api/graph") => {
            let out = (|| -> anyhow::Result<String> {
                let loaded = load_manifest(dir)?;
                let source = loaded.manifest.source.clone().unwrap_or_default();
                let (program, _) = load_program(dir, &source)?;
                Ok(serde_json::to_string(&crate::engine::dsl_graph(&program))?)
            })();
            match out {
                Ok(body) => response("200 OK", "application/json", body),
                Err(e) => response(
                    "500 Internal Server Error",
                    "application/json",
                    json!({ "error": e.to_string() }).to_string(),
                ),
            }
        }
        _ => response("404 Not Found", "text/plain", "not found".into()),
    };
    write_resp(&mut conn, &resp);
}

fn write_resp(conn: &mut TcpStream, resp: &str) {
    let _ = conn.write_all(resp.as_bytes());
}

/// 绑定 127.0.0.1 并服务（阻塞；测试用 ephemeral port）。
pub fn run_studio(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

/// 服务循环（thread-per-conn；并发模型：模型读取按请求现读，改动见 v2）。
pub fn serve(listener: TcpListener, dir: std::sync::Arc<Path>) {
    for stream in listener.incoming().flatten() {
        let dir = dir.clone();
        std::thread::spawn(move || handle(stream, &dir));
    }
}

pub fn studio_port(flag: Option<u16>) -> u16 {
    flag.unwrap_or_else(|| {
        std::env::var("FORMA_STUDIO_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT)
    })
    .max(1)
}
