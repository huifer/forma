//! SQLite target 回归测试（M10）：独立 SQLite 方言、engine 路由、MySQL 默认零变化。

use forma::emit::sqlite_type;
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
        "forma-sqlite-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app sqlite {
  version = "0.1.0"
}

entity Item {
  id        Uuid       @id @default(uuid)
  name      String(50) @required
  amount    Decimal(10,2)
  active    Bool
  createdAt Timestamp  @default(now) @readonly
  metadata  Json
}

api Items {
  entity Item
  operations [list get create update delete]
}
"#;

fn setup(dir: &Path, engine: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: sqlite\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: {engine}\n    out: generated/db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn sqlite_type_mapping_uses_sqlite_affinity() {
    let dir = tmp_dir("types");
    setup(&dir, "sqlite");
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let e = &program.entities[0];
    let expected = [
        ("id", "TEXT"),
        ("name", "TEXT"),
        ("amount", "NUMERIC"),
        ("active", "INTEGER"),
        ("createdAt", "TEXT"),
        ("metadata", "TEXT"),
    ];
    for (field, ty) in expected {
        let f = e.fields.iter().find(|f| f.name == field).unwrap();
        assert_eq!(sqlite_type(&program, f), ty, "{field}");
    }
}

#[test]
fn sqlite_build_uses_independent_steps_and_dialect() {
    let dir = tmp_dir("build");
    setup(&dir, "sqlite");
    let report = forma::build::run_build(&dir).unwrap();
    let steps: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(steps.contains(&"db-sqlite"), "{steps:?}");
    assert!(steps.contains(&"sqlite-migrate"), "{steps:?}");
    assert!(!steps.contains(&"db-mysql"), "{steps:?}");
    let ddl = fs::read_to_string(dir.join("generated/db/0001_init.sql")).unwrap();
    assert!(ddl.contains("PRAGMA foreign_keys = ON"), "{ddl}");
    assert!(ddl.contains("TEXT NOT NULL"), "{ddl}");
    assert!(ddl.contains("NUMERIC"), "{ddl}");
    assert!(ddl.contains("INTEGER NOT NULL"), "{ddl}");
    assert!(!ddl.contains("DATETIME"), "{ddl}");
    assert!(!ddl.contains("JSONB"), "{ddl}");
    assert!(!ddl.contains("TINYINT"), "{ddl}");
}

#[test]
fn mysql_engine_remains_default_route() {
    let dir = tmp_dir("mysql");
    setup(&dir, "mysql");
    let report = forma::build::run_build(&dir).unwrap();
    let steps: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(steps.contains(&"db-mysql"), "{steps:?}");
    assert!(!steps.contains(&"db-sqlite"), "{steps:?}");
}
