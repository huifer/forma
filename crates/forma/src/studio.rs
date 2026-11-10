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
  .history-btn { position: fixed; right: 16px; bottom: 16px; padding: 8px 14px; border-radius: 999px; border: 1px solid #2563eb; background: #2563eb; color: #fff; cursor: pointer; font-size: 12px; }
  .history-panel { position: fixed; top: 0; right: -360px; width: 340px; height: 100vh; background: #fff; border-left: 1px solid #e2e8f0; transition: right .2s ease; padding: 12px; overflow: auto; z-index: 20; }
  .history-panel.open { right: 0; }
  .collab-btn { position: fixed; right: 20px; bottom: 20px; padding: 10px 16px; border-radius: 999px; border: 1px solid #cbd5e1; background: #fff; cursor: pointer; z-index: 30; }
  .collab-panel { position: fixed; right: 20px; bottom: 72px; width: 320px; max-height: 60vh; overflow: auto; background: #fff; border: 1px solid #e2e8f0; border-radius: 8px; box-shadow: 0 10px 30px rgb(0 0 0 / 12%); display: none; z-index: 30; }
  .collab-panel.open { display: block; }
  .collab-section { padding: 10px 12px; border-top: 1px solid #f1f5f9; }
  .collab-list { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
  .presence-item { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .presence-item .dot { width: 8px; height: 8px; border-radius: 50%; background: #16a34a; }
  .event-item { font-size: 12px; padding: 4px 0; border-bottom: 1px dashed #f1f5f9; }
  .event-patch-conflict { color: #dc2626; }
  .event-patch { color: #16a34a; }
  .history-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
  .history-item { border: 1px solid #e2e8f0; border-radius: 8px; padding: 8px; margin-bottom: 8px; font-size: 12px; }
  .history-item .status { font-weight: 600; }
  .history-item .meta { color: #64748b; margin-top: 4px; word-break: break-all; }
  .history-item .ops { margin-top: 4px; }
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
<button class="history-btn" onclick="toggleHistory()">变更历史</button>
<aside class="history-panel" id="history">
  <div class="history-head"><strong>变更历史</strong><button class="mini" onclick="toggleHistory()">关闭</button></div>
  <div id="history-list" class="history-list"><p class="empty">暂无审计记录</p></div>
</aside>
<button class="collab-btn" onclick="toggleCollab()">协作</button>
<aside class="collab-panel" id="collab">
  <div class="history-head"><strong>协作（实时）</strong><button class="mini" onclick="toggleCollab()">关闭</button></div>
  <div class="collab-section"><strong>在线</strong><div id="presence-list" class="collab-list"><span class="empty">仅本机会话</span></div></div>
  <div class="collab-section"><strong>实时事件</strong><div id="event-feed" class="collab-list"><span class="empty">暂无事件</span></div></div>
</aside>
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

function toggleHistory() {
  document.getElementById('history').classList.toggle('open');
  loadHistory();
}

async function loadHistory() {
  const res = await fetch('/api/patch-history');
  const h = await res.json();
  const box = document.getElementById('history-list');
  if (!h.items || h.items.length === 0) { box.innerHTML = '<p class="empty">暂无审计记录</p>'; return; }
  box.innerHTML = h.items.map(it => {
    const ops = (it.operations || []).join(', ');
    const files = (it.filesChanged || []).join(', ');
    const status = it.status === 'applied' ? '<span class="status" style="color:#16a34a">✓ applied</span>'
      : '<span class="status" style="color:#dc2626">✗ ' + it.status + '</span>';
    return '<div class="history-item">' + status
      + '<div class="meta">' + it.ts + ' · build ' + it.buildFiles + ' files</div>'
      + (ops ? '<div class="ops">' + ops + '</div>' : '')
      + (files ? '<div class="meta">' + files + '</div>' : '')
      + (it.message ? '<div class="meta">' + it.message + '</div>' : '')
      + '</div>';
  }).join('') + '<p class="empty">' + h.note + '</p>';
}

// ── 协作（V3+）：SSE 实时事件 + 在线会话 ──
const presence = new Set();
function renderPresence() {
  const box = document.getElementById('presence-list');
  box.innerHTML = presence.size === 0
    ? '<span class="empty">仅本机会话</span>'
    : Array.from(presence).map(n => '<div class="presence-item"><span class="dot"></span>' + n + '</div>').join('');
}
function refreshAll() {
  fetch('/api/model').then(r => r.json()).then(m => { model = m; rebuildNav(m); });
  if (document.getElementById('history').classList.contains('open')) loadHistory();
}
function pushEvent(ev) {
  const feed = document.getElementById('event-feed');
  const empty = feed.querySelector('.empty');
  if (empty) empty.remove();
  const div = document.createElement('div');
  div.className = 'event-item event-' + ev.type;
  const time = new Date().toLocaleTimeString();
  if (ev.type === 'presence') {
    presence.add(ev.name);
    renderPresence();
    div.textContent = time + ' ' + ev.name + ' 加入';
  } else if (ev.type === 'patch') {
    div.textContent = time + ' ✓ 变更已应用（' + (ev.operations || []).join(', ') + '）';
    refreshAll();
  } else if (ev.type === 'patch-conflict') {
    const detail = ev.diagnostics ? ev.diagnostics.map(d => d.code).join(', ') : (ev.message || '');
    div.textContent = time + ' ✗ 冲突/拒绝：' + detail;
  } else {
    div.textContent = time + ' ' + JSON.stringify(ev);
  }
  feed.prepend(div);
}
function toggleCollab() { document.getElementById('collab').classList.toggle('open'); }
try {
  const es = new EventSource('/api/events');
  es.onmessage = (e) => { try { pushEvent(JSON.parse(e.data)); } catch (err) {} };
} catch (err) {}
fetch('/api/presence', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ name: '会话 ' + Math.floor(Math.random() * 1000) }),
}).catch(() => {});

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

/// Studio 协作总线（V3+）：SSE 客户端广播 + 在线会话列表。
///
/// 零新增依赖：`Arc<Mutex<Vec<Arc<Mutex<TcpStream>>>>>` 广播；
/// 每个 SSE 连接由独立线程持有，写失败即移除（客户端断开）。
#[derive(Clone, Default)]
pub struct StudioBus {
    sse: std::sync::Arc<std::sync::Mutex<Vec<std::sync::Arc<std::sync::Mutex<TcpStream>>>>>,
    presence: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
}

impl StudioBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// 广播一条 SSE 事件（JSON 字符串）。
    fn broadcast(&self, event: &str) {
        let msg = format!("data: {event}\n\n");
        let mut clients = self.sse.lock().unwrap();
        clients.retain(|c| {
            let Ok(mut s) = c.lock() else { return false };
            s.write_all(msg.as_bytes()).is_ok()
        });
    }

    fn join_presence(&self, name: &str) {
        let mut p = self.presence.lock().unwrap();
        p.retain(|x| x.get("name").and_then(|n| n.as_str()) != Some(name));
        p.push(json!({ "name": name }));
    }

    fn presence_list(&self) -> Vec<serde_json::Value> {
        self.presence.lock().unwrap().clone()
    }
}

fn handle(mut conn: TcpStream, dir: &Path, bus: &StudioBus) {
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
                    // 协作（V3+）：广播变更事件（含冲突），其他会话实时可见
                    let ev = if ok {
                        json!({
                            "type": "patch",
                            "status": "applied",
                            "patchId": report.patch_id,
                            "operations": serde_json::from_str::<serde_json::Value>(&body)
                                .ok()
                                .and_then(|v| v.get("operations").and_then(|a| a.as_array()).cloned())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|o| o.get("op").and_then(|x| x.as_str()).map(String::from))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                        })
                    } else {
                        json!({
                            "type": "patch-conflict",
                            "status": "rejected",
                            "diagnostics": report.diagnostics.iter().map(|d| json!({ "code": d.code, "message": d.message })).collect::<Vec<_>>(),
                        })
                    };
                    bus.broadcast(&ev.to_string());
                    response(
                        status,
                        "application/json",
                        serde_json::to_string(&report).unwrap_or_default(),
                    )
                }
                Err(e) => {
                    bus.broadcast(
                        &json!({ "type": "patch-conflict", "status": "error", "message": e.to_string() })
                            .to_string(),
                    );
                    response(
                        "400 Bad Request",
                        "application/json",
                        json!({ "error": e.to_string() }).to_string(),
                    )
                }
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
        ("GET", "/api/events") => {
            // 协作（V3+）：SSE 订阅变更 / 冲突 / 在线事件；15s keep-alive。
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";
            if conn.write_all(head.as_bytes()).is_err() {
                return;
            }
            let sink = std::sync::Arc::new(std::sync::Mutex::new(conn));
            bus.sse.lock().unwrap().push(sink.clone());
            loop {
                std::thread::sleep(std::time::Duration::from_secs(15));
                let mut s = sink.lock().unwrap();
                if s.write_all(b": keep-alive\n\n").is_err() {
                    break;
                }
            }
            bus.sse
                .lock()
                .unwrap()
                .retain(|c| !std::sync::Arc::ptr_eq(c, &sink));
            return;
        }
        ("POST", "/api/presence") => {
            let name = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
                .unwrap_or_else(|| "匿名".to_string());
            bus.join_presence(&name);
            bus.broadcast(&json!({ "type": "presence", "name": name }).to_string());
            response(
                "200 OK",
                "application/json",
                json!({ "ok": true, "online": bus.presence_list() }).to_string(),
            )
        }
        ("GET", "/api/patch-history") => {
            // V3：变更历史可视化（本地审阅面；多人实时协作不在 v0.3 范围）
            let body = serde_json::to_string(&patch_history(dir)).unwrap_or_default();
            response("200 OK", "application/json", body)
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
/// V3+：`bus` 承载 SSE 广播与在线会话（协作层）。
pub fn serve(listener: TcpListener, dir: std::sync::Arc<Path>, bus: StudioBus) {
    for stream in listener.incoming().flatten() {
        let dir = dir.clone();
        let bus = bus.clone();
        std::thread::spawn(move || handle(stream, &dir, &bus));
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

/// 变更历史条目（Studio V3 本地审阅面）。
///
/// 数据来源：`.forma/audit.jsonl`（append-only，RFC-0006 P5 审计落点）。
/// 「回放」= 在审阅面展示某次补丁的变更前后摘要（operations / filesChanged /
/// buildFiles / risk / message）；真实 DSL 内容可由 patchId 重新提交补丁事务，
/// 本函数只负责把历史变成可视化数据，不伪造 diff。
pub fn patch_history(dir: &Path) -> serde_json::Value {
    let events = crate::auditing::read_recent(dir, 200);
    let items: Vec<serde_json::Value> = events
        .iter()
        .rev()
        .map(|e| {
            let ops = e
                .get("operations")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let files = e
                .get("filesChanged")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({
                "ts": e.get("ts").cloned().unwrap_or_else(|| json!("")),
                "project": e.get("project").cloned().unwrap_or_else(|| json!("")),
                "status": e.get("status").cloned().unwrap_or_else(|| json!("")),
                "operations": ops,
                "filesChanged": files,
                "buildFiles": e.get("buildFiles").cloned().unwrap_or_else(|| json!(0)),
                "risk": e.get("risk").cloned().unwrap_or_else(|| json!({})),
                "approvalBy": e.get("approvalBy").cloned(),
                "message": e.get("message").cloned(),
            })
        })
        .collect();
    json!({
        "count": items.len(),
        "items": items,
        "note": "回放为审计摘要；重新应用请携带 patchId 走 /api/patch（事务幂等）",
    })
}
