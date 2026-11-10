//! 工作流定义导出（轨道 B 重新排期）回归：状态机派生、路由、opt-in。

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
    let dir = std::env::temp_dir().join(format!("forma-wf-{tag}-{}-{seq}-{nanos}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, workflow: Option<&str>) {
    let wf = match workflow {
        Some(v) => format!("\n  workflow:\n    framework: {v}\n    out: generated/workflow"),
        None => String::new(),
    };
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: wfdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend{wf}"),
    ).unwrap();
    fs::write(dir.join("forma/app.fm"), r#"
app wfdemo { version = "0.1.0" }
enum OrderStatus {
  draft
  paid
}
entity Order {
  id     Uuid        @id @default(uuid)
  status OrderStatus @default(draft)
  amount Decimal(12,2)
}
api Orders { entity Order operations [list get create update delete] }
api OrderPay {
  POST "/orders/{id}/pay"
  pre  { status == "draft" }
  effect { set Order.status = "paid" }
  audit { action "order.pay" }
}
"#).unwrap();
}

#[test]
fn workflow_target_generates_state_machine_spec() {
    let dir = tmp_dir("gen");
    write_project(&dir, Some("json"));
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"workflow-spec"), "{names:?}");
    let spec: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("generated/workflow/workflow.json")).unwrap(),
    ).unwrap();
    assert_eq!(spec["schema"], "forma-workflow/v1");
    // 状态：枚举 → 状态集
    let states = spec["states"].as_array().unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0]["entity"], "Order");
    assert!(states[0]["values"].as_array().unwrap().len() == 2);
    // 迁移：命令 → pre/effect
    let transitions = spec["transitions"].as_array().unwrap();
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0]["name"], "OrderPay");
    assert_eq!(transitions[0]["effect_value"], "paid");
    assert_eq!(transitions[0]["audit"], "order.pay");
    // README
    let readme = fs::read_to_string(dir.join("generated/workflow/README.md")).unwrap();
    assert!(readme.contains("OrderPay"), "{readme}");
}

#[test]
fn no_workflow_declaration_produces_zero_artifacts() {
    let dir = tmp_dir("none");
    write_project(&dir, None);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(!names.contains(&"workflow-spec"), "{names:?}");
    assert!(!dir.join("generated/workflow").exists());
}
