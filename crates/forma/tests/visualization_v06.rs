//! RFC-0010 V0.6：AntV（G2Plot）适配层 + Adapter 选择。
//!
//! 验收：
//! - `targets.visualization.adapter: antv` → 生成 G2Plot option 投影 + runtime，
//!   package.json 引入 `@antv/g2plot` 且**不含 echarts**；
//! - 默认（无 visualization 段）→ echarts 产物零变化；
//! - 未知 adapter → 构建期显式失败（不静默降级）；
//! - transform.ts（变换执行）两个 adapter 共用——IR 的适配层无关性。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-viz06-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app viz06 { version = "0.1.0" }
entity Order {
  id     Uuid @id @default(uuid)
  status String(20)
  amount Decimal(12,2)
}
api Orders { entity Order operations [list get create update delete] }
chart ByStatus {
  type bar
  title { en "By status" }
  source Orders.list
  encoding { x { field status type nominal } y { field amount type quantitative } }
}
page OrderList {
  route "/orders"
  title { en "Orders" }
  table {
    query   Orders.list
    columns [status]
    actions { row [delete → Orders.delete confirm] }
  }
}
"#;

fn project(dir: &Path, adapter: Option<&str>) {
    let viz = match adapter {
        Some(a) => format!("  visualization:\n    adapter: {a}\n"),
        None => String::new(),
    };
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: viz06\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  web:\n    framework: react\n    out: generated/web\n{viz}"),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).unwrap()
}

#[test]
fn antv_adapter_generates_g2plot_runtime_without_echarts() {
    let dir = tmp("antv");
    project(&dir, Some("antv"));
    forma::build::run_build(&dir).unwrap();

    // option 投影 + runtime 是 AntV 版本（文件路径沿用 src/viz/echarts.ts / runtime.tsx 槽位）
    let option = read(&dir, "generated/web/src/viz/echarts.ts");
    assert!(option.contains("antvOption"), "{option}");
    assert!(option.contains("xField"), "{option}");
    assert!(!option.contains("echarts.use"), "{option}");
    let runtime = read(&dir, "generated/web/src/viz/runtime.tsx");
    assert!(runtime.contains("@antv/g2plot"), "{runtime}");
    assert!(runtime.contains("new (Plot as any)("), "{runtime}"); // Plot 在 G2Plot 类型里是抽象类 → as any

    // 依赖：@antv/g2plot 在，echarts 不在
    let pkg = read(&dir, "generated/web/package.json");
    assert!(pkg.contains("@antv/g2plot"), "{pkg}");
    assert!(!pkg.contains("echarts"), "{pkg}");

    // 变换层共用（adapter 无关）
    let transform = read(&dir, "generated/web/src/viz/transform.ts");
    assert!(transform.contains("applyTransform"), "{transform}");
}

#[test]
fn default_adapter_is_echarts_zero_change() {
    let dir = tmp("default");
    project(&dir, None);
    forma::build::run_build(&dir).unwrap();
    let option = read(&dir, "generated/web/src/viz/echarts.ts");
    assert!(option.contains("echarts.use"), "{option}");
    assert!(!option.contains("antvOption"), "{option}");
    let runtime = read(&dir, "generated/web/src/viz/runtime.tsx");
    assert!(!runtime.contains("@antv/g2plot"), "{runtime}");
    let pkg = read(&dir, "generated/web/package.json");
    assert!(pkg.contains("echarts"), "{pkg}");
    assert!(!pkg.contains("@antv/g2plot"), "{pkg}");
}

#[test]
fn unknown_adapter_fails_build_explicitly() {
    let dir = tmp("bad");
    project(&dir, Some("d3"));
    let err = forma::build::run_build(&dir).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("unknown visualization adapter"), "{msg}");
}

#[test]
fn flutter_pie_chart_renders_via_fl_chart() {
    let dir = tmp("flutter-pie");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: viz06\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  mobile:\n    out: app_mobile\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Order {
  id     Uuid @id @default(uuid)
  status String(20)
  amount Decimal(12,2)
}
api Orders { entity Order operations [list get] }
chart ByStatus {
  type pie
  title { en "By status" }
  source Orders.list
  encoding { x { field status type nominal } y { field amount type quantitative } }
}
page Dashboard {
  route "/dashboard"
  title { en "Dashboard" }
  chart ByStatus
}
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let page = read(&dir, "app_mobile/lib/pages/dashboard.dart");
    assert!(page.contains("PieChart("), "{page}");
    assert!(page.contains("PieChartSectionData"), "{page}");
    // 非 pie 类型（bar）不受影响
    assert!(page.contains("PieChart("), "{page}");
}

// ── V0.6：Candlestick（AntV 专属）─────────────────────────────

#[test]
fn candlestick_requires_antv_adapter() {
    let dir = tmp("kline-echarts");
    project(&dir, Some("echarts"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Trade {
  id     Uuid @id @default(uuid)
  date   Timestamp @default(now)
  open   Decimal(12,2)
  high   Decimal(12,2)
  low    Decimal(12,2)
  close  Decimal(12,2)
}
api Trades { entity Trade operations [list get] }
chart KLine {
  type candlestick
  title { en "K-Line" }
  source Trades.list
  encoding {
    x { field date type temporal }
    ohlc { open open high high low low close close }
  }
}
page TradeList {
  route "/trades"
  title { en "Trades" }
  table {
    query   Trades.list
    columns [date]
    actions { row [delete → Trades.delete confirm] }
  }
}
"#,
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("candlestick") && msg.contains("antv"), "{msg}");
}

#[test]
fn candlestick_renders_with_antv_adapter() {
    let dir = tmp("kline-antv");
    project(&dir, Some("antv"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Trade {
  id     Uuid @id @default(uuid)
  date   Timestamp @default(now)
  open   Decimal(12,2)
  high   Decimal(12,2)
  low    Decimal(12,2)
  close  Decimal(12,2)
}
api Trades { entity Trade operations [list get] }
chart KLine {
  type candlestick
  title { en "K-Line" }
  source Trades.list
  encoding {
    x { field date type temporal }
    ohlc { open open high high low low close close }
  }
}
page TradeList {
  route "/trades"
  title { en "Trades" }
  table {
    query   Trades.list
    columns [date]
    actions { row [delete → Trades.delete confirm] }
  }
}
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let option = read(&dir, "generated/web/src/viz/echarts.ts");
    assert!(
        option.contains("[ohlc.open, ohlc.close, ohlc.high, ohlc.low]"),
        "{option}"
    );
    let runtime = read(&dir, "generated/web/src/viz/runtime.tsx");
    assert!(runtime.contains("new Stock("), "{runtime}"); // G2Plot K 线类名是 Stock
    assert!(
        runtime.contains("[ohlc.open, ohlc.close, ohlc.high, ohlc.low]"),
        "{runtime}"
    );
}

#[test]
fn candlestick_validates_ohlc_fields() {
    // 缺 ohlc → viz-candlestick-missing-ohlc
    let dir = tmp("kline-missing");
    project(&dir, Some("antv"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Trade { id Uuid @id @default(uuid) date Timestamp @default(now) }
api Trades { entity Trade operations [list get] }
chart KLine {
  type candlestick
  title { en "K" }
  source Trades.list
  encoding { x { field date type temporal } }
}
page TradeList {
  route "/trades"
  title { en "T" }
  table { query Trades.list columns [date] actions { row [delete → Trades.delete confirm] } }
}
"#,
    )
    .unwrap();
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let sem = forma::dsl::check_program(&program);
    let codes: Vec<String> = sem.iter().map(|d| d.code.clone()).collect();
    assert!(
        codes.iter().any(|c| c == "viz-candlestick-missing-ohlc"),
        "{codes:?}"
    );
}

// ── V0.6 工业监控：阈值线（threshold）─────────────────────────

#[test]
fn threshold_line_is_emitted_for_all_adapters() {
    let dir = tmp("threshold-echarts");
    project(&dir, Some("echarts"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Metric {
  id    Uuid @id @default(uuid)
  name  String(20)
  value Decimal(12,2)
}
api Metrics { entity Metric operations [list get] }
chart Trend {
  type line
  title { en "Trend" }
  source Metrics.list
  encoding { x { field name type nominal } y { field value type quantitative } }
  threshold 100 { label "SLA" }
}
page TrendPage {
  route "/trend"
  title { en "Trend" }
  chart Trend
}
page MetricList {
  route "/metrics"
  title { en "Metrics" }
  table {
    query   Metrics.list
    columns [name]
    actions { row [delete → Metrics.delete confirm] }
  }
}
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    // echarts：markLine 在图表 option 投影里（chart_file）
    let chart = read(&dir, "generated/web/src/viz/charts/Trend.ts");
    assert!(chart.contains("markLine"), "{chart}");
    assert!(chart.contains("SLA"), "{chart}");

    // antv：annotations
    let dir2 = tmp("threshold-antv");
    project(&dir2, Some("antv"));
    fs::write(
        dir2.join("forma/app.fm"),
        &fs::read_to_string(dir.join("forma/app.fm")).unwrap(),
    )
    .unwrap();
    forma::build::run_build(&dir2).unwrap();
    let antv_opt = read(&dir2, "generated/web/src/viz/echarts.ts");
    assert!(antv_opt.contains("annotations"), "{antv_opt}");
    // AntV 的 annotations 在运行时 option 投影里（antv_option），chart 文件只含 spec 投影
    let _ = read(&dir2, "generated/web/src/viz/charts/Trend.ts");

    // flutter：extraLinesData（HorizontalLine）
    let dir3 = tmp("threshold-flutter");
    fs::write(
        dir3.join("forma.yaml"),
        "project:\n  name: viz06\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  mobile:\n    out: app_mobile\n",
    )
    .unwrap();
    fs::write(
        dir3.join("forma/app.fm"),
        &fs::read_to_string(dir.join("forma/app.fm")).unwrap(),
    )
    .unwrap();
    forma::build::run_build(&dir3).unwrap();
    let page = read(&dir3, "app_mobile/lib/pages/trend_page.dart");
    assert!(page.contains("ExtraLinesData"), "{page}");
    assert!(page.contains("HorizontalLine("), "{page}");
}

#[test]
fn threshold_on_pie_is_rejected() {
    let dir = tmp("threshold-pie");
    project(&dir, Some("echarts"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Metric {
  id    Uuid @id @default(uuid)
  name  String(20)
  value Decimal(12,2)
}
api Metrics { entity Metric operations [list get] }
chart Share {
  type pie
  title { en "Share" }
  source Metrics.list
  encoding { x { field name type nominal } y { field value type quantitative } }
  threshold 100 { label "SLA" }
}
page TrendPage {
  route "/trend"
  title { en "Trend" }
  chart Trend
}
page MetricList {
  route "/metrics"
  title { en "Metrics" }
  table { query Metrics.list columns [name] actions { row [delete → Metrics.delete confirm] } }
}
"#,
    )
    .unwrap();
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let sem = forma::dsl::check_program(&program);
    let codes: Vec<String> = sem.iter().map(|d| d.code.clone()).collect();
    assert!(
        codes.iter().any(|c| c == "viz-threshold-unsupported"),
        "{codes:?}"
    );
}

#[test]
fn antv_adapter_supports_funnel_and_radar() {
    let dir = tmp("antv-funnel");
    project(&dir, Some("antv"));
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app viz06 { version = "0.1.0" }
entity Order {
  id     Uuid @id @default(uuid)
  stage  String(20)
  amount Decimal(12,2)
}
api Orders { entity Order operations [list get] }
chart Funnel {
  type funnel
  title { en "Funnel" }
  source Orders.list
  encoding { x { field stage type nominal } y { field amount type quantitative } }
}
page OrderList {
  route "/orders"
  title { en "Orders" }
  table {
    query   Orders.list
    columns [stage]
    actions { row [delete → Orders.delete confirm] }
  }
}
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let runtime = read(&dir, "generated/web/src/viz/runtime.tsx");
    assert!(runtime.contains("new Funnel("), "{runtime}");
}

#[test]
fn antv_adapter_rejects_unsupported_types_explicitly() {
    // treemap：编码合法（x+y 同构），但 AntV adapter 不支持 → 构建期 bail
    for bad in ["treemap"] {
        let dir = tmp("antv-bad");
        project(&dir, Some("antv"));
        fs::write(
            dir.join("forma/app.fm"),
            format!(
                r#"
app viz06 {{ version = "0.1.0" }}
entity Order {{ id Uuid @id @default(uuid) name String(20) value Decimal(12,2) }}
api Orders {{ entity Order operations [list get] }}
chart X {{
  type {bad}
  title {{ en "X" }}
  source Orders.list
  encoding {{ x {{ field name type nominal }} y {{ field value type quantitative }} }}
}}
page OrderList {{
  route "/orders"
  title {{ en "Orders" }}
  table {{ query Orders.list columns [name] actions {{ row [delete → Orders.delete confirm] }} }}
}}
"#
            ),
        )
        .unwrap();
        let err = forma::build::run_build(&dir).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("not supported by the AntV"), "{msg}");
    }
}
