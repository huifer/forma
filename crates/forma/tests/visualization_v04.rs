//! RFC-0010 V0.4：Tooltip/Legend（已具备）+ 联动事件模型（图表点击 → dashboard 筛选）。
//!
//! 验收：
//! - `select` 事件载荷带类目名（`name`）；
//! - dashboard 页把图表点击接到筛选器状态（x 字段在 filters 中声明时）；
//! - x 字段未声明为筛选器时不接线（避免 ListParams 类型不匹配）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-viz04-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app viz04 { version = "0.1.0" }

entity Order {
  id       Uuid @id @default(uuid)
  status   String(20)
  amount   Decimal(12,2)
  customer String(50)
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [status, customer] }
}

chart ByStatus {
  type bar
  title { en "By status" }
  source Orders.list
  encoding {
    x { field status type nominal }
    y { field amount type quantitative }
  }
}

chart ByCustomer {
  type bar
  title { en "By customer" }
  source Orders.list
  encoding {
    x { field customer type nominal }
    y { field amount type quantitative }
  }
}

page Dashboard {
  route "/dashboard"
  title { en "Dashboard" }
  dashboard {
    grid {
      ByStatus w 6 h 300
      ByCustomer w 6 h 300
    }
    filters [status]
  }
}

page OrderList {
  route "/orders"
  title { en "Orders" }
  table {
    query   Orders.list
    columns [customer]
    actions { row [delete → Orders.delete confirm] }
  }
}
"#;

fn build_project(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: viz04\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    forma::build::run_build(dir).unwrap();
}

#[test]
fn select_event_carries_category_name() {
    let dir = tmp("spec");
    build_project(&dir);
    let spec = fs::read_to_string(dir.join("generated/web/src/viz/spec.ts")).unwrap();
    let sel = spec
        .lines()
        .find(|l| l.contains("\"select\""))
        .expect("select variant");
    assert!(sel.contains("name"), "{sel}");
    // echarts 适配器传 name
    let echarts = fs::read_to_string(dir.join("generated/web/src/viz/echarts.ts")).unwrap();
    assert!(echarts.contains("name: params?.name"), "{echarts}");
}

#[test]
fn dashboard_wires_chart_click_to_declared_filters() {
    let dir = tmp("link");
    build_project(&dir);
    let page = fs::read_to_string(dir.join("generated/web/src/pages/Dashboard.tsx")).unwrap();
    // ByStatus 的 x=status 在 filters 中 → 接线
    assert!(
        page.contains("e.name !== undefined"),
        "chart click should drive filters: {page}"
    );
    assert!(page.contains("status: e.name"), "{page}");
    // ByCustomer 的 x=customer 未在 filters 中 → 不接线（只此一处 onEvent）
    assert_eq!(page.matches("onEvent=").count(), 1, "{page}");
    // 点击后其他图表重取（source 依赖 filters）
    assert!(page.contains("[filters]"), "{page}");
}

#[test]
fn chart_without_declared_filter_is_not_wired() {
    let dir = tmp("nolink");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: viz04\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    // filters 不含 status/customer → 两个图表都不接线
    fs::write(
        dir.join("forma/app.fm"),
        FM.replace("filters [status]", "filters [customer]")
            .replace("x { field customer type nominal }", "x { field customer type nominal }"),
    )
    .unwrap();
    // 只保留 ByCustomer（x=customer 在 filters）→ 1 处接线；移除 ByStatus 避免干扰
    let fm = FM
        .replace("filters [status]", "filters [customer]")
        .replace("      ByStatus w 6 h 300\n", "");
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
    forma::build::run_build(&dir).unwrap();
    let page = fs::read_to_string(dir.join("generated/web/src/pages/Dashboard.tsx")).unwrap();
    assert!(page.contains("customer: e.name"), "{page}");
    assert_eq!(page.matches("onEvent=").count(), 1, "{page}");
}
