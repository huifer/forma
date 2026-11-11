//! 实时推送（live → SSE）能力回归测试（M10/P1）。
//! 覆盖：DSL 解析、检查诊断、Rust 后端 live.rs 生成、SSE 端点、publish 钩子、零产物。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forma-live-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: live\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: rust\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        "app live { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\nlive ItemFeed {\n api Items.list\n events [create update delete]\n}\n",
    )
    .unwrap();
}

#[test]
fn live_generates_sse_module_and_publish_hooks() {
    let dir = tmp_dir("generate");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-rust"), "{names:?}");

    // src/live.rs 生成
    assert!(dir.join("generated/backend/src/live.rs").exists());
    let live = fs::read_to_string(dir.join("generated/backend/src/live.rs")).unwrap();
    assert!(live.contains("ChannelSpec"), "{live}");
    assert!(live.contains("ItemFeed"), "{live}");
    assert!(live.contains("pub fn publish"), "{live}");
    assert!(live.contains("Sse"), "{live}");
    // 订阅路由是动态段（handler 用 Path<String> 取通道名）——静态路径 + Path 提取器 = 500
    assert!(live.contains("/api/live/:name"), "{live}");

    // main.rs mod 声明
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(main.contains("mod live;"), "{main}");

    // routes：live router 挂载 + CRUD handler publish 钩子
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(routes.contains("crate::live::router()"), "{routes}");
    assert!(
        routes.contains("crate::live::publish(\"items\", \"create\")"),
        "{routes}"
    );
    assert!(
        routes.contains("crate::live::publish(\"items\", \"update\")"),
        "{routes}"
    );
    assert!(
        routes.contains("crate::live::publish(\"items\", \"delete\")"),
        "{routes}"
    );

    // Cargo.toml tokio-stream 依赖（sync feature）
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(cargo.contains("tokio-stream"), "{cargo}");
    assert!(cargo.contains("sync"), "{cargo}");

    // 无 live 声明 → 零产物
    let dir2 = tmp_dir("zero");
    fs::write(
        dir2.join("forma.yaml"),
        "project:\n  name: live\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n",
    )
    .unwrap();
    fs::write(
        dir2.join("forma/app.fm"),
        "app live { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\n",
    )
    .unwrap();
    forma::build::run_build(&dir2).unwrap();
    assert!(!dir2.join("generated/backend/src/live.rs").exists());
}

#[test]
fn live_check_rejects_unknown_api_and_event() {
    let dir = tmp_dir("check");
    setup(&dir);
    fs::write(
        dir.join("forma/app.fm"),
        "app live { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\nlive Bad {\n api NoSuchApi\n events [create]\n}\nlive Bad2 {\n api Items\n events [explode]\n}\n",
    )
    .unwrap();
    let err = forma::build::run_build(&dir).expect_err("diagnostics must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("dsl-live-unknown-api"), "{msg}");
    assert!(msg.contains("dsl-live-unknown-event"), "{msg}");
}

/// 回归（生产项目实跑抓出）：live SSE 路由必须带 `{name}` 动态段，且 handler 的
/// filter_map 闭包括号必须自洽。结构断言 + 生成后端真实 cargo check 双门。
#[test]
fn live_router_uses_dynamic_name_segment_and_handler_is_balanced() {
    let dir = tmp_dir("route");
    setup(&dir);
    forma::build::run_build(&dir).unwrap();
    let live = fs::read_to_string(dir.join("generated/backend/src/live.rs")).unwrap();
    assert!(
        live.contains("\"/api/live/:name\""),
        "static route with Path extractor → 500; axum 0.7 needs `:name`: {live}"
    );
    assert!(
        !live.contains("\"/api/live/ItemFeed\""),
        "static route must be gone: {live}"
    );
    // match 表达式作为闭包体：只需一个 } 收尾，多余 } 是 Rust 语法错误
    assert!(
        !live.contains("            }\n        });"),
        "filter_map has an extra closing brace: {live}"
    );
    assert!(live.contains("            }\n        );"), "{live}");
}
