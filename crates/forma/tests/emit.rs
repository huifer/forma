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

#[test]
fn mysql_emits_fks_indexes_and_decimal() {
    let (dir, program) = school_program();
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
    assert!(models.content.contains("pub school_id: Uuid"));
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
    let files = emit_web_react(&dir, &program, "generated/web", "school").unwrap();
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
