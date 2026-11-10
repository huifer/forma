//! 支付集成（Stripe PaymentIntent）回归：opt-in 路由、产物、显式拒绝。

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
        "forma-pay-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, payments: Option<&str>) {
    let pay = match payments {
        Some(v) => format!("\n    payments: {v}"),
        None => String::new(),
    };
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: paydemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend{pay}"),
    ).unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app paydemo { version = "0.1.0" }
entity Order {
  id     Uuid          @id @default(uuid)
  amount Decimal(12,2) @required
}
api Orders { entity Order operations [list get create update delete] }
"#,
    )
    .unwrap();
}

#[test]
fn stripe_payments_generates_intent_and_webhook() {
    let dir = tmp_dir("gen");
    write_project(&dir, Some("stripe"));
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"backend-payments"), "{names:?}");
    let pay = fs::read_to_string(dir.join("generated/backend/src/payments.rs")).unwrap();
    assert!(pay.contains("payment_intents"), "{pay}");
    assert!(pay.contains("STRIPE_SECRET_KEY"), "{pay}");
    assert!(pay.contains("stripe_webhook"), "{pay}");
    assert!(pay.contains("FORMA_PAYMENTS_UNCONFIGURED"), "{pay}");
    assert!(pay.contains("/api/payments"), "{pay}");
}

#[test]
fn no_payments_config_has_zero_payment_artifacts() {
    let dir = tmp_dir("none");
    write_project(&dir, None);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(!names.contains(&"backend-payments"), "{names:?}");
    assert!(!dir.join("generated/backend/src/payments.rs").exists());
}

#[test]
fn entity_without_decimal_rejects_payments() {
    let dir = tmp_dir("no-decimal");
    write_project(&dir, Some("stripe"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app paydemo { version = "0.1.0" }
entity Note {
  id   Uuid @id @default(uuid)
  body Text
}
api Notes { entity Note operations [list get create update delete] }
"#,
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    assert!(err.to_string().contains("Decimal"), "{err}");
}
