//! RFC-0007 §16：产物 `@generated` 标记覆盖（P3·G-6 收口）。
//!
//! 决议（spec:serialization §12）：JSON 格式不支持注释，豁免标记；
//! 所有权与完整性由 `.forma/artifacts.json` 的内容哈希保障。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-genv-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

#[test]
fn all_non_json_artifacts_carry_generated_marker() {
    let dir = tmp("all");
    std::fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: genv\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n  mobile:\n    out: app_mobile\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("forma/app.fm"),
        r#"
app genv { version = "0.1.0" }
entity Item { id Uuid @id @default(uuid) name String(50) @required }
api Items { entity Item operations [list get create update delete] }
page ItemList {
  route "/items"
  title { en "Items" }
  table {
    query   Items.list
    columns [name]
    actions { row [delete → Items.delete confirm] }
  }
}
"#,
    )
    .unwrap();
    let report = forma::build::run_build(&dir).unwrap();

    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut json_count = 0usize;
    for step in &report.steps {
        for f in &step.files {
            let name = f.path.rsplit('/').next().unwrap_or(&f.path);
            if name.ends_with(".json") {
                // 决议豁免：JSON 不支持注释
                json_count += 1;
                continue;
            }
            checked += 1;
            // shebang 文件（如 entrypoint.sh）：标记允许在首行之后
            let lines: Vec<&str> = f.content.lines().collect();
            let first = lines.first().copied().unwrap_or("");
            let marker_line = if first.starts_with("#!") {
                lines.get(1).copied().unwrap_or("")
            } else {
                first
            };
            let ok = marker_line.starts_with("// @generated")
                || marker_line.starts_with("# @generated")
                || marker_line.starts_with("<!-- @generated")
                || marker_line.starts_with("-- @generated")
                || marker_line.starts_with("/* @generated")
                || marker_line.starts_with("%% @generated");
            if !ok {
                violations.push(format!(
                    "{} (starts: {:?})",
                    f.path,
                    f.content.lines().next().unwrap_or("")
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "artifacts without @generated marker ({}):\n{}",
        violations.len(),
        violations.join("\n")
    );
    assert!(checked > 10, "expected a multi-target build, got {checked}");
    assert!(json_count > 0, "fixture should produce json artifacts");
    // 目录真实落盘（write_text 走 util）
    assert!(Path::new(&dir)
        .join("generated/backend/src/main.rs")
        .is_file());
}
