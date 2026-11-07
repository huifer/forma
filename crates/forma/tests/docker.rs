//! Docker / docker-compose 产物回归测试（M9 / F-002）：
//! - Dockerfile 是多阶段构建；
//! - compose build context 与 Dockerfile/Cargo/migrations 路径一致；
//! - entrypoint 等库 + 迁移 + 启动，环境变量可覆盖。

use forma::emit::emit_docker;
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
        "forma-docker-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app docker {
  version = "0.1.0"
}

entity Order {
  id Uuid @id @default(uuid)
  title String(80) @required
}

api Orders {
  entity Order
  operations [list get create update delete]
}

page OrderList {
  route "/orders"
  title { zh "订单" en "Orders" }
  table {
    query Orders.list
    columns [title]
  }
}
"#;

fn project(dir: &Path) -> forma::dsl::Program {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: docker\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let (p, diags) = forma::dsl::load_program(dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    p
}

#[test]
fn docker_artifacts_are_deployable_and_path_consistent() {
    let dir = tmp_dir("shape");
    let program = project(&dir);
    let files = emit_docker(&dir, &program, ".", "docker").unwrap();
    let get = |suffix: &str| {
        files
            .iter()
            .find(|f| f.path.ends_with(suffix))
            .unwrap()
            .content
            .clone()
    };
    let dockerfile = get("generated/backend/Dockerfile");
    let compose = get("docker-compose.yml");
    let entrypoint = get("generated/backend/entrypoint.sh");
    let ignore = get("generated/backend/.dockerignore");

    assert!(dockerfile.contains("FROM rust:"), "{dockerfile}");
    assert!(
        dockerfile.contains("FROM debian:bookworm-slim"),
        "{dockerfile}"
    );
    assert!(
        dockerfile.contains("COPY generated/backend/Cargo.toml"),
        "{dockerfile}"
    );
    assert!(
        dockerfile.contains("COPY db/migrations /app/migrations"),
        "{dockerfile}"
    );
    assert!(dockerfile.contains("ENV PORT=8080"), "{dockerfile}");
    assert!(
        dockerfile.contains("ENV DATABASE_URL=mysql://root:root@db:3306/docker"),
        "{dockerfile}"
    );

    assert!(compose.contains("image: mysql:8"), "{compose}");
    assert!(
        compose.contains("dockerfile: generated/backend/Dockerfile"),
        "{compose}"
    );
    assert!(compose.contains("condition: service_healthy"), "{compose}");
    assert!(compose.contains("db_data:/var/lib/mysql"), "{compose}");
    assert!(
        compose.contains("VITE_API_BASE: http://backend:8080/api"),
        "{compose}"
    );

    assert!(entrypoint.contains("mysql -h"), "{entrypoint}");
    assert!(
        entrypoint.contains("/app/migrations/*_upgrade.sql"),
        "{entrypoint}"
    );
    assert!(entrypoint.contains("exec \"$@\""), "{entrypoint}");
    assert!(ignore.contains("target/"), "{ignore}");

    // emitter contract：文件也确实落盘，而不是只返回 GeneratedFile
    assert!(dir.join("docker-compose.yml").is_file());
    assert!(dir.join("generated/backend/Dockerfile").is_file());
    assert!(dir.join("generated/backend/entrypoint.sh").is_file());
}

#[test]
fn no_entities_means_no_docker_artifacts() {
    let dir = tmp_dir("empty");
    let program = forma::dsl::Program {
        ir_version: forma::dsl::IR_VERSION.into(),
        app: None,
        enums: vec![],
        entities: vec![],
        rules: vec![],
        apis: vec![],
        commands: vec![],
        pages: vec![],
        charts: vec![],
        jobs: Vec::new(),
        lives: Vec::new(),
        exports: Vec::new(),
        auth: None,
    };
    assert!(emit_docker(&dir, &program, ".", "empty")
        .unwrap()
        .is_empty());
}
