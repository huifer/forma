//! Spring Boot target（轨道 B 重新排期）回归：Java/JPA 产物、路由、显式拒绝、默认零变化。

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
        "forma-spring-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app springdemo {
  version = "0.1.0"
}

entity Order {
  id        Uuid       @id @default(uuid)
  title     String(80) @required
  amount    Decimal(12,2)
  paid      Bool
  createdAt Timestamp  @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
}
"#;

fn write_project(dir: &Path, language: &str, framework: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: springdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: {language}\n    framework: {framework}\n    out: generated/backend\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn spring_target_generates_jpa_project() {
    let dir = tmp_dir("gen");
    write_project(&dir, "java", "spring");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-spring"), "{names:?}");
    assert!(!names.contains(&"backend-rust"), "{names:?}");

    let pom = fs::read_to_string(dir.join("generated/backend/pom.xml")).unwrap();
    assert!(pom.contains("spring-boot-starter-web"), "{pom}");
    assert!(pom.contains("spring-boot-starter-data-jpa"), "{pom}");
    let root = dir.join("generated/backend/src/main/java/dev/forma/springdemo");
    let entity = fs::read_to_string(root.join("order/Order.java")).unwrap();
    assert!(entity.contains("@Entity"), "{entity}");
    assert!(entity.contains("@Table(name = \"orders\")"), "{entity}");
    assert!(entity.contains("private BigDecimal amount;"), "{entity}");
    let ctrl = fs::read_to_string(root.join("order/OrderController.java")).unwrap();
    assert!(ctrl.contains("@RestController"), "{ctrl}");
    assert!(ctrl.contains("@RequestMapping(\"/api/orders\")"), "{ctrl}");
    assert!(ctrl.contains("@GetMapping"), "{ctrl}");
    assert!(ctrl.contains("@PostMapping"), "{ctrl}");
    let repo = fs::read_to_string(root.join("order/OrderRepository.java")).unwrap();
    assert!(repo.contains("JpaRepository<Order, String>"), "{repo}");
    let dto = fs::read_to_string(root.join("order/OrderDto.java")).unwrap();
    assert!(!dto.contains("createdAt"), "{dto}"); // readonly 排除
}

#[test]
fn spring_rejects_unsupported_security_semantics() {
    let dir = tmp_dir("reject");
    write_project(&dir, "java", "spring");
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app springdemo { version = "0.1.0" }
entity Order { id Uuid @id @default(uuid) }
api Orders { entity Order operations [list get create update delete] }
auth module { provider jwt roles [admin] }
"#,
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    assert!(err.to_string().contains("does not support"), "{err}");
}

#[test]
fn rust_default_remains_unchanged() {
    let dir = tmp_dir("default");
    write_project(&dir, "rust", "axum");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-rust"), "{names:?}");
    assert!(!names.contains(&"backend-spring"), "{names:?}");
}
