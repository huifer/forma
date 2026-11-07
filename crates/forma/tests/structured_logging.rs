//! 结构化日志回归测试（M10/P1）：RUST_LOG + LOG_FORMAT=json 生成契约。

use forma::emit::emit_rust_backend;
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
        "forma-log-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app logging {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
}
api Items {
  entity Item
  operations [list get create update delete]
}
"#;

fn generate(dir: &Path) -> Vec<forma::generators::GeneratedFile> {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: logging\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let (program, diags) = forma::dsl::load_program(dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    emit_rust_backend(dir, &program, "generated/backend", "logging").unwrap()
}

#[test]
fn generated_backend_has_structured_logging_configuration() {
    let dir = tmp_dir("config");
    let files = generate(&dir);
    let main = files
        .iter()
        .find(|f| f.path.ends_with("main.rs"))
        .unwrap()
        .content
        .clone();
    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("Cargo.toml"))
        .unwrap()
        .content
        .clone();
    let error = files
        .iter()
        .find(|f| f.path.ends_with("error.rs"))
        .unwrap()
        .content
        .clone();

    assert!(main.contains("EnvFilter::try_from_default_env()"), "{main}");
    assert!(main.contains("LOG_FORMAT"), "{main}");
    assert!(main.contains(".json()"), "{main}");
    assert!(cargo.contains("tracing-subscriber"), "{cargo}");
    assert!(
        cargo.contains("env-filter") && cargo.contains("json"),
        "{cargo}"
    );
    // Error / request / audit paths all use tracing with the same generated runtime.
    assert!(error.contains("tracing::error!"), "{error}");
}

#[test]
fn structured_logging_is_enabled_without_commands() {
    let dir = tmp_dir("no-command");
    let files = generate(&dir);
    let main = files
        .iter()
        .find(|f| f.path.ends_with("main.rs"))
        .unwrap()
        .content
        .clone();
    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("Cargo.toml"))
        .unwrap()
        .content
        .clone();
    assert!(main.contains("tracing_subscriber::fmt()"), "{main}");
    assert!(cargo.contains("tracing-subscriber"), "{cargo}");
}
