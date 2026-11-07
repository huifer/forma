//! gen-flutter 实时推送（live → SSE）订阅端回归测试（M10/P1）。
//! 覆盖：live_api.dart 生成、列表页订阅代码、无 live 零产物。

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
        "forma-flutterlive-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: flive\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  mobile:\n    framework: flutter\n    out: generated/mobile\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

#[test]
fn flutter_list_page_subscribes_to_live_sse() {
    let dir = tmp_dir("generate");
    write_project(
        &dir,
        "app flive { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\npage ItemList {\n route \"/items\"\n title { zh \"条目\" en \"Items\" }\n table { query Items.list columns [name] }\n}\nlive ItemFeed {\n api Items.list\n events [create update delete]\n}\n",
    );
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"mobile-flutter"), "{names:?}");

    // lib/live_api.dart 生成
    assert!(dir.join("generated/mobile/lib/live_api.dart").exists());
    let live_api = fs::read_to_string(dir.join("generated/mobile/lib/live_api.dart")).unwrap();
    assert!(live_api.contains("class LiveSubscriber"), "{live_api}");
    assert!(live_api.contains("text/event-stream"), "{live_api}");
    assert!(live_api.contains("void start()"), "{live_api}");
    assert!(live_api.contains("_client.getUrl"), "{live_api}");

    // 列表页订阅代码
    let page = fs::read_to_string(dir.join("generated/mobile/lib/pages/item_list.dart")).unwrap();
    assert!(
        page.contains("import '../live_api.dart';"),
        "{page}\n（缺 live import）"
    );
    assert!(
        page.contains("LiveSubscriber? _live;"),
        "{page}\n（缺 _live 字段）"
    );
    assert!(
        page.contains("_subscribeLive(\"/api/live/ItemFeed\")"),
        "{page}\n（缺订阅调用）"
    );
    assert!(
        page.contains("void _subscribeLive(String url)"),
        "{page}\n（缺订阅方法）"
    );
    assert!(page.contains("_reload();"), "{page}\n（缺 _reload 刷新）");

    // 无 live 声明 → 零产物
    let dir2 = tmp_dir("zero");
    write_project(
        &dir2,
        "app flive { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\npage ItemList {\n route \"/items\"\n title { zh \"条目\" en \"Items\" }\n table { query Items.list columns [name] }\n}\n",
    );
    forma::build::run_build(&dir2).unwrap();
    assert!(!dir2.join("generated/mobile/lib/live_api.dart").exists());
    let page2 = fs::read_to_string(dir2.join("generated/mobile/lib/pages/item_list.dart")).unwrap();
    assert!(!page2.contains("LiveSubscriber"), "{page2}");
    assert!(!page2.contains("_subscribeLive"), "{page2}");
}
