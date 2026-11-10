//! CLI 契约回归：帮助文本必须覆盖全部顶层命令、bench 不再污染 CWD、
//! `forma artifacts <dir>` 与 `forma artifacts verify <dir>` 等价。
//!
//! 背景（2026-11-10 完成度审计）：HELP 曾缺 7 个子命令；`forma bench` 默认把 100 个
//! `F008-*` 案例工程写进 CWD；`forma artifacts <dir>` 曾被误当成子命令而回落到 `.`。

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forma-cli-{tag}-{}-{}-{nanos}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn forma() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forma"))
}

#[test]
fn help_lists_every_top_level_command() {
    let out = forma().arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for cmd in [
        "init",
        "check",
        "validate",
        "doctor",
        "lint",
        "graph",
        "build",
        "diff",
        "diagrams",
        "seed",
        "migrate",
        "import",
        "patch",
        "artifacts",
        "bench",
        "docs",
        "capability",
        "studio",
        "mcp",
    ] {
        assert!(
            text.contains(&format!("forma {cmd}")),
            "HELP is missing `forma {cmd}`:\n{text}"
        );
    }
    assert!(
        text.contains("临时目录"),
        "HELP must document bench's scratch-root cleanup: {text}"
    );
}

#[test]
fn bench_scratch_root_lives_in_temp_dir() {
    let root = forma::bench::scratch_root();
    assert!(
        root.starts_with(std::env::temp_dir()),
        "scratch root must live under the system temp dir: {}",
        root.display()
    );
    assert_ne!(root, PathBuf::from("."));
}

#[test]
fn bench_without_dir_leaves_no_case_projects_in_cwd() {
    // `--skip-backend` 避免 10 次真实依赖编译；用临时 CWD 证明默认路径不落盘污染。
    let cwd = tmp_dir("benchcwd");
    let out = forma()
        .args(["bench", "--skip-backend", "--json"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bench failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["casesTotal"], 100);
    let leftovers: Vec<_> = fs::read_dir(&cwd)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("F008-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "bench polluted CWD with {:?}",
        leftovers.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
}

const FM: &str = r#"
app clismoke { version = "0.1.0" }
entity Note {
  id Uuid @id @default(uuid)
  title String(60) @required
}
api Notes { entity Note operations [list get create update delete] }
"#;

fn built_project() -> PathBuf {
    let dir = tmp_dir("artifacts");
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: clismoke\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    forma::build::run_build(&dir).unwrap();
    dir
}

#[test]
fn artifacts_accepts_dir_as_first_positional() {
    let dir = built_project();
    let as_dir = forma().args(["artifacts"]).arg(&dir).output().unwrap();
    let as_verify = forma()
        .args(["artifacts", "verify"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        as_dir.status.success(),
        "{}",
        String::from_utf8_lossy(&as_dir.stderr)
    );
    assert!(
        as_verify.status.success(),
        "{}",
        String::from_utf8_lossy(&as_verify.stderr)
    );
    let a = String::from_utf8_lossy(&as_dir.stdout);
    let b = String::from_utf8_lossy(&as_verify.stdout);
    assert!(a.contains("manifest hash"), "{a}");
    assert_eq!(a, b, "the two forms must be equivalent");
}
