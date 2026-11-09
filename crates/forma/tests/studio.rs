//! Studio v1 只读服务回归测试（M6/M9 P1-#10 第一步）：
//! `forma studio [dir] [--port N]` —— 语义模型 API + 最小 Web UI。
//!
//! 验收：
//! - GET /api/model：实体/字段/关系/索引/唯一/枚举/api/命令/页面/图表/auth；
//! - GET /api/graph：语义图（节点/边）；
//! - GET /：最小 UI（ER 导航 + 字段表）；
//! - 未知路径 404。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("forma-studio-{tag}-{}-{nanos}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path) {
    std::fs::create_dir_all(dir.join("forma")).unwrap();
    std::fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: studiodemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("forma/app.fm"),
        r#"app studiodemo {
  version = "0.1.0"
  locale  = ["zh", "en"]
}

enum Status {
  draft
  paid
}

auth module {
  provider jwt
  roles [admin, user]
}

entity Customer {
  id    Uuid @id @default(uuid)
  name  String(50) @required
  email Email @unique
}

entity Order {
  id       Uuid @id @default(uuid)
  customer Customer @required
  status   Status @default(draft)
  total    Decimal(12,2)
  createdAt Timestamp @default(now) @readonly

  index [status]
  unique [customer]
}

api Orders {
  entity Order
  operations [list get create update delete]
  list {
    filters [status]
    sort [createdAt desc]
    paginate offset
  }
}

page Orders {
  route "/orders"
  title { zh "订单" en "Orders" }
  table {
    query   Orders.list
    columns [status, total, createdAt: datetime]
  }
}
"#,
    )
    .unwrap();
}

fn get(port: u16, path: &str) -> (String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    let status = buf.lines().next().unwrap_or("").to_string();
    let body = buf.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

#[test]
fn studio_serves_model_graph_and_ui() {
    let dir = tmp_dir("v1");
    write_project(&dir);

    let listener = forma::studio::run_studio(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        forma::studio::serve(listener, Arc::from(dir.as_path()));
    });
    std::thread::sleep(Duration::from_millis(200));

    // 模型 API
    let (status, body) = get(port, "/api/model");
    assert!(status.contains("200"), "{status}");
    let model: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(model["project"]["name"], "studiodemo");
    assert_eq!(model["project"]["version"], "0.1.0");
    assert_eq!(model["auth"]["provider"], "jwt");
    assert_eq!(model["enums"][0]["variants"][0], "draft");
    let order = model["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Order")
        .unwrap();
    assert_eq!(order["relations"][0]["field"], "customer");
    assert_eq!(order["relations"][0]["target"], "Customer");
    assert_eq!(order["uniques"][0][0], "customer");
    assert_eq!(model["pages"][0]["kind"], "table");
    assert_eq!(model["apis"][0]["operations"][0], "list");

    // 语义图
    let (status, body) = get(port, "/api/graph");
    assert!(status.contains("200"), "{status}");
    let graph: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        graph.to_string().contains("Order"),
        "graph should mention Order: {graph}"
    );

    // 最小 UI
    let (status, body) = get(port, "/");
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("Forma Studio"), "{body}");
    assert!(body.contains("/api/model"));
    assert!(body.contains("entities"));

    // 404
    let (status, _) = get(port, "/nope");
    assert!(status.contains("404"), "{status}");
}

#[test]
fn studio_model_json_shape_is_stable() {
    // 直接断言 studio_model 的 JSON 结构（不启服务）
    let dir = tmp_dir("shape");
    write_project(&dir);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let model = forma::studio::studio_model(&program);
    for key in [
        "project",
        "entities",
        "enums",
        "apis",
        "commands",
        "pages",
        "charts",
        "auth",
        "irVersion",
    ] {
        assert!(model.get(key).is_some(), "model missing key {key}");
    }
}

fn post(port: u16, path: &str, body: &str) -> (String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    let status = buf.lines().next().unwrap_or("").to_string();
    let body_out = buf.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body_out)
}

#[test]
fn studio_v2_bidirectional_patch() {
    let dir = tmp_dir("v2");
    write_project(&dir);

    let listener = forma::studio::run_studio(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir_thread = dir.clone();
    std::thread::spawn(move || {
        forma::studio::serve(listener, Arc::from(dir_thread.as_path()));
    });
    std::thread::sleep(Duration::from_millis(200));

    let fm_path = dir.join("forma/app.fm");
    let original = std::fs::read_to_string(&fm_path).unwrap();

    // 1) 加字段（写回 .fm；带 patchId 以便验证幂等）
    let (status, body) = post(
        port,
        "/api/patch",
        r#"{"patchId":"studio-note","operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"note","type":"Text"},"required":false}]}"#,
    );
    assert!(status.contains("200"), "{status} {body}");
    let report: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        report["diagnostics"].as_array().unwrap().is_empty(),
        "{report}"
    );
    let updated = std::fs::read_to_string(&fm_path).unwrap();
    assert!(updated.contains("note Text"), "{updated}");
    assert_ne!(updated, original);

    // 2) 幂等：同 patchId 重复提交 → 零二次变更
    let (status, body) = post(
        port,
        "/api/patch",
        r#"{"patchId":"studio-note","operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"note","type":"Text"},"required":false}]}"#,
    );
    assert!(status.contains("200"), "{status} {body}");
    let report: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(report["idempotent"], true, "{report}");
    let after = std::fs::read_to_string(&fm_path).unwrap();
    assert_eq!(after, updated, "idempotent patch must not change the file");

    // 3) 切换必填
    let (status, body) = post(
        port,
        "/api/patch",
        r#"{"operations":[{"op":"set-entity-field-required","entity":"Order","field":"note","required":true}]}"#,
    );
    assert!(status.contains("200"), "{status} {body}");
    let fm = std::fs::read_to_string(&fm_path).unwrap();
    assert!(
        fm.lines()
            .any(|l| l.contains("note") && l.contains("@required")),
        "{fm}"
    );

    // 4) 未知操作 → 4xx + 诊断（零写入）
    let before = std::fs::read_to_string(&fm_path).unwrap();
    let (status, body) = post(
        port,
        "/api/patch",
        r#"{"operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"ghost","type":"Wibble"},"required":false}]}"#,
    );
    assert!(
        status.contains("400") || status.contains("409"),
        "{status} {body}"
    );
    assert_eq!(
        std::fs::read_to_string(&fm_path).unwrap(),
        before,
        "failed patch must not write"
    );

    // 5) 非法 JSON → 400
    let (status, _) = post(port, "/api/patch", "{not json");
    assert!(status.contains("400"), "{status}");
}

// ── Studio V3：变更历史可视化与回放 ─────────────────────────────

#[test]
fn studio_patch_history_is_visible_and_replayable() {
    let dir = tmp_dir("v3-history");
    write_project(&dir);

    let listener = forma::studio::run_studio(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir_thread = dir.clone();
    std::thread::spawn(move || {
        forma::studio::serve(listener, Arc::from(dir_thread.as_path()));
    });
    std::thread::sleep(Duration::from_millis(200));

    // 先打一次补丁，产生审计记录
    let (status, _) = post(
        port,
        "/api/patch",
        r#"{"patchId":"history-1","operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"note","type":"Text"},"required":false}]}"#,
    );
    assert!(status.contains("200"), "{status}");

    // 历史 API
    let (status, body) = get(port, "/api/patch-history");
    assert!(status.contains("200"), "{status}");
    let h: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(h["count"].as_u64().unwrap() >= 1, "{h}");
    let item = &h["items"][h["items"].as_array().unwrap().len() - 1];
    assert_eq!(item["status"], "applied", "{h}");
    assert!(
        item["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o == "add-entity-field"),
        "{h}"
    );
    assert!(
        item["filesChanged"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "forma/app.fm"),
        "{h}"
    );
    assert!(h["note"].as_str().unwrap().contains("patchId"), "{h}");

    // UI 含历史面板
    let (status, body) = get(port, "/");
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("变更历史"), "{body}");
    assert!(body.contains("/api/patch-history"), "{body}");

    // 回放：审计摘要 + patchId 可重新提交（幂等零二次变更）
    let (status, body) = post(
        port,
        "/api/patch",
        r#"{"patchId":"history-1","operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"note","type":"Text"},"required":false}]}"#,
    );
    assert!(status.contains("200"), "{status} {body}");
    let report: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(report["idempotent"], true, "{report}");
    let fm = std::fs::read_to_string(dir.join("forma/app.fm")).unwrap();
    assert!(fm.contains("note Text"), "{fm}");
}
