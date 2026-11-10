//! 数据导出（M9/P1）回归测试：`export <Api> as csv|xlsx`。
//!
//! 验收：
//! - 检查层 3 条 `dsl-export-*` 诊断（未知 api / 缺 list / 未知格式）+ 重复声明；
//! - 生成层：src/export.rs（EXPORT_TARGETS 静态展开 + CSV 流式端点 + xlsx 驱动
//!   （rust_xlsxwriter））、routes 挂载、main mod、OpenAPI 契约、Web 列表页
//!   「导出 CSV」按钮；xlsx 存在时 Cargo.toml 条件引入 rust_xlsxwriter；
//! - 无 export 声明的工程全链路零产物。

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
        "forma-export-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const MANIFEST: &str = r#"project:
  name: exportdemo
  version: 0.1.0

source: ["forma/**/*.fm"]

targets:
  db:
    engine: mysql
    out: db
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
  frontend:
    framework: react
    out: generated/web
"#;

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(dir.join("forma.yaml"), MANIFEST).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const APP_WITH_EXPORT: &str = r#"app exportdemo {
  version = "0.1.0"
}

entity Order {
  id        Uuid @id @default(uuid)
  customer  String(50) @required
  total     Decimal(12,2)
  createdAt Timestamp @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
}

export Orders as csv
export Orders as xlsx

page Orders {
  route "/orders"
  title { zh "订单" en "Orders" }
  table {
    query   Orders.list
    columns [customer, total, createdAt: datetime]
  }
}
"#;

fn load(dir: &Path) -> forma::dsl::Program {
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diags) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let errs: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
    assert!(errs.is_empty(), "fixture must parse: {errs:?}");
    program
}

#[test]
fn export_declaration_checks() {
    // 合法：csv + xlsx
    let dir = tmp_dir("ok");
    write_project(&dir, APP_WITH_EXPORT);
    let diags = forma::dsl::check_program(&load(&dir));
    let export_errs: Vec<_> = diags
        .iter()
        .filter(|d| d.code.starts_with("dsl-export"))
        .collect();
    assert!(export_errs.is_empty(), "{export_errs:?}");

    // 未知 api
    let dir = tmp_dir("unknown-api");
    write_project(
        &dir,
        r#"app exportdemo { version = "0.1.0" }
entity Order { id Uuid @id @default(uuid) }
api Orders { entity Order operations [list] }
export Missing as csv
"#,
    );
    let diags = forma::dsl::check_program(&load(&dir));
    assert!(
        diags.iter().any(|d| d.code == "dsl-export-unknown-api"),
        "{diags:?}"
    );

    // 缺 list 操作
    let dir = tmp_dir("no-list");
    write_project(
        &dir,
        r#"app exportdemo { version = "0.1.0" }
entity Order { id Uuid @id @default(uuid) }
api Orders { entity Order operations [get] }
export Orders as csv
"#,
    );
    let diags = forma::dsl::check_program(&load(&dir));
    assert!(
        diags.iter().any(|d| d.code == "dsl-export-no-list"),
        "{diags:?}"
    );

    // 未知格式
    let dir = tmp_dir("fmt");
    write_project(
        &dir,
        r#"app exportdemo { version = "0.1.0" }
entity Order { id Uuid @id @default(uuid) }
api Orders { entity Order operations [list] }
export Orders as json
"#,
    );
    let diags = forma::dsl::check_program(&load(&dir));
    assert!(
        diags.iter().any(|d| d.code == "dsl-export-unknown-format"),
        "{diags:?}"
    );

    // 重复声明：dsl-duplicate 在 merge 期（load_program 的 diagnostics）产生
    let dir = tmp_dir("dup");
    write_project(
        &dir,
        r#"app exportdemo { version = "0.1.0" }
entity Order { id Uuid @id @default(uuid) }
api Orders { entity Order operations [list] }
export Orders as csv
export Orders as csv
"#,
    );
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (_, diags) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    assert!(diags.iter().any(|d| d.code == "dsl-duplicate"), "{diags:?}");
}

#[test]
fn export_backend_and_web_generation() {
    let dir = tmp_dir("gen");
    write_project(&dir, APP_WITH_EXPORT);
    forma::build::run_build(&dir).unwrap();

    // Rust：export.rs + EXPORT_TARGETS 静态展开（列名与 DDL 一致）
    let export_rs = fs::read_to_string(dir.join("generated/backend/src/export.rs")).unwrap();
    assert!(export_rs.contains("pub static EXPORT_TARGETS"));
    assert!(
        export_rs.contains(r#"(&"orders", &"orders", &["id", "customer", "total", "created_at"])"#)
    );
    assert!(export_rs.contains("export.csv"));
    // xlsx 驱动：真实生成（rust_xlsxwriter），不再是 501 占位
    assert!(export_rs.contains("rust_xlsxwriter::Workbook"));
    assert!(export_rs.contains("export.xlsx\""));
    assert!(!export_rs.contains("ExportDriver"));
    // xlsx 存在 → 条件依赖 rust_xlsxwriter
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(cargo.contains("rust_xlsxwriter"), "{cargo}");
    // routes 挂载 + main mod + error 契约
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(routes.contains("crate::export::router"));
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(main.contains("mod export;"));
    let error = fs::read_to_string(dir.join("generated/backend/src/error.rs")).unwrap();
    assert!(
        !error.contains("ExportDriver"),
        "xlsx 已实现，501 变体应移除"
    );

    // OpenAPI 契约
    let openapi = fs::read_to_string(dir.join("generated/api-docs/openapi.json")).unwrap();
    assert!(openapi.contains("/api/orders/export.csv"));
    assert!(openapi.contains("/api/orders/export.xlsx"));

    // Web：列表页导出按钮
    let pages_dir = dir.join("generated/web/src/pages");
    let page = fs::read_dir(&pages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| fs::read_to_string(e.path()).unwrap_or_default())
        .find(|c| c.contains("导出 CSV"))
        .expect("list page must render export button");
    assert!(page.contains("/api/orders/export.csv"));
}

#[test]
fn no_export_project_has_zero_export_artifacts() {
    let dir = tmp_dir("zero");
    write_project(
        &dir,
        r#"app exportdemo { version = "0.1.0" }
entity Note {
  id   Uuid @id @default(uuid)
  body Text
}
api Notes { entity Note operations [list get create update delete] }
page Notes {
  route "/notes"
  title { zh "笔记" en "Notes" }
  table {
    query   Notes.list
    columns [body, createdAt: datetime]
  }
}
"#,
    );
    forma::build::run_build(&dir).unwrap();

    assert!(!dir.join("generated/backend/src/export.rs").exists());
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(!routes.contains("crate::export::router"));
    let error = fs::read_to_string(dir.join("generated/backend/src/error.rs")).unwrap();
    assert!(!error.contains("ExportDriver"));
    let openapi = fs::read_to_string(dir.join("generated/api-docs/openapi.json")).unwrap();
    assert!(!openapi.contains("/export.csv"));
    let pages_dir = dir.join("generated/web/src/pages");
    let page = fs::read_dir(&pages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| fs::read_to_string(e.path()).unwrap_or_default())
        .find(|c| c.contains("导出"))
        .unwrap_or_default();
    assert!(!page.contains("export.csv"), "{page}");
}

#[test]
fn csv_only_project_has_no_xlsx_dependency() {
    // 零新增依赖原则：只有 xlsx 导出声明才引入 rust_xlsxwriter
    let dir = tmp_dir("csv-only");
    write_project(
        &dir,
        r#"app exportdemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  total Decimal(12,2)
}
api Orders { entity Order operations [list get create update delete] }
export Orders as csv
page Orders {
  route "/orders"
  title { zh "订单" en "Orders" }
  table { query Orders.list columns [total] }
}
"#,
    );
    forma::build::run_build(&dir).unwrap();
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(!cargo.contains("rust_xlsxwriter"), "{cargo}");
    let export_rs = fs::read_to_string(dir.join("generated/backend/src/export.rs")).unwrap();
    assert!(!export_rs.contains("rust_xlsxwriter"), "{export_rs}");
}
