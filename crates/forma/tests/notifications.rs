//! 通知派发（@notify）能力回归测试（M10/P1）。
//! 覆盖：DSL 属性解析（括号参数）、检查诊断、Rust 后端 notify.rs 生成、command handler 触发。

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
        "forma-notify-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: notify\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: rust\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        "app notify { version = \"0.1.0\" }\nentity Order {\n id Uuid @id @default(uuid)\n total Decimal(10,2)\n paid Bool\n email Email @required @notify(email, \"ops@example.test\", OrderPaid)\n}\napi Orders {\n entity Order\n operations [list get create update delete]\n}\napi OrderPaid {\n  POST \"/orders/{id}/pay\"\n  label { zh \"支付\" en \"Pay\" }\n  effect { set Order.paid = true }\n}\n",
    )
    .unwrap();
}

#[test]
fn notify_generates_module_and_handler_hook() {
    let dir = tmp_dir("generate");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-rust"), "{names:?}");

    // src/notify.rs 生成 + 规则静态展开
    assert!(dir.join("generated/backend/src/notify.rs").exists());
    let notify = fs::read_to_string(dir.join("generated/backend/src/notify.rs")).unwrap();
    assert!(notify.contains("NotifyRule"), "{notify}");
    assert!(notify.contains("\"Order\""), "{notify}");
    assert!(notify.contains("\"OrderPaid\""), "{notify}");
    assert!(notify.contains("\"email\""), "{notify}");
    assert!(notify.contains("ops@example.test"), "{notify}");
    assert!(notify.contains("pub fn fire"), "{notify}");

    // main.rs mod 声明
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(main.contains("mod notify;"), "{main}");

    // command handler 成功后触发
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(
        routes.contains("crate::notify::fire(\"Order\", \"OrderPaid\", &id);"),
        "{routes}"
    );

    // 无通知实体 → 零产物
    let dir2 = tmp_dir("zero");
    setup(&dir2);
    fs::write(
        dir2.join("forma/app.fm"),
        "app notify { version = \"0.1.0\" }\nentity Order {\n id Uuid @id @default(uuid)\n total Decimal(10,2)\n}\napi Orders {\n entity Order\n operations [list get create update delete]\n}\n",
    )
    .unwrap();
    forma::build::run_build(&dir2).unwrap();
    assert!(!dir2.join("generated/backend/src/notify.rs").exists());
}

#[test]
fn notify_check_rejects_unknown_channel_and_event() {
    let dir = tmp_dir("check");
    setup(&dir);
    fs::write(
        dir.join("forma/app.fm"),
        "app notify { version = \"0.1.0\" }\nentity Order {\n id Uuid @id @default(uuid)\n total Decimal(10,2)\n a String(10) @notify(fax, \"x\", OrderPaid)\n b String(10) @notify(email, \"y\", NoSuchCmd)\n c String(10) @notify(webhook)\n}\napi Orders {\n entity Order\n operations [list get create update delete]\n}\napi OrderPaid {\n  POST \"/orders/{id}/pay\"\n  effect { set Order.paid = true }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        "app notify { version = \"0.1.0\" }\nentity Order {\n id Uuid @id @default(uuid)\n total Decimal(10,2)\n paid Bool\n a String(10) @notify(fax, \"x\")\n b String(10) @notify(email, \"y\", NoSuchCmd)\n c String(10) @notify(webhook)\n}\napi Orders {\n entity Order\n operations [list get create update delete]\n}\napi OrderPaid {\n  POST \"/orders/{id}/pay\"\n  effect { set Order.paid = true }\n}\n",
    )
    .unwrap();
    let err = forma::build::run_build(&dir).expect_err("diagnostics must fail the build");
    let msg = format!("{err:#}");
    assert!(msg.contains("dsl-notify-unknown-channel"), "{msg}");
    assert!(msg.contains("dsl-notify-missing-target"), "{msg}");
    assert!(msg.contains("dsl-notify-unknown-event"), "{msg}");
}
