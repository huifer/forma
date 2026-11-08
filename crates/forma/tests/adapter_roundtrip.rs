//! RFC-0002 §14 S2/S3 / §16 / §17：互通保真度回归（P3/C-2）。
//!
//! - S2：Lossy / Partial 导入产出机读 `LossReport`，且损失项在 `.fm` 注释里可见；
//! - S3：OpenAPI → `.fm` → OpenAPI、SQL DDL → `.fm` → DDL 的**可表达部分**语义等价；
//! - §16：文件顺序变化不改动 Program、畸形输入产出诊断而非 panic；
//! - S5：构建报告携带 IR 版本（由 `tests/cli_version.rs` 与 build 回归覆盖）。
//!
//! 边界：导入是保守骨架，不保证 1:1；测试只断言可表达部分，损失部分由报告显式记录。

use forma::adapters::{openapi, prisma_import, sql_import};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("forma-roundtrip-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const SQL: &str = r#"
CREATE TABLE authors (
  id   CHAR(36) NOT NULL PRIMARY KEY,
  name VARCHAR(100) NOT NULL
);
CREATE TABLE books (
  id        CHAR(36) NOT NULL PRIMARY KEY,
  author_id CHAR(36) NOT NULL,
  title     VARCHAR(200) NOT NULL,
  price     DECIMAL(10,2),
  blob      BLOB,
  FOREIGN KEY (author_id) REFERENCES authors(id)
);
"#;

const PRISMA: &str = r#"
model Author {
  id    String  @id @default(uuid())
  name  String
  books Book[]
}
model Book {
  id       String @id @default(uuid())
  authorId String
  author   Author @relation(fields: [authorId], references: [id])
  title    String
  blob     Bytes?
}
"#;

const OAS: &str = r#"
openapi: "3.0.0"
info:
  title: Pet Shop
  version: "1.0.0"
paths:
  /pets:
    get:
      tags: [Pet]
      responses: {'200': {description: ok}}
    post:
      tags: [Pet]
      responses: {'201': {description: ok}}
components:
  schemas:
    Pet:
      type: object
      required: [name]
      properties:
        id: {type: string, format: uuid}
        name: {type: string, maxLength: 50}
        age: {type: integer}
        price: {type: number}
        active: {type: boolean}
        photos: {type: array, items: {type: string}}
        mood: {type: string, enum: [happy, sad]}
    Owner:
      type: object
      properties:
        id: {type: string, format: uuid}
        title: {type: string}
"#;

fn load(dir: &Path, fm: &str) -> forma::dsl::Program {
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
    let (p, diags) = forma::dsl::load_program(dir, &["forma/**/*.fm".into()]).unwrap();
    let errs: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
    assert!(errs.is_empty(), "parse errors: {errs:?}\n---\n{fm}");
    let sem = forma::dsl::check_program(&p);
    let sem_errs: Vec<_> = sem.iter().filter(|d| d.severity == "error").collect();
    assert!(
        sem_errs.is_empty(),
        "check_program errors: {sem_errs:?}\n---\n{fm}"
    );
    p
}

// ── S2：机读损失报告 ───────────────────────────────────────────

#[test]
fn lossy_imports_emit_machine_readable_loss_reports() {
    let (dsl, sql_loss) = sql_import::sql_ddl_to_dsl_with_report(SQL, "schema.sql").unwrap();
    assert_eq!(sql_loss.format, "sql");
    assert_eq!(sql_loss.source, "schema.sql");
    assert_eq!(sql_loss.entities, 2);
    assert!(sql_loss.is_lossy());
    assert!(
        sql_loss
            .items
            .iter()
            .any(|i| i.construct.contains("books.blob")),
        "{:?}",
        sql_loss.items
    );
    assert!(dsl.contains("以下构造未映射"), "{dsl}");

    let (dsl, prisma_loss) =
        prisma_import::prisma_to_dsl_with_report(PRISMA, "schema.prisma").unwrap();
    assert_eq!(prisma_loss.format, "prisma");
    assert!(
        prisma_loss
            .items
            .iter()
            .any(|i| i.reason.contains("reverse list relation")),
        "{:?}",
        prisma_loss.items
    );
    assert!(dsl.contains("reverse list relation"), "{dsl}");

    let (_dsl, oas_loss) = openapi::openapi_to_dsl_with_report(OAS, "petstore.yaml").unwrap();
    assert_eq!(oas_loss.format, "openapi");
    let constructs: Vec<&str> = oas_loss
        .items
        .iter()
        .map(|i| i.construct.as_str())
        .collect();
    assert!(constructs.contains(&"Pet.photos"), "{constructs:?}");
    assert!(constructs.contains(&"Pet.mood"), "{constructs:?}");

    // JSON 形状稳定、可被外部工具消费
    for report in [&sql_loss, &prisma_loss, &oas_loss] {
        let v: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), report.items.len());
        assert!(v["source"].is_string());
        assert!(v["entities"].is_u64());
    }
}

#[test]
fn lossless_import_has_empty_report() {
    let clean =
        "CREATE TABLE users (id CHAR(36) NOT NULL PRIMARY KEY, name VARCHAR(50) NOT NULL);\n";
    let (_dsl, report) = sql_import::sql_ddl_to_dsl_with_report(clean, "clean.sql").unwrap();
    assert!(!report.is_lossy(), "{:?}", report.items);
    let v: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 0);
}

// ── S3：往返语义等价（可表达部分）──────────────────────────────

#[test]
fn openapi_roundtrip_preserves_expressible_fields_and_operations() {
    let dir = tmp("oas");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: petshop\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    let (dsl, loss) = openapi::openapi_to_dsl_with_report(OAS, "petstore.yaml").unwrap();
    let program = load(&dir, &dsl);

    // 导出视图（同源 emitter）
    let files = forma::emit::emit_openapi(&dir, &program, "generated/api-docs").unwrap();
    let spec_text = files
        .iter()
        .find(|f| f.path.ends_with("openapi.json"))
        .unwrap()
        .content
        .clone();
    let out: serde_json::Value = serde_json::from_str(&spec_text).unwrap();

    // 字段集合：可表达部分全部保留；被跳过的 photos 不出现
    let props = &out["components"]["schemas"]["Pet"]["properties"];
    for key in ["id", "name", "age", "price", "active", "mood"] {
        assert!(props.get(key).is_some(), "missing {key}: {props}");
    }
    assert!(
        props.get("photos").is_none(),
        "array must be reported as loss: {props}"
    );
    // 类型映射：Decimal → string 是序列化契约（spec:types），不是往返失真
    assert_eq!(props["name"]["type"], "string");
    assert_eq!(props["age"]["type"], "integer");
    assert_eq!(props["active"]["type"], "boolean");
    assert_eq!(props["price"]["type"], "string");

    // 操作：/pets get/post 在导出视图中保留（Forma 统一 /api 前缀）
    let paths = &out["paths"];
    assert!(paths.get("/api/pets").is_some(), "{paths}");
    assert!(paths["/api/pets"].get("get").is_some(), "{paths}");
    assert!(paths["/api/pets"].get("post").is_some(), "{paths}");

    // 损失项与导出结果一致：跳过的构造在报告中逐条可查
    assert!(loss.items.iter().any(|i| i.construct == "Pet.photos"));
    assert!(loss.items.iter().any(|i| i.construct == "Pet.mood"));
}

#[test]
fn sql_ddl_roundtrip_preserves_tables_columns_and_relations() {
    let dir = tmp("sql");
    let (dsl, _loss) = sql_import::sql_ddl_to_dsl_with_report(SQL, "schema.sql").unwrap();
    let program = load(&dir, &dsl);

    let tables = forma::emit::mysql::plan_tables(&program);
    let names: Vec<&str> = tables.iter().map(|t| t.table.as_str()).collect();
    assert!(names.contains(&"authors"), "{names:?}");
    assert!(names.contains(&"books"), "{names:?}");

    let books = tables.iter().find(|t| t.table == "books").unwrap();
    let cols: Vec<&str> = books.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(cols.contains(&"title"), "{cols:?}");
    assert!(
        cols.contains(&"author_id"),
        "关系列必须保持 {{rel}}_id: {cols:?}"
    );
    assert!(cols.contains(&"price"), "{cols:?}");
    let price = books.columns.iter().find(|c| c.name == "price").unwrap();
    assert_eq!(price.sql_type, "DECIMAL(10,2)", "{price:?}");
    // BLOB 在损失报告里，不出现在物理表计划中
    assert!(!cols.contains(&"blob"), "{cols:?}");
    assert!(
        books
            .fks
            .iter()
            .any(|fk| fk.column == "author_id" && fk.ref_table == "authors"),
        "{:?}",
        books.fks
    );
}

// ── §16：确定性与畸形输入 ─────────────────────────────────────

#[test]
fn file_order_does_not_change_program() {
    let dir = tmp("order");
    fs::write(
        dir.join("forma/a.fm"),
        "app x { version = \"0.1.0\" }\nentity A { id Uuid @id @default(uuid) }\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/b.fm"),
        "entity B { id Uuid @id @default(uuid) }\n",
    )
    .unwrap();
    let (p1, _) =
        forma::dsl::load_program(&dir, &["forma/a.fm".into(), "forma/b.fm".into()]).unwrap();
    let (p2, _) =
        forma::dsl::load_program(&dir, &["forma/b.fm".into(), "forma/a.fm".into()]).unwrap();
    let names1: Vec<&str> = p1.entities.iter().map(|e| e.name.as_str()).collect();
    let names2: Vec<&str> = p2.entities.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names1, names2, "source order must not affect Program");
}

#[test]
fn malformed_inputs_return_errors_not_panics() {
    assert!(openapi::openapi_to_dsl("::: not yaml :::", "bad.yaml").is_err());
    assert!(prisma_import::prisma_to_dsl("datasource db {}", "empty.prisma").is_err());
    assert!(sql_import::sql_ddl_to_dsl("", "empty.sql").is_err());
}

// ── CLI：--loss-report 端到端 ──────────────────────────────────

#[test]
fn cli_loss_report_flag_writes_json_file() {
    let dir = tmp("cli");
    let src = dir.join("schema.sql");
    let out = dir.join("out.fm");
    let report = dir.join("loss.json");
    fs::write(&src, SQL).unwrap();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_forma"))
        .args([
            "import",
            "sql",
            src.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--loss-report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{:?}",
        String::from_utf8_lossy(&status.stderr)
    );
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(v["format"], "sql");
    assert!(!v["items"].as_array().unwrap().is_empty());
    assert!(out.is_file());
}

#[test]
fn repeated_check_program_is_deterministic() {
    let dir = tmp("determinism");
    let (dsl, _) = sql_import::sql_ddl_to_dsl_with_report(SQL, "schema.sql").unwrap();
    fs::write(dir.join("forma/app.fm"), &dsl).unwrap();
    let mut runs = Vec::new();
    for _ in 0..3 {
        let (p, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
        let codes: Vec<String> = forma::dsl::check_program(&p)
            .iter()
            .map(|d| format!("{}:{}", d.severity, d.code))
            .collect();
        runs.push(codes);
    }
    assert_eq!(runs[0], runs[1]);
    assert_eq!(runs[1], runs[2]);
}

#[test]
fn build_report_exposes_ir_version() {
    let dir = tmp("irversion");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: irversion\n  version: 0.9.9\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    let (dsl, _) = sql_import::sql_ddl_to_dsl_with_report(
        "CREATE TABLE t (id CHAR(36) NOT NULL PRIMARY KEY, name VARCHAR(50));\n",
        "t.sql",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), &dsl).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    assert_eq!(report.ir_version, forma::dsl::IR_VERSION);
    // app.version 与 IR 版本相互独立
    assert_ne!(report.project_name, forma::dsl::IR_VERSION);
}
