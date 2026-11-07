//! M9 工程化：生成项目 Makefile + GitHub Actions CI 回归测试。

use forma::emit::engineering::{emit_backend_engineering, emit_web_engineering};
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
        "forma-eng-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app eng {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
}
api Items {
  entity Item
  operations [list get create update delete]
}
page ItemList {
  route "/items"
  title { zh "条目" en "Items" }
  table {
    query Items.list
    columns [name]
  }
}
"#;

fn make_program(dir: &Path) -> forma::dsl::Program {
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let (p, _) = forma::dsl::load_program(dir, &["forma/**/*.fm".into()]).unwrap();
    p
}

#[test]
fn backend_makefile_has_lifecycle_targets() {
    let dir = tmp_dir("makefile");
    let program = make_program(&dir);
    let files = emit_backend_engineering(&dir, &program, "generated/backend", "eng").unwrap();
    let mk = files.iter().find(|f| f.path.ends_with("Makefile")).unwrap();

    for target in [
        "build:", "check:", "test:", "run:", "migrate:", "seed:", "docker:",
    ] {
        assert!(
            mk.content.contains(target),
            "Makefile missing {target}:\n{}",
            mk.content
        );
    }
    assert!(
        mk.content.contains("DATABASE_URL ?="),
        "DATABASE_URL overridable"
    );
    assert!(
        mk.content.contains("PORT         ?= 8080"),
        "PORT overridable:\n{}",
        mk.content
    );
    // .PHONY 声明
    assert!(mk.content.contains(".PHONY"), "{}", mk.content);
}

#[test]
fn backend_ci_includes_check_test_build() {
    let dir = tmp_dir("ci");
    let program = make_program(&dir);
    let files = emit_backend_engineering(&dir, &program, "generated/backend", "eng").unwrap();
    let ci = files.iter().find(|f| f.path.ends_with("ci.yml")).unwrap();

    assert!(ci.content.contains("cargo check"), "{}", ci.content);
    assert!(ci.content.contains("cargo test"), "{}", ci.content);
    assert!(
        ci.content.contains("cargo build --release"),
        "{}",
        ci.content
    );
    assert!(
        ci.content.contains("dtolnay/rust-toolchain@stable"),
        "{}",
        ci.content
    );
}

#[test]
fn web_ci_includes_tsc_vitest_build() {
    let dir = tmp_dir("web-ci");
    let program = make_program(&dir);
    let files = emit_web_engineering(&dir, &program, "generated/web").unwrap();
    let ci = files.iter().find(|f| f.path.ends_with("ci.yml")).unwrap();

    assert!(ci.content.contains("npx tsc --noEmit"), "{}", ci.content);
    assert!(ci.content.contains("npx vitest run"), "{}", ci.content);
    assert!(ci.content.contains("npm run build"), "{}", ci.content);
}

#[test]
fn backend_makefile_written_to_disk() {
    let dir = tmp_dir("makefile-disk");
    let program = make_program(&dir);
    emit_backend_engineering(&dir, &program, "generated/backend", "eng").unwrap();
    assert!(dir.join("generated/backend/Makefile").is_file());
    assert!(dir
        .join("generated/backend/.github/workflows/ci.yml")
        .is_file());
}
