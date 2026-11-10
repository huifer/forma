//! 可视化（RFC-0001）验收测试：
//! - DSL chart 块解析 → Chart IR；
//! - 语义校验（viz-* 诊断码）；
//! - ECharts Adapter 确定性投影；
//! - viz-web 产物结构与无泄漏断言。
//!
//! 并行安全：全部产物写入 `tmp_dir("唯一 tag")`（见 forma-parallel-test-tmp-race）。

use forma::dsl::load_program;
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
        "forma-viz-{tag}-{}-{seq}-{nanos}",
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

/// 最小可编译工程：一个实体 + api + chart。
const BASE_MODEL: &str = r#"
app vizdemo {
  version = "0.1.0"
  locale  = ["zh", "en"]
}

enum OrderStatus {
  pending
  paid
}

entity Order {
  id        Uuid        @id @default(uuid)
  region    String(50)  @required
  amount    Decimal(12,2) @required
  quantity  Int         @required
  status    OrderStatus @default(pending)
  createdAt Timestamp   @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [region, status] sort [createdAt desc] }
}
"#;

fn parse(fm: &str) -> forma::dsl::Program {
    let dir = tmp_dir("parse");
    write_project(&dir, fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diagnostics) = load_program(&loaded.dir, &source).unwrap();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "unexpected DSL errors: {errors:?}");
    program
}

fn check(fm: &str) -> Vec<forma::validate::Diagnostic> {
    let dir = tmp_dir("check");
    write_project(&dir, fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, mut diagnostics) = load_program(&loaded.dir, &source).unwrap();
    diagnostics.extend(forma::dsl::check_program(&program));
    diagnostics
}

// ── 1. 图表类型编译：10 种类型各一条 fixture ────────────────

#[test]
fn all_ten_chart_types_lower_and_project() {
    let cases = [
        ("line", "x nominal, y quantitative"),
        ("bar", "x nominal, y quantitative"),
        ("area", "x nominal, y quantitative"),
        ("pie", "x nominal, y quantitative"),
        ("scatter", "x quantitative, y quantitative"),
        ("funnel", "x nominal, y quantitative"),
        ("gauge", "y quantitative"),
        ("radar", "x nominal, y quantitative"),
        ("heatmap", "x nominal, y nominal, color quantitative"),
        ("treemap", "x nominal, y quantitative"),
    ];
    for (chart_type, _desc) in &cases {
        let encoding = match *chart_type {
            "gauge" => "    y { field quantity type quantitative }\n".to_string(),
            "scatter" => "    x { field quantity type quantitative }\n    y { field amount type quantitative }\n".to_string(),
            "heatmap" => "    x { field region type nominal }\n    y { field status type nominal }\n    color { field quantity type quantitative }\n".to_string(),
            _ => "    x { field region type nominal }\n    y { field amount type quantitative }\n".to_string(),
        };
        let fm = format!(
            "{BASE_MODEL}\nchart T {{\n  type {chart_type}\n  title {{ zh \"测试\" en \"Test\" }}\n  source Orders.list\n  encoding {{\n{encoding}  }}\n}}\n"
        );
        let program = parse(&fm);
        assert_eq!(program.charts.len(), 1, "type {chart_type} should parse");
        let decl = &program.charts[0];
        let spec = forma::visualization::lower_chart(&program, decl)
            .unwrap_or_else(|e| panic!("type {chart_type} should lower: {e:?}"));
        assert_eq!(spec.chart_type.as_str(), *chart_type);

        // 每种类型都能投影出合法的 ECharts option（含 series）
        let option = forma::visualization::echarts::chart_option(&spec);
        let series = option
            .get("series")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("type {chart_type}: option.series missing"));
        assert!(!series.is_empty(), "type {chart_type}: empty series");
    }
}

// ── 2. 语义校验（viz-* 诊断码）────────────────────────────

#[test]
fn unknown_chart_type_is_an_error() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type waterfall\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-unknown-type"),
        "expected viz-unknown-type, got {diags:?}"
    );
}

#[test]
fn unknown_api_is_an_error() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Nope.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-unknown-api"),
        "{diags:?}"
    );
}

#[test]
fn unknown_field_is_an_error_with_hard_failure() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field nope type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    let d = diags
        .iter()
        .find(|d| d.code == "viz-unknown-field")
        .expect("expected viz-unknown-field");
    assert!(d.message.contains("nope"), "{}", d.message);
}

#[test]
fn type_mismatch_is_an_error() {
    // Timestamp 字段声明为 quantitative
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field createdAt type quantitative }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-type-mismatch"),
        "{diags:?}"
    );
}

#[test]
fn shape_mismatch_missing_y_is_an_error() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-shape-mismatch"),
        "{diags:?}"
    );
}

#[test]
fn aggregate_conflicts_are_errors() {
    // count 带字段
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field value type quantitative }}\n  }}\n  transform {{\n    aggregate {{ op count field amount groupBy [region] }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-aggregate-conflict"),
        "{diags:?}"
    );

    // sum 缺字段
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field value type quantitative }}\n  }}\n  transform {{\n    aggregate {{ op sum groupBy [region] }}\n  }}\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-aggregate-conflict"),
        "{diags:?}"
    );
}

#[test]
fn page_referencing_unknown_chart_is_an_error() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n\npage P {{\n  route \"/t\"\n  title {{ zh \"T\" }}\n  chart Missing\n}}\n"
    );
    let diags = check(&fm);
    assert!(
        diags.iter().any(|d| d.code == "viz-unknown-chart"),
        "{diags:?}"
    );
}

#[test]
fn duplicate_chart_is_an_error() {
    let chart = "chart T {\n  type line\n  source Orders.list\n  encoding {\n    x { field region type nominal }\n    y { field amount type quantitative }\n  }\n}\n";
    let fm = format!("{BASE_MODEL}\n{chart}\n{chart}\n");
    let diags = check(&fm);
    assert!(diags.iter().any(|d| d.code == "viz-duplicate"), "{diags:?}");
}

// ── 3. IR 语义 ────────────────────────────────────────────

#[test]
fn relation_dimension_maps_to_display_name_column() {
    let fm = r#"
app vizdemo { version = "0.1.0"  locale = ["zh", "en"] }

entity School {
  id   Uuid  @id @default(uuid)
  name String(100) @required
}

entity Student {
  id     Uuid   @id @default(uuid)
  school School @required
  name   String(50) @required
  age    Int    @required
}

api Students {
  entity Student
  operations [list get create update delete]
}

chart BySchool {
  type bar
  source Students.list
  encoding {
    x { field school type nominal }
    y { field age type quantitative }
  }
}
"#;
    let program = parse(fm);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    assert_eq!(spec.encoding.x.as_ref().unwrap().data_key, "school_name");
    assert_eq!(spec.encoding.y.as_ref().unwrap().data_key, "age");
}

#[test]
fn aggregate_output_field_is_a_valid_value_channel() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field value type quantitative }}\n  }}\n  transform {{\n    aggregate {{ op sum field amount groupBy [region] }}\n  }}\n}}\n"
    );
    let program = parse(&fm);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    assert_eq!(spec.encoding.y.as_ref().unwrap().data_key, "value");
    // 变换计划可序列化为 camelCase JSON（Runtime 契约）
    let plan = forma::visualization::transform_plan_json(&spec.transform);
    let agg = &plan.as_array().unwrap()[0];
    assert_eq!(agg["op"], "aggregate");
    assert_eq!(agg["agg"], "sum");
    assert_eq!(agg["dataKey"], "amount");
    assert_eq!(agg["as"], "value");
}

// ── 4. ECharts Adapter：确定性投影 ─────────────────────────

#[test]
fn option_projection_is_deterministic() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field createdAt type temporal }}\n    y {{ field amount type quantitative format currency }}\n    series {{ field status type nominal }}\n  }}\n  interaction {{ zoom true }}\n}}\n"
    );
    let program = parse(&fm);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    let a = serde_json::to_string(&forma::visualization::echarts::chart_option(&spec)).unwrap();
    let b = serde_json::to_string(&forma::visualization::echarts::chart_option(&spec)).unwrap();
    assert_eq!(a, b, "同一 IR 两次投影必须 byte 级一致");
    // spec 投影同样确定
    let c = serde_json::to_string(&forma::visualization::spec_json(&spec)).unwrap();
    let d = serde_json::to_string(&forma::visualization::spec_json(&spec)).unwrap();
    assert_eq!(c, d);
}

#[test]
fn cartesian_option_carries_axes_and_empty_data() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n  axis {{\n    x {{ label \"区域\" }}\n    y {{ label \"金额\" }}\n  }}\n}}\n"
    );
    let program = parse(&fm);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    let option = forma::visualization::echarts::chart_option(&spec);

    assert_eq!(option["xAxis"][0]["type"], "category");
    assert_eq!(option["yAxis"][0]["type"], "value");
    assert_eq!(option["xAxis"][0]["name"], "区域");
    assert_eq!(option["yAxis"][0]["name"], "金额");
    assert_eq!(option["series"][0]["type"], "bar");
    // 数据位置留空 → Runtime 绑定
    assert_eq!(
        option["series"][0]["data"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        0
    );
}

#[test]
fn temporal_x_uses_time_axis_and_zoom_adds_data_zoom() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field createdAt type temporal }}\n    y {{ field amount type quantitative }}\n  }}\n  interaction {{ zoom true }}\n}}\n"
    );
    let program = parse(&fm);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    let option = forma::visualization::echarts::chart_option(&spec);
    assert_eq!(option["xAxis"][0]["type"], "time");
    assert!(
        option.get("dataZoom").is_some(),
        "zoom=true 需要 dataZoom 组件"
    );
}

#[test]
fn pie_has_no_axes_and_heatmap_has_visual_map() {
    let pie = format!(
        "{BASE_MODEL}\nchart T {{\n  type pie\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let program = parse(&pie);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    let option = forma::visualization::echarts::chart_option(&spec);
    assert!(option.get("xAxis").is_none(), "pie 不应有坐标轴");
    assert_eq!(option["series"][0]["type"], "pie");

    let heat = format!(
        "{BASE_MODEL}\nchart T {{\n  type heatmap\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field status type nominal }}\n    color {{ field quantity type quantitative }}\n  }}\n}}\n"
    );
    let program = parse(&heat);
    let spec = forma::visualization::lower_chart(&program, &program.charts[0]).unwrap();
    let option = forma::visualization::echarts::chart_option(&spec);
    assert_eq!(option["series"][0]["type"], "heatmap");
    assert!(option.get("visualMap").is_some(), "heatmap 需要 visualMap");
}

// ── 5. viz-web 产物 ───────────────────────────────────────

#[test]
fn viz_web_emits_runtime_and_chart_files() {
    let fm = format!(
        "{BASE_MODEL}\nchart Trend {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field createdAt type temporal }}\n    y {{ field amount type quantitative format currency }}\n    series {{ field status type nominal }}\n  }}\n}}\n\npage Dash {{\n  route \"/dash\"\n  title {{ zh \"看板\" en \"Dashboard\" }}\n  chart Trend\n}}\n"
    );
    let dir = tmp_dir("emit");
    write_project(&dir, &fm);
    let report = forma::build::run_build(&dir).expect("build should succeed");

    assert!(
        report.step_names().contains(&"viz-web"),
        "build 应包含 viz-web 步骤，实际 {:?}",
        report.step_names()
    );

    let web = dir.join("generated/web");
    for rel in [
        "src/viz/spec.ts",
        "src/viz/transform.ts",
        "src/viz/echarts.ts",
        "src/viz/runtime.tsx",
        "src/viz/viz.css",
        "src/viz/charts/Trend.ts",
        "src/pages/Dash.tsx",
    ] {
        assert!(web.join(rel).is_file(), "missing {rel}");
    }

    // 图表产物三件套：Spec / Option / Source
    let chart_ts = fs::read_to_string(web.join("src/viz/charts/Trend.ts")).unwrap();
    assert!(chart_ts.contains("export const TrendSpec: ChartSpec"));
    assert!(chart_ts.contains("export const TrendOption: Record<string, unknown>"));
    assert!(chart_ts.contains("export const TrendSource"));
    assert!(
        chart_ts.contains("OrdersApi.list"),
        "取数应绑定生成的 API 客户端"
    );

    // 页面挂载
    let page = fs::read_to_string(web.join("src/pages/Dash.tsx")).unwrap();
    assert!(page.contains("<FormaChart"));
    assert!(page.contains("TrendSpec"));

    // package.json 按需引入 echarts
    let pkg = fs::read_to_string(web.join("package.json")).unwrap();
    assert!(
        pkg.contains("\"echarts\""),
        "有 chart 时应声明 echarts 依赖"
    );

    // 无占位符泄漏（契约：flutter/web 产物不得残留模板占位符）
    for rel in ["src/viz/echarts.ts", "src/viz/charts/Trend.ts"] {
        let content = fs::read_to_string(web.join(rel)).unwrap();
        assert!(!content.contains("%%"), "{rel} 残留模板占位符");
    }
}

#[test]
fn no_charts_means_no_viz_artifacts() {
    let dir = tmp_dir("noviz");
    write_project(&dir, BASE_MODEL);
    let report = forma::build::run_build(&dir).unwrap();
    // 步骤可以注册，但不得产出任何可视化文件
    let viz_files = report
        .steps
        .iter()
        .filter(|s| s.name == "viz-web")
        .flat_map(|s| s.files.iter())
        .count();
    assert_eq!(viz_files, 0, "无 chart 时不应产出 viz 产物");
    assert!(!dir.join("generated/web/src/viz").exists());
}

#[test]
fn semantic_error_blocks_build_and_lists_diagnostics() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field nope type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let dir = tmp_dir("err");
    write_project(&dir, &fm);
    let err = forma::build::run_build(&dir).expect_err("build 应失败");
    let msg = format!("{err:#}");
    assert!(msg.contains("viz-unknown-field"), "{msg}");
}

// ── 6. 增量更新：改一个图表不影响其他产物 ──────────────────

#[test]
fn editing_one_chart_leaves_other_artifacts_untouched() {
    let fm = format!(
        "{BASE_MODEL}\nchart A {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n\nchart B {{\n  type pie\n  source Orders.list\n  encoding {{\n    x {{ field status type nominal }}\n    y {{ field quantity type quantitative }}\n  }}\n}}\n"
    );
    let dir = tmp_dir("incr");
    write_project(&dir, &fm);
    forma::build::run_build(&dir).unwrap();

    let a_before = fs::read_to_string(dir.join("generated/web/src/viz/charts/A.ts")).unwrap();
    let b_before = fs::read_to_string(dir.join("generated/web/src/viz/charts/B.ts")).unwrap();

    // 修改 chart A 的类型：A 变，B 不变
    let fm2 = fm.replace("chart A {\n  type bar", "chart A {\n  type line");
    write_project(&dir, &fm2);
    forma::build::run_build(&dir).unwrap();

    let a_after = fs::read_to_string(dir.join("generated/web/src/viz/charts/A.ts")).unwrap();
    let b_after = fs::read_to_string(dir.join("generated/web/src/viz/charts/B.ts")).unwrap();

    assert_ne!(a_before, a_after, "chart A 应重新生成");
    assert_eq!(b_before, b_after, "chart B 不应被破坏");
}

// ── 7. MCP 工具面 ────────────────────────────────────────

#[test]
fn mcp_viz_catalog_and_describe() {
    let dir = tmp_dir("mcp");
    write_project(&dir, BASE_MODEL);
    let catalog = forma::mcp::call_tool(&dir, "forma_viz_catalog", &Default::default()).unwrap();
    let types: Vec<String> = catalog["chartTypes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(types.len(), 16); // V0.6：candlestick + map + scattermap + graph 加入目录
    assert!(types.contains(&"line".to_string()));
    // V0.2：metric / table 已在目录中
    assert!(types.contains(&"metric".to_string()));
    assert!(types.contains(&"table".to_string()));

    let mut args = serde_json::Map::new();
    args.insert("chartType".into(), serde_json::json!("radar"));
    let described = forma::mcp::call_tool(&dir, "forma_viz_describe", &args).unwrap();
    assert_eq!(described["shape"], "radial");
    assert_eq!(described["requiredChannels"], serde_json::json!(["x", "y"]));

    let mut bad = serde_json::Map::new();
    bad.insert("chartType".into(), serde_json::json!("waterfall"));
    let err = forma::mcp::call_tool(&dir, "forma_viz_describe", &bad).unwrap_err();
    assert!(format!("{err:#}").contains("unknown chart type"));
}

#[test]
fn mcp_viz_validate_reports_spec_summaries() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type line\n  source Orders.list\n  encoding {{\n    x {{ field createdAt type temporal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let dir = tmp_dir("mcpv");
    write_project(&dir, &fm);
    let out = forma::mcp::call_tool(&dir, "forma_viz_validate", &Default::default()).unwrap();
    assert_eq!(out["chartCount"], 1);
    assert_eq!(out["errors"], 0);
    let chart = &out["charts"][0];
    assert_eq!(chart["id"], "T");
    assert_eq!(chart["api"], "Orders");
    assert_eq!(chart["encoding"]["y"]["dataKey"], "amount");
}

#[test]
fn mcp_viz_tools_reject_federated_projects() {
    let dir = tmp_dir("fed");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: fed\n  version: 0.1.0\n",
    )
    .unwrap();
    let err = forma::mcp::call_tool(&dir, "forma_viz_validate", &Default::default()).unwrap_err();
    assert!(format!("{err:#}").contains("DSL"), "{err:#}");
}

// ── 8. DSL 摘要（graph）包含图表 ──────────────────────────

#[test]
fn dsl_graph_summary_includes_charts() {
    let fm = format!(
        "{BASE_MODEL}\nchart T {{\n  type bar\n  source Orders.list\n  encoding {{\n    x {{ field region type nominal }}\n    y {{ field amount type quantitative }}\n  }}\n}}\n"
    );
    let dir = tmp_dir("graph");
    write_project(&dir, &fm);
    let graph = forma::mcp::call_tool(&dir, "forma_graph", &Default::default()).unwrap();
    let summary = &graph["summary"];
    assert_eq!(summary["chartCount"], 1);
    assert_eq!(summary["charts"][0]["name"], "T");
    assert_eq!(summary["charts"][0]["source"], "Orders.list");
}
