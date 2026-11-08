//! A-7：补丁操作集扩展到图表 / 看板节点（spec:ir §15.4）。
//!
//! 验收：新操作走既有事务（快照 → 编辑 → check_program → 重建 → 错误整体回滚）；
//! 图表类型白名单、看板栅格边界、重复槽位均有显式拒绝。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-patchviz-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const BASE: &str = r#"app vizpatch {
  version = "0.1.0"
}

entity Order {
  id        Uuid @id @default(uuid)
  customer  String(50) @required
  total     Decimal(12,2)
  status    String
  createdAt Timestamp @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
  list {
    sort [createdAt desc]
  }
}

chart ByCustomer {
  type bar
  title { en "By customer" }
  source Orders.list
  encoding {
    x { field customer type nominal }
    y { field total type quantitative }
  }
}

chart ByStatus {
  type pie
  title { en "By status" }
  source Orders.list
  encoding {
    x { field status type nominal }
    y { field total type quantitative }
  }
}

page SalesDashboard {
  route "/dashboard"
  title { en "Sales" }
  dashboard {
    grid {
      ByCustomer w 6 h 220
    }
  }
}
"#;

fn write_project(dir: &Path, fm: &str) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: vizpatch\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

fn source(dir: &Path) -> String {
    fs::read_to_string(dir.join("forma/app.fm")).unwrap()
}

fn patch(dir: &Path, ops: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    forma::patch::apply_patch_input(
        dir,
        forma::patch::PatchInput::Dsl(serde_json::from_value(serde_json::json!({
            "operations": ops
        }))?),
    )
}

#[test]
fn set_chart_type_applies_and_passes_revalidation() {
    let dir = tmp("charttype");
    write_project(&dir, BASE);
    let report = patch(
        &dir,
        serde_json::json!([
            { "op": "set-chart-type", "chart": "ByCustomer", "chartType": "line" }
        ]),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    assert_eq!(report["operations"][0]["op"], "set-chart-type");
    let src = source(&dir);
    assert!(src.contains("type line"), "{src}");
    assert!(!src.contains("type bar"), "{src}");
    // 语义检查仍通过
    let (p, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.iter().all(|d| d.severity != "error"), "{diags:?}");
    let sem = forma::dsl::check_program(&p);
    assert!(sem.iter().all(|d| d.severity != "error"), "{sem:?}");
}

#[test]
fn set_chart_type_rejects_unknown_type_with_zero_writes() {
    let dir = tmp("chartbad");
    write_project(&dir, BASE);
    let before = source(&dir);
    let report = patch(
        &dir,
        serde_json::json!([
            { "op": "set-chart-type", "chart": "ByCustomer", "chartType": "waterfall" }
        ]),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "failed", "{report}");
    assert_eq!(source(&dir), before, "失败必须零写入");
    assert!(
        report["operations"][0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("unknown chart type"),
        "{report}"
    );
}

#[test]
fn set_chart_type_invalid_transition_rolls_back_transaction() {
    let dir = tmp("chartrollback");
    write_project(&dir, BASE);
    let before = source(&dir);
    // metric 禁止编码通道 → 编辑后 check_program 报错 → 整体回滚
    let report = patch(
        &dir,
        serde_json::json!([
            { "op": "set-chart-type", "chart": "ByCustomer", "chartType": "metric" }
        ]),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "failed", "{report}");
    assert_eq!(source(&dir), before, "语义错误必须整体回滚");
}

#[test]
fn add_dashboard_chart_inserts_grid_slot() {
    let dir = tmp("dashadd");
    write_project(&dir, BASE);
    let report = patch(
        &dir,
        serde_json::json!([
            { "op": "add-dashboard-chart", "page": "SalesDashboard", "chart": "ByStatus", "w": 6, "h": 260 }
        ]),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    let src = source(&dir);
    assert!(src.contains("ByStatus w 6 h 260"), "{src}");
    // 缩进沿用既有条目
    assert!(src.contains("      ByStatus w 6 h 260"), "{src}");
    let (p, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let sem = forma::dsl::check_program(&p);
    assert!(sem.iter().all(|d| d.severity != "error"), "{sem:?}");
}

#[test]
fn add_dashboard_chart_rejects_duplicate_and_bad_geometry() {
    let dir = tmp("dashbad");
    write_project(&dir, BASE);
    let before = source(&dir);
    for ops in [
        serde_json::json!([
            { "op": "add-dashboard-chart", "page": "SalesDashboard", "chart": "ByCustomer", "w": 6, "h": 220 }
        ]),
        serde_json::json!([
            { "op": "add-dashboard-chart", "page": "SalesDashboard", "chart": "ByStatus", "w": 13, "h": 220 }
        ]),
        serde_json::json!([
            { "op": "add-dashboard-chart", "page": "SalesDashboard", "chart": "ByStatus", "w": 6, "h": 0 }
        ]),
    ] {
        let report = patch(&dir, ops).unwrap();
        assert_eq!(report["operations"][0]["status"], "failed", "{report}");
        assert_eq!(source(&dir), before, "拒绝必须零写入");
    }
}
