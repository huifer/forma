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
const VIZ_CSS: &str = include_str!("templates/viz/viz.css");

/// 生成可视化产物。工程没有 chart 时不产出任何文件（不引入运行时体积）。
pub fn emit_visualization(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> Result<Vec<GeneratedFile>> {
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

    let mut files: Vec<GeneratedFile> = vec![
        file(out_dir, "src/viz/spec.ts", SPEC_TS),
        file(out_dir, "src/viz/transform.ts", TRANSFORM_TS),
        file(out_dir, "src/viz/echarts.ts", &echarts_runtime(&charts)),
        file(out_dir, "src/viz/runtime.tsx", RUNTIME_TSX),
        file(out_dir, "src/viz/viz.css", VIZ_CSS),
    ];
    for spec in &charts {
        files.push(file(
            out_dir,
            &format!("src/viz/charts/{}.ts", spec.id),
            chart_file(spec).as_str(),
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
fn chart_file(spec: &ChartSpec) -> String {
    let name = &spec.id;
    let spec_json =
        serde_json::to_string_pretty(&visualization::spec_json(spec)).unwrap_or_default();
    let option_json = serde_json::to_string_pretty(&visualization::echarts::chart_option(spec))
        .unwrap_or_default();
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
         \x20 fetch: () => {api}Api.list({{}}) as unknown as Promise<Row[]>,\n\
         }};\n",
        name = name,
        api = spec.data.api,
        spec_json = spec_json,
        option_json = option_json,
    )
}
