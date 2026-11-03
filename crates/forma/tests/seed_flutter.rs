//! gen-seed（确定性案例数据）与 gen-flutter（移动端适配层）的回归测试。
//!
//! 并行测试约定：凡写文件一律走 tmp_dir（唯一目录），绝不写 CWD 共享路径。

use forma::dsl::{lexer::lex, parser::parse};
use forma::emit::{emit_flutter, emit_mysql, emit_seed};
use forma::manifest::load_manifest;
use std::path::PathBuf;

fn school_program() -> (PathBuf, forma::dsl::Program) {
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

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forma-seedflutter-{}-{}-{:?}",
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

// ── gen-seed ────────────────────────────────────────────

#[test]
fn seed_is_deterministic_byte_for_byte() {
    let (_dir, program) = school_program();
    let a = emit_seed(&tmp_dir("det-a"), &program, "db/seed").unwrap();
    let b = emit_seed(&tmp_dir("det-b"), &program, "db/seed").unwrap();
    assert!(!a.is_empty());
    assert_eq!(a.len(), b.len());
    for (fa, fb) in a.iter().zip(b.iter()) {
        assert_eq!(fa.path, fb.path);
        assert_eq!(fa.content, fb.content, "non-deterministic: {}", fa.path);
    }
}

#[test]
fn seed_sql_orders_parents_before_children_and_is_idempotent() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("order");
    let files = emit_seed(&dir, &program, "db/seed").unwrap();
    let sql_file = files.iter().find(|f| f.path.ends_with(".sql")).unwrap();
    assert!(
        sql_file.path.ends_with("seed_0_3_0.sql"),
        "{}",
        sql_file.path
    );

    let sql = &sql_file.content;
    assert!(sql.contains("INSERT IGNORE INTO"));
    assert!(sql.contains("SET NAMES utf8mb4;"));
    // 拓扑序：schools 在 students/courses 之前，students/courses 在 enrollments 之前
    let pos = |t: &str| sql.find(&format!("INSERT IGNORE INTO {t} ")).unwrap();
    assert!(pos("schools") < pos("students"));
    assert!(pos("schools") < pos("courses"));
    assert!(pos("students") < pos("enrollments"));
    assert!(pos("courses") < pos("enrollments"));
    assert!(pos("enrollments") < pos("vote_records"));
}

#[test]
fn seed_respects_constraints_and_rules() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("constraints");
    let files = emit_seed(&dir, &program, "db/seed").unwrap();

    // students.sql：email 唯一、age 在 [6,80]、枚举合法
    let students = files
        .iter()
        .find(|f| f.path.ends_with("fixtures/students.json"))
        .unwrap();
    let rows: Vec<serde_json::Value> = {
        let body = students
            .content
            .lines()
            .filter(|l| !l.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str(&body).unwrap()
    };
    assert_eq!(rows.len(), 5);
    let mut emails = std::collections::HashSet::new();
    for r in &rows {
        let email = r["email"].as_str().unwrap();
        assert!(email.ends_with("@example.test"), "{email}");
        assert!(emails.insert(email.to_string()), "duplicate email {email}");
        if let Some(age) = r["age"].as_i64() {
            assert!((6..=80).contains(&age), "age {age} violates min/max");
        }
    }
    // 前 2 行填满可空字段，后 3 行可空字段为 NULL
    assert!(rows[0]["phone"].is_string());
    assert!(rows[2]["phone"].is_null());

    // enrollments：status 枚举合法且 check enrolledAt >= createdAt 成立
    let enrollments = files
        .iter()
        .find(|f| f.path.ends_with("fixtures/enrollments.json"))
        .unwrap();
    let body = enrollments
        .content
        .lines()
        .filter(|l| !l.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
    let valid = ["pending", "active", "completed", "cancelled"];
    for r in &rows {
        assert!(valid.contains(&r["status"].as_str().unwrap()));
        let enrolled = r["enrolled_at"].as_str().unwrap();
        let created = r["created_at"].as_str().unwrap();
        assert!(
            enrolled >= created,
            "check rule violated: {enrolled} < {created}"
        );
    }

    // FK 引用与父表主键一一对应
    let schools = files
        .iter()
        .find(|f| f.path.ends_with("fixtures/schools.json"))
        .unwrap();
    let body = schools
        .content
        .lines()
        .filter(|l| !l.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let schools: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
    let school_ids: std::collections::HashSet<String> = schools
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    for r in &rows_of(&files, "fixtures/students.json") {
        assert!(school_ids.contains(r["school_id"].as_str().unwrap()));
    }
}

fn rows_of(files: &[forma::generators::GeneratedFile], suffix: &str) -> Vec<serde_json::Value> {
    let f = files.iter().find(|f| f.path.ends_with(suffix)).unwrap();
    let body = f
        .content
        .lines()
        .filter(|l| !l.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&body).unwrap()
}

#[test]
fn seed_matches_baseline_schema_columns() {
    // 种子列集合必须与 0001 基线 DDL 的列一致（v0.3.0：students 含 phone）
    let (_dir, program) = school_program();
    let dir = tmp_dir("schema");
    let sql = emit_mysql(&dir, &program, "generated/db").unwrap();
    let ddl = &sql[0].content;
    let seed = emit_seed(&dir, &program, "db/seed").unwrap();
    let seed_sql = &seed
        .iter()
        .find(|f| f.path.ends_with(".sql"))
        .unwrap()
        .content;
    for col in [
        "id",
        "school_id",
        "name",
        "email",
        "phone",
        "age",
        "created_at",
    ] {
        assert!(ddl.contains(col), "DDL missing {col}");
    }
    assert!(seed_sql.contains("phone, age, created_at"), "{seed_sql}");
}

// ── gen-flutter ─────────────────────────────────────────

#[test]
fn flutter_emits_full_project_shape() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("flutter-shape");
    let files = emit_flutter(&dir, &program, "app_mobile", "school").unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    for expected in [
        "app_mobile/pubspec.yaml",
        "app_mobile/analysis_options.yaml",
        "app_mobile/lib/models/models.dart",
        "app_mobile/lib/api/api.dart",
        "app_mobile/lib/validation/validators.dart",
        "app_mobile/lib/ui/format.dart",
        "app_mobile/lib/main.dart",
        "app_mobile/lib/pages/student_list.dart",
        "app_mobile/lib/pages/student_edit.dart",
        "app_mobile/lib/pages/enrollment_edit.dart",
        "app_mobile/lib/pages/candidate_edit.dart",
    ] {
        assert!(paths.contains(&expected), "missing {expected}");
    }
}

#[test]
fn flutter_api_matches_backend_contract() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("flutter-api");
    let files = emit_flutter(&dir, &program, "app_mobile", "school").unwrap();
    let api = files
        .iter()
        .find(|f| f.path.ends_with("api/api.dart"))
        .unwrap();
    // 路由与 Rust 后端一致
    assert!(api.content.contains("'/students'"));
    assert!(api.content.contains("'/api'") || api.content.contains("kApiBase"));
    // 查询参数契约：page/page_size/q
    assert!(api.content.contains("'page_size'"));
    assert!(api.content.contains("'q'"));
    // VoteRecord 只有 list/get/create（无 update/delete）
    let start = api.content.find("class VoteRecordApi").unwrap();
    let vote = &api.content[start..];
    let end = vote.find("\n}").unwrap_or(vote.len());
    let vote = &vote[..end];
    assert!(vote.contains("create("));
    assert!(!vote.contains("update("));
    assert!(!vote.contains("remove("));
}

#[test]
fn flutter_models_use_camel_fields_with_snake_keys() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("flutter-models");
    let files = emit_flutter(&dir, &program, "app_mobile", "school").unwrap();
    let models = files
        .iter()
        .find(|f| f.path.ends_with("models/models.dart"))
        .unwrap();
    // 实体字段 camelCase
    assert!(models.content.contains("final String schoolId;"));
    assert!(models.content.contains("final DateTime createdAt;"));
    assert!(models.content.contains("final int? age;"));
    // JSON 键保持 snake_case（serde 契约）
    assert!(models.content.contains("j['school_id'] as String"));
    assert!(models
        .content
        .contains("'created_at': createdAt.toIso8601String()"));
    // ListRow 显示列
    assert!(models.content.contains("final String? schoolName;"));
    // Create/Update 输入体用裸关系名（后端契约）
    assert!(models.content.contains("'school': school,"));
    // Decimal 保持 string（rust_decimal serde 契约）
    // 枚举 fromName 为 camelCase
    assert!(models
        .content
        .contains("EnrollmentStatus? enrollmentStatusFromName"));
}

#[test]
fn flutter_validators_compile_dsl_rules() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("flutter-validators");
    let files = emit_flutter(&dir, &program, "app_mobile", "school").unwrap();
    let validators = files
        .iter()
        .find(|f| f.path.ends_with("validation/validators.dart"))
        .unwrap();
    // rules：Student.age min 6（zh 消息）
    assert!(validators.content.contains("入学年龄不得小于 6 岁"));
    // check 规则：DateTime 无原生 >= → compareTo 编译
    assert!(validators.content.contains("compareTo"));
    assert!(validators.content.contains("报名时间不能早于创建时间"));
}

#[test]
fn flutter_output_is_deterministic() {
    let (_dir, program) = school_program();
    let a = emit_flutter(&tmp_dir("flutter-det-a"), &program, "app_mobile", "school").unwrap();
    let b = emit_flutter(&tmp_dir("flutter-det-b"), &program, "app_mobile", "school").unwrap();
    assert_eq!(a.len(), b.len());
    for (fa, fb) in a.iter().zip(b.iter()) {
        assert_eq!(fa.content, fb.content, "non-deterministic: {}", fa.path);
    }
}

#[test]
fn flutter_no_template_placeholders_leak() {
    let (_dir, program) = school_program();
    let dir = tmp_dir("flutter-leak");
    let files = emit_flutter(&dir, &program, "app_mobile", "school").unwrap();
    for f in &files {
        assert!(!f.content.contains("%%"), "placeholder leak in {}", f.path);
        assert!(
            !f.content.contains("__PAGE__"),
            "placeholder leak in {}",
            f.path
        );
    }
}

#[test]
fn dsl_parser_smoke_for_seed_fixtures_helper() {
    // 保证 parse 入口可用（防止上面 helper 重构后静默失效）
    let p = parse(&lex("app demo { version = \"0.1.0\" }")).program;
    assert_eq!(p.app.unwrap().version, "0.1.0");
}
