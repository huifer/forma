//! 前端扩栈（轨道 B 重新排期）回归：SvelteKit / Angular 路由与产物、默认 React 零变化。

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
    let dir = std::env::temp_dir().join(format!("forma-fealt-{tag}-{}-{seq}-{nanos}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, framework: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: fedemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: {framework}\n    out: generated/web\n"),
    ).unwrap();
    fs::write(dir.join("forma/app.fm"), r#"
app fedemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  title String(80) @required
}
api Orders { entity Order operations [list get create update delete] }
page OrderList {
  route "/orders"
  title { en "Orders" }
  table { query Orders.list columns [title] }
}
"#).unwrap();
}

#[test]
fn sveltekit_target_generates_routes_from_dsl_pages() {
    let dir = tmp_dir("sveltekit");
    write_project(&dir, "sveltekit");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"frontend-sveltekit"), "{names:?}");
    assert!(!names.contains(&"web-react"), "{names:?}");
    let page = fs::read_to_string(dir.join("generated/web/src/routes/orders/+page.svelte")).unwrap();
    assert!(page.contains("{#each rows as row}"), "{page}");
    assert!(page.contains("fetch("), "{page}");
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(pkg.contains("@sveltejs/kit"), "{pkg}");
    assert!(dir.join("generated/web/src/app.html").exists());
}

#[test]
fn angular_target_generates_standalone_components() {
    let dir = tmp_dir("angular");
    write_project(&dir, "angular");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"frontend-angular"), "{names:?}");
    let comp = fs::read_to_string(dir.join("generated/web/src/app/pages/orders.component.ts")).unwrap();
    assert!(comp.contains("@Component"), "{comp}");
    assert!(comp.contains("standalone: true"), "{comp}");
    assert!(comp.contains("{{row.id}}"), "{comp}");
    let routes = fs::read_to_string(dir.join("generated/web/src/app/app.routes.ts")).unwrap();
    assert!(routes.contains("OrdersComponent"), "{routes}");
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(pkg.contains("@angular/core"), "{pkg}");
}

#[test]
fn default_react_target_remains_unchanged() {
    let dir = tmp_dir("default");
    write_project(&dir, "react");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"web-react"), "{names:?}");
    assert!(!names.contains(&"frontend-sveltekit"), "{names:?}");
    assert!(!names.contains(&"frontend-angular"), "{names:?}");
}
