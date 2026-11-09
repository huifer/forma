use forma::dsl::lexer::lex;
use forma::dsl::parser::parse;
use forma::emit::{diff_programs, emit_mysql, emit_openapi, emit_rust_backend, emit_web_react};
use forma::manifest::{load_manifest, LoadedManifest};
use std::path::PathBuf;

fn school_manifest() -> LoadedManifest {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/school")
        .canonicalize()
        .unwrap();
    load_manifest(&dir).unwrap()
}

fn school_program() -> (PathBuf, forma::dsl::Program) {
    let loaded = school_manifest();
    let (program, diags) =
        forma::dsl::load_program(&loaded.dir, &loaded.manifest.source.clone().unwrap()).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    (loaded.dir, program)
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forma-emit-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn mysql_emits_fks_indexes_and_decimal() {
    let (_dir, program) = school_program();
    // 写入独立 tmp：基线 0001_init.sql 不可变，不能污染 examples/school 的真实基线
    let dir = tmp_dir("baseline");
    let files = emit_mysql(&dir, &program, "generated/db").unwrap();
    assert_eq!(files.len(), 1);
    let sql = &files[0].content;
    // 三重关系外键
    assert_eq!(sql.matches("FOREIGN KEY").count(), 9, "{sql}");
    assert!(sql.contains("REFERENCES schools (id)"));
    assert!(sql.contains("BIGINT")); // capacity
    assert!(sql.contains("UNIQUE KEY"));
    assert!(sql.contains("VARCHAR(320)")); // Email
    assert!(sql.contains("NOT NULL") && sql.contains("schools"));

    // 基线不可变：后续构建不得覆写 0001_init.sql（升级走 gen-migrate 增量迁移）
    let again = emit_mysql(&dir, &program, "generated/db").unwrap();
    assert!(again.is_empty(), "baseline must not be rewritten");
}

#[test]
fn rust_backend_emits_compilable_shape() {
    let (dir, program) = school_program();
    let files = emit_rust_backend(&dir, &program, "generated/backend", "school").unwrap();
    assert!(files.len() >= 8);
    let models = files
        .iter()
        .find(|f| f.path.ends_with("models.rs"))
        .unwrap();
    assert!(models.content.contains("pub struct Student"));
    // Uuid 列在 MySQL 侧是 CHAR(36) → Rust 侧承载文本 UUID（sqlx 的 Uuid 仅支持 16 字节二进制）
    assert!(models.content.contains("pub school_id: String"));
    assert!(models.content.contains("pub id: String"));
    let validators = files
        .iter()
        .find(|f| f.path.ends_with("validators.rs"))
        .unwrap();
    // 规则编译：min 校验 + check 表达式
    assert!(
        validators.content.contains("age must be >= 6")
            || validators.content.contains("入学年龄不得小于 6 岁")
    );
    assert!(validators.content.contains("chrono::Utc::now()"));
    let routes = files
        .iter()
        .find(|f| f.path.ends_with("routes.rs"))
        .unwrap();
    assert!(routes.content.contains("/api/students"));
}

#[test]
fn web_emits_pages_and_validators() {
    let (dir, program) = school_program();
    let files = emit_web_react(
        &dir,
        &program,
        "generated/web",
        "school",
        false,
        false,
        "echarts",
    )
    .unwrap();
    let pages: Vec<&str> = files
        .iter()
        .filter_map(|f| f.path.split('/').next_back())
        .collect();
    assert!(pages.contains(&"StudentList.tsx"));
    assert!(pages.contains(&"EnrollmentEdit.tsx"));
    let api = files.iter().find(|f| f.path.ends_with("api.ts")).unwrap();
    assert!(api.content.contains("export const StudentApi"));
    assert!(api
        .content
        .contains("export const StudentsApi = StudentApi;"));
    let validators = files
        .iter()
        .find(|f| f.path.ends_with("validators.ts"))
        .unwrap();
    assert!(validators.content.contains("validateCreateEnrollment"));
}

#[test]
fn web_form_edit_maps_relation_id_to_bare_input_name() {
    // 编辑页初始值：实体响应是 `{rel}_id`，Create/Update 输入体是裸关系名 `{rel}`。
    // 后端 serde 两套键名体系——表单加载时必须显式映射，否则关系选择初始值为空。
    let (dir, program) = school_program();
    let files = emit_web_react(
        &dir,
        &program,
        "generated/web",
        "school",
        false,
        false,
        "echarts",
    )
    .unwrap();
    let edit = files
        .iter()
        .find(|f| f.path.ends_with("EnrollmentEdit.tsx"))
        .unwrap();
    assert!(
        edit.content.contains("mapped[\"student\"] = v[\"student_id\"]"),
        "{}",
        edit.content
    );
    assert!(
        edit.content.contains("mapped[\"course\"] = v[\"course_id\"]"),
        "{}",
        edit.content
    );
}

#[test]
fn openapi_emits_paths_and_schemas() {
    let (dir, program) = school_program();
    let files = emit_openapi(&dir, &program, "generated/api-docs").unwrap();
    let json = files
        .iter()
        .find(|f| f.path.ends_with("openapi.json"))
        .unwrap();
    assert!(json.content.contains("/api/students"));
    assert!(json.content.contains("/api/enrollments"));
}

#[test]
fn diff_detects_version_changes() {
    let old = r#"
app demo { version = "0.1.0" }
enum Status { pending approved }
entity Order {
  id Uuid @id @default(uuid)
  amount Decimal(12,2) @required
  status Status @default(pending)
}
"#;
    let new = r#"
app demo { version = "0.2.0" }
enum Status { pending approved rejected }
entity Order {
  id Uuid @id @default(uuid)
  amount Decimal(12,2) @required
  paymentStatus String? 
  status Status @default(pending)
}
"#;
    let old_p = parse(&lex(old)).program;
    let new_p = parse(&lex(new)).program;
    let report = diff_programs(&old_p, &new_p);
    assert_eq!(report.summary.modified, 2, "{report:?}");

    let order = report.entities.iter().find(|e| e.name == "Order").unwrap();
    assert_eq!(order.status, "modified");
    assert_eq!(
        order
            .fields_added
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["paymentStatus"]
    );
    let status = report.enums.iter().find(|e| e.name == "Status").unwrap();
    assert_eq!(status.variants_added, vec!["rejected"]);
}
