//! gen-rust 产物的**运行时契约**回归测试。
//!
//! 这些断言来自真实 MySQL 8 端到端调试中真实抓到的 bug：
//! SELECT 尾逗号（1064）、命名占位符（sqlx 运行时不支持）、
//! 搜索的三值逻辑（NULL 列 → 整行被过滤）、关系列名（school vs school_id）、
//! JOIN 表名大小写、CHAR(36) 与 Uuid 二进制不匹配、op 裁剪与导入冗余。
//!
//! cargo test 覆盖不到 SQL 语义，所以这些断言直接锁住生成源码的关键结构，
//! 配合 docs/case-protocol.zh-CN.md 的 E2E 用例一起构成双保险。

use forma::dsl::Program;
use forma::emit::emit_rust_backend;
use forma::manifest::load_manifest;
use std::path::PathBuf;

fn school_program() -> (PathBuf, Program) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/school")
        .canonicalize()
        .unwrap();
    let loaded = load_manifest(&dir).unwrap();
    let (program, diags) =
        forma::dsl::load_program(&loaded.dir, &loaded.manifest.source.clone().unwrap()).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    (loaded.dir, program)
}

fn backend_files() -> Vec<forma::generators::GeneratedFile> {
    let (_dir, program) = school_program();
    let dir = std::env::temp_dir().join(format!(
        "forma-backend-contract-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    emit_rust_backend(&dir, &program, "generated/backend", "school").unwrap()
}

fn file_ending(files: &[forma::generators::GeneratedFile], suffix: &str) -> String {
    files
        .iter()
        .find(|f| f.path.ends_with(suffix))
        .unwrap_or_else(|| panic!("missing {suffix}"))
        .content
        .clone()
}

#[test]
fn select_list_has_no_trailing_comma_and_keeps_newline() {
    // MySQL 1064：s.created_at,\nFROM ... → 尾逗号；s.created_at        FROM → 缺换行
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    assert!(
        repo.contains("s.created_at\n        FROM schools AS s"),
        "SELECT 列表必须以换行结尾且无尾逗号"
    );
    assert!(
        !repo.contains(",\n        FROM"),
        "SELECT 列表不得带尾逗号（MySQL 1064）"
    );
}

#[test]
fn where_clause_uses_positional_placeholders_only() {
    // sqlx::query() 运行时只支持位置占位符 ?（命名参数仅 query! 宏支持）
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    assert!(!repo.contains("?school"), "不得使用命名占位符 ?school");
    assert!(!repo.contains("?q IS NULL"), "不得使用命名占位符 ?q");
    assert!(repo.contains("? IS NULL OR s.school_id = ?"), "{repo}");
}

#[test]
fn search_is_or_semantics_with_coalesce() {
    // 三值逻辑：FALSE OR NULL = NULL → 整行被过滤（搜索全部落空）
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    assert!(
        repo.contains("COALESCE(s.email LIKE CONCAT('%', ?, '%'), 0) = 1"),
        "搜索列需 COALESCE 处理 NULL"
    );
    assert!(
        repo.contains("? IS NULL OR (COALESCE(s.email LIKE CONCAT('%', ?, '%'), 0) = 1 OR COALESCE(s.phone LIKE CONCAT('%', ?, '%'), 0) = 1)"),
        "多搜索字段应为 OR 语义"
    );
}

#[test]
fn relation_columns_use_id_suffix_in_sql_but_bare_name_in_input() {
    // DDL 里关系列是 school_id（gen-mysql）；输入体字段是裸名 school（Rust serde）
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    assert!(
        repo.contains("INSERT INTO students (id, school_id, name, email, phone, age, created_at)"),
        "INSERT 列名必须是 school_id"
    );
    assert!(
        repo.contains(".bind(input.school.clone())"),
        "绑定值取自输入体的裸关系名 school"
    );
    assert!(
        repo.contains("UPDATE students SET school_id = ?, name = ?, email = ?, phone = ?, age = ? WHERE id = ?"),
        "UPDATE SET 列名必须是 school_id"
    );
}

#[test]
fn join_table_names_match_mysql_ddl() {
    // LEFT JOIN Schools → MySQL(Linux) 表名大小写敏感 → 1064
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    assert!(
        repo.contains("LEFT JOIN schools AS t1 ON s.school_id = t1.id"),
        "JOIN 表名必须是 gen-mysql 的 snake_case 复数"
    );
    assert!(
        !repo.contains("LEFT JOIN Schools"),
        "不得使用原始实体名做表名"
    );
}

#[test]
fn uuid_columns_are_text_end_to_end() {
    // MySQL 列是 CHAR(36)；sqlx 的 Uuid 解码只支持 16 字节二进制 → 文本 UUID 走 String
    let files = backend_files();
    let models = file_ending(&files, "models.rs");
    assert!(models.contains("pub id: String"), "模型层 id 应为 String");
    assert!(
        models.contains("pub school_id: String"),
        "关系列应为 String"
    );
    let repo = file_ending(&files, "repo.rs");
    assert!(
        repo.contains("Uuid::new_v4().to_string()"),
        "生成主键应是文本 UUID"
    );
    assert!(repo.contains(".bind(id)"), "路径参数按文本绑定");
}

#[test]
fn repo_functions_follow_declared_ops() {
    // VoteRecord 只声明 [list get create] → 不应生成 update/delete 仓储函数
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    assert!(repo.contains("pub async fn insert_vote_record"));
    assert!(!repo.contains("pub async fn update_vote_record"));
    assert!(!repo.contains("pub async fn delete_vote_record"));
    assert!(!repo.contains("pub async fn get_vote_record"));
    // find_* 供 get/update/delete 复用
    assert!(repo.contains("pub async fn find_vote_record"));
}

#[test]
fn list_params_only_declare_q_when_search_exists() {
    // 无 search 字段的实体不应有 q 字段（否则 dead_code 警告）
    let files = backend_files();
    let repo = file_ending(&files, "repo.rs");
    let school_params = repo
        .split("pub struct SchoolListParams")
        .nth(1)
        .unwrap()
        .split("}")
        .next()
        .unwrap();
    assert!(!school_params.contains("pub q"), "{school_params}");
    let student_params = repo
        .split("pub struct StudentListParams")
        .nth(1)
        .unwrap()
        .split("}")
        .next()
        .unwrap();
    assert!(student_params.contains("pub q"), "{student_params}");
}

#[test]
fn routes_imports_and_guard_binding_are_conditional() {
    let files = backend_files();
    let routes = file_ending(&files, "routes.rs");
    // 无守卫的删除处理器：row 只用于存在性检查 → 不可有 unused 警告
    assert!(routes.contains("let _row = repo::find_school"));
    assert!(routes.contains("let row = repo::find_enrollment"));
    // 校验器走全限定路径 → 不需要 glob 导入
    assert!(!routes.contains("use crate::validators::*;"));
    assert!(routes.contains("crate::validators::validate_create_school"));
}

#[test]
fn order_by_supports_multiple_sort_fields() {
    // spec：sort [createdAt desc, amount] —— 逐项生成，不静默截断到第一个
    let dsl = r#"
app demo { version = "0.1.0" }
entity Item {
  id        Uuid      @id @default(uuid)
  amount    Decimal(12,2) @required
  createdAt Timestamp @default(now) @readonly
}
api Items {
  entity Item
  operations [list]
  list { sort [createdAt desc, amount] paginate offset }
}
"#;
    let program = forma::dsl::parser::parse(&forma::dsl::lexer::lex(dsl)).program;
    let dir = std::env::temp_dir().join(format!(
        "forma-sort-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let files = emit_rust_backend(&dir, &program, "generated/backend", "demo").unwrap();
    let repo = files.iter().find(|f| f.path.ends_with("repo.rs")).unwrap();
    assert!(
        repo.content
            .contains("ORDER BY s.created_at DESC, s.amount ASC"),
        "{}",
        repo.content
    );
}

#[test]
fn cursor_pagination_fails_loudly_not_silently() {
    // 设计宪法 6.2：emitter 遇到未识别构造显式报错，不静默跳过
    let dsl = r#"
app demo { version = "0.1.0" }
entity Item {
  id Uuid @id @default(uuid)
  amount Decimal(12,2) @required
}
api Items {
  entity Item
  operations [list]
  list { paginate cursor }
}
"#;
    let program = forma::dsl::parser::parse(&forma::dsl::lexer::lex(dsl)).program;
    let dir = std::env::temp_dir().join(format!(
        "forma-cursor-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let err = emit_rust_backend(&dir, &program, "generated/backend", "demo").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("cursor"), "{msg}");
    assert!(msg.contains("offset"), "错误信息应说明当前支持的范围");
}
