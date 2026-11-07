//! React Hook Form 适配（M9/P1）回归测试：`targets.frontend.usesRhf: true` opt-in。
//!
//! 验收：
//! - opt-in 工程：package.json 引入 react-hook-form，表单页用 `useForm`/`register`/
//!   `useController`，校验仍复用 validators.ts（不引入第二套 schema）；
//! - 缺省（无 usesRhf）：受控 state 表单零变化（无 react-hook-form / useForm）。

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
        "forma-rhf-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_manifest(dir: &Path, uses_rhf: bool) {
    let flag = if uses_rhf { "true" } else { "false" };
    fs::write(
        dir.join("forma.yaml"),
        format!(
            r#"project:
  name: rhfdemo
  version: 0.1.0

source: ["forma/**/*.fm"]

targets:
  db:
    engine: mysql
    out: db
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
  frontend:
    framework: react
    out: generated/web
    usesRhf: {flag}
"#
        ),
    )
    .unwrap();
}

const APP: &str = r#"app rhfdemo {
  version = "0.1.0"
}

enum Status {
  draft
  active
}

entity Task {
  id     Uuid @id @default(uuid)
  title  String(100) @required
  note   Text?
  status Status @default(draft)
  owner  User @required
  createdAt Timestamp @default(now) @readonly
}

entity User {
  id   Uuid @id @default(uuid)
  name String(50) @required
}

api Tasks {
  entity Task
  operations [list get create update delete]
}

api Users {
  entity User
  operations [list get]
}

page Tasks {
  route "/tasks"
  title { zh "任务" en "Tasks" }
  table {
    query   Tasks.list
    columns [title, status, createdAt: datetime]
  }
}

page TaskEdit {
  route "/tasks/{id}"
  title { zh "编辑任务" en "Edit Task" }
  form Task {
    layout stack {
      title  Input @required
      note   Textarea
      status Select
      owner  Select
    }
    submit → Tasks.update redirect "/tasks"
    submit create → Tasks.create redirect "/tasks"
  }
}
"#;

#[test]
fn rhf_optin_generates_react_hook_form_pages() {
    let dir = tmp_dir("optin");
    fs::create_dir_all(dir.join("forma")).unwrap();
    write_manifest(&dir, true);
    fs::write(dir.join("forma/app.fm"), APP).unwrap();
    forma::build::run_build(&dir).unwrap();

    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(pkg.contains("react-hook-form"), "{pkg}");

    let pages = dir.join("generated/web/src/pages");
    let page = fs::read_to_string(pages.join("TaskEdit.tsx")).unwrap();
    assert!(page.contains("import { useForm, useController } from \"react-hook-form\""));
    assert!(page.contains("const form = useForm<Record<string, any>>();"));
    // 文本字段用 register
    assert!(page.contains("form.register(\"title\")"));
    // Select 用 useController（受控 field.value/onChange）
    assert!(page.contains("useController({ name: \"owner\", control: form.control })"));
    assert!(page.contains("useController({ name: \"status\", control: form.control })"));
    // 校验仍走 validators.ts（单一来源），错误用 setError
    assert!(page.contains("validateCreateTask(values)"));
    assert!(page.contains("form.setError(er.field, { message: er.message })"));
    // 关系下拉的 options query 仍然生成
    assert!(page.contains("UserOptionsQuery"));
}

#[test]
fn default_project_keeps_controlled_form_zero_change() {
    let dir = tmp_dir("default");
    fs::create_dir_all(dir.join("forma")).unwrap();
    write_manifest(&dir, false);
    fs::write(dir.join("forma/app.fm"), APP).unwrap();
    forma::build::run_build(&dir).unwrap();

    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(!pkg.contains("react-hook-form"), "{pkg}");
    let page = fs::read_to_string(dir.join("generated/web/src/pages/TaskEdit.tsx")).unwrap();
    assert!(!page.contains("useForm"));
    assert!(page.contains("setValues((p) =>"));
}
