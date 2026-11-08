//! 可视化 V0.5 回归测试（roadmap P1-#9）：Theme / Preview / 浏览器渲染验证门。
//!
//! 验收：
//! - DSL `theme <token>`：白名单 default | dark | data-screen（未知 → viz-unknown-theme）；
//! - 生成层：spec 携带 theme；theme.tsx（token 调色板 + FormaVizThemeProvider +
//!   useFormaVizTheme）；preview.tsx（全部图表 mock 数据 + 主题切换 + 自检标记）+
//!   viz-preview.html 独立入口；
//! - 行为已于浏览器验证：gate = ok、ECharts 主题切换重 init（canvas 保留）、
//!   metric/table CSS 变量切换。

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
        "forma-viz05-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: vizdemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const BASE: &str = r#"app vizdemo {
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
  theme dark
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

fn check(dir: &Path, fm: &str) -> Vec<forma::validate::Diagnostic> {
    write_project(dir, fm);
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    forma::dsl::check_program(&program)
}

#[test]
fn theme_token_validation() {
    // 合法 token
    let dir = tmp_dir("ok");
    let diags = check(&dir, BASE);
    assert!(
        !diags.iter().any(|d| d.code == "viz-unknown-theme"),
        "{diags:?}"
    );

    // 未知 token
    let dir = tmp_dir("bad");
    let diags = check(&dir, &BASE.replace("theme dark", "theme neon"));
    assert!(
        diags.iter().any(|d| d.code == "viz-unknown-theme"),
        "{diags:?}"
    );
}

#[test]
fn theme_preview_and_gate_artifacts() {
    let dir = tmp_dir("gen");
    write_project(&dir, BASE);
    forma::build::run_build(&dir).unwrap();

    // spec 携带 theme（显式声明优先）
    let metric = fs::read_to_string(dir.join("generated/web/src/viz/charts/Total.ts")).unwrap();
    assert!(metric.contains(r#""theme": "dark""#), "{metric}");
    let table = fs::read_to_string(dir.join("generated/web/src/viz/charts/Rows.ts")).unwrap();
    assert!(table.contains(r#""theme": "default""#), "{table}");

    // theme.tsx：token 调色板 + Provider + Hook
    let theme = fs::read_to_string(dir.join("generated/web/src/viz/theme.tsx")).unwrap();
    assert!(theme.contains("FormaVizThemeProvider"));
    assert!(theme.contains("useFormaVizTheme"));
    assert!(theme.contains("data-screen"));

    // 预览页 + 独立入口（渲染验证门）
    let preview = fs::read_to_string(dir.join("generated/web/src/viz/preview.tsx")).unwrap();
    assert!(preview.contains("FormaVizThemeProvider"));
    assert!(preview.contains("vizStatus"));
    assert!(preview.contains("MockRows"));
    // mock 数据自包含（无 fetch / 无 api import）
    assert!(!preview.contains("Api.list"));
    assert!(dir.join("generated/web/viz-preview.html").exists());

    // metric/table-only 工程：echarts stub，但 theme.tsx 仍生成（provider 可用）
    let echarts = fs::read_to_string(dir.join("generated/web/src/viz/echarts.ts")).unwrap();
    assert!(!echarts.contains("from \"echarts/core\""));
}

#[test]
fn echarts_theme_applied_in_runtime() {
    // 带 ECharts 图的工程：createChart 以主题对象 init（V0.5 Runtime 侧应用）
    let dir = tmp_dir("echarts");
    write_project(
        &dir,
        &format!(
            "{BASE}\nchart ByCustomer {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field customer type nominal }}\n    y {{ field total type quantitative }}\n  }}\n}}\n\npage ByCustomerPage {{\n  route \"/viz/by\"\n  title {{ zh \"x\" }}\n  chart ByCustomer\n}}\n"
        ),
    );
    forma::build::run_build(&dir).unwrap();

    let echarts = fs::read_to_string(dir.join("generated/web/src/viz/echarts.ts")).unwrap();
    assert!(echarts.contains("echartsThemeObject(spec.theme)"));
    assert!(echarts.contains("themeObject"));
    // package.json 重新引入 echarts（needs_echarts）
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(pkg.contains("echarts"), "{pkg}");
}
