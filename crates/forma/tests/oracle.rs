//! Oracle target（轨道 B 重新排期）回归：独立 Oracle 方言、engine 路由、默认零变化。

use forma::emit::oracle_type;
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
        "forma-oracle-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app oracle {
  version = "0.1.0"
}

entity Item {
  id        Uuid       @id @default(uuid)
  name      String(50) @required
  amount    Decimal(10,2)
  active    Bool
  count     Int
  createdAt Timestamp  @default(now) @readonly
  metadata  Json
  note      Text
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
            "project:\n  name: oracle\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: {engine}\n    out: generated/db\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn oracle_type_mapping_uses_oracle_types() {
    let dir = tmp_dir("types");
    setup(&dir, "oracle");
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let e = &program.entities[0];
    let expected = [
        ("id", "VARCHAR2(36)"),
        ("name", "VARCHAR2(50)"),
        ("amount", "NUMBER(10,2)"),
        ("active", "NUMBER(1)"),
        ("count", "NUMBER(10)"),
        ("createdAt", "TIMESTAMP"),
        ("metadata", "CLOB"),
        ("note", "CLOB"),
    ];
    for (field, ty) in expected {
        let f = e.fields.iter().find(|f| f.name == field).unwrap();
        assert_eq!(oracle_type(&program, f), ty, "{field}");
    }
}

#[test]
fn oracle_build_uses_independent_steps_and_dialect() {
    let dir = tmp_dir("build");
    setup(&dir, "oracle");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"db-oracle"), "{names:?}");
    assert!(names.contains(&"oracle-migrate"), "{names:?}");
    assert!(!names.contains(&"db-mysql"), "{names:?}");
    let sql = fs::read_to_string(dir.join("generated/db/0001_init.sql")).unwrap();
    assert!(sql.contains("EXECUTE IMMEDIATE"), "{sql}");
    assert!(sql.contains("SQLCODE != -955"), "{sql}");
    assert!(sql.contains("CREATE TABLE items"), "{sql}");
    assert!(sql.contains("name VARCHAR2(50) NOT NULL"), "{sql}");
    assert!(sql.contains("amount NUMBER(10,2)"), "{sql}");
    let up = fs::read_to_string(dir.join("generated/db/migrations/0001_upgrade.sql")).unwrap();
    assert!(up.contains("EXECUTE IMMEDIATE"), "{up}");
}

#[test]
fn mysql_default_has_no_oracle_artifacts() {
    let dir = tmp_dir("default");
    setup(&dir, "mysql");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"db-mysql"), "{names:?}");
    assert!(!names.contains(&"db-oracle"), "{names:?}");
}
