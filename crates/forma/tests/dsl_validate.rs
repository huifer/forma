//! CLI/MCP 一致性与内建类型清单回归测试：
//! - **假绿修复**（RFC-0003 M4）：DSL 工程必须走 `check_program` 管线，
//!   带语义错误的工程**必须**报 error（此前退出码 0、0 diagnostics）。
//! - **单一权威类型清单**：`Json` 等类型必须同时在检查层与各生成层可用。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use forma::engine::{check_project, is_dsl_project, ProjectCheck};
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
        "forma-dslval-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: dslval\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const GOOD_FM: &str = r#"
app dslval {
  version = "0.1.0"
}

entity Order {
  id     Uuid   @id @default(uuid)
  amount Decimal(12,2) @required
}

api Orders {
  entity Order
  operations [list get create]
}
"#;

/// 语义错误：`meta` 引用了未登记的类型。
const BAD_FM: &str = r#"
app dslval {
  version = "0.1.0"
}

entity Order {
  id   Uuid @id @default(uuid)
  meta NoSuchType @required
}
"#;

// ── 1. 假绿修复 ────────────────────────────────────────────────

#[test]
fn dsl_project_with_semantic_error_is_reported() {
    let dir = tmp_dir("bad");
    write_project(&dir, BAD_FM);
    assert!(is_dsl_project(&dir), "project must be detected as DSL");

    let check = match check_project(&dir).unwrap() {
        ProjectCheck::Dsl(c) => c,
        ProjectCheck::Federal(_) => panic!("DSL project must not take the federal path"),
    };
    assert!(
        check.error_count() > 0,
        "DSL project with semantic errors must report errors (this was the false-green bug)"
    );
    assert!(
        check
            .diagnostics
            .iter()
            .any(|d| d.code == "dsl-unknown-type"),
        "expected dsl-unknown-type, got {:?}",
        check
            .diagnostics
            .iter()
            .map(|d| &d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn valid_dsl_project_reports_no_errors() {
    let dir = tmp_dir("good");
    write_project(&dir, GOOD_FM);
    let check = match check_project(&dir).unwrap() {
        ProjectCheck::Dsl(c) => c,
        ProjectCheck::Federal(_) => panic!("DSL project must not take the federal path"),
    };
    assert_eq!(
        check.error_count(),
        0,
        "unexpected errors: {:?}",
        check.diagnostics
    );
    assert_eq!(check.program.entities.len(), 1);
    assert_eq!(check.project_name, "dslval");
}

#[test]
fn federal_project_still_takes_federal_path() {
    // 联邦工程（无 `source`）→ 仍走语义图管线
    let dir = tmp_dir("federal");
    fs::create_dir_all(dir.join("specifications/schemas")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: fed\n  version: 0.1.0\n\nspecifications:\n  schemas:\n    - path: specifications/schemas/*.sql\n",
    )
    .unwrap();
    fs::write(
        dir.join("specifications/schemas/orders.sql"),
        "CREATE TABLE orders (id CHAR(36) PRIMARY KEY, amount DECIMAL(12,2) NOT NULL);\n",
    )
    .unwrap();
    assert!(!is_dsl_project(&dir));
    match check_project(&dir).unwrap() {
        ProjectCheck::Federal(_) => {}
        ProjectCheck::Dsl(_) => panic!("federal project must not take the DSL path"),
    }
}

// ── 2. 单一权威类型清单（Json） ──────────────────────────────────

#[test]
fn json_type_is_accepted_and_mapped_by_emitters() {
    let dir = tmp_dir("json");
    write_project(
        &dir,
        r#"
app dslval {
  version = "0.1.0"
}

entity Order {
  id   Uuid @id @default(uuid)
  meta Json @required
}

api Orders {
  entity Order
  operations [list get create]
}
"#,
    );
    let check = match check_project(&dir).unwrap() {
        ProjectCheck::Dsl(c) => c,
        ProjectCheck::Federal(_) => panic!("expected DSL path"),
    };
    assert_eq!(
        check.error_count(),
        0,
        "Json must be a known builtin type: {:?}",
        check.diagnostics
    );

    // 生成端必须给出映射（Rust: serde_json::Value；MySQL: JSON）
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/backend");
    forma::emit::emit_rust_backend(&loaded.dir, &program, &out.to_string_lossy(), "dslval")
        .unwrap();
    let models = fs::read_to_string(out.join("src/models.rs")).unwrap();
    assert!(
        models.contains("serde_json::Value"),
        "Rust model must use serde_json::Value:\n{models}"
    );
}

#[test]
fn min_max_on_non_numeric_type_is_rejected() {
    // min/max 只对数值类型有意义——否则生成端会产出无法编译的比较
    let dir = tmp_dir("minmax");
    write_project(
        &dir,
        r#"
app dslval {
  version = "0.1.0"
}

entity Order {
  id   Uuid @id @default(uuid)
  name String(50)
}

rules OrderRules {
  field Order.name {
    min 3 { zh "太短" en "too short" }
  }
}
"#,
    );
    let check = match check_project(&dir).unwrap() {
        ProjectCheck::Dsl(c) => c,
        ProjectCheck::Federal(_) => panic!("expected DSL path"),
    };
    assert!(
        check
            .diagnostics
            .iter()
            .any(|d| d.code == "dsl-invalid-constraint"),
        "expected dsl-invalid-constraint, got {:?}",
        check
            .diagnostics
            .iter()
            .map(|d| &d.code)
            .collect::<Vec<_>>()
    );
}

// ── 3. CLI ↔ MCP 不允许漂移 ─────────────────────────────────────

#[test]
fn cli_and_mcp_agree_on_dsl_errors() {
    // 治理铁律：CLI 与 MCP **必须**跑同一条管线。
    // 同一个带语义错误的 DSL 工程，两边都**必须**报 error。
    let dir = tmp_dir("drift");
    write_project(&dir, BAD_FM);

    let cli_errors = match check_project(&dir).unwrap() {
        ProjectCheck::Dsl(c) => c.error_count(),
        ProjectCheck::Federal(_) => panic!("expected DSL path"),
    };
    assert!(cli_errors > 0, "CLI 必须报出语义错误");

    let mcp = forma::mcp::call_tool(&dir, "forma_check", &serde_json::Map::new()).unwrap();
    let mcp_errors = mcp["errors"].as_u64().unwrap_or(0) as usize;
    assert_eq!(
        cli_errors, mcp_errors,
        "CLI 与 MCP 的错误数必须一致（不允许漂移）：CLI={cli_errors} MCP={mcp_errors}"
    );
    assert!(mcp_errors > 0, "MCP 必须报出语义错误");

    // 合法工程：两边都零错误
    let dir2 = tmp_dir("drift-ok");
    write_project(&dir2, GOOD_FM);
    let ok_cli = match check_project(&dir2).unwrap() {
        ProjectCheck::Dsl(c) => c.error_count(),
        ProjectCheck::Federal(_) => panic!("expected DSL path"),
    };
    let ok_mcp = forma::mcp::call_tool(&dir2, "forma_check", &serde_json::Map::new()).unwrap();
    assert_eq!(ok_cli, 0);
    assert_eq!(ok_mcp["errors"].as_u64().unwrap_or(1), 0);
}

#[test]
fn builtin_types_cover_all_emitter_mappings() {
    // 单一权威清单：生成层支持的类型必须在检查层也可用（否则用户写了类型却被拒绝）
    let dir = tmp_dir("types");
    write_project(&dir, GOOD_FM);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/backend");
    forma::emit::emit_rust_backend(&loaded.dir, &program, &out.to_string_lossy(), "dslval")
        .unwrap();
    // 每个内置类型都必须有 Rust 映射（缺一个就会落到兜底类型）
    // 参数化类型必须带合法参数（Decimal 必须是 (p,s)，否则 dsl-invalid-type-arg）
    for ty in forma::dsl::BUILTIN_TYPES {
        let decl = match ty {
            "String" => "String(50)",
            "Decimal" => "Decimal(12,2)",
            other => other,
        };
        let fm = format!(
            "app t {{ version = \"0.1.0\" }}\nentity E {{\n  id Uuid @id @default(uuid)\n  v {decl}\n}}\napi Es {{\n  entity E\n  operations [list get create]\n}}\n"
        );
        let d2 = tmp_dir("types-one");
        write_project(&d2, &fm);
        let l2 = forma::manifest::load_manifest(&d2).unwrap();
        let s2 = l2.manifest.source.clone().unwrap();
        let (p2, _) = forma::dsl::load_program(&l2.dir, &s2).unwrap();
        let diags = forma::dsl::check_program(&p2);
        let errs: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
        assert!(
            errs.is_empty(),
            "builtin type '{ty}' must pass the check layer: {errs:?}"
        );
    }
}

// ── 4. 类型参数边界（P1-2）─────────────────────────────────────

fn type_arg_codes(fm: &str) -> Vec<String> {
    let dir = tmp_dir("typearg");
    write_project(&dir, fm);
    match check_project(&dir).unwrap() {
        ProjectCheck::Dsl(c) => c.diagnostics.iter().map(|d| d.code.clone()).collect(),
        ProjectCheck::Federal(_) => panic!("expected DSL path"),
    }
}

fn entity_with(field_decl: &str) -> String {
    format!(
        r#"
app dslval {{ version = "0.1.0" }}

entity Order {{
  id   Uuid @id @default(uuid)
  {field_decl}
}}
"#
    )
}

#[test]
fn string_length_zero_is_rejected() {
    let codes = type_arg_codes(&entity_with("name String(0) @required"));
    assert!(
        codes.contains(&"dsl-invalid-type-arg".to_string()),
        "{codes:?}"
    );
}

#[test]
fn string_length_above_varchar_limit_is_rejected() {
    let codes = type_arg_codes(&entity_with("name String(99999)"));
    assert!(
        codes.contains(&"dsl-invalid-type-arg".to_string()),
        "{codes:?}"
    );
}

#[test]
fn decimal_scale_above_precision_is_rejected() {
    let codes = type_arg_codes(&entity_with("amount Decimal(2,5) @required"));
    assert!(
        codes.contains(&"dsl-invalid-type-arg".to_string()),
        "{codes:?}"
    );
}

#[test]
fn decimal_precision_above_mysql_limit_is_rejected() {
    let codes = type_arg_codes(&entity_with("amount Decimal(66,2) @required"));
    assert!(
        codes.contains(&"dsl-invalid-type-arg".to_string()),
        "{codes:?}"
    );
}

#[test]
fn decimal_requires_two_arguments() {
    let codes = type_arg_codes(&entity_with("amount Decimal(12) @required"));
    assert!(
        codes.contains(&"dsl-invalid-type-arg".to_string()),
        "{codes:?}"
    );
}

#[test]
fn non_parameterized_type_rejects_arguments() {
    let codes = type_arg_codes(&entity_with("qty Int(5) @required"));
    assert!(
        codes.contains(&"dsl-invalid-type-arg".to_string()),
        "{codes:?}"
    );
}

#[test]
fn valid_type_arguments_pass() {
    let codes = type_arg_codes(&entity_with("name String(100) @required"));
    assert!(
        !codes.contains(&"dsl-invalid-type-arg".to_string()),
        "String(100) must be valid: {codes:?}"
    );
    let codes = type_arg_codes(&entity_with("amount Decimal(12,2) @required"));
    assert!(
        !codes.contains(&"dsl-invalid-type-arg".to_string()),
        "Decimal(12,2) must be valid: {codes:?}"
    );
}

// ── 5. 属性形式 min/max 的类型校验（P2-5）────────────────────────

#[test]
fn attr_min_max_on_non_numeric_type_is_rejected() {
    let codes = type_arg_codes(&entity_with("name String(50) @min(3)"));
    assert!(
        codes.contains(&"dsl-invalid-constraint".to_string()),
        "{codes:?}"
    );
}

#[test]
fn attr_min_max_without_numeric_argument_is_rejected() {
    let codes = type_arg_codes(&entity_with("age Int @min"));
    assert!(
        codes.contains(&"dsl-invalid-constraint".to_string()),
        "{codes:?}"
    );
}

#[test]
fn attr_min_max_on_numeric_type_passes() {
    let codes = type_arg_codes(&entity_with("age Int? @min(6) @max(80)"));
    assert!(
        !codes.contains(&"dsl-invalid-constraint".to_string()),
        "{codes:?}"
    );
}
