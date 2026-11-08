//! 可视化语义校验：形状与通道匹配、聚合冲突、目录（catalog）。
//!
//! 诊断码见 `docs/rfc/0001-visualization.zh-CN.md` §6。

use super::{ChartSpec, ChartType, DataFormat, ShapeFamily, ValueType};
use crate::validate::Diagnostic;

fn diag(code: &str, message: String, line: usize) -> Diagnostic {
    Diagnostic {
        severity: "error".into(),
        code: code.into(),
        message,
        file: None,
        line: Some(line),
        nodes: None,
    }
}

fn warn(code: &str, message: String, line: usize) -> Diagnostic {
    Diagnostic {
        severity: "warning".into(),
        code: code.into(),
        message,
        file: None,
        line: Some(line),
        nodes: None,
    }
}

/// 形状级校验：必需通道是否齐备、通道类型是否匹配图表语义。
pub fn validate_chart(spec: &mut ChartSpec, errors: &mut Vec<Diagnostic>) {
    let line = 0;
    let shape = spec.shape;
    let enc = &spec.encoding;

    // V0.5 theme token 白名单（RFC-0010 §7.4）
    if !super::THEME_TOKENS.contains(&spec.theme.as_str()) {
        errors.push(diag(
            "viz-unknown-theme",
            format!(
                "chart '{}' uses unknown theme '{}' (allowed: {})",
                spec.id,
                spec.theme,
                super::THEME_TOKENS.join(", ")
            ),
            line,
        ));
    }

    let require = |errors: &mut Vec<Diagnostic>,
                   spec_id: &str,
                   channel: &str,
                   accepted: &[ValueType],
                   label: &str| {
        match channel_value(spec, channel) {
            None => errors.push(diag(
                "viz-shape-mismatch",
                format!(
                    "chart '{}' of type {} requires a `{}` encoding channel ({})",
                    spec_id,
                    spec.chart_type.as_str(),
                    channel,
                    label
                ),
                line,
            )),
            Some(ch) => {
                if !accepted.contains(&ch.value_type) {
                    errors.push(diag(
                        "viz-type-mismatch",
                        format!(
                            "chart '{}' `{}` channel is {} but must be {}",
                            spec_id,
                            channel,
                            ch.value_type.as_str(),
                            accepted
                                .iter()
                                .map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        ),
                        line,
                    ));
                }
            }
        }
    };

    match shape {
        ShapeFamily::Cartesian => {
            require(
                errors,
                &spec.id,
                "x",
                &[ValueType::Nominal, ValueType::Ordinal, ValueType::Temporal],
                "category or time",
            );
            require(errors, &spec.id, "y", &[ValueType::Quantitative], "numeric");
            if enc.series.is_some() {
                let ok = matches!(
                    enc.series.as_ref().map(|c| c.value_type),
                    Some(ValueType::Nominal) | Some(ValueType::Ordinal)
                );
                if !ok {
                    errors.push(diag(
                        "viz-type-mismatch",
                        format!(
                            "chart '{}' `series` channel must be nominal/ordinal",
                            spec.id
                        ),
                        line,
                    ));
                }
            }
        }
        ShapeFamily::Xy => {
            require(errors, &spec.id, "x", &[ValueType::Quantitative], "numeric");
            require(errors, &spec.id, "y", &[ValueType::Quantitative], "numeric");
        }
        ShapeFamily::PartToWhole => {
            require(
                errors,
                &spec.id,
                "x",
                &[ValueType::Nominal, ValueType::Ordinal],
                "category",
            );
            require(errors, &spec.id, "y", &[ValueType::Quantitative], "numeric");
        }
        ShapeFamily::Radial => {
            require(
                errors,
                &spec.id,
                "x",
                &[ValueType::Nominal, ValueType::Ordinal],
                "category",
            );
            require(errors, &spec.id, "y", &[ValueType::Quantitative], "numeric");
        }
        ShapeFamily::Gauge => {
            require(errors, &spec.id, "y", &[ValueType::Quantitative], "numeric");
        }
        ShapeFamily::Heatmap => {
            require(
                errors,
                &spec.id,
                "x",
                &[ValueType::Nominal, ValueType::Ordinal],
                "category",
            );
            require(
                errors,
                &spec.id,
                "y",
                &[ValueType::Nominal, ValueType::Ordinal],
                "category",
            );
            require(
                errors,
                &spec.id,
                "color",
                &[ValueType::Quantitative],
                "numeric",
            );
        }
        ShapeFamily::Metric => {
            // V0.2 指标卡：无编码通道；恰一个 aggregate 变换
            if enc.x.is_some() || enc.y.is_some() || enc.series.is_some() || enc.color.is_some() {
                errors.push(diag(
                    "viz-metric-encoding",
                    format!(
                        "chart '{}' of type metric takes no encoding channels (metric = single aggregate value)",
                        spec.id
                    ),
                    line,
                ));
            }
            let agg_count = spec
                .transform
                .ops
                .iter()
                .filter(|op| matches!(op, super::TransformOp::Aggregate { .. }))
                .count();
            if agg_count == 0 {
                errors.push(diag(
                    "viz-metric-no-aggregate",
                    format!(
                        "chart '{}' of type metric requires exactly one aggregate transform",
                        spec.id
                    ),
                    line,
                ));
            } else if agg_count > 1 {
                errors.push(diag(
                    "viz-metric-multi-aggregate",
                    format!(
                        "chart '{}' of type metric has {} aggregate transforms (exactly one required)",
                        spec.id, agg_count
                    ),
                    line,
                ));
            }
        }
        ShapeFamily::Table => {
            // V0.2 数据表：无编码通道；仅允许 filter/sort/limit（行级变换）
            if enc.x.is_some() || enc.y.is_some() || enc.series.is_some() || enc.color.is_some() {
                errors.push(diag(
                    "viz-table-encoding",
                    format!(
                        "chart '{}' of type table takes no encoding channels (table renders raw rows)",
                        spec.id
                    ),
                    line,
                ));
            }
            for op in &spec.transform.ops {
                if matches!(
                    op,
                    super::TransformOp::Aggregate { .. } | super::TransformOp::Bucket { .. }
                ) {
                    errors.push(diag(
                        "viz-table-transform",
                        format!(
                            "chart '{}' of type table only allows filter/sort/limit transforms",
                            spec.id
                        ),
                        line,
                    ));
                    break;
                }
            }
        }
    }

    // color 通道目前只有热力图使用；series 通道只对笛卡尔形状有意义
    if shape != ShapeFamily::Heatmap && enc.color.is_some() {
        errors.push(warn(
            "viz-shape-mismatch",
            format!(
                "chart '{}' declares a `color` channel which only heatmap uses (ignored)",
                spec.id
            ),
            line,
        ));
    }
    if shape != ShapeFamily::Cartesian && enc.series.is_some() {
        errors.push(warn(
            "viz-shape-mismatch",
            format!(
                "chart '{}' declares a `series` channel which only cartesian shapes use (ignored)",
                spec.id
            ),
            line,
        ));
    }
}

fn channel_value<'a>(spec: &'a ChartSpec, name: &str) -> Option<&'a super::Channel> {
    match name {
        "x" => spec.encoding.x.as_ref(),
        "y" => spec.encoding.y.as_ref(),
        "series" => spec.encoding.series.as_ref(),
        "color" => spec.encoding.color.as_ref(),
        _ => None,
    }
}

/// 可视化目录：供 MCP `forma_viz_catalog` 使用。
pub fn catalog() -> serde_json::Value {
    serde_json::json!({
        "chartTypes": ChartType::ALL.iter().map(|t| serde_json::json!({
            "type": t.as_str(),
            "shape": t.shape().as_str(),
        })).collect::<Vec<_>>(),
        "valueTypes": ["temporal", "quantitative", "nominal", "ordinal"],
        "formats": ["currency", "percent", "integer", "decimal", "datetime", "date", "none"],
        "transformOps": ["filter", "bucket", "aggregate", "sort", "limit"],
        "aggregateOps": ["sum", "avg", "count", "min", "max"],
        "bucketUnits": ["hour", "day", "week", "month", "year"],
        "compareOps": ["eq", "ne", "gt", "gte", "lt", "lte", "in"],
        "shapes": ["cartesian", "xy", "part-to-whole", "radial", "gauge", "heatmap", "metric", "table"],
        "adapters": ["echarts"],
    })
}

/// 单个图表类型的规格说明（供 MCP `forma_viz_describe` 使用）。
pub fn describe(chart_type: &str) -> Option<serde_json::Value> {
    let t = ChartType::parse(chart_type)?;
    let (required, optional) = match t.shape() {
        ShapeFamily::Cartesian => (vec!["x", "y"], vec!["series"]),
        ShapeFamily::Xy => (vec!["x", "y"], vec![]),
        ShapeFamily::PartToWhole => (vec!["x", "y"], vec![]),
        ShapeFamily::Radial => (vec!["x", "y"], vec![]),
        ShapeFamily::Gauge => (vec!["y"], vec![]),
        ShapeFamily::Heatmap => (vec!["x", "y", "color"], vec![]),
        ShapeFamily::Metric => (vec![], vec![]),
        ShapeFamily::Table => (vec![], vec![]),
    };
    Some(serde_json::json!({
        "type": t.as_str(),
        "shape": t.shape().as_str(),
        "requiredChannels": required,
        "optionalChannels": optional,
        "defaultFormat": match t.shape() {
            ShapeFamily::Cartesian | ShapeFamily::Xy | ShapeFamily::PartToWhole
            | ShapeFamily::Radial | ShapeFamily::Gauge | ShapeFamily::Heatmap
            | ShapeFamily::Metric | ShapeFamily::Table => DataFormat::None.as_str(),
        },
    }))
}
