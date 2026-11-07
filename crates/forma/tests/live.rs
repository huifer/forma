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
    assert!(live.contains("/api/live/ItemFeed"), "{live}");

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
