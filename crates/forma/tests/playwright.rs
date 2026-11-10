//! Playwright e2e 生成（轨道 B 重新排期）回归：opt-in 路由、DSL 静态展开、默认零产物。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forma-pw-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app pwdemo {
  version = "0.1.0"
}

entity Order {
  id     Uuid       @id @default(uuid)
  title  String(80) @required
  total  Decimal(12,2)
  paid   Bool
}

api Orders {
  entity Order
  operations [list get create update delete]
}

page OrderList {
  route "/orders"
  title { en "Orders" }
  table {
    query   Orders.list
    columns [title, total]
  }
}
"#;

fn write_project(dir: &Path, e2e: Option<&str>) {
    let e2e_line = match e2e {
        Some(v) => format!("\n    e2e: {v}"),
        None => String::new(),
    };
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: pwdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web{e2e_line}\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn playwright_target_generates_self_contained_e2e_suite() {
    let dir = tmp_dir("gen");
    write_project(&dir, Some("playwright"));
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"web-e2e"), "{names:?}");

    let base = dir.join("generated/web/e2e");
    let cfg = fs::read_to_string(base.join("playwright.config.ts")).unwrap();
    assert!(cfg.contains("defineConfig"), "{cfg}");
    assert!(cfg.contains("E2E_BASE_URL"), "{cfg}");
    let pkg = fs::read_to_string(base.join("package.json")).unwrap();
    assert!(pkg.contains("@playwright/test"), "{pkg}");
    let spec = fs::read_to_string(base.join("crud.spec.ts")).unwrap();
    assert!(spec.contains("await page.goto('/orders')"), "{spec}");
    assert!(spec.contains("Orders list renders"), "{spec}");
    assert!(spec.contains("'/orders/new'"), "{spec}");
    let readme = fs::read_to_string(base.join("README.md")).unwrap();
    assert!(readme.contains("npm test"), "{readme}");
    let tsconfig = fs::read_to_string(base.join("tsconfig.json")).unwrap();
    assert!(tsconfig.contains("ES2020"), "{tsconfig}");
}

#[test]
fn no_e2e_flag_produces_zero_e2e_artifacts() {
    let dir = tmp_dir("none");
    write_project(&dir, None);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(!names.contains(&"web-e2e"), "{names:?}");
    assert!(!dir.join("generated/web/e2e").exists());
}

#[test]
fn unknown_e2e_framework_is_rejected() {
    let dir = tmp_dir("unknown");
    write_project(&dir, Some("cypress"));
    let err = forma::build::run_build(&dir).unwrap_err();
    assert!(err.to_string().contains("supported: playwright"), "{err}");
}
