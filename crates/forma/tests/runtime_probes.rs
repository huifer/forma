//! RFC-0005 §16：生成应用的 `/health` / `/ready` 探针与密钥注入契约。
//!
//! 验收：`/health` 不依赖外部服务；`/ready` 真实探测数据库且失败 503（fail-closed）；
//! 探针公开挂载（不经业务鉴权）；完整 router 与 entity-only 极简 router 都提供；
//! 生成产物不硬编码密钥值（只经环境变量注入，缺失时关闭对应能力）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-probes-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn manifest() -> &'static str {
    "project:\n  name: probes\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n"
}

fn project(dir: &Path, fm: &str) {
    fs::write(dir.join("forma.yaml"), manifest()).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const FULL: &str = r#"
app probes { version = "0.1.0" }
auth module { provider jwt roles [admin] tokenHours 12 }
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
}
api Items { entity Item operations [list get create update delete] }
"#;

const ENTITY_ONLY: &str = r#"
app probes { version = "0.1.0" }
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
}
"#;

fn routes(dir: &Path) -> String {
    fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap()
}

#[test]
fn generated_backend_exposes_public_health_and_ready_probes() {
    let dir = tmp("full");
    project(&dir, FULL);
    forma::build::run_build(&dir).unwrap();
    let r = routes(&dir);
    assert!(r.contains("pub fn health_routes(db: Db)"), "{r}");
    assert!(
        r.contains(".route(\"/health\", axum::routing::get(health_handler))"),
        "{r}"
    );
    assert!(
        r.contains(".route(\"/ready\", axum::routing::get(ready_handler))"),
        "{r}"
    );
    assert!(
        r.contains("app = app.merge(health_routes(db.clone()))"),
        "{r}"
    );
    // ready 真实探测数据库，失败返回 503（fail-closed）
    assert!(
        r.contains("sqlx::query(\"SELECT 1\").execute(db.as_ref()).await"),
        "{r}"
    );
    assert!(r.contains("SERVICE_UNAVAILABLE"), "{r}");
    // 探针挂公开 app，不在 protected 组内（require_auth 只作用于 protected）
    let health_pos = r.find("app = app.merge(health_routes").unwrap();
    let protected_pos = r.find("let mut protected").unwrap();
    assert!(
        health_pos < protected_pos,
        "probes must be registered before protected group"
    );
    assert!(!r.contains("health_routes(db.clone()).layer"), "{r}");
}

#[test]
fn entity_only_backend_also_exposes_probes_and_has_no_dead_code_warnings() {
    let dir = tmp("entityonly");
    project(&dir, ENTITY_ONLY);
    forma::build::run_build(&dir).unwrap();
    let r = routes(&dir);
    assert!(r.contains("pub fn health_routes(db: Db)"), "{r}");
    assert!(r.contains("health_routes(Arc::new(db))"), "{r}");
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(
        main.contains("#![allow(dead_code)]"),
        "entity-only contract modules need crate-level allow: {main}"
    );
}

#[test]
fn config_failures_are_structured_fail_closed_not_panics() {
    // RFC-0005 §16：配置 / 连接失败 → 结构化错误码 + 非零退出，而非 panic
    let dir = tmp("failclosed");
    project(&dir, FULL);
    forma::build::run_build(&dir).unwrap();
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(
        !main.contains("expect(\"DATABASE_URL"),
        "配置缺失不得 panic: {main}"
    );
    assert!(
        !main.contains("expect(\"failed to connect"),
        "连接失败不得 panic: {main}"
    );
    assert!(main.contains("FORMA_CONFIG_MISSING"), "{main}");
    assert!(main.contains("FORMA_DB_UNREACHABLE"), "{main}");
    assert!(main.contains("std::process::exit(1)"), "{main}");
}

#[test]
fn generated_artifacts_do_not_hardcode_secret_values() {
    let dir = tmp("secrets");
    project(&dir, FULL);
    forma::build::run_build(&dir).unwrap();
    let src = dir.join("generated/backend/src");
    let auth = fs::read_to_string(src.join("auth.rs")).unwrap();
    // 密钥只经环境变量注入，且未设置时降级为开发默认值（非真实凭据）
    assert!(auth.contains("std::env::var(\"JWT_SECRET\")"), "{auth}");
    assert!(!auth.contains("PRIVATE KEY"), "{auth}");
    assert!(!auth.contains("BEGIN RSA"), "{auth}");
    // 全量扫描：生成源码里不得出现疑似硬编码凭据字面量
    for entry in fs::read_dir(&src).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&p).unwrap();
        assert!(!text.contains("PRIVATE KEY"), "{}: {}", p.display(), text);
        assert!(!text.contains("BEGIN RSA"), "{}: {}", p.display(), text);
        assert!(!text.contains("password = \""), "{}: {}", p.display(), text);
    }
    // JWT_SECRET 只作为环境变量名出现（读取点唯一）
    assert_eq!(auth.matches("JWT_SECRET").count(), 1, "{auth}");
}

#[test]
fn tenant_admin_token_is_env_gated_without_hardcoded_value() {
    let dir = tmp("tenant");
    project(
        &dir,
        r#"
app probes { version = "0.1.0" }
auth module { provider jwt roles [admin] tokenHours 12 }
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  tenant String @tenant @required
}
api Items { entity Item operations [list get create update delete] }
"#,
    );
    forma::build::run_build(&dir).unwrap();
    let auth = fs::read_to_string(dir.join("generated/backend/src/auth.rs")).unwrap();
    // 未设置 env → 403 关闭（默认安全），不内置 token 字面量
    assert!(
        auth.contains("std::env::var(\"TENANT_ADMIN_TOKEN\")"),
        "{auth}"
    );
    assert!(auth.contains("AppError::Forbidden"), "{auth}");
    assert!(auth.contains("tenant admin is disabled"), "{auth}");
}
