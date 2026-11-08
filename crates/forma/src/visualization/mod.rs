//! 可视化 IR：Chart IR —— 与语法解耦、与目标图表库无关的统一语义。
//!
//! 设计见 `docs/rfc/0001-visualization.zh-CN.md`：
//! DSL 描述意图 → Chart IR 表达统一语义 → Adapter（echarts.rs）做目标平台适配 →
//! Runtime（TS）负责取数/变换/渲染。
//!
//! 铁律：
//! 1. IR 里不出现任何 ECharts 概念（没有 xAxis/series/dataZoom 这种词）；
//! 2. IR → option 的投影是纯函数且确定性（同 IR 两次投影 byte 级一致）；
//! 3. 数据变换（filter/bucket/aggregate/sort/limit）属于 IR 语义，不属于 Adapter ——
////    这样 AntV / D3 适配器可以复用同一套变换语义。

pub mod echarts;
pub mod validate;

use crate::dsl::{ChartDecl, Message};
use crate::validate::Diagnostic;

// ── 语义类型 ────────────────────────────────────────────

/// V0.5 design token 白名单（RFC-0010 §7.4）。
pub const THEME_TOKENS: [&str; 3] = ["default", "dark", "data-screen"];

/// DSL `theme` 声明 → 合法 token（未知值回落 default；校验层另行报错）。
pub fn theme_token(declared: &Option<String>) -> String {
    match declared.as_deref() {
        Some(t) if THEME_TOKENS.contains(&t) => t.to_string(),
        _ => "default".to_string(),
    }
}

/// 图表语义类型（与任何图表库无关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Line,
    Bar,
    Area,
    Pie,
    Scatter,
    Funnel,
    Gauge,
    Radar,
    Heatmap,
    Treemap,
    /// 指标卡（V0.2）：单个聚合值 + 格式化（KPI）
    Metric,
    /// 数据表（V0.2）：原始行以 <table> 渲染（真实数据回退）
    Table,
    /// K 线（V0.6 / AntV）：开高低收四字段（G2Plot Candlestick；echarts 不支持 → 构建期拒绝）
    Candlestick,
}

impl ChartType {
    pub const ALL: [ChartType; 13] = [
        ChartType::Line,
        ChartType::Bar,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Scatter,
        ChartType::Funnel,
        ChartType::Gauge,
        ChartType::Radar,
        ChartType::Heatmap,
        ChartType::Treemap,
        ChartType::Metric,
        ChartType::Table,
        ChartType::Candlestick,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "line" => Some(ChartType::Line),
            "bar" => Some(ChartType::Bar),
            "area" => Some(ChartType::Area),
            "pie" => Some(ChartType::Pie),
            "scatter" => Some(ChartType::Scatter),
            "funnel" => Some(ChartType::Funnel),
            "gauge" => Some(ChartType::Gauge),
            "radar" => Some(ChartType::Radar),
            "heatmap" => Some(ChartType::Heatmap),
            "treemap" => Some(ChartType::Treemap),
            "metric" => Some(ChartType::Metric),
            "table" => Some(ChartType::Table),
            "candlestick" => Some(ChartType::Candlestick),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ChartType::Line => "line",
            ChartType::Bar => "bar",
            ChartType::Area => "area",
            ChartType::Pie => "pie",
            ChartType::Scatter => "scatter",
            ChartType::Funnel => "funnel",
            ChartType::Gauge => "gauge",
            ChartType::Radar => "radar",
            ChartType::Heatmap => "heatmap",
            ChartType::Treemap => "treemap",
            ChartType::Metric => "metric",
            ChartType::Table => "table",
            ChartType::Candlestick => "candlestick",
        }
    }

    /// 数据形状家族：决定「行数据 → option 数据」的整形规则。
    pub fn shape(&self) -> ShapeFamily {
        match self {
            ChartType::Line | ChartType::Bar | ChartType::Area => ShapeFamily::Cartesian,
            ChartType::Scatter => ShapeFamily::Xy,
            ChartType::Pie | ChartType::Funnel | ChartType::Treemap => ShapeFamily::PartToWhole,
            ChartType::Gauge => ShapeFamily::Gauge,
            ChartType::Radar => ShapeFamily::Radial,
            ChartType::Heatmap => ShapeFamily::Heatmap,
            ChartType::Metric => ShapeFamily::Metric,
            ChartType::Table => ShapeFamily::Table,
            // K 线：x 时间 + 四值（AntV 专用渲染；形状按笛卡尔整形）
            ChartType::Candlestick => ShapeFamily::Cartesian,
        }
    }
}

/// 数据形状家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeFamily {
    /// 笛卡尔：x 类目/时间 + y 数值（可多系列）
    Cartesian,
    /// 二维散点：x 数值 + y 数值
    Xy,
    /// 部分-整体：分类 + 数值
    PartToWhole,
    /// 雷达：多指标
    Radial,
    /// 仪表：单值
    Gauge,
    /// 热力：x 分类 + y 分类 + 数值
    Heatmap,
    /// 指标卡：单个聚合值（V0.2）
    Metric,
    /// 数据表：原始行渲染（V0.2）
    Table,
}

impl ShapeFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShapeFamily::Cartesian => "cartesian",
            ShapeFamily::Xy => "xy",
            ShapeFamily::PartToWhole => "part-to-whole",
            ShapeFamily::Radial => "radial",
            ShapeFamily::Gauge => "gauge",
            ShapeFamily::Heatmap => "heatmap",
            ShapeFamily::Metric => "metric",
            ShapeFamily::Table => "table",
        }
    }
}

/// 视觉通道的值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Temporal,
    Quantitative,
    Nominal,
    Ordinal,
}

impl ValueType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "temporal" => Some(ValueType::Temporal),
            "quantitative" => Some(ValueType::Quantitative),
            "nominal" => Some(ValueType::Nominal),
            "ordinal" => Some(ValueType::Ordinal),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ValueType::Temporal => "temporal",
            ValueType::Quantitative => "quantitative",
            ValueType::Nominal => "nominal",
            ValueType::Ordinal => "ordinal",
        }
    }
}

/// 数值/时间格式化语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Currency,
    Percent,
    Integer,
    Decimal,
    DateTime,
    Date,
    None,
}

impl DataFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "currency" => Some(DataFormat::Currency),
            "percent" => Some(DataFormat::Percent),
            "integer" => Some(DataFormat::Integer),
            "decimal" => Some(DataFormat::Decimal),
            "datetime" => Some(DataFormat::DateTime),
            "date" => Some(DataFormat::Date),
            "none" => Some(DataFormat::None),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DataFormat::Currency => "currency",
            DataFormat::Percent => "percent",
            DataFormat::Integer => "integer",
            DataFormat::Decimal => "decimal",
            DataFormat::DateTime => "datetime",
            DataFormat::Date => "date",
            DataFormat::None => "none",
        }
    }
}

// ── IR 结构 ──────────────────────────────────────────────

/// 数据绑定：图表从哪个 api 操作取数。
#[derive(Debug, Clone)]
pub struct DataBinding {
    pub api: String,
    pub operation: String,
    /// 由 api 反查得到的实体名
    pub entity: String,
}

/// 一个视觉通道（x / y / series / color）。
#[derive(Debug, Clone)]
pub struct Channel {
    /// DSL 侧字段路径
    pub field: String,
    /// 运行时数据键（snake_case；关系字段取 `{rel}_name`）
    pub data_key: String,
    pub value_type: ValueType,
    pub format: DataFormat,
    /// 轴标题（仅 axis 通道）
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Encoding {
    pub x: Option<Channel>,
    pub y: Option<Channel>,
    pub series: Option<Channel>,
    pub color: Option<Channel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
}

impl CompareOp {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "eq" => Some(CompareOp::Eq),
            "ne" => Some(CompareOp::Ne),
            "gt" => Some(CompareOp::Gt),
            "gte" => Some(CompareOp::Gte),
            "lt" => Some(CompareOp::Lt),
            "lte" => Some(CompareOp::Lte),
            "in" => Some(CompareOp::In),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CompareOp::Eq => "eq",
            CompareOp::Ne => "ne",
            CompareOp::Gt => "gt",
            CompareOp::Gte => "gte",
            CompareOp::Lt => "lt",
            CompareOp::Lte => "lte",
            CompareOp::In => "in",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketUnit {
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl BucketUnit {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hour" => Some(BucketUnit::Hour),
            "day" => Some(BucketUnit::Day),
            "week" => Some(BucketUnit::Week),
            "month" => Some(BucketUnit::Month),
            "year" => Some(BucketUnit::Year),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            BucketUnit::Hour => "hour",
            BucketUnit::Day => "day",
            BucketUnit::Week => "week",
            BucketUnit::Month => "month",
            BucketUnit::Year => "year",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggOp {
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

impl AggOp {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sum" => Some(AggOp::Sum),
            "avg" => Some(AggOp::Avg),
            "count" => Some(AggOp::Count),
            "min" => Some(AggOp::Min),
            "max" => Some(AggOp::Max),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AggOp::Sum => "sum",
            AggOp::Avg => "avg",
            AggOp::Count => "count",
            AggOp::Min => "min",
            AggOp::Max => "max",
        }
    }
}

/// 数据变换算子（IR 语义；运行时通用执行，Adapter 不参与）。
#[derive(Debug, Clone)]
pub enum TransformOp {
    Filter {
        field: String,
        data_key: String,
        cmp: CompareOp,
        value: String,
    },
    Bucket {
        field: String,
        data_key: String,
        by: BucketUnit,
        as_name: String,
    },
    Aggregate {
        agg: AggOp,
        field: Option<String>,
        data_key: Option<String>,
        group_by: Vec<String>,
        group_keys: Vec<String>,
        as_name: String,
    },
    Sort {
        by: String,
        data_key: String,
        desc: bool,
    },
    Limit {
        n: usize,
    },
}

#[derive(Debug, Clone, Default)]
pub struct TransformPlan {
    pub ops: Vec<TransformOp>,
}

#[derive(Debug, Clone, Default)]
pub struct AxisSpec {
    pub x: Option<Channel>,
    pub y: Option<Channel>,
}

#[derive(Debug, Clone)]
pub struct SeriesSpec {
    /// 是否按 series 通道分组（多系列）
    pub grouped: bool,
    pub stack: Option<String>,
    /// 折线是否平滑
    pub smooth: bool,
    /// 是否显示数据点符号
    pub show_symbol: bool,
}

#[derive(Debug, Clone)]
pub struct TooltipSpec {
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct LegendSpec {
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct InteractionSpec {
    pub zoom: bool,
}

#[derive(Debug, Clone)]
pub struct ResponsiveSpec {
    pub min_height: u32,
}

#[derive(Debug, Clone, Default)]
pub struct A11ySpec {
    pub aria_label: Option<String>,
}

/// 图表完整规格（Chart IR）。
#[derive(Debug, Clone)]
pub struct ChartSpec {
    pub id: String,
    pub chart_type: ChartType,
    pub shape: ShapeFamily,
    pub title: Message,
    pub data: DataBinding,
    pub encoding: Encoding,
    pub transform: TransformPlan,
    pub axis: AxisSpec,
    pub series: SeriesSpec,
    pub tooltip: TooltipSpec,
    pub legend: LegendSpec,
    pub interaction: InteractionSpec,
    pub theme: String,
    pub responsive: ResponsiveSpec,
    pub accessibility: A11ySpec,
    /// V0.6 工业监控：阈值线（y 值 + 可选标签）；None = 无
    pub threshold: Option<ThresholdSpec>,
}

#[derive(Debug, Clone)]
pub struct ThresholdSpec {
    pub value: f64,
    pub label: String,
}

// ── IR → JSON 投影（给 Runtime 消费）─────────────────────

/// Chart IR → JSON（camelCase 键，与运行时 TS 类型一一对应）。
///
/// 缺省键一律「省略」而不是写 null：运行时 tsconfig 开了 strictNullChecks，
/// `xAxis: null` 不是合法值。
pub fn spec_json(spec: &ChartSpec) -> serde_json::Value {
    use serde_json::{json, Map, Value};
    let mut m = Map::new();
    m.insert("id".into(), json!(spec.id));
    m.insert("chartType".into(), json!(spec.chart_type.as_str()));
    m.insert("shape".into(), json!(spec.shape.as_str()));
    m.insert(
        "title".into(),
        json!(crate::dsl::pick_message(&spec.title, &spec.id)),
    );
    m.insert("entity".into(), json!(spec.data.entity));
    m.insert("api".into(), json!(spec.data.api));
    m.insert("operation".into(), json!(spec.data.operation));
    m.insert("encoding".into(), encoding_json(&spec.encoding));
    m.insert("transform".into(), transform_plan_json(&spec.transform));
    m.insert("grouped".into(), json!(spec.series.grouped));
    if let Some(x) = &spec.axis.x {
        m.insert("xAxis".into(), channel_json(x));
    }
    if let Some(y) = &spec.axis.y {
        m.insert("yAxis".into(), channel_json(y));
    }
    m.insert("tooltip".into(), json!(spec.tooltip.enabled));
    m.insert("legend".into(), json!(spec.legend.enabled));
    m.insert("zoom".into(), json!(spec.interaction.zoom));
    m.insert("smooth".into(), json!(spec.series.smooth));
    m.insert("showSymbol".into(), json!(spec.series.show_symbol));
    m.insert("minHeight".into(), json!(spec.responsive.min_height));
    m.insert("theme".into(), json!(spec.theme));
    m.insert(
        "ariaLabel".into(),
        json!(spec
            .accessibility
            .aria_label
            .clone()
            .unwrap_or_else(|| spec.id.clone())),
    );
    Value::Object(m)
}

fn channel_json(c: &Channel) -> serde_json::Value {
    use serde_json::{json, Map, Value};
    let mut m = Map::new();
    m.insert("field".into(), json!(c.field));
    m.insert("dataKey".into(), json!(c.data_key));
    m.insert("valueType".into(), json!(c.value_type.as_str()));
    m.insert("format".into(), json!(c.format.as_str()));
    if let Some(l) = &c.label {
        m.insert("label".into(), json!(l));
    }
    Value::Object(m)
}

fn encoding_json(enc: &Encoding) -> serde_json::Value {
    use serde_json::{Map, Value};
    let mut m = Map::new();
    if let Some(c) = &enc.x {
        m.insert("x".into(), channel_json(c));
    }
    if let Some(c) = &enc.y {
        m.insert("y".into(), channel_json(c));
    }
    if let Some(c) = &enc.series {
        m.insert("series".into(), channel_json(c));
    }
    if let Some(c) = &enc.color {
        m.insert("color".into(), channel_json(c));
    }
    Value::Object(m)
}

/// 变换计划 → JSON（Runtime 通用执行器消费）。
pub fn transform_plan_json(plan: &TransformPlan) -> serde_json::Value {
    use serde_json::{json, Map, Value};
    let mut ops: Vec<Value> = Vec::new();
    for op in &plan.ops {
        let mut m = Map::new();
        match op {
            TransformOp::Filter {
                field,
                data_key,
                cmp,
                value,
            } => {
                m.insert("op".into(), json!("filter"));
                m.insert("field".into(), json!(field));
                m.insert("dataKey".into(), json!(data_key));
                m.insert("cmp".into(), json!(cmp.as_str()));
                m.insert("value".into(), json!(value));
            }
            TransformOp::Bucket {
                field,
                data_key,
                by,
                as_name,
            } => {
                m.insert("op".into(), json!("bucket"));
                m.insert("field".into(), json!(field));
                m.insert("dataKey".into(), json!(data_key));
                m.insert("by".into(), json!(by.as_str()));
                m.insert("as".into(), json!(as_name));
            }
            TransformOp::Aggregate {
                agg,
                field,
                data_key,
                group_by,
                group_keys,
                as_name,
            } => {
                m.insert("op".into(), json!("aggregate"));
                m.insert("agg".into(), json!(agg.as_str()));
                if let Some(f) = field {
                    m.insert("field".into(), json!(f));
                }
                if let Some(k) = data_key {
                    m.insert("dataKey".into(), json!(k));
                }
                m.insert("groupBy".into(), json!(group_by));
                m.insert("groupKeys".into(), json!(group_keys));
                m.insert("as".into(), json!(as_name));
            }
            TransformOp::Sort { by, data_key, desc } => {
                m.insert("op".into(), json!("sort"));
                m.insert("by".into(), json!(by));
                m.insert("dataKey".into(), json!(data_key));
                m.insert("desc".into(), json!(desc));
            }
            TransformOp::Limit { n } => {
                m.insert("op".into(), json!("limit"));
                m.insert("n".into(), json!(n));
            }
        }
        ops.push(Value::Object(m));
    }
    Value::Array(ops)
}

// ── 降级：DSL 声明 → Chart IR ───────────────────────────

/// 字段解析结果：DSL 字段 → 运行时数据键 + 值类型。
#[derive(Debug, Clone)]
pub struct FieldResolution {
    pub data_key: String,
    pub value_type: ValueType,
    pub is_relation: bool,
}

/// 解析实体上的一个字段（V0.1 只支持直接字段）。
pub fn resolve_field(
    program: &crate::dsl::Program,
    entity: &str,
    path: &str,
) -> Option<FieldResolution> {
    let ent = crate::dsl::find_entity(program, entity)?;
    if path.contains('.') {
        return None; // V0.1 不支持跨关系路径
    }
    let field = ent.fields.iter().find(|f| f.name == path)?;
    if crate::dsl::is_relation(program, &field.type_name) {
        // 关系维度取显示名列（与后端 ListRow 的 `{rel}_name` 对齐）
        return Some(FieldResolution {
            data_key: format!("{}_name", crate::identity::to_snake_case(&field.name)),
            value_type: ValueType::Nominal,
            is_relation: true,
        });
    }
    let value_type = field_value_type(program, &field.type_name);
    Some(FieldResolution {
        data_key: crate::identity::to_snake_case(&field.name),
        value_type,
        is_relation: false,
    })
}

/// DSL 类型 → 值类型。
pub fn field_value_type(_program: &crate::dsl::Program, type_name: &str) -> ValueType {
    match type_name {
        "Timestamp" | "Date" => ValueType::Temporal,
        "Int" | "Float" | "Decimal" => ValueType::Quantitative,
        // 其余（字符串/枚举/未知）都是类别型
        _ => ValueType::Nominal,
    }
}

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

/// DSL `chart` 块 → Chart IR。失败返回诊断列表。
pub fn lower_chart(
    program: &crate::dsl::Program,
    decl: &ChartDecl,
) -> Result<ChartSpec, Vec<Diagnostic>> {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // V0.5：未知 theme token 显式报错（不静默回落 default）
    if let Some(t) = &decl.theme {
        if !THEME_TOKENS.contains(&t.as_str()) {
            errors.push(diag(
                "viz-unknown-theme",
                format!(
                    "chart '{}' uses unknown theme '{}' (allowed: {})",
                    decl.name,
                    t,
                    THEME_TOKENS.join(", ")
                ),
                decl.line,
            ));
        }
    }

    let Some(chart_type) = ChartType::parse(&decl.chart_type) else {
        return Err(vec![diag(
            "viz-unknown-type",
            format!(
                "chart '{}' has unknown type '{}' (supported: {})",
                decl.name,
                decl.chart_type,
                ChartType::ALL
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            decl.line,
        )]);
    };

    // 数据绑定
    let Some((api_name, operation)) = decl.source.clone() else {
        return Err(vec![diag(
            "viz-unknown-api",
            format!("chart '{}' has no `source`", decl.name),
            decl.line,
        )]);
    };
    let Some(api) = program.apis.iter().find(|a| a.name == api_name) else {
        return Err(vec![diag(
            "viz-unknown-api",
            format!(
                "chart '{}' references unknown api '{}'",
                decl.name, api_name
            ),
            decl.line,
        )]);
    };
    if !api.operations.iter().any(|o| o == &operation) {
        return Err(vec![diag(
            "viz-unknown-operation",
            format!(
                "chart '{}' uses operation '{}.{}' which is not in operations [{}]",
                decl.name,
                api_name,
                operation,
                api.operations.join(", ")
            ),
            decl.line,
        )]);
    }
    let entity = api.entity.clone();

    // V0.6 candlestick：必须 x + ohlc 四字段，且四字段都是实体上的数值字段
    if chart_type == ChartType::Candlestick {
        let Some(ohlc) = &decl.encoding.ohlc else {
            return Err(vec![diag(
                "viz-candlestick-missing-ohlc",
                format!(
                    "chart '{}' (candlestick) requires `ohlc {{ open high low close }}` encoding",
                    decl.name
                ),
                decl.line,
            )]);
        };
        if decl.encoding.x.is_none() {
            return Err(vec![diag(
                "viz-candlestick-missing-x",
                format!(
                    "chart '{}' (candlestick) requires an `x` channel",
                    decl.name
                ),
                decl.line,
            )]);
        }
        let Some(ent) = program.entities.iter().find(|e| e.name == entity) else {
            return Err(vec![diag(
                "viz-unknown-entity",
                format!("unknown entity '{entity}'"),
                decl.line,
            )]);
        };
        for (label, field) in [
            ("open", &ohlc.open),
            ("high", &ohlc.high),
            ("low", &ohlc.low),
            ("close", &ohlc.close),
        ] {
            let Some(fd) = ent.fields.iter().find(|f| &f.name == field) else {
                errors.push(diag(
                    "viz-candlestick-unknown-field",
                    format!(
                        "chart '{}' ohlc.{} references unknown field '{entity}.{}'",
                        decl.name, label, field
                    ),
                    decl.line,
                ));
                continue;
            };
            if !matches!(fd.type_name.as_str(), "Int" | "Float" | "Decimal") {
                errors.push(diag(
                    "viz-candlestick-non-numeric",
                    format!(
                        "chart '{}' ohlc.{} field '{entity}.{}' must be numeric (got {})",
                        decl.name, label, field, fd.type_name
                    ),
                    decl.line,
                ));
            }
        }
    }

    // 聚合输出字段（如 `op count` 产生 value）：编码通道可直接引用它
    let agg_output = aggregate_output_name(decl);

    // 编码通道
    let encoding = lower_encoding(program, decl, &entity, agg_output.as_deref(), &mut errors);

    // 变换
    let transform = lower_transform(program, decl, &entity, &mut errors);

    // 轴
    let axis = AxisSpec {
        x: lower_axis_channel(
            program,
            decl,
            &entity,
            agg_output.as_deref(),
            &mut errors,
            true,
        ),
        y: lower_axis_channel(
            program,
            decl,
            &entity,
            agg_output.as_deref(),
            &mut errors,
            false,
        ),
    };

    let mut spec = ChartSpec {
        id: decl.name.clone(),
        chart_type,
        shape: chart_type.shape(),
        title: decl.title.clone(),
        data: DataBinding {
            api: api_name,
            operation,
            entity,
        },
        encoding,
        transform,
        axis,
        series: SeriesSpec {
            grouped: decl.encoding.series.is_some(),
            stack: None,
            smooth: decl.appearance.smooth.unwrap_or(false),
            show_symbol: decl.appearance.show_symbol.unwrap_or(true),
        },
        tooltip: TooltipSpec {
            enabled: decl.tooltip.unwrap_or(true),
        },
        legend: LegendSpec {
            enabled: decl.legend.unwrap_or(decl.encoding.series.is_some()),
        },
        interaction: InteractionSpec {
            zoom: decl.zoom.unwrap_or(false),
        },
        theme: theme_token(&decl.theme),
        responsive: ResponsiveSpec { min_height: 240 },
        accessibility: A11ySpec {
            aria_label: crate::dsl::pick_message(&decl.title, &decl.name).into(),
        },
        threshold: decl.threshold.as_ref().map(|t| ThresholdSpec {
            value: t.value,
            label: t.label.clone(),
        }),
    };

    // 形状级校验（必需通道 / 类型匹配）
    validate::validate_chart(&mut spec, &mut errors);

    if errors.is_empty() {
        Ok(spec)
    } else {
        Err(errors)
    }
}

/// 聚合算子的输出字段名（默认 "value"）：编码通道可以引用它作为数值通道。
fn aggregate_output_name(decl: &ChartDecl) -> Option<String> {
    decl.transform.iter().find_map(|op| match op {
        crate::dsl::TransformDecl::Aggregate { .. } => Some("value".to_string()),
        _ => None,
    })
}

fn lower_channel(
    program: &crate::dsl::Program,
    decl: &ChartDecl,
    entity: &str,
    agg_output: Option<&str>,
    ch: &crate::dsl::ChannelDecl,
    errors: &mut Vec<Diagnostic>,
) -> Option<Channel> {
    if ch.field.is_empty() {
        errors.push(diag(
            "viz-unknown-field",
            format!("chart '{}' has a channel without `field`", decl.name),
            decl.line,
        ));
        return None;
    }
    let Some(res) = resolve_field(program, entity, &ch.field) else {
        // 聚合输出字段：数值通道（count/sum/avg… 的结果）
        if agg_output == Some(ch.field.as_str()) {
            return Some(Channel {
                field: ch.field.clone(),
                data_key: ch.field.clone(),
                value_type: ValueType::Quantitative,
                format: ch
                    .format
                    .as_deref()
                    .and_then(DataFormat::parse)
                    .unwrap_or(DataFormat::None),
                label: ch.label.clone(),
            });
        }
        errors.push(diag(
            "viz-unknown-field",
            format!(
                "chart '{}' references unknown field '{}.{}'",
                decl.name, entity, ch.field
            ),
            decl.line,
        ));
        return None;
    };
    // 声明类型与实际类型的一致性
    let declared = ch.value_type.as_deref().and_then(ValueType::parse);
    if let Some(d) = declared {
        if !type_compatible(res.value_type, d) {
            errors.push(diag(
                "viz-type-mismatch",
                format!(
                    "chart '{}' declares field '{}' as {} but it is {}",
                    decl.name,
                    ch.field,
                    d.as_str(),
                    res.value_type.as_str()
                ),
                decl.line,
            ));
        }
    }
    let format = match ch.format.as_deref() {
        None => default_format(res.value_type),
        Some(f) => match DataFormat::parse(f) {
            Some(fmt) => fmt,
            None => {
                errors.push(diag(
                    "viz-unknown-format",
                    format!("chart '{}' uses unknown format '{}'", decl.name, f),
                    decl.line,
                ));
                DataFormat::None
            }
        },
    };
    Some(Channel {
        field: ch.field.clone(),
        data_key: res.data_key,
        value_type: declared.unwrap_or(res.value_type),
        format,
        label: ch.label.clone(),
    })
}

/// 实际类型与声明类型是否相容（nominal/ordinal 互认）。
fn type_compatible(actual: ValueType, declared: ValueType) -> bool {
    // nominal/ordinal 互认（同属类别型）
    matches!(
        (actual, declared),
        (ValueType::Temporal, ValueType::Temporal)
            | (ValueType::Quantitative, ValueType::Quantitative)
            | (ValueType::Nominal, ValueType::Nominal)
            | (ValueType::Nominal, ValueType::Ordinal)
            | (ValueType::Ordinal, ValueType::Nominal)
            | (ValueType::Ordinal, ValueType::Ordinal)
    )
}

fn default_format(vt: ValueType) -> DataFormat {
    match vt {
        ValueType::Temporal => DataFormat::None,
        ValueType::Quantitative => DataFormat::None,
        _ => DataFormat::None,
    }
}

fn lower_encoding(
    program: &crate::dsl::Program,
    decl: &ChartDecl,
    entity: &str,
    agg_output: Option<&str>,
    errors: &mut Vec<Diagnostic>,
) -> Encoding {
    Encoding {
        x: decl
            .encoding
            .x
            .as_ref()
            .and_then(|c| lower_channel(program, decl, entity, agg_output, c, errors)),
        y: decl
            .encoding
            .y
            .as_ref()
            .and_then(|c| lower_channel(program, decl, entity, agg_output, c, errors)),
        series: decl
            .encoding
            .series
            .as_ref()
            .and_then(|c| lower_channel(program, decl, entity, agg_output, c, errors)),
        color: decl
            .encoding
            .color
            .as_ref()
            .and_then(|c| lower_channel(program, decl, entity, agg_output, c, errors)),
    }
}

fn lower_axis_channel(
    program: &crate::dsl::Program,
    decl: &ChartDecl,
    entity: &str,
    agg_output: Option<&str>,
    errors: &mut Vec<Diagnostic>,
    is_x: bool,
) -> Option<Channel> {
    let src = if is_x {
        decl.axis.x.as_ref()
    } else {
        decl.axis.y.as_ref()
    };
    let mut ch = src?.clone();
    // 轴通道未声明 field 时回落到编码通道（轴继承编码的字段与类型）
    if ch.field.is_empty() {
        let enc = if is_x {
            decl.encoding.x.as_ref()
        } else {
            decl.encoding.y.as_ref()
        };
        let enc = enc?;
        ch.field = enc.field.clone();
        if ch.value_type.is_none() {
            ch.value_type = enc.value_type.clone();
        }
        if ch.format.is_none() {
            ch.format = enc.format.clone();
        }
    }
    lower_channel(program, decl, entity, agg_output, &ch, errors)
}

fn lower_transform(
    program: &crate::dsl::Program,
    decl: &ChartDecl,
    entity: &str,
    errors: &mut Vec<Diagnostic>,
) -> TransformPlan {
    let mut ops = Vec::new();
    for op in &decl.transform {
        match op {
            crate::dsl::TransformDecl::Filter { field, cmp, value } => {
                let Some(res) = resolve_field(program, entity, field) else {
                    errors.push(diag(
                        "viz-unknown-field",
                        format!(
                            "chart '{}' filter references unknown field '{}.{}'",
                            decl.name, entity, field
                        ),
                        decl.line,
                    ));
                    continue;
                };
                let Some(cmp) = CompareOp::parse(cmp) else {
                    errors.push(diag(
                        "viz-unknown-transform",
                        format!("chart '{}' uses unknown filter op '{}'", decl.name, cmp),
                        decl.line,
                    ));
                    continue;
                };
                ops.push(TransformOp::Filter {
                    field: field.clone(),
                    data_key: res.data_key,
                    cmp,
                    value: value.clone(),
                });
            }
            crate::dsl::TransformDecl::Bucket { field, by } => {
                let Some(res) = resolve_field(program, entity, field) else {
                    errors.push(diag(
                        "viz-unknown-field",
                        format!(
                            "chart '{}' bucket references unknown field '{}.{}'",
                            decl.name, entity, field
                        ),
                        decl.line,
                    ));
                    continue;
                };
                if res.value_type != ValueType::Temporal {
                    errors.push(diag(
                        "viz-type-mismatch",
                        format!(
                            "chart '{}' buckets field '{}' which is {} (bucket requires temporal)",
                            decl.name,
                            field,
                            res.value_type.as_str()
                        ),
                        decl.line,
                    ));
                    continue;
                }
                let Some(unit) = BucketUnit::parse(by) else {
                    errors.push(diag(
                        "viz-unknown-transform",
                        format!("chart '{}' uses unknown bucket unit '{}'", decl.name, by),
                        decl.line,
                    ));
                    continue;
                };
                ops.push(TransformOp::Bucket {
                    field: field.clone(),
                    data_key: res.data_key.clone(),
                    by: unit,
                    as_name: res.data_key,
                });
            }
            crate::dsl::TransformDecl::Aggregate {
                op,
                field,
                group_by,
            } => {
                let Some(agg) = AggOp::parse(op) else {
                    errors.push(diag(
                        "viz-unknown-transform",
                        format!("chart '{}' uses unknown aggregate op '{}'", decl.name, op),
                        decl.line,
                    ));
                    continue;
                };
                let (data_key, _) = match field {
                    Some(f) => {
                        let Some(res) = resolve_field(program, entity, f) else {
                            errors.push(diag(
                                "viz-unknown-field",
                                format!(
                                    "chart '{}' aggregate references unknown field '{}.{}'",
                                    decl.name, entity, f
                                ),
                                decl.line,
                            ));
                            continue;
                        };
                        if res.value_type != ValueType::Quantitative {
                            errors.push(diag(
                                "viz-type-mismatch",
                                format!(
                                    "chart '{}' aggregates field '{}' which is {} (aggregate requires quantitative)",
                                    decl.name,
                                    f,
                                    res.value_type.as_str()
                                ),
                                decl.line,
                            ));
                            continue;
                        }
                        (Some(res.data_key), Some(f.clone()))
                    }
                    None => (None, None),
                };
                // count 不带 field；其余聚合必须带 field
                if agg == AggOp::Count && data_key.is_some() {
                    errors.push(diag(
                        "viz-aggregate-conflict",
                        format!(
                            "chart '{}' uses `op count` with a field (count takes no field)",
                            decl.name
                        ),
                        decl.line,
                    ));
                    continue;
                }
                if agg != AggOp::Count && data_key.is_none() {
                    errors.push(diag(
                        "viz-aggregate-conflict",
                        format!(
                            "chart '{}' uses `op {}` without a field",
                            decl.name,
                            agg.as_str()
                        ),
                        decl.line,
                    ));
                    continue;
                }
                let mut group_keys = Vec::new();
                let mut bad = false;
                for g in group_by {
                    let Some(res) = resolve_field(program, entity, g) else {
                        errors.push(diag(
                            "viz-unknown-field",
                            format!(
                                "chart '{}' aggregate groupBy references unknown field '{}.{}'",
                                decl.name, entity, g
                            ),
                            decl.line,
                        ));
                        bad = true;
                        continue;
                    };
                    group_keys.push(res.data_key);
                }
                if bad {
                    continue;
                }
                ops.push(TransformOp::Aggregate {
                    agg,
                    field: field.clone(),
                    data_key,
                    group_by: group_by.clone(),
                    group_keys,
                    as_name: "value".into(),
                });
            }
            crate::dsl::TransformDecl::Sort { by, desc } => {
                let Some(res) = resolve_field(program, entity, by) else {
                    errors.push(diag(
                        "viz-unknown-field",
                        format!(
                            "chart '{}' sort references unknown field '{}.{}'",
                            decl.name, entity, by
                        ),
                        decl.line,
                    ));
                    continue;
                };
                ops.push(TransformOp::Sort {
                    by: by.clone(),
                    data_key: res.data_key,
                    desc: *desc,
                });
            }
            crate::dsl::TransformDecl::Limit { n } => {
                ops.push(TransformOp::Limit { n: *n });
            }
        }
    }
    TransformPlan { ops }
}
