//! RFC-0007 §16：产物写入的内容哈希跳过与原子写。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-atomic-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn unchanged_content_is_not_rewritten() {
    let dir = tmp("skip");
    let p = dir.join("artifact.txt");
    assert!(forma::util::write_text_changed(&p, "hello\n").unwrap());
    assert_eq!(fs::read_to_string(&p).unwrap(), "hello\n");
    // 同内容 → 零写入
    assert!(!forma::util::write_text_changed(&p, "hello\n").unwrap());
    // 换行归一化口径（CRLF/CR → LF）：视为同一产物
    assert!(!forma::util::write_text_changed(&p, "hello\r\n").unwrap());
    // 内容变化 → 写入
    assert!(forma::util::write_text_changed(&p, "changed\n").unwrap());
    assert_eq!(fs::read_to_string(&p).unwrap(), "changed\n");
}

#[test]
fn atomic_write_leaves_no_temp_files_and_creates_parents() {
    let dir = tmp("atomic");
    let p = dir.join("nested/deep/artifact.txt");
    assert!(forma::util::write_text_changed(&p, "x\n").unwrap());
    assert_eq!(fs::read_to_string(&p).unwrap(), "x\n");
    let leftovers: Vec<String> = fs::read_dir(dir.join("nested/deep"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}

#[test]
fn consecutive_builds_do_not_rewrite_unchanged_artifacts() {
    let dir = tmp("build");
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: atomicbuild\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        "app atomicbuild { version = \"0.1.0\" }\nentity Item { id Uuid @id @default(uuid) name String(50) @required }\napi Items { entity Item operations [list get] }\n",
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let target = dir.join("generated/backend/src/models.rs");
    let before = fs::metadata(&target).unwrap().modified().unwrap();
    let content_before = fs::read_to_string(&target).unwrap();
    // 第二次构建：内容哈希相同 → write_text_changed 返回 false
    assert!(!forma::util::write_text_changed(Path::new(&target), &content_before).unwrap());
    forma::build::run_build(&dir).unwrap();
    let after = fs::metadata(&target).unwrap().modified().unwrap();
    assert_eq!(before, after, "unchanged artifacts must not be rewritten");
    assert_eq!(fs::read_to_string(&target).unwrap(), content_before);
}
