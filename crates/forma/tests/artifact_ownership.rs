//! 产物清单与人工修改检测回归测试（F-002 / ADR-0003）：
//! - 产物清单 `.forma/artifacts.json` 记录每个产物的哈希与归属；
//! - **人工修改过的生成物不被静默覆盖**（构建后恢复人工版本 + `artifact-modified` 警告）；
//! - `--adopt`：登记为 Adopted（人工版本胜出，永不覆盖）；
//! - `--overwrite`：生成版本胜出 + 审计留痕；
//! - `--strict`：冲突提升为 error。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use forma::build::{run_build_with, OwnershipPolicy};
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
        "forma-artifacts-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const MANIFEST: &str = "project:\n  name: ownership\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n";

fn write_project(dir: &Path) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(dir.join("forma.yaml"), MANIFEST).unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app ownership {
  version = "0.1.0"
}

entity Item {
  id   Uuid      @id @default(uuid)
  name String(50) @required
}

api Items {
  entity Item
  operations [list get create update delete]
}
"#,
    )
    .unwrap();
}

fn build(dir: &Path, policy: OwnershipPolicy) -> forma::build::BuildReport {
    run_build_with(dir, policy).unwrap()
}

fn artifact_diags(report: &forma::build::BuildReport) -> Vec<&forma::validate::Diagnostic> {
    report.artifact_diagnostics.iter().collect()
}

fn codes(report: &forma::build::BuildReport) -> Vec<&str> {
    artifact_diags(report)
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

const MODELS: &str = "generated/backend/src/models.rs";

// ── 1. 清单登记 + 干净重建零冲突 ────────────────────────────────

#[test]
fn manifest_records_generated_files_and_clean_rebuild_has_no_conflict() {
    let dir = tmp_dir("clean");
    write_project(&dir);
    build(&dir, OwnershipPolicy::default());

    let manifest_text = fs::read_to_string(dir.join(".forma/artifacts.json")).unwrap();
    assert!(manifest_text.contains(MODELS), "{manifest_text}");
    assert!(manifest_text.contains("generated"), "{manifest_text}");

    // 干净重建：无任何冲突诊断
    let report = build(&dir, OwnershipPolicy::default());
    assert!(
        !codes(&report).contains(&"artifact-modified"),
        "{:?}",
        artifact_diags(&report)
    );
}

// ── 2. 人工修改不被静默覆盖 ─────────────────────────────────────

#[test]
fn manual_modification_is_preserved_and_warned() {
    let dir = tmp_dir("manual");
    write_project(&dir);
    build(&dir, OwnershipPolicy::default());

    // 人工修改生成物（ADR-0003 的核心场景）
    let human = "// 人工调整：保留此注释\n\n";
    let original = fs::read_to_string(dir.join(MODELS)).unwrap();
    fs::write(dir.join(MODELS), format!("{human}{original}")).unwrap();

    let report = build(&dir, OwnershipPolicy::default());
    assert!(codes(&report).contains(&"artifact-modified"), "{report:?}");
    // 人工版本被恢复（emitter 的覆盖被撤销）
    let content = fs::read_to_string(dir.join(MODELS)).unwrap();
    assert!(
        content.starts_with(human),
        "human version must survive:\n{content}"
    );
    // 非严格 → warning
    assert_eq!(artifact_diags(&report)[0].severity, "warning");
}

#[test]
fn strict_mode_promotes_conflict_to_error() {
    let dir = tmp_dir("strict");
    write_project(&dir);
    build(&dir, OwnershipPolicy::default());
    let original = fs::read_to_string(dir.join(MODELS)).unwrap();
    fs::write(dir.join(MODELS), format!("// human\n\n{original}")).unwrap();

    let report = build(
        &dir,
        OwnershipPolicy {
            strict: true,
            ..Default::default()
        },
    );
    let d = artifact_diags(&report)
        .into_iter()
        .find(|d| d.code == "artifact-modified")
        .expect("conflict diagnostic");
    assert_eq!(d.severity, "error", "{d:?}");
}

// ── 3. --adopt：登记为 Adopted，之后永不覆盖 ────────────────────

#[test]
fn adopt_registers_human_version_and_never_overwrites() {
    let dir = tmp_dir("adopt");
    write_project(&dir);
    build(&dir, OwnershipPolicy::default());
    let human = "// adopted: 人工逻辑\npub struct Extra;\n\n".to_string();
    let original = fs::read_to_string(dir.join(MODELS)).unwrap();
    fs::write(dir.join(MODELS), format!("{human}{original}")).unwrap();

    // adopt：人工版本胜出
    let report = build(
        &dir,
        OwnershipPolicy {
            adopt: true,
            ..Default::default()
        },
    );
    assert!(codes(&report).contains(&"artifact-adopted"), "{report:?}");
    assert!(fs::read_to_string(dir.join(MODELS))
        .unwrap()
        .starts_with(&human));

    // 清单登记为 adopted
    let manifest_text = fs::read_to_string(dir.join(".forma/artifacts.json")).unwrap();
    assert!(manifest_text.contains("adopted"), "{manifest_text}");

    // 后续构建：永不覆盖、无警告
    let again = build(&dir, OwnershipPolicy::default());
    assert!(
        !codes(&again).contains(&"artifact-modified"),
        "{:?}",
        artifact_diags(&again)
    );
    assert!(fs::read_to_string(dir.join(MODELS))
        .unwrap()
        .starts_with(&human));
}

// ── 4. --overwrite：显式覆盖 + 审计 ─────────────────────────────

#[test]
fn overwrite_discards_human_version_and_audits() {
    let dir = tmp_dir("overwrite");
    write_project(&dir);
    build(&dir, OwnershipPolicy::default());
    let original = fs::read_to_string(dir.join(MODELS)).unwrap();
    fs::write(dir.join(MODELS), "// human\n\n".to_string() + &original).unwrap();

    let report = build(
        &dir,
        OwnershipPolicy {
            overwrite: true,
            ..Default::default()
        },
    );
    assert!(
        codes(&report).contains(&"artifact-overwritten"),
        "{report:?}"
    );
    // 生成版本胜出
    assert!(!fs::read_to_string(dir.join(MODELS))
        .unwrap()
        .starts_with("// human"));
    // 审计留痕
    let audit = fs::read_to_string(dir.join(".forma/audit.jsonl")).unwrap();
    assert!(audit.contains("artifact-overwrite"), "{audit}");
    assert!(audit.contains(MODELS), "{audit}");
}

// ── 5. 规格演进后重建：正常覆盖（非人工修改不受保护）─────────────

#[test]
fn spec_eviction_rebuilds_normally_without_conflict() {
    let dir = tmp_dir("evolve");
    write_project(&dir);
    build(&dir, OwnershipPolicy::default());

    // 改规格（不是改产物）→ 重建应当直接覆盖
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app ownership {
  version = "0.1.0"
}

entity Item {
  id    Uuid      @id @default(uuid)
  name  String(50) @required
  stock Int
}

api Items {
  entity Item
  operations [list get create update delete]
}
"#,
    )
    .unwrap();
    let report = build(&dir, OwnershipPolicy::default());
    assert!(
        !codes(&report).contains(&"artifact-modified"),
        "spec-driven changes must not be treated as conflicts: {:?}",
        artifact_diags(&report)
    );
    let models = fs::read_to_string(dir.join(MODELS)).unwrap();
    assert!(models.contains("pub stock"), "{models}");
}
