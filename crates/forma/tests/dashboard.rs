//! Dashboard IR（RFC-0010 V0.3 / P1-#8）回归测试：
//! `page { dashboard { grid { ... } filters [...] gap N } }`。
//!
//! 验收：
//! - 检查层：条目图表必须存在（viz-unknown-chart）/ 无条目（dsl-dashboard-empty）/
//!   筛选字段必须是绑定图 api 的 list filter（dsl-dashboard-unknown-filter）；
//! - 生成层：dashboard 页（CSS Grid + useMemo source + 筛选器联动取数）；
//! - 非 Web 目标显式拒绝（宪法 6.2，不静默降级）；
//! - 行为已于浏览器验证（vite dev + chrome-devtools：筛选变化 → 带参重取数）。

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
        "forma-dash-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_manifest(dir: &Path, frontend: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            r#"project:
  name: dashdemo
  version: 0.1.0

source: ["forma/**/*.fm"]

targets:
  web:
    framework: {frontend}
    out: generated/web
"#
        ),
    )
    .unwrap();
}

const BASE: &str = r#"app dashdemo {
  version = "0.1.0"
}

entity Order {
  id       Uuid @id @default(uuid)
  customer String(50) @required
  total    Decimal(12,2)
  status   String
  createdAt Timestamp @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
  list {
    filters [status]
    sort [createdAt desc]
    paginate cursor
  }
}

chart Total {
  type metric
  title { zh "总销售额" en "Total" }
  source Orders.list
  transform {
    aggregate { op sum field total }
  }
  axis {
    y { field value format currency }
  }
}

chart Rows {
  type table
  title { zh "明细" en "Rows" }
  source Orders.list
  transform {
    limit 20
  }
}
"#;

const DASH_PAGE: &str = r#"page SalesDashboard {
  route "/dashboard"
  title { zh "销售看板" en "Sales" }
  dashboard {
    gap 16
    grid {
      Total w 4 h 220
      Rows w 8 h 320
    }
    filters [status]
  }
}
"#;

fn check(dir: &Path, fm: &str) -> Vec<forma::validate::Diagnostic> {
    fs::create_dir_all(dir.join("forma")).unwrap();
    write_manifest(dir, "react");
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, parse_diags) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let parse_errs: Vec<_> = parse_diags
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(parse_errs.is_empty(), "fixture must parse: {parse_errs:?}");
    forma::dsl::check_program(&program)
}

#[test]
fn dashboard_checks() {
    // 合法看板
    let dir = tmp_dir("ok");
    let diags = check(&dir, &format!("{BASE}\n{DASH_PAGE}"));
    let dash_errs: Vec<_> = diags
        .iter()
        .filter(|d| d.code.contains("dashboard"))
        .collect();
    assert!(dash_errs.is_empty(), "{dash_errs:?}");

    // 未知图表
    let dir = tmp_dir("unknown-chart");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\npage Bad {{\n  route \"/bad\"\n  title {{ zh \"x\" }}\n  dashboard {{\n    grid {{\n      Missing w 6 h 200\n    }}\n  }}\n}}\n"
        ),
    );
    assert!(
        diags.iter().any(|d| d.code == "viz-unknown-chart"),
        "{diags:?}"
    );

    // 空条目
    let dir = tmp_dir("empty");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\npage Bad {{\n  route \"/bad\"\n  title {{ zh \"x\" }}\n  dashboard {{\n    grid {{}}\n  }}\n}}\n"
        ),
    );
    assert!(
        diags.iter().any(|d| d.code == "dsl-dashboard-empty"),
        "{diags:?}"
    );

    // 筛选字段不是绑定 api 的 list filter（customer 未声明）
    let dir = tmp_dir("bad-filter");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\npage Bad {{\n  route \"/bad\"\n  title {{ zh \"x\" }}\n  dashboard {{\n    grid {{\n      Total w 12 h 200\n    }}\n    filters [customer]\n  }}\n}}\n"
        ),
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "dsl-dashboard-unknown-filter"),
        "{diags:?}"
    );
}

#[test]
fn dashboard_page_generation_with_linked_filters() {
    let dir = tmp_dir("gen");
    fs::create_dir_all(dir.join("forma")).unwrap();
    write_manifest(&dir, "react");
    fs::write(dir.join("forma/app.fm"), format!("{BASE}\n{DASH_PAGE}")).unwrap();
    forma::build::run_build(&dir).unwrap();

    let page = fs::read_to_string(dir.join("generated/web/src/pages/SalesDashboard.tsx")).unwrap();
    // Grid 布局（12 列制 + 条目跨度/高度）
    assert!(page.contains("gridTemplateColumns"));
    assert!(page.contains("gridColumn: \"span 4\""));
    assert!(page.contains("minHeight: 320"));
    // 联动：useMemo source（引用稳定，筛选变化才重取数）
    assert!(page.contains("React.useMemo"));
    // cursor api 解包 + 筛选参数注入
    assert!(page.contains("cursor: null"));
    assert!(page.contains("\"status\": filters[\"status\"] || null"));
    // 重置按钮
    assert!(page.contains("onClick={() => setFilters({})}"));
}

#[test]
fn non_web_targets_reject_dashboard_pages() {
    // Flutter（mobile target）/ Vue：宪法 6.2 显式拒绝（不静默降级）
    let cases: [(&str, &str); 2] = [
        (
            "flutter",
            r#"targets:
  mobile:
    framework: flutter
    out: generated/mobile
"#,
        ),
        (
            "vue",
            r#"targets:
  frontend:
    framework: vue
    out: generated/web
"#,
        ),
    ];
    for (name, targets_yaml) in cases {
        let dir = tmp_dir(&format!("reject-{name}"));
        fs::create_dir_all(dir.join("forma")).unwrap();
        fs::write(
            dir.join("forma.yaml"),
            format!(
                "project:\n  name: dashdemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\n{targets_yaml}"
            ),
        )
        .unwrap();
        fs::write(dir.join("forma/app.fm"), format!("{BASE}\n{DASH_PAGE}")).unwrap();
        let err = forma::build::run_build(&dir).unwrap_err();
        assert!(
            err.to_string().contains("dashboard"),
            "{name} must reject dashboard explicitly: {err}"
        );
    }
}
