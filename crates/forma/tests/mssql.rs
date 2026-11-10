//! SQL Server target（轨道 B 重新排期）回归：独立 T-SQL 方言、engine 路由、MySQL 默认零变化。

use forma::emit::mssql_type;
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
        "forma-mssql-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app mssql {
  version = "0.1.0"
}

entity Item {
  id        Uuid       @id @default(uuid)
  name      String(50) @required
  amount    Decimal(10,2)
  active    Bool
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
            "project:\n  name: mssql\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: {engine}\n    out: generated/db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn mssql_type_mapping_uses_tsql_types() {
    let dir = tmp_dir("types");
    setup(&dir, "mssql");
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let e = &program.entities[0];
    let expected = [
        ("id", "UNIQUEIDENTIFIER"),
        ("name", "NVARCHAR(50)"),
        ("amount", "DECIMAL(10,2)"),
        ("active", "BIT"),
        ("createdAt", "DATETIME2"),
        ("metadata", "NVARCHAR(MAX)"),
        ("note", "NVARCHAR(MAX)"),
    ];
    for (field, ty) in expected {
        let f = e.fields.iter().find(|f| f.name == field).unwrap();
        assert_eq!(mssql_type(&program, f), ty, "{field}");
    }
}

#[test]
fn mssql_build_uses_independent_steps_and_dialect() {
    let dir = tmp_dir("build");
    setup(&dir, "mssql");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"db-mssql"), "{names:?}");
    assert!(names.contains(&"mssql-migrate"), "{names:?}");
    assert!(!names.contains(&"db-mysql"), "{names:?}");

    let sql = fs::read_to_string(dir.join("generated/db/0001_init.sql")).unwrap();
    // T-SQL 幂等建表（OBJECT_ID 守卫）
    assert!(sql.contains("IF OBJECT_ID(N'dbo.items', N'U') IS NULL"), "{sql}");
    assert!(sql.contains("CREATE TABLE dbo.items"), "{sql}");
    // 方言映射
    assert!(sql.contains("[name] NVARCHAR(50) NOT NULL"), "{sql}");
    assert!(sql.contains("[amount] DECIMAL(10,2)"), "{sql}");
    assert!(sql.contains("[active] BIT"), "{sql}");
    assert!(sql.contains("[metadata] NVARCHAR(MAX)"), "{sql}");
    // 迁移链同构
    let up = fs::read_to_string(dir.join("generated/db/migrations/0001_upgrade.sql")).unwrap();
    assert!(up.contains("IF OBJECT_ID(N'dbo.items', N'U') IS NULL"), "{up}");
}

#[test]
fn mssql_relations_render_fk_and_name_columns() {
    // 关系图测试夹具（Student/Enrollment）：关系列 UNIQUEIDENTIFIER + FK REFERENCES
    let dir = tmp_dir("fk");
    setup(&dir, "mssql");
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app mssql { version = "0.1.0" }
entity Student {
  id   Uuid @id @default(uuid)
  name String(50) @required
}
entity Enrollment {
  id      Uuid @id @default(uuid)
  student Student @required
}
api Enrollments { entity Enrollment operations [list get create update delete] }
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let sql = fs::read_to_string(dir.join("generated/db/0001_init.sql")).unwrap();
    assert!(sql.contains("[student_id] UNIQUEIDENTIFIER NOT NULL"), "{sql}");
    assert!(
        sql.contains("REFERENCES dbo.students ([id])"),
        "{sql}"
    );
}

#[test]
fn default_engine_remains_mysql_zero_change() {
    let dir = tmp_dir("default");
    setup(&dir, "mysql");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"db-mysql"), "{names:?}");
    assert!(!names.contains(&"db-mssql"), "{names:?}");
}
