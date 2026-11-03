//! ECharts Adapter：Chart IR → ECharts option 骨架。
//!
//! 契约（RFC-0001 §7）：
//! - 纯函数：输入只有 IR，不读环境/时间/随机数；
//! - 确定性：同一 IR 两次投影结果一致（键序由 serde_json 的有序 Map 决定）；
//! - 数据位置留空，由 Runtime 绑定（Runtime 持有 spec，因此格式化与整形也在 Runtime 侧）；
//! - option 里不出现任何 IR 之外的目标库概念之外的语义泄漏（option 本身当然全是
//!   ECharts 概念——但它只是产物，不是规格）。

use super::{ChartSpec, ChartType, ShapeFamily};
use serde_json::{json, Value};

/// 生成 ECharts option 骨架（数据数组为空，等待 Runtime 绑定）。
pub fn chart_option(spec: &ChartSpec) -> Value {
    let title = crate::dsl::pick_message(&spec.title, &spec.id);
    let mut option = json!({});
    if !title.is_empty() {
        option["title"] = json!({ "text": title });
    }
    option["animation"] = json!(false);

    // 提示框：笛卡尔形状用 axis 触发，其余用 item
    if spec.tooltip.enabled {
        let trigger = if spec.shape == ShapeFamily::Cartesian
            || spec.shape == ShapeFamily::Xy
            || spec.shape == ShapeFamily::Heatmap
        {
            "axis"
        } else {
            "item"
        };
        option["tooltip"] = json!({ "trigger": trigger });
    }

    // 图例
    option["legend"] = json!({
        "show": spec.legend.enabled,
        "type": if spec.series.grouped { "scroll" } else { "plain" },
        "top": 8,
    });

    // 缩放交互 → dataZoom 组件（inside + slider）
    if spec.interaction.zoom {
        option["dataZoom"] = json!([
            { "type": "inside", "filterMode": "none" },
            { "type": "slider", "filterMode": "none", "bottom": 8, "height": 18 },
        ]);
    }

    match spec.shape {
        ShapeFamily::Cartesian => cartesian(spec, &mut option),
        ShapeFamily::Xy => xy(spec, &mut option),
        ShapeFamily::PartToWhole => part_to_whole(spec, &mut option),
        ShapeFamily::Radial => radial(spec, &mut option),
        ShapeFamily::Gauge => gauge(spec, &mut option),
        ShapeFamily::Heatmap => heatmap(spec, &mut option),
    }

    option
}

fn axis_name(spec: &ChartSpec, is_x: bool) -> Option<String> {
    let ch = if is_x {
        spec.axis.x.as_ref()
    } else {
        spec.axis.y.as_ref()
    };
    ch.and_then(|c| c.label.clone()).filter(|s| !s.is_empty())
}

fn cartesian(spec: &ChartSpec, option: &mut Value) {
    let x_type = match spec.encoding.x.as_ref().map(|c| c.value_type) {
        Some(super::ValueType::Temporal) => "time",
        _ => "category",
    };
    let mut x_axis = json!({ "type": x_type });
    if x_type == "category" {
        x_axis["data"] = json!([]);
    }
    if let Some(name) = axis_name(spec, true) {
        x_axis["name"] = json!(name);
    }
    let mut y_axis = json!({ "type": "value" });
    if let Some(name) = axis_name(spec, false) {
        y_axis["name"] = json!(name);
    }
    option["xAxis"] = json!([x_axis]);
    option["yAxis"] = json!([y_axis]);
    option["grid"] =
        json!({ "left": 56, "right": 24, "top": 48, "bottom": 48, "containLabel": true });

    let series_type = match spec.chart_type {
        ChartType::Bar => "bar",
        _ => "line",
    };
    let mut series = json!({
        "type": series_type,
        "name": "",
        "data": [],
    });
    if spec.chart_type == ChartType::Area {
        series["areaStyle"] = json!({ "opacity": 0.24 });
    }
    if series_type == "line" {
        series["smooth"] = json!(spec.series.smooth);
        series["showSymbol"] = json!(spec.series.show_symbol);
    }
    if let Some(stack) = &spec.series.stack {
        series["stack"] = json!(stack);
    }
    option["series"] = json!([series]);
}

fn xy(spec: &ChartSpec, option: &mut Value) {
    let mut x_axis = json!({ "type": "value" });
    if let Some(name) = axis_name(spec, true) {
        x_axis["name"] = json!(name);
    }
    let mut y_axis = json!({ "type": "value" });
    if let Some(name) = axis_name(spec, false) {
        y_axis["name"] = json!(name);
    }
    option["xAxis"] = json!([x_axis]);
    option["yAxis"] = json!([y_axis]);
    option["grid"] =
        json!({ "left": 56, "right": 24, "top": 48, "bottom": 48, "containLabel": true });
    option["series"] = json!([{ "type": "scatter", "data": [] }]);
}

fn part_to_whole(spec: &ChartSpec, option: &mut Value) {
    let mut series = json!({ "data": [] });
    match spec.chart_type {
        ChartType::Pie => {
            series["type"] = json!("pie");
            series["radius"] = json!("62%");
            series["label"] = json!({ "formatter": "{b}\n{d}%" });
        }
        ChartType::Funnel => {
            series["type"] = json!("funnel");
            series["sort"] = json!("descending");
            series["gap"] = json!(2);
        }
        ChartType::Treemap => {
            series["type"] = json!("treemap");
            series["roam"] = json!(false);
            series["nodeClick"] = json!(false);
        }
        _ => {}
    }
    option["series"] = json!([series]);
}

fn radial(_spec: &ChartSpec, option: &mut Value) {
    option["radar"] = json!([{ "indicator": [] }]);
    option["series"] = json!([{ "type": "radar", "data": [] }]);
}

fn gauge(spec: &ChartSpec, option: &mut Value) {
    let mut series = json!({
        "type": "gauge",
        "data": [],
        "detail": { "valueAnimation": false },
    });
    if let Some(name) = axis_name(spec, false) {
        series["title"] = json!({ "formatter": name });
    }
    option["series"] = json!([series]);
}

fn heatmap(spec: &ChartSpec, option: &mut Value) {
    let mut x_axis = json!({ "type": "category", "data": [] });
    if let Some(name) = axis_name(spec, true) {
        x_axis["name"] = json!(name);
    }
    let mut y_axis = json!({ "type": "category", "data": [] });
    if let Some(name) = axis_name(spec, false) {
        y_axis["name"] = json!(name);
    }
    option["xAxis"] = json!([x_axis]);
    option["yAxis"] = json!([y_axis]);
    option["grid"] =
        json!({ "left": 56, "right": 24, "top": 48, "bottom": 72, "containLabel": true });
    option["visualMap"] = json!({
        "min": 0,
        "max": 0,
        "calculable": true,
        "orient": "horizontal",
        "left": "center",
        "bottom": 8,
    });
    option["series"] = json!([{ "type": "heatmap", "data": [] }]);
}
