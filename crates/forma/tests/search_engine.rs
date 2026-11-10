//! 外部搜索引擎（Meilisearch / Elasticsearch）回归测试。
//!
//! 契约：
//! - 存在 `searchable` 实体 → 生成 `src/search.rs`（引擎客户端 + 写路径同步 + 读路径 id 过滤）；
//! - 引擎选择运行时化（`SEARCH_ENGINE` 环境变量；未配置 → no-op，回退 SQL LIKE）；
//! - create/update/delete handler 带索引同步钩子；list repo 带引擎分支；
//! - 条件依赖：reqwest 仅当存在 searchable 实体时引入。

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
        "forma-search-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: searchdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const APP: &str = r#"app searchdemo { version = "0.1.0" }
entity Article {
  id      Uuid @id @default(uuid)
  title   String(200) @required
  body    Text
  searchable [title, body]
}
api Articles {
  entity Article
  operations [list get create update delete]
  list { search [title, body] paginate offset }
}
"#;

#[test]
fn searchable_entity_generates_engine_adapter() {
    let dir = tmp_dir("gen");
    write_project(&dir, APP);
    forma::build::run_build(&dir).unwrap();

    let search = fs::read_to_string(dir.join("generated/backend/src/search.rs")).unwrap();
    assert!(search.contains("SEARCH_SPECS"), "{search}");
    assert!(
        search.contains("SearchSpec { table: \"articles\" }"),
        "{search}"
    );
    assert!(search.contains("fn engine()"), "{search}");
    assert!(search.contains("search_ids"), "{search}");
    assert!(search.contains("index_document"), "{search}");
    assert!(search.contains("delete_document"), "{search}");
    assert!(search.contains("MEILISEARCH_URL"), "{search}");
    assert!(search.contains("ELASTICSEARCH_URL"), "{search}");
    // 未配置引擎 → None（回退 LIKE）
    assert!(search.contains("falling back to SQL LIKE"), "{search}");

    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(main.contains("mod search;"), "{main}");

    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(cargo.contains("reqwest"), "{cargo}");

    // 写路径钩子
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(
        routes.contains("crate::search::index_document(\"articles\""),
        "{routes}"
    );
    assert!(
        routes.contains("crate::search::delete_document(\"articles\""),
        "{routes}"
    );

    // 读路径分支
    let repo = fs::read_to_string(dir.join("generated/backend/src/repo.rs")).unwrap();
    assert!(
        repo.contains("let use_engine = engine_ids.is_some();"),
        "{repo}"
    );
    assert!(
        repo.contains("crate::search::search_ids(\"articles\""),
        "{repo}"
    );
    assert!(repo.contains("s.id IN"), "{repo}");
}

#[test]
fn project_without_searchable_has_no_search_module() {
    let dir = tmp_dir("none");
    write_project(
        &dir,
        r#"app searchdemo { version = "0.1.0" }
entity Note {
  id   Uuid @id @default(uuid)
  body Text
}
api Notes { entity Note operations [list get create update delete] }
"#,
    );
    forma::build::run_build(&dir).unwrap();
    assert!(!dir.join("generated/backend/src/search.rs").exists());
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(!main.contains("mod search;"), "{main}");
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(!cargo.contains("reqwest"), "{cargo}");
}
