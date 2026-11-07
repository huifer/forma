//! 可视化 V0.2 回归测试（roadmap P1-#7）：
//! Data Binding 完整化（分页/cursor 源）+ Metric Card + Table widget。
//!
//! 验收：
//! - 检查层：metric 必须恰有一个 aggregate、禁止编码通道；table 禁止 aggregate/bucket；
//! - 生成层：metric/table 产物（option = `{}`、source cursor 解包、table 列定义）；
//!   metric/table-only 工程不引入 echarts（echarts 适配层为 stub）；cursor api 不再被拒；
//! - 行为于浏览器验证（vite dev + chrome-devtools）：metric 聚合值格式化、table 过滤/排序行数。

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
        "forma-viz02-{tag}-{}-{seq}-{nanos}",
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
    sort [createdAt desc]
    paginate cursor
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
fn metric_requires_single_aggregate_and_no_encoding() {
    // 合法 metric
    let dir = tmp_dir("metric-ok");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\nchart Total {{\n  type metric\n  title {{ zh \"总额\" en \"Total\" }}\n  source Orders.list\n  transform {{\n    aggregate {{ op sum field total }}\n  }}\n  axis {{\n    y {{ field value format currency }}\n  }}\n}}\n"
        ),
    );
    let viz_errs: Vec<_> = diags
        .iter()
        .filter(|d| d.code.starts_with("viz-"))
        .collect();
    assert!(viz_errs.is_empty(), "{viz_errs:?}");

    // 缺 aggregate
    let dir = tmp_dir("metric-noagg");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\nchart Total {{\n  type metric\n  source Orders.list\n  transform {{ limit 5 }}\n}}\n"
        ),
    );
    assert!(
        diags.iter().any(|d| d.code == "viz-metric-no-aggregate"),
        "{diags:?}"
    );

    // 带编码通道
    let dir = tmp_dir("metric-enc");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\nchart Total {{\n  type metric\n  source Orders.list\n  encoding {{ y {{ field total type quantitative }} }}\n  transform {{\n    aggregate {{ op sum field total }}\n  }}\n}}\n"
        ),
    );
    assert!(
        diags.iter().any(|d| d.code == "viz-metric-encoding"),
        "{diags:?}"
    );
}

#[test]
fn table_rejects_aggregate_and_bucket() {
    // 合法 table（filter/sort/limit）
    let dir = tmp_dir("table-ok");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\nchart Rows {{\n  type table\n  source Orders.list\n  transform {{\n    filter {{ field status eq \"active\" }}\n    sort {{ by createdAt desc }}\n    limit 20\n  }}\n}}\n"
        ),
    );
    let viz_errs: Vec<_> = diags
        .iter()
        .filter(|d| d.code.starts_with("viz-"))
        .collect();
    assert!(viz_errs.is_empty(), "{viz_errs:?}");

    // 带 aggregate
    let dir = tmp_dir("table-agg");
    let diags = check(
        &dir,
        &format!(
            "{BASE}\nchart Rows {{\n  type table\n  source Orders.list\n  transform {{\n    aggregate {{ op sum field total }}\n  }}\n}}\n"
        ),
    );
    assert!(
        diags.iter().any(|d| d.code == "viz-table-transform"),
        "{diags:?}"
    );
}

#[test]
fn metric_table_generation_and_cursor_binding() {
    let dir = tmp_dir("gen");
    write_project(
        &dir,
        &format!(
            "{BASE}\nchart Total {{\n  type metric\n  title {{ zh \"总销售额\" en \"Total\" }}\n  source Orders.list\n  transform {{\n    aggregate {{ op sum field total }}\n  }}\n  axis {{\n    y {{ field value format currency }}\n  }}\n}}\n\nchart Rows {{\n  type table\n  title {{ zh \"明细\" en \"Rows\" }}\n  source Orders.list\n  transform {{\n    filter {{ field status eq \"active\" }}\n    sort {{ by createdAt desc }}\n    limit 20\n  }}\n}}\n\npage TotalPage {{\n  route \"/viz/total\"\n  title {{ zh \"总销售额\" en \"Total\" }}\n  chart Total\n}}\n"
        ),
    );
    forma::build::run_build(&dir).unwrap();

    // cursor api 不再被 build 拒绝（V0.2 数据绑定完整化）
    let metric = fs::read_to_string(dir.join("generated/web/src/viz/charts/Total.ts")).unwrap();
    assert!(metric.contains("cursor: null"), "{metric}");
    assert!(metric.contains("p.items ?? []"));
    assert!(metric.contains("export const TotalOption: Record<string, unknown> = {};"));

    let table = fs::read_to_string(dir.join("generated/web/src/viz/charts/Rows.ts")).unwrap();
    assert!(table.contains("columns:"));
    assert!(table.contains(r#""key":"created_at""#));
    assert!(table.contains(r#""format":"datetime""#));

    // metric/table-only → echarts 适配层为 stub，package.json 不引入 echarts
    let echarts = fs::read_to_string(dir.join("generated/web/src/viz/echarts.ts")).unwrap();
    assert!(!echarts.contains("from \"echarts/core\""), "{echarts}");
    assert!(echarts.contains("export function formatValue"));
    let pkg = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(!pkg.contains("echarts"), "{pkg}");
}
