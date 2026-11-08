//! zustand 状态生成（M9/P1）回归测试：`targets.frontend.usesZustand: true` opt-in。
//!
//! 验收：
//! - opt-in 工程：package.json 引入 zustand，生成 src/store.ts（每列表页一个 store，
//!   从页面数据依赖派生 filters/search），列表页状态声明切换到 store（setter 签名兼容）；
//! - 缺省（无 usesZustand）：局部 useState 零变化（无 store.ts / zustand）。

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
        "forma-zustand-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_manifest(dir: &Path, uses_zustand: bool) {
    let flag = if uses_zustand { "true" } else { "false" };
    fs::write(
        dir.join("forma.yaml"),
        format!(
            r#"project:
  name: zdemo
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
    usesZustand: {flag}
"#
        ),
    )
    .unwrap();
}

const APP: &str = r#"app zdemo {
  version = "0.1.0"
}

entity Task {
  id     Uuid @id @default(uuid)
  title  String(100) @required
  status String
  createdAt Timestamp @default(now) @readonly
}

api Tasks {
  entity Task
  operations [list get create update delete]
  list {
    filters [status]
    search  [title]
    sort    [createdAt desc]
    paginate offset
  }
}

page Tasks {
  route "/tasks"
  title { zh "任务" en "Tasks" }
  table {
    query   Tasks.list
    columns [title, status, createdAt: datetime]
  }
}
"#;

#[test]
fn zustand_optin_derives_store_from_page_dependencies() {
    let dir = tmp_dir("optin");
    fs::create_dir_all(dir.join("forma")).unwrap();
    write_manifest(&dir, true);
    fs::write(dir.join("forma/app.fm"), APP).unwrap();
    forma::build::run_build(&dir).unwrap();

    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(pkg.contains("zustand"), "{pkg}");

    let store = fs::read_to_string(dir.join("generated/web/src/store.ts")).unwrap();
    assert!(store.contains("import { create } from \"zustand\""));
    assert!(store.contains("export const useTaskListStore = create<"));
    // 从页面数据依赖派生：filters [status]
    assert!(store.contains("filters [status]"));

    let page = fs::read_to_string(dir.join("generated/web/src/pages/Tasks.tsx")).unwrap();
    // 状态声明切换到 store（setter 签名兼容：setFilters 仍是 updater 调用）
    assert!(page.contains("const filters = useTaskListStore((s) => s.filters);"));
    assert!(page.contains("const setFilters = useTaskListStore((s) => s.setFilters);"));
    assert!(page.contains("const search = useTaskListStore((s) => s.search);"));
    assert!(!page.contains("useState<Record<string, string | null>>"));
    // store 导入
    assert!(page.contains("import { useTaskListStore } from \"../store\";"));
    // queryKey / 输入绑定零变化
    assert!(page.contains("queryKey: [\"task\", filters, search, page]"));
    assert!(page.contains("onChange={(e) => setSearch(e.target.value)}"));
}

#[test]
fn default_project_keeps_local_usestate_zero_change() {
    let dir = tmp_dir("default");
    fs::create_dir_all(dir.join("forma")).unwrap();
    write_manifest(&dir, false);
    fs::write(dir.join("forma/app.fm"), APP).unwrap();
    forma::build::run_build(&dir).unwrap();

    assert!(!dir.join("generated/web/src/store.ts").exists());
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(!pkg.contains("zustand"), "{pkg}");
    let page = fs::read_to_string(dir.join("generated/web/src/pages/Tasks.tsx")).unwrap();
    // V0.4 drill-down：初始筛选从 URL query 读取（dashboard 跳转携带）
    assert!(
        page.contains("new URLSearchParams(window.location.search)"),
        "{page}"
    );
    assert!(!page.contains("useTaskListStore"));
}
