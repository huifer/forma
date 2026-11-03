use forma::dsl::lexer::lex;
use forma::dsl::parser::parse;
use forma::emit::diff_programs;
use forma::emit::migrate::diff_sql;
use forma::emit::mysql::plan_tables;

fn prog(src: &str) -> forma::dsl::Program {
    parse(&lex(src)).program
}

const BASE: &str = r#"
app school { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  email Email @required @unique
}
entity Course {
  id Uuid @id @default(uuid)
  title String(120) @required
}
entity Enrollment {
  id Uuid @id @default(uuid)
  student Student @required
  course Course @required
  enrolledAt Timestamp @default(now)
}
"#;

#[test]
fn version_upgrade_add_entity_and_relation() {
    let v1 = prog(BASE);
    let v2_src = r#"
app school { version = "0.2.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  email Email @required @unique
}
entity Course {
  id Uuid @id @default(uuid)
  title String(120) @required
}
entity Enrollment {
  id Uuid @id @default(uuid)
  student Student @required
  course Course @required
  enrolledAt Timestamp @default(now)
}
entity Election {
  id Uuid @id @default(uuid)
  title String(200) @required
}
entity Candidate {
  id Uuid @id @default(uuid)
  election Election @required
  student Student @required
  slogan String(500) @required
}
"#;
    let v2 = prog(v2_src);
    let report = diff_programs(&v1, &v2);
    // 新增 2 实体
    let added: Vec<&str> = report
        .entities
        .iter()
        .filter(|e| e.status == "added")
        .map(|e| e.name.as_str())
        .collect();
    assert!(added.contains(&"Election"), "{added:?}");
    assert!(added.contains(&"Candidate"), "{added:?}");
    // 迁移 SQL 包含 CREATE TABLE
    let old_plans = plan_tables(&v1);
    let new_plans = plan_tables(&v2);
    let sql = diff_sql(&old_plans, &new_plans);
    assert!(sql.contains("CREATE TABLE elections"), "{sql}");
    assert!(sql.contains("CREATE TABLE candidates"), "{sql}");
    assert!(sql.contains("FOREIGN KEY"), "{sql}");
}

#[test]
fn version_upgrade_drop_entity() {
    let full = prog(BASE);
    let reduced = prog(
        r#"
app school { version = "0.2.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  email Email @required @unique
}
"#,
    );
    let report = diff_programs(&full, &reduced);
    let removed: Vec<&str> = report
        .entities
        .iter()
        .filter(|e| e.status == "removed")
        .map(|e| e.name.as_str())
        .collect();
    assert!(removed.contains(&"Course"), "{removed:?}");
    assert!(removed.contains(&"Enrollment"), "{removed:?}");
    // 迁移 SQL
    let old_plans = plan_tables(&full);
    let new_plans = plan_tables(&reduced);
    let sql = diff_sql(&old_plans, &new_plans);
    assert!(sql.contains("DROP TABLE IF EXISTS courses"), "{sql}");
    assert!(sql.contains("DROP TABLE IF EXISTS enrollments"), "{sql}");
}

#[test]
fn version_upgrade_modify_column_type() {
    let v1 = prog(BASE);
    let v2 = prog(
        r#"
app school { version = "0.2.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(200) @required
  email Email @required @unique
}
entity Course {
  id Uuid @id @default(uuid)
  title String(120) @required
}
entity Enrollment {
  id Uuid @id @default(uuid)
  student Student @required
  course Course @required
  enrolledAt Timestamp @default(now)
}
"#,
    );
    let old_plans = plan_tables(&v1);
    let new_plans = plan_tables(&v2);
    let sql = diff_sql(&old_plans, &new_plans);
    assert!(sql.contains("MODIFY COLUMN name VARCHAR(200)"), "{sql}");
}

#[test]
fn version_diff_reports_field_changes() {
    let v1 = prog(BASE);
    let v2 = prog(&format!(
        "{BASE}\nentity Extra {{ id Uuid @id @default(uuid) }}"
    ));
    let report = diff_programs(&v1, &v2);
    assert_eq!(report.summary.added, 1, "{report:?}");
    assert!(!report.summary.unchanged);
}

#[test]
fn version_same_dsl_unchanged() {
    let p1 = prog(BASE);
    let p2 = prog(BASE);
    let report = diff_programs(&p1, &p2);
    assert!(report.summary.unchanged, "{report:?}");
}
