//! FastAPI target 回归测试（M10）：Python + FastAPI + SQLAlchemy 2.0 + Pydantic。

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
        "forma-fastapi-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app fastapi {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
  active Bool
}
api Items {
  entity Item
  operations [list get create update delete]
}
"#;

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: fastapi\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: python\n    framework: fastapi\n    out: generated/backend\n",
    ).unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn fastapi_target_generates_crud_project() {
    let dir = tmp_dir("crud");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"backend-fastapi"), "{names:?}");
    assert!(!names.contains(&"backend-rust"), "{names:?}");
    let pyproject = fs::read_to_string(dir.join("generated/backend/pyproject.toml")).unwrap();
    assert!(pyproject.contains("fastapi"), "{pyproject}");
    assert!(pyproject.contains("sqlalchemy"), "{pyproject}");
    let models = fs::read_to_string(dir.join("generated/backend/src/models.py")).unwrap();
    assert!(models.contains("class Item(Base)"), "{models}");
    assert!(models.contains("mapped_column"), "{models}");
    let schemas = fs::read_to_string(dir.join("generated/backend/src/schemas.py")).unwrap();
    assert!(schemas.contains("class CreateItem"), "{schemas}");
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.py")).unwrap();
    assert!(routes.contains("@router.get(\"/\")"), "{routes}");
    assert!(routes.contains("@router.post"), "{routes}");
    assert!(routes.contains("@router.put"), "{routes}");
    assert!(routes.contains("@router.delete"), "{routes}");
}

#[test]
fn fastapi_target_rejects_security_semantics() {
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
