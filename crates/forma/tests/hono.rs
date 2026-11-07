//! Hono backend target 回归测试（M10）：轻量 Node/TypeScript CRUD 子集。

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
        "forma-hono-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app hono {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
}
api Items {
  entity Item
  operations [list get create update delete]
}
"#;

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: hono\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: typescript\n    framework: hono\n    out: generated/backend\n",
    ).unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn hono_target_generates_crud_project() {
    let dir = tmp_dir("crud");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"backend-hono"), "{names:?}");
    assert!(!names.contains(&"backend-rust"), "{names:?}");
    let package = fs::read_to_string(dir.join("generated/backend/package.json")).unwrap();
    assert!(package.contains("\"hono\""), "{package}");
    assert!(package.contains("\"mysql2\""), "{package}");
    let index = fs::read_to_string(dir.join("generated/backend/src/index.ts")).unwrap();
    assert!(index.contains("new Hono()"), "{index}");
    assert!(index.contains("app.get(\"/api/items\""), "{index}");
    assert!(index.contains("app.post(\"/api/items\""), "{index}");
    assert!(index.contains("app.put(\"/api/items/:id\""), "{index}");
    assert!(index.contains("app.delete(\"/api/items/:id\""), "{index}");
    let validators = fs::read_to_string(dir.join("generated/backend/src/validators.ts")).unwrap();
    assert!(validators.contains("createItemSchema"), "{validators}");
}

#[test]
fn hono_target_rejects_security_semantics() {
    let dir = tmp_dir("reject");
    setup(&dir);
    fs::write(
        dir.join("forma/app.fm"),
        FM.replace(
            "api Items {",
            "auth module { provider jwt roles [admin] }\n\napi Items {",
        ),
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("unsupported DSL features"), "{msg}");
    assert!(msg.contains("auth module"), "{msg}");
}
