//! Redis 键设计派生回归测试（roadmap M9 / P1-#2）。
//!
//! 验收：`targets.db.engine: redis` 时构建产出 `redis/key-design.{md,json}`，
//! 键名带 `forma:{project}:{version}` 边界前缀、覆盖缓存/索引/唯一/会话/任务/限流；
//! 默认 engine（mysql）与 postgres/sqlite 工程零 Redis 产物。
//!
//! 并行安全：产物写入 tmp_dir("唯一 tag")。

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
        "forma-redis-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_manifest(dir: &Path, db_engine: &str, db_out: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            r#"project:
  name: redisdemo
  version: 0.1.0

source: ["forma/**/*.fm"]

targets:
  db:
    engine: {db_engine}
    out: {db_out}
"#
        ),
    )
    .unwrap();
}

fn write_app(dir: &Path) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"app redisdemo {
  version = "0.1.0"
}

auth module {
  provider jwt
  roles [admin, user]
  tokenHours 2
}

entity Article {
  id        Uuid @id @default(uuid)
  author    User @required
  title     String(200) @required
  status    String
  createdAt Timestamp @default(now) @readonly

  index [author]
  unique [title]
}

entity User {
  id    Uuid @id @default(uuid)
  name  String(50) @required

  unique [name]
}

api Articles {
  entity Article
  operations [list get create update delete]
}

job cleanup {
  cron "0 2 * * *"
  op Articles
}

live ArticleFeed {
  api Articles.list
  events [create update delete]
}
"#,
    )
    .unwrap();
}

#[test]
fn redis_target_derives_key_design() {
    let dir = tmp_dir("derive");
    write_manifest(&dir, "redis", "db");
    write_app(&dir);

    let report = forma::build::run_build(&dir).unwrap();
    assert!(
        report.steps.iter().any(|s| s.name == "db-redis"),
        "build must include db-redis step: {:?}",
        report.steps.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
    );

    let md = fs::read_to_string(dir.join("db/redis/key-design.md")).unwrap();
    let json = fs::read_to_string(dir.join("db/redis/key-design.json")).unwrap();

    // 前缀带 project:version 边界
    assert!(md.contains("forma:redisdemo:0.1.0"), "md prefix:\n{md}");
    // 单实体缓存
    assert!(md.contains("forma:redisdemo:0.1.0:cache:article:{id}"));
    // 列表缓存
    assert!(md.contains(":cache:article:list"));
    // 索引 → zset
    assert!(md.contains("forma:redisdemo:0.1.0:idx:article:author"));
    // 唯一约束 → set
    assert!(md.contains("forma:redisdemo:0.1.0:uq:article:title:{value}"));
    // auth 会话（tokenHours 2 → 7200s）
    assert!(md.contains("forma:redisdemo:0.1.0:session:{token}"));
    assert!(json.contains("\"ttlSecs\": 7200"));
    // 任务锁
    assert!(md.contains("forma:redisdemo:0.1.0:job:cleanup:last_run"));
    // SSE 订阅集合
    assert!(md.contains("forma:redisdemo:0.1.0:live:article_feed:subscribers"));
    // 限流
    assert!(md.contains("forma:redisdemo:0.1.0:rl:{key}:{window}"));

    // JSON 可解析且 keyCount > 0
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["keyCount"].as_u64().unwrap() > 0);
}

#[test]
fn default_and_relational_engines_produce_no_redis_artifacts() {
    // 默认（无 engine）→ mysql 主线零变化
    let dir = tmp_dir("default");
    write_manifest(&dir, "mysql", "db");
    write_app(&dir);
    let _ = forma::build::run_build(&dir).unwrap();
    assert!(!dir.join("db/redis").exists(), "mysql project must have no redis artifacts");

    // postgres / sqlite 同样零 redis 产物
    for engine in ["postgres", "sqlite"] {
        let d = tmp_dir(engine);
        write_manifest(&d, engine, "db");
        write_app(&d);
        let _ = forma::build::run_build(&d).unwrap();
        assert!(
            !d.join("db/redis").exists(),
            "{engine} project must have no redis artifacts"
        );
    }
}
