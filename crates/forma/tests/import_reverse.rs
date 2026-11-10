//! SQL DDL / Prisma schema 逆向导入回归（roadmap M7 / M10，轨道 A-8）。
//!
//! 验收：生成的 `.fm` 必须通过 `load_program` + `check_program`（不是只看字符串），
//! 且关键映射（关系 / 枚举 / @id / 索引 / 显式跳过）正确。

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-import-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn assert_valid_dsl(dsl: &str) {
    let dir = tmp("validate");
    fs::write(dir.join("app.fm"), dsl).unwrap();
    let (program, parse_diags) = forma::dsl::load_program(&dir, &["*.fm".to_string()]).unwrap();
    let errors: Vec<_> = parse_diags
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "parse errors: {errors:?}\n---\n{dsl}");
    let sem = forma::dsl::check_program(&program);
    let sem_errors: Vec<_> = sem.iter().filter(|d| d.severity == "error").collect();
    assert!(
        sem_errors.is_empty(),
        "check_program errors: {sem_errors:?}\n---\n{dsl}"
    );
}

const SQL: &str = r#"
CREATE TABLE schools (
  id CHAR(36) NOT NULL PRIMARY KEY,
  name VARCHAR(100) NOT NULL,
  city VARCHAR(50),
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE students (
  id CHAR(36) NOT NULL PRIMARY KEY,
  school_id CHAR(36) NOT NULL,
  name VARCHAR(50) NOT NULL,
  email VARCHAR(120) NOT NULL UNIQUE,
  age INT,
  gpa DECIMAL(4,2),
  meta JSON,
  blob BLOB,
  FOREIGN KEY (school_id) REFERENCES schools(id)
);
CREATE INDEX idx_students_name ON students (name);
"#;

#[test]
fn sql_ddl_import_generates_valid_dsl_with_relations() {
    let dsl = forma::adapters::sql_import::sql_ddl_to_dsl(SQL, "schema.sql").unwrap();
    assert_valid_dsl(&dsl);
    assert!(dsl.contains("entity School {"), "{dsl}");
    assert!(dsl.contains("entity Student {"), "{dsl}");
    assert!(dsl.contains("id Uuid @id @default(uuid)"), "{dsl}");
    assert!(dsl.contains("school School @required"), "{dsl}");
    assert!(dsl.contains("email String(120) @required @unique"), "{dsl}");
    assert!(dsl.contains("gpa Decimal(4,2)"), "{dsl}");
    assert!(dsl.contains("meta Json"), "{dsl}");
    assert!(dsl.contains("index [name]"), "{dsl}");
    assert!(dsl.contains("api Schools {"), "{dsl}");
    assert!(dsl.contains("city String(50)?"), "{dsl}");
    assert!(dsl.contains("age Int?"), "{dsl}");
    // BLOB 无法保守映射 → 不作为字段出现，但显式列在注释里
    assert!(!dsl.contains("\n  blob "), "{dsl}");
    assert!(dsl.contains("students.blob"), "{dsl}");
    // 未支持类型不静默编造
    assert!(dsl.contains("以下构造未映射"), "{dsl}");
}

#[test]
fn sql_ddl_import_rejects_empty_input_and_name_collisions() {
    assert!(forma::adapters::sql_import::sql_ddl_to_dsl("", "empty.sql").is_err());
    let collision =
        "CREATE TABLE users (id INT PRIMARY KEY);\nCREATE TABLE user (id INT PRIMARY KEY);\n";
    let e = forma::adapters::sql_import::sql_ddl_to_dsl(collision, "x.sql").unwrap_err();
    assert!(e.to_string().contains("both map to entity"), "{e}");
}

const PRISMA: &str = r#"
datasource db {
  provider = "mysql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

enum Grade {
  a
  b
}

model School {
  id       String    @id @default(uuid())
  name     String
  city     String?
  students Student[]
}

model Student {
  id       String  @id @default(uuid())
  schoolId String
  school   School  @relation(fields: [schoolId], references: [id])
  email    String  @unique
  age      Int?
  score    Decimal
  grade    Grade
  blob     Bytes?
  @@index([email, age])
}
"#;

#[test]
fn prisma_import_generates_valid_dsl_with_enum_and_relation() {
    let dsl = forma::adapters::prisma_import::prisma_to_dsl(PRISMA, "schema.prisma").unwrap();
    assert_valid_dsl(&dsl);
    assert!(dsl.contains("enum Grade {"), "{dsl}");
    assert!(dsl.contains("entity School {"), "{dsl}");
    assert!(dsl.contains("entity Student {"), "{dsl}");
    assert!(dsl.contains("school School @required"), "{dsl}");
    assert!(dsl.contains("score Decimal(10,2) @required"), "{dsl}");
    assert!(dsl.contains("grade Grade @required"), "{dsl}");
    assert!(dsl.contains("index [email, age]"), "{dsl}");
    assert!(dsl.contains("city String(255)?"), "{dsl}");
    assert!(dsl.contains("api Students {"), "{dsl}");
    // 反向列表关系 / Bytes 不作为字段出现（但显式列在注释里）
    assert!(!dsl.contains("students Student[]"), "{dsl}");
    assert!(!dsl.contains("\n  blob "), "{dsl}");
    assert!(dsl.contains("Student.blob"), "{dsl}");
    assert!(!dsl.contains("schoolId"), "{dsl}");
    assert!(dsl.contains("reverse list relation"), "{dsl}");
}

#[test]
fn prisma_import_rejects_schema_without_models() {
    assert!(forma::adapters::prisma_import::prisma_to_dsl("datasource db {}", "x.prisma").is_err());
}

#[test]
fn mysql_table_key_constraints_roundtrip_from_forma_own_ddl() {
    // 回归（2026-11-10 验证实跑抓出）：Forma 自产 MySQL DDL 每表都带
    // `UNIQUE KEY uq_*` / `KEY idx_*` / `FULLTEXT KEY ft_* WITH PARSER ngram`，
    // 表内 KEY 约束曾让 `forma import sql` 直接硬错误（expected "(" got KEY）。
    // 现在三种约束必须可保守映射：UNIQUE(单列)→@unique、KEY→index、FULLTEXT→searchable。
    let ddl = r#"CREATE TABLE articles (
  id CHAR(36) NOT NULL,
  school_id CHAR(36) NOT NULL,
  title VARCHAR(200) NOT NULL,
  slug VARCHAR(120) NOT NULL,
  body TEXT,
  views BIGINT,
  PRIMARY KEY (id),
  UNIQUE KEY uq_articles_slug (slug),
  KEY idx_articles_school_id_title (school_id, title),
  FULLTEXT KEY ft_articles_title_body (title, body) WITH PARSER ngram,
  CONSTRAINT fk_articles_school_id FOREIGN KEY (school_id) REFERENCES schools (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
CREATE TABLE schools (
  id CHAR(36) NOT NULL PRIMARY KEY,
  name VARCHAR(100) NOT NULL
);
"#;
    let (dsl, report) =
        forma::adapters::sql_import::sql_ddl_to_dsl_with_report(ddl, "0001_init.sql").unwrap();
    // UNIQUE KEY（单列）→ @unique
    assert!(dsl.contains("slug String(120) @required @unique"), "{dsl}");
    // 表内 KEY → index（school_id 经关系映射为字段 school）
    assert!(dsl.contains("index [school, title]"), "{dsl}");
    // FULLTEXT KEY → searchable（与 Forma searchable 契约同形）
    assert!(dsl.contains("searchable [title, body]"), "{dsl}");
    // CONSTRAINT ... FOREIGN KEY → 关系字段
    assert!(dsl.contains("school School @required"), "{dsl}");
    // 全部构造可表达 → 零损失
    assert!(
        report.items.is_empty(),
        "unexpected losses: {:?}",
        report.items
    );
    // 生成物必须通过语义检查（不产生孤儿 index / 非法 searchable）
    let dir = std::env::temp_dir().join(format!(
        "forma-sqlimport-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("forma")).unwrap();
    std::fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: sqlrt\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    std::fs::write(dir.join("forma/app.fm"), &dsl).unwrap();
    let (program, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".to_string()]).unwrap();
    assert!(
        diags.iter().all(|d| d.severity != "error"),
        "imported DSL has semantic errors: {diags:?}"
    );
    assert_eq!(program.entities.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mysql_composite_unique_key_is_reported_not_dropped() {
    let ddl = r#"CREATE TABLE enrollments (
  id CHAR(36) NOT NULL PRIMARY KEY,
  student_id CHAR(36) NOT NULL,
  term VARCHAR(20) NOT NULL,
  UNIQUE KEY uq_enrollments_student_term (student_id, term)
);
"#;
    let (_, report) =
        forma::adapters::sql_import::sql_ddl_to_dsl_with_report(ddl, "x.sql").unwrap();
    assert!(
        report
            .items
            .iter()
            .any(|i| i.construct.contains("UNIQUE(student_id, term)")),
        "{report:?}"
    );
}
