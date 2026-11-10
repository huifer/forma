//! RFC-0010 V0.6+：关系图（`type graph` + `link { source target }`）回归。
//!
//! 验收：
//! - DSL `link` 绑定 → GraphSpec（边键 `{rel}_id`、标签键 `{rel}_name`）；
//! - ECharts：series type=graph（force 布局，数据由 Runtime 绑定）；
//! - AntV：G6 渲染器按需注入（无 graph 图表 → 零引用、零依赖）；
//! - 检查层：缺 link / 未知字段显式报错。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-graph-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app graphdemo { version = "0.1.0" }
entity Student {
  id   Uuid @id @default(uuid)
  name String(50) @required
}
entity Course {
  id    Uuid @id @default(uuid)
  title String(100) @required
}
entity Enrollment {
  id     Uuid @id @default(uuid)
  student Student?
  course  Course?
}
api Students { entity Student operations [list get create update delete] }
api Courses { entity Course operations [list get create update delete] }
api Enrollments { entity Enrollment operations [list get create update delete] }
chart EnrollmentGraph {
  type graph
  title { en "Enrollment graph" }
  source Enrollments.list
  link { source student target course }
}
page GraphPage {
  route "/graph"
  title { en "Graph" }
  chart EnrollmentGraph
}
"#;

fn write_project(dir: &Path, adapter: Option<&str>) {
    let viz = match adapter {
        Some(a) => format!("  visualization:\n    adapter: {a}\n"),
        None => String::new(),
    };
    fs::write(
        dir.join("forma.yaml"),
        format!("project:\n  name: graphdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  web:\n    framework: react\n    out: generated/web\n{viz}"),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn graph_chart_generates_spec_and_echarts_series() {
    let dir = tmp("echarts");
    write_project(&dir, None);
    forma::build::run_build(&dir).unwrap();

    let chart =
        fs::read_to_string(dir.join("generated/web/src/viz/charts/EnrollmentGraph.ts")).unwrap();
    assert!(chart.contains("\"chartType\": \"graph\""), "{chart}");
    assert!(chart.contains("\"shape\": \"graph\""), "{chart}");
    // 关系字段 → 边键 `{rel}_id` + 标签键 `{rel}_name`
    assert!(chart.contains("\"sourceKey\": \"student_id\""), "{chart}");
    assert!(chart.contains("\"targetKey\": \"course_id\""), "{chart}");
    assert!(
        chart.contains("\"sourceLabelKey\": \"student_name\""),
        "{chart}"
    );
    // ECharts option：graph series（force 布局，数据留空）
    assert!(chart.contains("\"type\": \"graph\""), "{chart}");
    assert!(chart.contains("\"layout\": \"force\""), "{chart}");
    assert!(chart.contains("\"links\": []"), "{chart}");
    // 运行时整形 + 绑定
    let transform = fs::read_to_string(dir.join("generated/web/src/viz/transform.ts")).unwrap();
    assert!(transform.contains("shapeGraph"), "{transform}");
    let echarts = fs::read_to_string(dir.join("generated/web/src/viz/echarts.ts")).unwrap();
    assert!(echarts.contains("case \"graph\""), "{echarts}");
    // ECharts runtime 经 shapeData/bindData 处理 graph（无 G6 引用）
    let runtime = fs::read_to_string(dir.join("generated/web/src/viz/runtime.tsx")).unwrap();
    assert!(runtime.contains("shapeData"), "{runtime}");
    assert!(!runtime.contains("@antv/g6"), "{runtime}");
    // 无 graph 时不引入 G6
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(!pkg.contains("@antv/g6"), "{pkg}");
}

#[test]
fn graph_chart_antv_adapter_injects_g6() {
    let dir = tmp("antv");
    write_project(&dir, Some("antv"));
    forma::build::run_build(&dir).unwrap();
    let runtime = fs::read_to_string(dir.join("generated/web/src/viz/runtime.tsx")).unwrap();
    assert!(
        runtime.contains("import G6 from \"@antv/g6\";"),
        "{runtime}"
    );
    assert!(runtime.contains("function GraphView("), "{runtime}");
    // 调用分支与组件同源注入（曾因无条件文本在无 graph 工程报 TS2304）
    assert!(
        runtime.contains("spec.chartType === \"graph\""),
        "{runtime}"
    );
    assert!(runtime.contains("shapeGraph"), "{runtime}");
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(pkg.contains("@antv/g6"), "{pkg}");
    assert!(!pkg.contains("\"echarts\""), "{pkg}");
}

#[test]
fn antv_without_graph_has_no_dangling_graphview_reference() {
    // 回归：AntV runtime 的 graph 调用分支曾是无条件文本，无 graph 图表的工程
    // （如 examples/viz-antv）生成对 GraphView 的悬空引用 → 生成工程 `tsc --noEmit`
    // 报 TS2304。组件定义 / import / 调用分支三者必须同源注入。
    let dir = tmp("antv-nograph");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: antvnograph\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  web:\n    framework: react\n    out: generated/web\n  visualization:\n    adapter: antv\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app antvnograph { version = "0.1.0" }
entity Order {
  id     Uuid          @id @default(uuid)
  status String(20)    @required
  amount Decimal(10,2)
}
api Orders { entity Order operations [list get create update delete] }
chart ByStatus {
  type bar
  title { en "Orders by status" }
  source Orders.list
  encoding {
    x { field status type nominal }
    y { field amount type quantitative }
  }
}
page Dashboard {
  title { en "Dashboard" }
  chart ByStatus
}
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let runtime = fs::read_to_string(dir.join("generated/web/src/viz/runtime.tsx")).unwrap();
    assert!(
        !runtime.contains("GraphView"),
        "no-graph antv runtime must not reference GraphView: {runtime}"
    );
    assert!(!runtime.contains("@antv/g6"), "{runtime}");
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(!pkg.contains("@antv/g6"), "{pkg}");
}

#[test]
fn graph_chart_requires_link_binding() {
    let dir = tmp("missing-link");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: graphdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app graphdemo { version = "0.1.0" }
entity Node { id Uuid @id @default(uuid) name String(50) }
api Nodes { entity Node operations [list get create update delete] }
chart BadGraph {
  type graph
  title { en "Bad" }
  source Nodes.list
}
"#,
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("viz-graph-missing-link"), "{msg}");
}

#[test]
fn graph_link_unknown_field_is_rejected() {
    let dir = tmp("unknown-field");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: graphdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app graphdemo { version = "0.1.0" }
entity Node { id Uuid @id @default(uuid) name String(50) }
api Nodes { entity Node operations [list get create update delete] }
chart BadGraph {
  type graph
  title { en "Bad" }
  source Nodes.list
  link { source missing target also_missing }
}
"#,
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("viz-unknown-field"), "{msg}");
}
