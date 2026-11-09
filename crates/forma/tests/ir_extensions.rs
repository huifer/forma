//! IR 扩展（RFC-0004 §9 / §16-3）：`extensions { <namespace>.<key> = <value> }`。
//!
//! 契约三条：
//! - **保留**：多文件合并后条目完整、来源可归属（M1 级 file/line）；
//! - **不被擅解释**：对全部产物 byte 级不敏感（同一工程加不加扩展，输出一致）；
//! - **fail-closed**：非法键 / 重复键显式诊断，不静默忽略。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use forma::engine::{build_project, BuildOptions};
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
        "forma-ext-{tag}-{}-{seq}-{nanos}",
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
        "project:\n  name: extdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  db:\n    engine: mysql\n    out: db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n  web:\n    framework: react\n    router: tanstack\n    data: tanstack-query\n    ui: shadcn\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const BASE_FM: &str = r#"
app extdemo { version = "0.1.0" }

entity Order {
  id     Uuid @id @default(uuid)
  amount Decimal(12,2) @required
}

api Orders {
  entity Order
  operations [list get create]
}

page OrderList {
  route "/orders"
  title { en "Orders" }
  table { query Orders.list columns [amount] }
}
"#;

const EXT_FM: &str = r#"
app extdemo { version = "0.1.0" }

entity Order {
  id     Uuid @id @default(uuid)
  amount Decimal(12,2) @required
}

api Orders {
  entity Order
  operations [list get create]
}

page OrderList {
  route "/orders"
  title { en "Orders" }
  table { query Orders.list columns [amount] }
}

extensions {
  visualization.echarts.animation = true
  acme.audit.level = "strict"
  acme.quota.max = 1000
}
"#;

#[test]
fn extensions_are_preserved_with_source_location() {
    let dir = tmp_dir("preserve");
    write_project(&dir, EXT_FM);
    let (program, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(program.extensions.len(), 3);
    let e = &program.extensions[0];
    assert_eq!(e.namespace, "visualization.echarts");
    assert_eq!(e.key, "animation");
    assert!(matches!(
        e.value,
        forma::dsl::parser::ExtensionValue::Bool(true)
    ));
    assert_eq!(e.file.as_deref(), Some("forma/app.fm"));
    assert!(e.line > 0);
    // 点分路径：命名空间 = 除最后一段
    assert_eq!(program.extensions[1].namespace, "acme.audit");
    assert_eq!(program.extensions[1].key, "level");
}

#[test]
fn extensions_do_not_change_any_generated_artifact() {
    // 「不被擅解释」：同一工程加不加 extensions，全部产物 byte 级一致。
    let with = tmp_dir("with-ext");
    write_project(&with, EXT_FM);
    let without = tmp_dir("without-ext");
    write_project(&without, BASE_FM);
    let opts = |dir: PathBuf| BuildOptions {
        project_dir: dir,
        force: true,
        entities: None,
        dry_run: false,
        ownership: Default::default(),
    };
    let a = build_project(&opts(with)).unwrap();
    let b = build_project(&opts(without)).unwrap();
    assert!(
        a.diagnostics.iter().all(|d| d.severity != "error"),
        "{:?}",
        a.diagnostics
    );
    assert_eq!(a.files.len(), b.files.len(), "file sets must match");
    let mut am: std::collections::BTreeMap<&str, &str> = a
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.content.as_str()))
        .collect();
    for f in &b.files {
        let same = am.remove(f.path.as_str());
        assert_eq!(
            same,
            Some(f.content.as_str()),
            "artifact changed by extensions: {}",
            f.path
        );
    }
    assert!(am.is_empty(), "extra files with extensions: {am:?}");
}

#[test]
fn duplicate_extension_keys_are_rejected() {
    let dir = tmp_dir("dup");
    write_project(
        &dir,
        r#"
app extdemo { version = "0.1.0" }
entity Order {
  id     Uuid @id @default(uuid)
  amount Decimal(12,2) @required
}
api Orders { entity Order operations [list] }
extensions {
  acme.audit.level = "strict"
  acme.audit.level = "loose"
}
"#,
    );
    let (_p, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(
        diags.iter().any(|d| d.code == "dsl-duplicate-extension"),
        "{diags:?}"
    );
}

#[test]
fn malformed_extension_keys_are_parse_errors() {
    let dir = tmp_dir("malformed");
    write_project(
        &dir,
        r#"
app extdemo { version = "0.1.0" }
entity Order {
  id     Uuid @id @default(uuid)
  amount Decimal(12,2) @required
}
api Orders { entity Order operations [list] }
extensions {
  audit = "strict"
}
"#,
    );
    let (_p, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(
        diags
            .iter()
            .any(|d| d.code == "dsl-parse" && d.message.contains("namespace")),
        "{diags:?}"
    );
}

#[test]
fn extensions_merge_across_files_with_attribution() {
    let dir = tmp_dir("merge");
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: extdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), BASE_FM).unwrap();
    fs::write(
        dir.join("forma/extra.fm"),
        r#"
extensions {
  acme.audit.level = "strict"
}
"#,
    )
    .unwrap();
    let (program, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(program.extensions.len(), 1);
    assert_eq!(program.extensions[0].file.as_deref(), Some("forma/extra.fm"));
}
