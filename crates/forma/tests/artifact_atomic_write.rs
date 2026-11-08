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

#[test]
fn manifest_hash_chain_is_stable_and_tamper_sensitive() {
    use forma::artifacts::{manifest_hash, ArtifactEntry, ArtifactManifest, Ownership};
    let mut m = ArtifactManifest {
        version: 1,
        artifacts: Default::default(),
        manifest_hash: String::new(),
        signature: String::new(),
    };
    m.artifacts.insert(
        "a.txt".into(),
        ArtifactEntry {
            hash: "h1".into(),
            ownership: Ownership::Generated,
            generator: "gen-x".into(),
            source: None,
        },
    );
    let h1 = manifest_hash(&m);
    // 同内容同哈希
    assert_eq!(manifest_hash(&m), h1);
    // 改条目哈希 → 变化
    m.artifacts.get_mut("a.txt").unwrap().hash = "h2".into();
    assert_ne!(manifest_hash(&m), h1);
    // 改路径 → 变化
    let mut m2 = m.clone();
    let entry = m2.artifacts.remove("a.txt").unwrap();
    m2.artifacts.insert("b.txt".into(), entry);
    assert_ne!(manifest_hash(&m2), manifest_hash(&m));
}

#[test]
fn signature_verification_requires_signing_key() {
    // 未设 FORMA_SIGNING_KEY 时：验签返回 false（不 panic），签名生成为空
    // （不直接操作进程 env，避免并行测试竞态；验证纯函数行为）
    let m = forma::artifacts::sign_manifest("some-hash");
    if std::env::var("FORMA_SIGNING_KEY")
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
    {
        assert!(!m.is_empty());
        assert!(forma::artifacts::verify_signature("some-hash", &m));
        assert!(!forma::artifacts::verify_signature("other-hash", &m));
    } else {
        assert!(m.is_empty());
    }
}
