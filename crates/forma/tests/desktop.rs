//! Desktop target（轨道 B 重新排期）回归：Electron / Tauri 包装、未知框架拒绝、默认零产物。

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
        "forma-desktop-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, framework: Option<&str>) {
    let desktop = match framework {
        Some(f) => format!("\n  desktop:\n    framework: {f}\n    out: generated/desktop"),
        None => String::new(),
    };
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: deskdemo\n  version: 0.2.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web{desktop}\n"
        ),
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app deskdemo { version = "0.2.0" }
entity Note {
  id   Uuid @id @default(uuid)
  body Text
}
api Notes { entity Note operations [list get create update delete] }
page NoteList {
  route "/notes"
  title { en "Notes" }
  table { query Notes.list columns [body] }
}
"#,
    )
    .unwrap();
}

#[test]
fn electron_wrapper_generates_window_shell() {
    let dir = tmp_dir("electron");
    write_project(&dir, Some("electron"));
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"desktop-wrapper"), "{names:?}");
    let base = dir.join("generated/desktop/electron");
    let main = fs::read_to_string(base.join("main.cjs")).unwrap();
    assert!(main.contains("BrowserWindow"), "{main}");
    assert!(main.contains("FORMA_DIST"), "{main}");
    assert!(main.contains("FORMA_URL"), "{main}");
    let pkg = fs::read_to_string(base.join("package.json")).unwrap();
    assert!(pkg.contains("electron"), "{pkg}");
    assert!(base.join("preload.cjs").exists());
}

#[test]
fn tauri_wrapper_generates_src_tauri_scaffold() {
    let dir = tmp_dir("tauri");
    write_project(&dir, Some("tauri"));
    forma::build::run_build(&dir).unwrap();
    let base = dir.join("generated/desktop/src-tauri");
    let conf = fs::read_to_string(base.join("tauri.conf.json")).unwrap();
    assert!(conf.contains("frontendDist"), "{conf}");
    assert!(conf.contains("deskdemo"), "{conf}");
    let cargo = fs::read_to_string(base.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("tauri-build"), "{cargo}");
    assert!(base.join("src/main.rs").exists());
    assert!(base.join("build.rs").exists());
}

#[test]
fn no_desktop_target_has_zero_artifacts_and_unknown_rejected() {
    let dir = tmp_dir("none");
    write_project(&dir, None);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(!names.contains(&"desktop-wrapper"), "{names:?}");
    let bad = tmp_dir("bad");
    write_project(&bad, Some("flutter"));
    let err = forma::build::run_build(&bad).unwrap_err();
    assert!(err.to_string().contains("supported: electron | tauri"), "{err}");
}
