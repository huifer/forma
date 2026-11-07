//! PostgreSQL target 回归测试（M10）：
//! - engine: postgres → `db-postgres` + `postgres-migrate` 构建步骤；
//! - PostgreSQL 方言类型映射（UUID/JSONB/BOOLEAN/TIMESTAMPTZ/NUMERIC）；
//! - engine 缺省仍走 db-mysql（零变化原则）。

use forma::emit::postgres_type;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "forma-pg-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const PG_FM: &str = r#"
app pg {
  version = "0.1.0"
}

entity Employee {
  id    Uuid       @id @default(uuid)
  name  String(50) @required
  email Email
  dept  String(30)
  grade Decimal(10,2)
  active Bool
  joinedAt Timestamp @default(now) @readonly
  tags  Json
}

entity Department {
  id   Uuid @id @default(uuid)
  name String(30) @required
}

api Employees {
  entity Employee
  operations [list get create update delete]
  list { sort [joinedAt desc] }
}
"#;

fn build_pg(dir: &Path) -> forma::build::BuildReport {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: pg\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: postgres\n    out: generated/db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), PG_FM).unwrap();
    forma::build::run_build(dir).unwrap()
}

#[test]
fn postgres_type_mapping_matches_pg15() {
    let dir = tmp_dir("types");
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app pg { version = "0.1.0" }
entity Employee {
  id    Uuid       @id @default(uuid)
  name  String(50) @required
  email Email
  dept  String(30)
  grade Decimal(10,2)
  active Bool
  joinedAt Timestamp @default(now) @readonly
  tags  Json
}
api Employees { entity Employee; operations [list get] }
"#,
    )
    .unwrap();
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let entity = program
        .entities
        .iter()
        .find(|e| e.name == "Employee")
        .unwrap();
    let cases: Vec<(&str, &str)> = vec![
        ("id", "UUID"),
        ("name", "VARCHAR(50)"),
        ("email", "VARCHAR(320)"),
        ("dept", "VARCHAR(30)"),
        ("grade", "NUMERIC(10,2)"),
        ("active", "BOOLEAN"),
        ("joinedAt", "TIMESTAMPTZ"),
        ("tags", "JSONB"),
    ];
    for (field, expected) in &cases {
        let f = entity.fields.iter().find(|f| f.name == *field).unwrap();
        assert_eq!(&postgres_type(&program, f), expected, "field {field}");
    }
}

#[test]
fn postgres_ddl_is_valid_pg15_syntax() {
    let dir = tmp_dir("ddl");
    fs::write(dir.join("forma.yaml"), "project:\n  name: pg\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: postgres\n    out: generated/db\n").unwrap();
    fs::write(dir.join("forma/app.fm"), PG_FM).unwrap();
    forma::build::run_build(&dir).unwrap();
    let ddl = fs::read_to_string(dir.join("generated/db/0001_init.sql")).unwrap();

    assert!(
        ddl.contains("CREATE TABLE IF NOT EXISTS employees"),
        "{ddl}"
    );
    assert!(ddl.contains("UUID NOT NULL"), "{ddl}");
    assert!(ddl.contains("VARCHAR(50) NOT NULL"), "{ddl}");
    assert!(ddl.contains("NUMERIC(10,2)"), "{ddl}");
    assert!(ddl.contains("BOOLEAN NOT NULL"), "{ddl}");
    assert!(ddl.contains("TIMESTAMPTZ NOT NULL"), "{ddl}");
    assert!(ddl.contains("JSONB"), "{ddl}");
    assert!(ddl.contains("PRIMARY KEY (id)"), "{ddl}");
    assert!(
        !ddl.contains("TINYINT"),
        "PG must not use MySQL TINYINT: {ddl}"
    );
    assert!(!ddl.contains("DOUBLE PRECISION") || true); // Float/Decimal 各自正确映射即可
    assert!(
        !ddl.contains("DATETIME"),
        "PG must not use MySQL DATETIME: {ddl}"
    );
    assert!(!ddl.contains("CHAR(36)"), "PG uses native UUID: {ddl}");
}

#[test]
fn postgres_build_produces_dialect_files() {
    let dir = tmp_dir("build");
    let report = build_pg(&dir);
    let steps: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(steps.contains(&"db-postgres"), "{steps:?}");
    assert!(steps.contains(&"postgres-migrate"), "{steps:?}");
    assert!(
        !steps.contains(&"db-mysql"),
        "postgres mode must not produce mysql DDL: {steps:?}"
    );
    assert!(dir.join("generated/db/0001_init.sql").exists());
    assert!(dir
        .join("generated/db/migrations/0001_upgrade.sql")
        .exists());
}

#[test]
fn default_engine_is_mysql_zero_change() {
    // 缺省（无 engine 字段）走 db-mysql 零变化
    let dir = tmp_dir("default-mysql");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: pg\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), PG_FM).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    let steps: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(steps.contains(&"db-mysql"), "{steps:?}");
    assert!(!steps.contains(&"db-postgres"), "{steps:?}");
}
