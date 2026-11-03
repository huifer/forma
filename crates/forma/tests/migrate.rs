use forma::dsl::lexer::lex;
use forma::dsl::parser::parse;
use forma::emit::migrate::diff_sql;
use forma::emit::mysql::plan_tables;
use forma::emit::{diff_programs, emit_diagrams, emit_migration, emit_mysql};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forma-mig-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn program(src: &str) -> forma::dsl::Program {
    parse(&lex(src)).program
}

const V1: &str = r#"
app demo { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  tag String(20)?
  index [name]
}
"#;

const V2_ADD_FIELD: &str = r#"
app demo { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  phone String(20)?
  tag String(20)?
  index [name]
}
"#;

const V3_DROP_FIELD: &str = r#"
app demo { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  index [name]
}
"#;

const V4_DROP_TABLE: &str = r#"
app demo { version = "0.1.0" }
"#;

#[test]
fn migration_add_column_on_schema_change() {
    let dir = tmp_dir("addcol");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    let first = emit_migration(&dir, &p1, "generated/db").unwrap();
    assert!(first.is_none(), "首次构建只保存基线快照");

    let p2 = program(V2_ADD_FIELD);
    let second = emit_migration(&dir, &p2, "generated/db").unwrap();
    let file = second.expect("should generate upgrade migration");
    assert!(file.path.ends_with("0002_upgrade.sql"), "{}", file.path);
    assert!(
        file.content
            .contains("ALTER TABLE students ADD COLUMN phone VARCHAR(20) NULL;"),
        "{}",
        file.content
    );
}

#[test]
fn migration_drop_column() {
    let p1 = program(V1);
    let p3 = program(V3_DROP_FIELD);
    let old = forma::emit::mysql::plan_tables(&p1);
    let new = forma::emit::mysql::plan_tables(&p3);
    let sql = forma::emit::migrate::diff_sql(&old, &new);
    eprintln!(
        "DEBUG sql={:?} old={:?} new={:?}",
        sql.len(),
        old.iter()
            .map(|t| (t.table.clone(), t.columns.len()))
            .collect::<Vec<_>>(),
        new.iter()
            .map(|t| (t.table.clone(), t.columns.len()))
            .collect::<Vec<_>>()
    );
    assert!(!sql.is_empty(), "diff_sql returned empty");
}

#[test]
fn migration_drop_table() {
    let dir = tmp_dir("droptable");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();

    let p4 = program(V4_DROP_TABLE);
    let file = emit_migration(&dir, &p4, "generated/db")
        .unwrap()
        .expect("migration");
    assert!(
        file.content.contains("DROP TABLE IF EXISTS students;"),
        "{}",
        file.content
    );
}

#[test]
fn migration_modify_column_type() {
    let dir = tmp_dir("modcol");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();

    let v2 = V1.replace("String(50)", "String(100)");
    let p2 = program(&v2);
    let file = emit_migration(&dir, &p2, "generated/db")
        .unwrap()
        .expect("migration");
    assert!(
        file.content
            .contains("MODIFY COLUMN name VARCHAR(100) NOT NULL;"),
        "{}",
        file.content
    );
}

#[test]
fn no_migration_when_schema_unchanged() {
    let dir = tmp_dir("stable");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();
    let again = emit_migration(&dir, &p1, "generated/db").unwrap();
    assert!(again.is_none(), "schema unchanged → no migration");
}

// ── 可视化图表 ─────────────────────────────────────────

#[test]
fn diagrams_emit_er_dataflow_status() {
    let dir = tmp_dir("diagrams");
    let p = program(V1);
    let files = emit_diagrams(&dir, &p, "generated/diagrams").unwrap();
    assert!(files.len() >= 4);

    let er = files.iter().find(|f| f.path.ends_with("er.mmd")).unwrap();
    assert!(er.content.contains("erDiagram"));
    assert!(er.content.contains("Student {"), "{}", er.content);

    let dataflow = files
        .iter()
        .find(|f| f.path.ends_with("dataflow.mmd"))
        .unwrap();
    assert!(dataflow.content.contains("flowchart LR"));
    assert!(dataflow.content.contains("MySQL students"));

    let status = files.iter().find(|f| f.path.ends_with("status.mmd"));
    // V1 无枚举 → 无状态图（school 用例有 EnrollmentStatus）
    let _ = status;

    let html = files
        .iter()
        .find(|f| f.path.ends_with("diagrams.html"))
        .unwrap();
    assert!(html.content.contains("mermaid"));
}

#[test]
fn status_flow_includes_guard_notes() {
    let src = r#"
app demo { version = "0.1.0" }
enum Status { pending active completed }
entity Order {
  id Uuid @id @default(uuid)
  status Status @default(pending)
}
rules R {
  guard Order.delete {
    deny status == "completed"
      reason { zh "不可删" }
  }
}
"#;
    let p = program(src);
    // 独立 tmp 目录：并行测试下不能共用 CWD 的 generated/ 输出（文件竞态）
    let files = emit_diagrams(&tmp_dir("status-guard"), &p, "generated/diagrams").unwrap();
    let status = files
        .iter()
        .find(|f| f.path.ends_with("status.mmd"))
        .unwrap();
    assert!(status.content.contains("stateDiagram-v2"));
    // 变体名加枚举命名空间前缀，避免撞 Mermaid 关键字（note/state/end…）
    assert!(status.content.contains("[*] --> Status_pending"));
    assert!(status.content.contains("Status_pending --> Status_active"));
    assert!(status.content.contains("Status_active --> Status_completed"));
    assert!(status
        .content
        .contains("note right of Status_completed : completed 时不可 删除"));
}

/// 回归：变体名撞 Mermaid 关键字（note/state/end/direction）时，
/// 旧版直接输出裸名导致 `Syntax error in t…`，必须加枚举命名空间前缀。
#[test]
fn status_flow_namespaces_variants_that_collide_with_mermaid_keywords() {
    let src = r#"
app demo { version = "0.1.0" }
enum Note { note state end direction }
entity Ticket {
  id Uuid @id @default(uuid)
  status Note @default(note)
}
"#;
    let p = program(src);
    let files = emit_diagrams(&tmp_dir("status-kw"), &p, "generated/diagrams").unwrap();
    let status = files
        .iter()
        .find(|f| f.path.ends_with("status.mmd"))
        .unwrap();
    for kw in ["note", "state", "end", "direction"] {
        // 不得出现裸关键字作为状态名（即行内直接跟关键字且无前缀）
        let bare = format!("    {} --> {}\n", kw, kw);
        assert!(
            !status.content.contains(&bare),
            "bare mermaid keyword leaked: {kw}"
        );
        assert!(
            status.content.contains(&format!("Note_{kw}")),
            "namespaced state missing for {kw}"
        );
    }
    assert!(status.content.contains("[*] --> Note_note"));
}

#[test]
fn diff_reports_entity_changes() {
    let old = r#"
app demo { version = "0.1.0" }
entity A { id Uuid @id @default(uuid) name String(50) @required }
"#;
    let new = r#"
app demo { version = "0.1.0" }
entity A { id Uuid @id @default(uuid) name String(100) @required email Email @required }
"#;
    let report = diff_programs(&parse(&lex(old)).program, &parse(&lex(new)).program);
    assert_eq!(report.summary.modified, 1);
    let a = report.entities.iter().find(|e| e.name == "A").unwrap();
    assert!(a.fields_added.iter().any(|f| f.name == "email"));
    assert!(a
        .fields_modified
        .iter()
        .any(|m| m.name == "name" && m.new.contains("100")));
}

#[test]
fn diff_sql_direct_add_and_drop() {
    let v1 = "app demo { version = \"0.1.0\" }\nentity Student {\n  id Uuid @id @default(uuid)\n  name String(50) @required\n  tag String(20)?\n}";
    let v3 = "app demo { version = \"0.1.0\" }\nentity Student {\n  id Uuid @id @default(uuid)\n  name String(100) @required\n}";
    let old = plan_tables(&parse(&lex(v1)).program);
    let new = plan_tables(&parse(&lex(v3)).program);
    let sql = diff_sql(&old, &new);
    assert!(sql.contains("DROP COLUMN tag"), "{sql}");
    assert!(sql.contains("MODIFY COLUMN name VARCHAR(100)"), "{sql}");
}
