//! 规则跨语言一致性向量（P1-3 / F-004）。
//!
//! **向量 = 共享语义契约**：同一条 DSL 规则编译到 Rust / TS / Dart 三端，
//! 必须携带**相同的边界字面量与比较方向**。任何一端漂移（边界写错、方向反转、
//! 字面量类型歧义）都会让本测试变红。
//!
//! Rust 侧（本文件）断言三端生成物的字面量一致；
//! **行为**验证由 `validation_vectors.rs` 从 DSL 派生并生成两端测试文件——
//! `generated/web/src/validators.test.ts`（vitest）与
//! `app_mobile/test/rule_vector_test.dart`（flutter_test），在各自 CI 真实执行；
//! Rust（权威）的行为验证在 e2e 冒烟（校验失败 → 400）。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

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
        "forma-vectors-{tag}-{}-{seq}-{nanos}",
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
        "project:\n  name: vectors\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n  mobile:\n    framework: flutter\n    out: generated/mobile\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

/// F-004：行为向量测试文件由同一派生函数（validation_vectors.rs）生成，
/// 两端必须携带**同一组边界字面量**（同源保证）。
#[test]
fn generated_behavior_tests_are_same_source() {
    let dir = tmp_dir("same-source");
    write_project(&dir, &build_fm());
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let web = forma::emit::emit_web_react(&dir, &program, "generated/web", "vectors").unwrap();
    let mobile = forma::emit::emit_flutter(&dir, &program, "generated/mobile", "vectors").unwrap();

    let web_test = web
        .iter()
        .find(|f| f.path.ends_with("validators.test.ts"))
        .expect("web behavior vector test must be generated");
    let dart_test = mobile
        .iter()
        .find(|f| f.path.ends_with("rule_vector_test.dart"))
        .expect("dart behavior vector test must be generated");

    // 同源：两端用例数一致（每个 test( 出现一次）
    let web_cases = web_test.content.matches("test(").count();
    let dart_cases = dart_test.content.matches("test('V").count();
    assert!(
        web_cases > 0 && web_cases == dart_cases,
        "{web_cases} vs {dart_cases}"
    );

    // 边界字面量：派生自同一 DSL 约束（如 min 6 / max 500）
    for literal in ["6", "500", "0.01", "1"] {
        assert!(
            web_test.content.contains(literal) && dart_test.content.contains(literal),
            "boundary literal {literal} must appear in both generated tests"
        );
    }
}

/// 一条一致性向量：字段声明 + 三端生成物必须携带的语义片段。
struct Vector {
    name: &'static str,
    field_decl: &'static str,
    rust: &'static str,
    ts: &'static str,
    dart: &'static str,
}

/// 向量集（与参考应用 `app_mobile/test/rule_vector_test.dart` 的行为用例同源）。
const VECTORS: &[Vector] = &[
    // Int.min（可空）：Rust 用 Option 解构；TS 用 ?? 0 兜底；Dart 用 ! 解构
    Vector {
        name: "int_min_optional",
        field_decl: "age Int? @min(6)",
        rust: "if let Some(v) = input.age { if v < 6 {",
        ts: "(input.age ?? 0) < 6",
        dart: "input.age! < 6",
    },
    // Int.max（必填）：三端同形直比
    Vector {
        name: "int_max_required",
        field_decl: "capacity Int @required @max(500)",
        rust: "input.capacity > 500",
        ts: "input.capacity > 500",
        dart: "input.capacity > 500",
    },
    // Int.min（必填）
    Vector {
        name: "int_min_required",
        field_decl: "qty Int @required @min(1)",
        rust: "input.qty < 1",
        ts: "input.qty < 1",
        dart: "input.qty < 1",
    },
    // Decimal.min：Rust 字面量**必须**显式类型——`Decimal > 0.01` 的字面量
    // 会产生多个候选 impl（i32/i64/…）→ 类型不明确，生成工程编译失败
    Vector {
        name: "decimal_min",
        field_decl: "price Decimal(12,2) @required @min(0.01)",
        rust: "rust_decimal::Decimal::try_from(0.01f64).unwrap_or_default()",
        ts: "Number(input.price ?? 0) < 0.01",
        dart: "(num.tryParse(input.price) ?? 0) < 0.01",
    },
    // required：三端同文案（i18n 消息来自 DSL）
    Vector {
        name: "required",
        field_decl: "title String(200) @required",
        rust: "必填 / required",
        ts: "必填 / required",
        dart: "必填 / required",
    },
];

fn build_fm() -> String {
    let fields: String = VECTORS
        .iter()
        .map(|v| format!("  {}\n", v.field_decl))
        .collect();
    format!(
        r#"
app vectors {{
  version = "0.1.0"
}}

entity Order {{
  id Uuid @id @default(uuid)
{fields}}}

api Orders {{
  entity Order
  operations [list get create update delete]
  list {{ sort [id] }}
}}

page OrderList {{
  route "/orders"
  title {{ zh "订单" en "Orders" }}
  table {{
    query   Orders.list
    columns [id]
    actions {{
      create → OrderEdit
      row    [edit → OrderEdit, delete → Orders.delete confirm]
    }}
  }}
}}

page OrderEdit {{
  route "/orders/{{id}}"
  title {{ zh "编辑订单" en "Edit order" }}
  form Order {{
    layout stack {{
      title Input @required
    }}
    submit → Orders.update redirect "/orders"
    submit create → Orders.create redirect "/orders"
  }}
}}
"#
    )
}

#[test]
fn rule_boundaries_are_consistent_across_targets() {
    let dir = tmp_dir("consistency");
    write_project(&dir, &build_fm());
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "vector DSL must be valid: {errors:?}");

    let out = dir.join("generated");
    forma::emit::emit_rust_backend(
        &loaded.dir,
        &program,
        &out.join("backend").to_string_lossy(),
        "vectors",
    )
    .unwrap();
    forma::emit::emit_web_react(
        &loaded.dir,
        &program,
        &out.join("web").to_string_lossy(),
        "vectors",
    )
    .unwrap();
    forma::emit::emit_flutter(
        &loaded.dir,
        &program,
        &out.join("mobile").to_string_lossy(),
        "vectors",
    )
    .unwrap();

    let rust = fs::read_to_string(out.join("backend/src/validators.rs")).unwrap();
    let ts = fs::read_to_string(out.join("web/src/validators.ts")).unwrap();
    let dart = fs::read_to_string(out.join("mobile/lib/validation/validators.dart")).unwrap();

    for v in VECTORS {
        assert!(
            rust.contains(v.rust),
            "vector `{}`: Rust boundary drifted.\n  expected: {}\n  rust:\n{}",
            v.name,
            v.rust,
            rust
        );
        assert!(
            ts.contains(v.ts),
            "vector `{}`: TS boundary drifted.\n  expected: {}\n  ts:\n{}",
            v.name,
            v.ts,
            ts
        );
        assert!(
            dart.contains(v.dart),
            "vector `{}`: Dart boundary drifted.\n  expected: {}\n  dart:\n{}",
            v.name,
            v.dart,
            dart
        );
    }
}

#[test]
fn decimal_literal_is_unambiguous_in_generated_code() {
    // 专项锁定：Decimal 与整数字面量比较会产生多候选 impl → 生成物**不得**出现
    // `Decimal::try_from(1)` 这类无类型标注形式（历史缺陷，见 P1-3 的连带修复）。
    let dir = tmp_dir("decimal");
    let fm = r#"
app vectors { version = "0.1.0" }

entity Order {
  id    Uuid   @id @default(uuid)
  price Decimal(12,2) @required @min(1)
}

api Orders {
  entity Order
  operations [list get create update delete]
}
"#;
    write_project(&dir, fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let backend = dir.join("generated/backend");
    forma::emit::emit_rust_backend(&loaded.dir, &program, &backend.to_string_lossy(), "vectors")
        .unwrap();
    let validators = fs::read_to_string(backend.join("src/validators.rs")).unwrap();
    assert!(
        validators.contains("rust_decimal::Decimal::try_from(1i64)"),
        "integer Decimal literal must be explicitly typed:\n{validators}"
    );
    assert!(
        !validators.contains("Decimal::try_from(1)"),
        "ambiguous Decimal literal must not appear:\n{validators}"
    );
}
