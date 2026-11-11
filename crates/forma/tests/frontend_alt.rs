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
    let dir = std::env::temp_dir().join(format!(
        "forma-fealt-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, framework: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: fedemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: {framework}\n    out: generated/web\n"),
    ).unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
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
"#,
    )
    .unwrap();
}

#[test]
fn sveltekit_target_generates_routes_from_dsl_pages() {
    let dir = tmp_dir("sveltekit");
    write_project(&dir, "sveltekit");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"frontend-sveltekit"), "{names:?}");
    assert!(!names.contains(&"web-react"), "{names:?}");
    let page =
        fs::read_to_string(dir.join("generated/web/src/routes/orders/+page.svelte")).unwrap();
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
    let comp =
        fs::read_to_string(dir.join("generated/web/src/app/pages/orders.component.ts")).unwrap();
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

/// 回归（2026-11-11 验证实跑抓出）：
/// ① Angular `src/main.ts` 曾导入 `./app.component` / `./app.routes`，但文件在
///    `src/app/` → 生成工程 `tsc` 报 TS2307（模块找不到）。
/// ② SvelteKit 的依赖区间未锁 `@sveltejs/vite-plugin-svelte`，npm 解析到 v7
///    （要求 svelte 5）而工程声明 svelte ^4 → `npm install` 直接 ERESOLVE 失败。
/// ③ SvelteKit tsconfig 缺 `module`（`moduleResolution: bundler` 要求 module ≥ es2015）。
#[test]
fn angular_main_imports_app_scoped_modules() {
    let dir = tmp_dir("angularimport");
    write_project(&dir, "angular");
    forma::build::run_build(&dir).unwrap();
    let main = fs::read_to_string(dir.join("generated/web/src/main.ts")).unwrap();
    assert!(
        main.contains("from './app/app.component'"),
        "angular main.ts must import the app-scoped component: {main}"
    );
    assert!(main.contains("from './app/app.routes'"), "{main}");
    assert!(!main.contains("from './app.component'"), "{main}");
    // 文件确实在 src/app/ 下
    assert!(dir.join("generated/web/src/app/app.component.ts").exists());
    assert!(dir.join("generated/web/src/app/app.routes.ts").exists());
}

#[test]
fn sveltekit_pins_compatible_plugin_and_declares_module() {
    let dir = tmp_dir("sveltekitdeps");
    write_project(&dir, "sveltekit");
    forma::build::run_build(&dir).unwrap();
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(
        pkg.contains("\"@sveltejs/vite-plugin-svelte\""),
        "sveltekit must pin vite-plugin-svelte (npm resolves v7 → needs svelte 5): {pkg}"
    );
    let tsconfig = fs::read_to_string(dir.join("generated/web/tsconfig.json")).unwrap();
    assert!(
        tsconfig.contains("\"module\""),
        "sveltekit tsconfig must declare module (bundler resolution requires it): {tsconfig}"
    );
}
