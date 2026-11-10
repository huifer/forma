//! 原生/小程序移动端扩栈（轨道 B 重新排期）回归：四框架路由与产物、默认 Flutter 零变化。

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
    let dir = std::env::temp_dir().join(format!("forma-mnative-{tag}-{}-{seq}-{nanos}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, framework: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: mnativedemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  mobile:\n    framework: {framework}\n    out: generated/mobile\n"),
    ).unwrap();
    fs::write(dir.join("forma/app.fm"), r#"
app mnativedemo { version = "0.1.0" }
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
fn swiftui_target_generates_swift_views() {
    let dir = tmp_dir("swiftui");
    write_project(&dir, "swiftui");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"mobile-swiftui"), "{names:?}");
    let view = fs::read_to_string(dir.join("generated/mobile/Sources/mnativedemo/OrderListListView.swift")).unwrap();
    assert!(view.contains("struct OrderListListView"), "{view}");
    assert!(view.contains("URLSession"), "{view}");
    assert!(dir.join("generated/mobile/Package.swift").exists());
}

#[test]
fn compose_target_generates_gradle_and_screens() {
    let dir = tmp_dir("compose");
    write_project(&dir, "compose");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"mobile-compose"), "{names:?}");
    let root = dir.join("generated/mobile/app/src/main/java/dev/forma/mnativedemo");
    let screen = fs::read_to_string(root.join("OrderListListScreen.kt")).unwrap();
    assert!(screen.contains("@Composable"), "{screen}");
    assert!(dir.join("generated/mobile/app/build.gradle.kts").exists());
}

#[test]
fn miniprogram_target_generates_wxml_pages() {
    let dir = tmp_dir("mp");
    write_project(&dir, "miniprogram");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"mobile-miniprogram"), "{names:?}");
    let app = fs::read_to_string(dir.join("generated/mobile/app.json")).unwrap();
    assert!(app.contains("pages/order_list/index"), "{app}");
    let wxml = fs::read_to_string(dir.join("generated/mobile/pages/order_list/index.wxml")).unwrap();
    assert!(wxml.contains("{{item.id}}"), "{wxml}");
    let js = fs::read_to_string(dir.join("generated/mobile/pages/order_list/index.js")).unwrap();
    assert!(js.contains("wx.request"), "{js}");
}

#[test]
fn uniapp_target_generates_vue_pages_and_default_flutter_unchanged() {
    let dir = tmp_dir("uniapp");
    write_project(&dir, "uniapp");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"mobile-uniapp"), "{names:?}");
    let vue = fs::read_to_string(dir.join("generated/mobile/pages/order_list/index.vue")).unwrap();
    assert!(vue.contains("uni.request"), "{vue}");
    assert!(dir.join("generated/mobile/pages.json").exists());

    let d = tmp_dir("default");
    write_project(&d, "flutter");
    let r = forma::build::run_build(&d).unwrap();
    let n: Vec<&str> = r.steps.iter().map(|s| s.name).collect();
    assert!(n.contains(&"mobile-flutter"), "{n:?}");
    assert!(!n.contains(&"mobile-uniapp"), "{n:?}");
}
