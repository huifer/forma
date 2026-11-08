//! spec:protocols §10.5：命令幂等键（Idempotency-Key）跨端一致性回归。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-idem-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app idem { version = "0.1.0" }
auth module { provider jwt roles [admin] }
entity Enrollment {
  id     Uuid       @id @default(uuid)
  status String(20)
}
api Enrollments {
  entity Enrollment
  operations [list get]
}

page EnrollmentList {
  route "/enrollments"
  title { en "Enrollments" }
  table {
    query   Enrollments.list
    columns [status]
    actions { row [delete → Enrollments.delete confirm] }
  }
}

api EnrollmentApprove {
  POST "/enrollments/{id}/approve"
  label { en "Approve" }
  input { note String? }
  pre { status == "pending" }
  effect { set Enrollment.status = "approved" }
  audit { action "enrollment.approve" }
}
"#;

fn build_project(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: idem\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  db:\n    engine: mysql\n    out: db\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n  mobile:\n    out: app_mobile\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    forma::build::run_build(dir).unwrap();
}

fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).unwrap()
}

#[test]
fn audit_log_persists_idempotency_key() {
    let dir = tmp("audit");
    build_project(&dir);
    // DDL：audit_logs 含 idempotency_key 列（可空，兼容存量）
    let sql = read(&dir, "db/migrations/0001_init.sql");
    assert!(sql.contains("idempotency_key"), "{sql}");
    // 审计写入含该列
    let audit = read(&dir, "generated/backend/src/audit.rs");
    assert!(audit.contains("idempotency_key"), "{audit}");
}

#[test]
fn command_handler_reads_idempotency_key_header() {
    let dir = tmp("handler");
    build_project(&dir);
    let routes = read(&dir, "generated/backend/src/routes.rs");
    assert!(
        routes.contains("headers.get(\"idempotency-key\")"),
        "{routes}"
    );
    // log_action 调用把键传给审计
    assert!(routes.contains("idempotency_key,"), "{routes}");
}

#[test]
fn web_and_flutter_clients_generate_idempotency_keys() {
    let dir = tmp("clients");
    build_project(&dir);
    let web = read(&dir, "generated/web/src/api.ts");
    assert!(
        web.contains("\"Idempotency-Key\": crypto.randomUUID()"),
        "{web}"
    );
    let dart = read(&dir, "app_mobile/lib/api/api.dart");
    assert!(dart.contains("newIdempotencyKey()"), "{dart}");
    // Flutter 键生成器存在且为顶层函数
    assert!(dart.contains("String newIdempotencyKey()"), "{dart}");
}
