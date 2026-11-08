//! viz-web 适配层：Chart IR → web 运行时（TS/TSX）+ 每图表的规格产物。
//!
//! 产物分工（RFC-0001 §8.8）：
//! - `src/viz/spec.ts` / `transform.ts` —— 适配器无关的类型与变换语义；
//! - `src/viz/echarts.ts` —— ECharts 适配层运行时（option 绑定 / 格式化 / 事件翻译）；
//! - `src/viz/runtime.tsx` —— React 组件（取数 → 变换 → 渲染 → 事件 → 响应式）；
//! - `src/viz/charts/<Name>.ts` —— 每个图表的 Spec / Option / Source。

use super::Program;
use crate::generators::GeneratedFile;
use crate::util::{sha256_hex, write_text};
use crate::visualization::{self, ChartSpec};
use anyhow::Result;
use std::path::Path;

const SPEC_TS: &str = include_str!("templates/viz/spec.ts");
const TRANSFORM_TS: &str = include_str!("templates/viz/transform.ts");
const ECHARTS_TS: &str = include_str!("templates/viz/echarts.ts");
const RUNTIME_TSX: &str = include_str!("templates/viz/runtime.tsx");
/// AntV（G2Plot）Adapter 运行时（RFC-0010 V0.6 / 2026-11-08）
const ANTV_OPTION_TS: &str = include_str!("templates/viz/antv_option.ts");
const ANTV_RUNTIME_TSX: &str = include_str!("templates/viz/antv_runtime.tsx");
const VIZ_CSS: &str = include_str!("templates/viz/viz.css");
const THEME_TSX: &str = include_str!("templates/viz/theme.tsx");

/// 生成可视化产物。工程没有 chart 时不产出任何文件（不引入运行时体积）。
pub fn emit_visualization(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Vec<GeneratedFile>> {
    emit_visualization_with_adapter(project_dir, program, out_dir, "echarts")
}

/// 按 Adapter 生成可视化运行时（RFC-0010 §2 分层：IR/变换 adapter 无关）。
/// `adapter`: "echarts"（默认）| "antv"（G2Plot）。
pub fn emit_visualization_with_adapter(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    adapter: &str,
) -> Result<Vec<GeneratedFile>> {
    if !matches!(adapter, "echarts" | "antv") {
        anyhow::bail!("unknown visualization adapter '{adapter}' (expected echarts|antv)");
    }
    // candlestick 是 AntV（G2Plot）专属：echarts 无内置 K 线，显式拒绝而非静默降级
    if adapter != "antv" && program.charts.iter().any(|c| c.chart_type == "candlestick") {
        anyhow::bail!(
            "chart type 'candlestick' requires adapter 'antv' (echarts has no built-in candlestick; set targets.visualization.adapter: antv)"
        );
    }
    let mut charts: Vec<ChartSpec> = Vec::new();
    for decl in &program.charts {
        // 语义错误已由 check_program 报出；此处跳过无法降级的图表。
        if let Ok(spec) = visualization::lower_chart(program, decl) {
            charts.push(spec);
        }
    }
    if charts.is_empty() {
        return Ok(Vec::new());
    }

    // echarts adapter 需要按图表类型生成 use() 注册；antv 用通用 option 投影
    let echarts_code = if adapter == "antv" {
        String::new()
    } else {
        echarts_runtime(&charts)
    };
    let mut files: Vec<GeneratedFile> = vec![
        file(out_dir, "src/viz/spec.ts", SPEC_TS),
        file(out_dir, "src/viz/transform.ts", TRANSFORM_TS),
        file(
            out_dir,
            "src/viz/echarts.ts",
            if adapter == "antv" {
                ANTV_OPTION_TS
            } else {
                &echarts_code
            },
        ),
        file(
            out_dir,
            "src/viz/runtime.tsx",
            if adapter == "antv" {
                ANTV_RUNTIME_TSX
            } else {
                RUNTIME_TSX
            },
        ),
        file(out_dir, "src/viz/theme.tsx", THEME_TSX),
        file(out_dir, "src/viz/viz.css", VIZ_CSS),
        // V0.5 预览与浏览器渲染验证门：全部图表 + mock 数据 + 主题切换 + 自检标记
        file(
            out_dir,
            "src/viz/preview.tsx",
            preview_tsx(program, &charts).as_str(),
        ),
        file(out_dir, "viz-preview.html", preview_html().as_str()),
    ];
    for spec in &charts {
        files.push(file(
            out_dir,
            &format!("src/viz/charts/{}.ts", spec.id),
            chart_file(program, spec).as_str(),
        ));
    }

    let mut written = Vec::new();
    for f in files {
        let abs = project_dir.join(&f.path);
        let hash = sha256_hex(&f.content);
        let unchanged = abs.is_file()
            && crate::util::read_text(&abs)
                .map(|c| sha256_hex(&c) == hash)
                .unwrap_or(false);
        if !unchanged {
            write_text(&abs, &f.content)?;
        }
        written.push(f);
    }
    Ok(written)
}

/// 按工程实际用到的图表类型生成按需加载的 ECharts 适配层运行时。
fn echarts_runtime(charts: &[ChartSpec]) -> String {
    let mut chart_ctors: Vec<&str> = Vec::new();
    let mut components: Vec<&str> = vec![
        "TooltipComponent",
        "LegendComponent",
        "TitleComponent",
        "GridComponent",
    ];
    for spec in charts {
        let ctor = match spec.chart_type {
            // V0.2：metric/table 由 React 组件渲染，不需要 ECharts 图表构造器
            crate::visualization::ChartType::Metric | crate::visualization::ChartType::Table => {
                continue;
            }
            crate::visualization::ChartType::Line | crate::visualization::ChartType::Area => {
                "LineChart"
            }
            crate::visualization::ChartType::Bar => "BarChart",
            crate::visualization::ChartType::Pie => "PieChart",
            crate::visualization::ChartType::Scatter => "ScatterChart",
            crate::visualization::ChartType::Funnel => "FunnelChart",
            crate::visualization::ChartType::Gauge => "GaugeChart",
            crate::visualization::ChartType::Radar => "RadarChart",
            crate::visualization::ChartType::Heatmap => "HeatmapChart",
            crate::visualization::ChartType::Treemap => "TreemapChart",
            // echarts 无内置 K 线（需 custom series）——该分支只在 antv adapter 下可达
            // （emit_visualization_with_adapter 对 echarts + candlestick 提前 bail）
            crate::visualization::ChartType::Candlestick => "CandlestickChart",
        };
        if !chart_ctors.contains(&ctor) {
            chart_ctors.push(ctor);
        }
        if spec.chart_type == crate::visualization::ChartType::Radar
            && !components.contains(&"RadarComponent")
        {
            components.push("RadarComponent");
        }
        if spec.chart_type == crate::visualization::ChartType::Heatmap
            && !components.contains(&"VisualMapComponent")
        {
            components.push("VisualMapComponent");
        }
        if spec.interaction.zoom && !components.contains(&"DataZoomComponent") {
            components.push("DataZoomComponent");
        }
    }
    chart_ctors.sort_unstable();
    components.sort_unstable();

    // V0.2：仅 metric/table widget（无 ECharts 图表）→ echarts 适配层生成纯函数 stub，
    // 不再引入 echarts 依赖（metric/table 仅用 formatValue）。
    if chart_ctors.is_empty() {
        return String::from(
            "// @generated by Forma (gen-viz) — do not edit manually.\n\
             // 本工程无 ECharts 图表（仅 metric/table widget）——echarts 适配层为 stub。\n\
             // 纯格式化函数保留（metric/table 共用）；createChart/bindData 在此工程不可用。\n\
             \n\
             export function bindData(\n\
             \x20 _option: Record<string, unknown>,\n\
             \x20 _spec: unknown,\n\
             \x20 _data: unknown,\n\
             ): Record<string, unknown> {\n\
             \x20 return {};\n\
             }\n\
             \n\
             export function applyFormat(_opt: unknown, _spec: unknown): void {}\n\
             \n\
             export function formatValue(value: unknown, fmt: string): string {\n\
             \x20 if (value === null || value === undefined || value === \"\") return \"—\";\n\
             \x20 const n = Number(value);\n\
             \x20 switch (fmt) {\n\
             \x20   case \"currency\":\n\
             \x20     return Number.isFinite(n)\n\
             \x20       ? `¥${n.toLocaleString(\"zh-CN\", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`\n\
             \x20       : String(value);\n\
             \x20   case \"percent\":\n\
             \x20     return Number.isFinite(n) ? `${n}%` : String(value);\n\
             \x20   case \"integer\":\n\
             \x20     return Number.isFinite(n) ? n.toLocaleString(\"zh-CN\") : String(value);\n\
             \x20   case \"decimal\":\n\
             \x20     return Number.isFinite(n) ? n.toFixed(2) : String(value);\n\
             \x20   default:\n\
             \x20     return String(value);\n\
             \x20 }\n\
             }\n\
             \n\
             export function createChart(\n\
             \x20 _el: HTMLDivElement,\n\
             \x20 _spec: unknown,\n\
             \x20 _option: unknown,\n\
             \x20 _onEvent?: unknown,\n\
             ): { setOption: (o: unknown) => void; resize: () => void; dispose: () => void } {\n\
             \x20 throw new Error(\"echarts not installed: this project has no echarts charts\");\n\
             }\n",
        );
    }

    let mut imports = String::new();
    if !chart_ctors.is_empty() {
        imports.push_str(&format!(
            "import {{ {} }} from \"echarts/charts\";\n",
            chart_ctors.join(", ")
        ));
    }
    imports.push_str(&format!(
        "import {{ {} }} from \"echarts/components\";\n",
        components.join(", ")
    ));
    let mut use_list: Vec<String> = chart_ctors.iter().map(|c| c.to_string()).collect();
    use_list.extend(components.iter().map(|c| c.to_string()));
    let use_args = use_list
        .iter()
        .map(|c| format!("\n  {c},"))
        .collect::<Vec<_>>()
        .join("");

    ECHARTS_TS
        .replace("%%ECHARTS_IMPORTS%%", &imports)
        .replace("%%ECHARTS_USE%%", &use_args)
}

fn file(out_dir: &str, rel: &str, content: &str) -> GeneratedFile {
    GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content: content.to_string(),
        generator: "gen-viz".into(),
    }
}

/// 单个图表的产物：Spec（IR 投影）+ Option（ECharts 骨架）+ Source（取数绑定）。
///
/// V0.2：metric/table widget 不经过 ECharts（option = `{}`）；
/// Source 对 cursor 分页 api 解包 `{items, nextCursor}` 信封；
/// table widget 额外携带列定义（实体字段 → key/label/format）。
fn chart_file(program: &Program, spec: &ChartSpec) -> String {
    let name = &spec.id;
    let spec_json =
        serde_json::to_string_pretty(&visualization::spec_json(spec)).unwrap_or_default();
    // V0.2：metric/table 不经 ECharts 渲染 → option 为空对象（运行时忽略）
    let option_json = if matches!(
        spec.chart_type,
        crate::visualization::ChartType::Metric | crate::visualization::ChartType::Table
    ) {
        "{}".into()
    } else {
        serde_json::to_string_pretty(&visualization::echarts::chart_option(spec))
            .unwrap_or_default()
    };
    // 分页模式：cursor api 需解包信封（V0.2 数据绑定完整化）
    let cursor_mode = program
        .apis
        .iter()
        .find(|a| a.name == spec.data.api)
        .map(|a| a.list.as_ref().and_then(|l| l.paginate.as_deref()) == Some("cursor"))
        .unwrap_or(false);
    let fetch = if cursor_mode {
        format!(
            "() =>\n      {api}Api.list({{ cursor: null, pageSize: 200 }}).then((p: any) => p.items ?? [])",
            api = spec.data.api
        )
    } else {
        format!(
            "() => {api}Api.list({{}}) as unknown as Promise<Row[]>",
            api = spec.data.api
        )
    };
    // table widget：列定义（实体字段 → 运行时 key / 人类标签 / 格式）
    let columns_json = if spec.chart_type == crate::visualization::ChartType::Table {
        let cols: Vec<serde_json::Value> = program
            .entities
            .iter()
            .find(|e| e.name == spec.data.entity)
            .map(|e| {
                e.fields
                    .iter()
                    .map(|f| {
                        let key = if crate::dsl::is_relation(program, &f.type_name) {
                            format!("{}_name", crate::identity::to_snake_case(&f.name))
                        } else {
                            crate::identity::to_snake_case(&f.name)
                        };
                        let format = match f.type_name.as_str() {
                            "Timestamp" => "datetime",
                            "Date" => "date",
                            "Decimal" | "Float" => "decimal",
                            "Int" => "integer",
                            _ => "none",
                        };
                        serde_json::json!({ "key": key, "label": humanize_field(&f.name), "format": format })
                    })
                    .collect()
            })
            .unwrap_or_default();
        format!(
            "\n  columns: {} as Array<{{ key: string; label: string; format: string }}>",
            serde_json::to_string(&cols).unwrap_or_else(|_| "[]".into())
        )
    } else {
        String::new()
    };
    format!(
        "// @generated by Forma (gen-viz) — do not edit manually.\n\
         // 图表「{name}」：Spec = Chart IR 投影，Option = ECharts option 骨架，Source = 取数绑定。\n\
         import type {{ ChartSpec, Row }} from \"../spec\";\n\
         import {{ {api}Api }} from \"../../api\";\n\
         \n\
         export const {name}Spec: ChartSpec = {spec_json};\n\
         \n\
         export const {name}Option: Record<string, unknown> = {option_json};\n\
         \n\
         export const {name}Source = {{\n\
         \x20 fetch: {fetch},{columns_json}\n\
         }};\n",
        name = name,
        api = spec.data.api,
        spec_json = spec_json,
        option_json = option_json,
        fetch = fetch,
        columns_json = columns_json,
    )
}

/// 字段名 → 人类可读表头（去掉下划线、首字母大写）。
fn humanize_field(name: &str) -> String {
    let snake = crate::identity::to_snake_case(name);
    let mut out = String::new();
    for (i, part) in snake.split('_').enumerate() {
        if part.is_empty() {
            continue;
        }
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// V0.5 预览页（Preview + 浏览器渲染验证门）：
/// 全部图表以 **mock 数据** 渲染（自包含，无需后端）+ 主题切换 + 自检标记。
///
/// 渲染门用法（项目根）：
/// 1. `cd generated/web && npx vite --port <PORT>`
/// 2. Chrome headless 打开 `/viz-preview.html`
///    （`--headless=new --virtual-time-budget=8000 --dump-dom`）
/// 3. 断言 DOM 含 `data-viz-status="ok"`（任何图表取数/渲染失败 = `error`）。
fn preview_tsx(program: &Program, charts: &[ChartSpec]) -> String {
    let mut chart_imports = String::new();
    let mut mocks = String::new();
    let mut sources = String::new();
    let mut cards = String::new();
    for spec in charts {
        let c = &spec.id;
        let snake = crate::identity::to_snake_case(c);
        chart_imports.push_str(&format!(
            "import {{ {c}Spec, {c}Option, {c}Source }} from \"./charts/{c}\";\n"
        ));
        let rows = mock_rows_json(program, &spec.data.entity);
        mocks.push_str(&format!("const {snake}MockRows: Row[] = {rows};\n\n"));
        sources.push_str(&format!(
            "const {snake}Source: VizSource = {{\n  ...{c}Source,\n  fetch: async () => {snake}MockRows,\n}};\n\n"
        ));
        cards.push_str(&format!(
            "        <section key=\"{c}\" style={{ {{ marginBottom: 32 }} }}>\n          <h2 style={{ {{ fontSize: 16, margin: \"0 0 8px\" }} }}>{{{c}Spec.title}}</h2>\n          <FormaChart\n            spec={{{c}Spec}}\n            option={{{c}Option}}\n            source={{{snake}Source}}\n          />\n        </section>\n"
        ));
    }
    format!(
        r#"// @generated by Forma (gen-viz) — do not edit manually.
// V0.5 预览 + 浏览器渲染验证门：全部图表（mock 数据）+ 主题切换 + 自检标记。
// 门：vite dev 下打开 /viz-preview.html，断言 body.dataset.vizStatus === "ok"。
import React from "react";
import {{ createRoot }} from "react-dom/client";
import {{ FormaChart, type VizSource }} from "./runtime";
import {{ FormaVizThemeProvider, VIZ_THEME_TOKENS }} from "./theme";
import type {{ Row, VizThemeToken }} from "./spec";
{chart_imports}
{mocks}
{sources}
function VizPreview() {{
  const [token, setToken] = React.useState<VizThemeToken>("default");
  // 渲染门自检：全部图表挂载后无错误 → ok
  React.useEffect(() => {{
    const t = setTimeout(() => {{
      document.body.dataset.vizStatus = document.querySelector(".forma-viz__error")
        ? "error"
        : "ok";
    }}, 400);
    return () => clearTimeout(t);
  }}, [token]);
  return (
    <FormaVizThemeProvider token={{token}}>
      <div style={{{{ padding: 24 }}}}>
        <div style={{{{ marginBottom: 16 }}}}>
          <select
            value={{token}}
            onChange={{(e) => setToken(e.target.value as VizThemeToken)}}
            aria-label="theme preview"
          >
            {{VIZ_THEME_TOKENS.map((t) => (
              <option key={{t}} value={{t}}>
                {{t}}
              </option>
            ))}}
          </select>
        </div>
{cards}      </div>
    </FormaVizThemeProvider>
  );
}}

const root = document.getElementById("root");
if (root) {{
  createRoot(root).render(<VizPreview />);
}}
"#,
        chart_imports = chart_imports,
        mocks = mocks,
        sources = sources,
        cards = cards,
    )
}

fn preview_html() -> String {
    String::from(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Forma viz preview</title>
  </head>
  <body style="margin: 0">
    <div id="root"></div>
    <script type="module" src="/src/viz/preview.tsx"></script>
  </body>
</html>
"#,
    )
}

/// 从实体字段合成确定性 mock 行（V0.5 渲染门：自包含取数，不依赖后端）。
fn mock_rows_json(program: &Program, entity: &str) -> String {
    use serde_json::json;
    let Some(ent) = program.entities.iter().find(|e| e.name == entity) else {
        return "[]".into();
    };
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for n in 1..=12u64 {
        let mut row = serde_json::Map::new();
        for f in &ent.fields {
            let key = crate::identity::to_snake_case(&f.name);
            let v = match f.type_name.as_str() {
                "Uuid" => json!(format!("mock-{:012}", n)),
                "Int" => json!(n * 7 + 3),
                "Float" => json!((n as f64) * 1.5 + 0.5),
                "Decimal" => json!(format!("{:.2}", (n as f64) * 111.0 + 7.0)),
                "Bool" => json!(n % 2 == 0),
                "Timestamp" => json!(format!("2026-01-{:02}T10:00:00Z", n)),
                "Date" => json!(format!("2026-01-{:02}", n)),
                "Json" => json!({ "k": n }),
                "Email" => json!(format!("mock{n}@example.test")),
                t if crate::dsl::is_enum(program, t) => {
                    let variant = program
                        .enums
                        .iter()
                        .find(|e| e.name == t)
                        .and_then(|e| e.variants.first())
                        .map(|v| v.name.clone())
                        .unwrap_or_else(|| "unknown".into());
                    json!(variant)
                }
                t if crate::dsl::is_relation(program, t) => {
                    row.insert(format!("{key}_id"), json!(format!("mock-{:012}", n)));
                    json!(format!("关联 {}{}", t, n))
                }
                _ => json!(format!("示例 {} {}", f.name, n)),
            };
            row.insert(key, v);
        }
        rows.push(serde_json::Value::Object(row));
    }
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
}
