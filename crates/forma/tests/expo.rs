//! Expo（React Native）target 回归测试（M10）：脚手架、共享 TS 层与页面路由。
//! 验证：生成的 TypeScript 必须通过 `tsc --noEmit`（纯 cargo test 覆盖不到 TS 层）。

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
        "forma-expo-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app expo {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
}
api Items {
  entity Item
  operations [list get create update delete]
}
page ItemList {
  route "/items"
  title { zh "条目" en "Items" }
  table { query Items.list columns [name, amount] }
}
"#;

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: expo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  mobile:\n    framework: expo\n    out: generated/mobile\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn expo_target_generates_rn_project() {
    let dir = tmp_dir("project");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"mobile-expo"), "{names:?}");
    assert!(!names.contains(&"mobile-flutter"), "{names:?}");

    let package = fs::read_to_string(dir.join("generated/mobile/package.json")).unwrap();
    assert!(package.contains("\"expo\""), "{package}");
    assert!(package.contains("\"react-native\""), "{package}");

    let app = fs::read_to_string(dir.join("generated/mobile/App.tsx")).unwrap();
    assert!(app.contains("QueryClientProvider"), "{app}");
    assert!(app.contains("ROUTES"), "{app}");

    // 共享 TS 层复用 web_react 契约
    let api = fs::read_to_string(dir.join("generated/mobile/src/api.ts")).unwrap();
    assert!(api.contains("ItemApi"), "{api}");
    let types = fs::read_to_string(dir.join("generated/mobile/src/types.ts")).unwrap();
    assert!(types.contains("Item"), "{types}");
    let validators = fs::read_to_string(dir.join("generated/mobile/src/validators.ts")).unwrap();
    assert!(validators.contains("validateCreateItem"), "{validators}");

    // 页面是 React Native 组件
    let page = fs::read_to_string(dir.join("generated/mobile/src/pages/item_list.tsx")).unwrap();
    assert!(page.contains("react-native"), "{page}");
    assert!(page.contains("FlatList"), "{page}");
    assert!(page.contains("StyleSheet"), "{page}");
    assert!(!page.contains("shadcn"), "{page}");
    assert!(!page.contains("<div"), "{page}");

    // 路由 + App 引用
    let main = fs::read_to_string(dir.join("generated/mobile/main.tsx")).unwrap();
    assert!(main.contains("registerRootComponent(App)"), "{main}");
    assert!(dir.join("generated/mobile/app.json").exists());
}

#[test]
fn expo_target_keeps_default_flutter_mobile_unchanged() {
    let dir = tmp_dir("default");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: mob\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  mobile:\n    out: generated/mobile\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"mobile-flutter"), "{names:?}");
    assert!(!names.contains(&"mobile-expo"), "{names:?}");
}
