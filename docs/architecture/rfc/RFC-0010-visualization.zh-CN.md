---
id: RFC-0010
title: Visualization Specification and Rendering System
status: Accepted            # V0.1–V0.3 / V0.5 已落地（metric/table + dashboard IR + theme/preview/渲染门，见 §13 分期与 CHANGELOG）
version: 1.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-04
domain: capability
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0004]
related_adrs: [ADR-0001]
related_specs: [spec:ir]
breaking_change: false
---

# RFC-0010：Forma 可视化规格与渲染系统

> 状态：**已接受（Accepted）· 实现分期 V0.1 落地中**（本 RFC 由 docs/rfc/0001-visualization.zh-CN.md 迁入治理体系统一编号）
> 日期：2026-11-05
> 关联：RFC-0001（架构总览）、RFC-0004（能力系统）、[dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md) §11、[roadmap.zh-CN.md](../../roadmap.zh-CN.md)
> 决策人：Forma 维护者
> 影响范围：DSL 语法（新增 `chart` / `dashboard` 顶层构造）、Unified IR、web-react 适配层、MCP 工具面、Verify

---

## 0. 摘要

Forma 目前的能力边界是：**Specification → IR → Compiler → Code**（数据 / API / 业务规则 / CRUD 页面）。

这条链路的末端止于「代码」。但对 AI 主导的开发而言，**可视化是规格最终呈现给用户的主要出口**：一个业务系统真正的价值不在 DDL 和 axum handler，而在「这张趋势图能不能看、这张看板能不能联动」。

AI 今天要做一个数据大屏，必须自己写 ECharts option、Tooltip/Legend/DataZoom 配置、响应式布局、交互事件、数据格式转换。代价是：

- **Token 昂贵**：同一段 option 样板在每个项目里被重复生成；
- **质量不稳定**：不同 Agent 生成的 option 风格、边界处理（null / 空数组 / 时区）差异极大；
- **不可校验**：option 是最终形态的 JS 对象，没有可静态检查的语义层，字段拼错、类型不匹配只能等运行时报错；
- **不可迁移**：一旦把 ECharts 的配置格式写进规格，规格就被某个图表库绑架。

本 RFC 提议：**把可视化提升为 Forma 的第三个一等核心子系统**，并与已有的 Compiler / Verify 并列：

| 子系统 | 解决的问题 |
|---|---|
| Forma Compiler | 规格如何变成代码与工程产物 |
| Forma Verify | 产物是否正确、可运行、安全 |
| **Forma Visualization** | **规格如何变成可运行、可交互、可维护的可视化页面** |

关键设计立场（详见 §1.3）：

> **不要让 ECharts 的配置格式成为 Forma 自己的规格语言。**
> YAML/DSL 描述意图 → Forma IR 表达统一语义 → Adapter 负责目标平台适配 → Runtime 负责实际渲染与交互。

本 RFC 同时给出 **V0.1 可执行切片**的完整实现：DSL `chart` 块 → Chart IR → ECharts Adapter → React Runtime → 页面挂载 → 验收测试（已落地，见 §13 与 `crates/forma/src/visualization/`）。

---

## Metadata（元信息）

- RFC 编号：RFC-0010；状态：`Accepted`（实现分期 V0.1–V0.3 / V0.5 已落地，见 §13）
- 分层：可视化属 **Tier 2 可扩展层**（RFC-0001 §7.3），不进入 Semantic Core
- 关联：[RFC-0001](../rfc/RFC-0001-architecture-overview.zh-CN.md)、[RFC-0004](../rfc/RFC-0004-capability-system.zh-CN.md)、
  [ADR-0002](../adr/accepted/ADR-0002-canonical-ir-program-over-federation-graph.zh-CN.md)

## 1. 动机与问题陈述

### 1.1 现状缺口

Forma 的六个 emitter（mysql / rust_backend / web_react / flutter / openapi / diagrams）覆盖了「数据—后端—表单页面」。缺口在两处：

1. **图表**：DSL 里没有任何表达「把某些字段画成某种图」的构造。AI 想加一个趋势图，只能绕过 Forma 直接改生成出来的 React 文件——而生成物是 `@generated` 的，下次 `forma build` 就被覆盖。这是 Forma 架构的**原则性违规**，不是能力缺失。
2. **仪表盘 / 分析页面**：`dsl-spec.zh-CN.md` §11 里 `dashboard` 被标注为「P2」占位。真实业务页面不是单个图表，而是 KPI 卡 + 趋势图 + 排名表 + 筛选器 + 联动。

### 1.2 为什么必须是独立子系统而不是「前端模板的附属功能」

如果把 ECharts 生成逻辑塞进 `web_react.rs`，会立刻产生三个不可逆的债：

| 债 | 后果 |
|---|---|
| DSL 里出现 ECharts 概念（`xAxis` / `series` / `dataZoom`） | 规格被单一图表库绑架；换 AntV / D3 意味着重写所有规格 |
| 图表语义无法校验 | 「y 轴是时间还是分类」「是否需要 Top N」「是否有双 Y 轴」无法在编译期判定 |
| 图表逻辑不可复用 | 同一份图表语义无法投影到 React / Flutter / 数据大屏 / 服务端静态图 |

因此本 RFC 把 Visualization 建成 **Specification / IR / Adapter / Runtime / Verify 五层**，与 Data / API / UI 同级，纳入 Unified IR 而不是旁挂。

### 1.3 设计原则（不可违反）

1. **DSL 不泄漏目标概念**：`.fm` 里没有 ECharts、没有 React。`chart` 块描述的是「语义图表类型 + 编码 + 变换」，不是 option。
2. **IR 是唯一语义来源**：Chart IR / Dashboard IR 表达统一语义，Adapter 是 IR → 目标 option 的**确定性纯函数投影**。
3. **同一语义，多种投影**：Line 语义在 web-react 投影为 ECharts option，在 mobile-flutter 投影为自绘/内嵌图，在 future adapter 投影为 SVG。当前只实现 ECharts，但 IR 不含 ECharts 概念。
4. **编译期可判定**：类型不匹配、字段不存在、形状不匹配、聚合与分组冲突——全部在 Check 阶段报诊断，绝不留给运行时。
5. **确定性**：同一规格 → 同一产物（byte 级）。这是 Forma 全链路已有的铁律（见 memory: seed determinism）。
6. **受控逃逸口**：`nativeExtension` 允许在明确标记为「不可移植」的前提下使用目标库原生能力；不牺牲 ECharts 表达力，也不污染标准规格。

### 1.4 非目标（本文档不解决）

- 不做 BI 语义建模（多维数据集、语义层、指标口径管理）——V0.6 之后单独立项。
- 不做地图/GIS、网络图、蜡烛图等 Level 4 领域可视化——留 `nativeExtension` 与后续 adapter。
- 不替代 TanStack Table / shadcn-ui——Forma 适配既有生态，不重复造 UI 组件库。
- 不做可视化编辑器（拖拽 Studio）——Studio v2（M9）之后可能消费本 IR，但不在本 RFC 范围。

---

## Non-goals（明确不做）

- 不做 BI 语义建模（多维数据集、语义层、指标口径管理）——V0.6 之后单独立项（§16 未决问题）
- 不把 ECharts 配置格式泄漏为 DSL 词汇（分层红线：DSL 只有语义，Adapter 才认识 ECharts，见 §2 / §7）
- 不让图表类型扩展修改 Semantic Core（Tier 2 边界，见 §17）
- 不覆盖移动端复杂图表（V0.6 之后评估，§16）

## 2. 分层模型与术语

### 2.1 五层

```text
   AI / Human Specification
   业务意图 · 数据字段 · 图表类型 · 交互规则 · 布局约束
                │
                ▼
   Visualization Specification            ← DSL: chart / dashboard 块
   Chart Spec / Dashboard Spec / Data Binding / Interaction
                │
                ▼
   Forma Unified IR
   Data IR · API IR · Business IR · UI IR · Chart IR · Dashboard IR · Workflow IR
                │
                ▼
   Visualization Adapter                  ← 纯函数：IR → 目标 option
   ECharts（当前） · AntV（P1） · D3（P2） · 其他 Renderer
                │
                ▼
   Visualization Runtime                  ← 执行：取数 / 变换 / 渲染 / 事件 / 响应式 / 主题
   React Components · Data Binding · Event Bus · ResizeObserver · ThemeProvider
```

### 2.2 各层落在仓库的哪个位置

| 层 | 位置 | 说明 |
|---|---|---|
| Specification | `crates/forma/src/dsl/parser.rs`（`chart` / `dashboard` 块） | 词法复用现有 lexer |
| Chart IR / Dashboard IR | `crates/forma/src/visualization/` | 新增模块；IR 载体 |
| Adapter | `crates/forma/src/visualization/echarts.rs` | 纯函数 `chart_option(spec) -> serde_json::Value` |
| Runtime（源码模板） | `crates/forma/src/emit/visualization.rs` | 生成 TS/TSX 到 web target |
| Check | `crates/forma/src/visualization/validate.rs` + `dsl/mod.rs::check_program` | 诊断码 `viz-*` |
| Verify | `crates/forma/tests/visualization.rs` | 快照 + 确定性 + 语义错误 |

### 2.3 关键术语

| 术语 | 定义 |
|---|---|
| **Chart Spec** | 单个可视化组件的完整规格（数据、编码、变换、轴、系列、提示、图例、交互、主题、响应式、无障碍） |
| **Shape Family** | 数据形状家族：cartesian / xy / part-to-whole / radial / gauge / heatmap。决定「行数据 → option 数据」的整形规则 |
| **Encoding** | 字段到视觉通道（x / y / series / color / size）的映射声明 |
| **Transform Plan** | 由 IR 编译出的、可序列化的数据变换计划（filter / bucket / aggregate / sort / limit），由 Runtime 通用执行 |
| **Dashboard Spec** | 多个 widget 的组合规格：布局、筛选器、共享数据、联动关系 |
| **Native Extension** | 受控的目标库原生配置透传，标记为不可移植 |

---

## 3. DSL 规格

### 3.1 顶层 `chart` 块（V0.1 已实现）

```fm
chart EnrollmentTrend {
  type    line
  title   { zh "报名趋势" en "Enrollment trend" }

  source  Enrollments.list

  encoding {
    x      { field enrolledAt type temporal format datetime }
    y      { field value      type quantitative format integer }
    series { field status     type nominal }
  }

  transform {
    filter    { field status eq "active" }
    bucket    { field enrolledAt by day }
    aggregate { op count groupBy [enrolledAt, status] }   // 输出字段名固定为 value
    sort      { by enrolledAt desc }
    limit     30
  }

  axis {
    x { label "报名时间" type datetime }
    y { label "报名数"   format integer }
  }

  tooltip     { enabled true }
  legend      { enabled true }
  interaction { zoom true }
  appearance  { smooth true showSymbol false }
}
```

> `y` 引用的 `value` 是聚合输出字段（见 §4.4），不需要实体上存在同名字段。

**语法细则**

| 构造 | 语法 | 语义 |
|---|---|---|
| `type` | `line \| bar \| area \| pie \| scatter \| funnel \| gauge \| radar \| heatmap \| treemap` | 语义图表类型 |
| `title` | `{ zh "…" en "…" }` | i18n 标题（复用 `Message`） |
| `source` | `<ApiName>.<operation>` | 数据绑定；指向已声明的 `api` 块的操作 |
| `encoding.x/y/series/color` | `{ field <path> [type <t>] [format <f>] }` | 视觉通道映射；`color` 仅 heatmap 使用（单元格值） |
| `transform.*` | 见 §4.4 | 数据变换 |
| `axis.x/y` | `{ label "…" [type <t>] [format <f>] }` | 轴语义 |
| `tooltip` / `legend` | `{ enabled <bool> }` | 提示 / 图例开关 |
| `interaction` | `{ zoom <bool> }` | 交互开关 |
| `appearance` | `{ smooth <bool> showSymbol <bool> }` | 线条/符号样式 |

**类型词表**

| 词 | 含义 | 允许的字段类型 |
|---|---|---|
| `temporal` | 时间 | `Timestamp` / `Date` |
| `quantitative` | 数值 | `Int` / `Float` / `Decimal` |
| `nominal` | 无序分类 | `String` / `Email` / `Text` / enum / relation |
| `ordinal` | 有序分类 | 同 `nominal` |

**format 词表**：`currency` / `percent` / `integer` / `decimal` / `datetime` / `date` / `none`。

### 3.2 顶层 `dashboard` 块（V0.3 目标，本 RFC 定义但不落地）

```fm
dashboard SalesOverview {
  title { zh "销售分析" en "Sales overview" }

  layout {
    type    grid
    columns 12
    gap     16
  }

  filters {
    - id date-range
      type date-range
      binding Orders.createdAt
    - id region
      type select
      binding Orders.region
  }

  widgets {
    - id total-revenue
      type metric
      source Orders.list
      field amount
      agg sum
      position { x 0 y 0 w 3 h 2 }

    - id revenue-trend
      type chart
      chart EnrollmentTrend
      position { x 3 y 0 w 9 h 4 }

    - id region-ranking
      type table
      source Orders.list
      position { x 0 y 4 w 6 h 4 }
  }

  interactions {
    - source region-ranking
      event  row-click
      target revenue-trend
      action  filter
      field  region
  }
}
```

要点：

- `position` **不是唯一布局方式**。布局语义支持 `grid` / `flex` / `responsive`，固定坐标只是大屏场景的一种策略；`grid` 之外的布局用 `order` + `span` 表达。
- `filters` 绑定到字段（`binding`）而不是绑定到某个 widget，运行时作为共享的 filter state 注入所有引用同源的数据请求。
- `interactions` 描述**联动关系**，不是绑定某个图表库的事件名（见 §8.4 事件模型）。

### 3.3 页面挂载

```fm
page EnrollmentDashboard {
  route "/enrollments/dashboard"
  title { zh "报名分析" en "Enrollment analytics" }
  chart EnrollmentTrend          // V0.1：单图表页面
  // dashboard SalesOverview    // V0.3：仪表盘页面
}
```

`page` 新增第四种 body：`chart <ChartName>`。语义检查会验证 chart 存在。

### 3.4 词法影响

现有 lexer 已支持 Ident / Str / Num / Punct / Arrow。`chart` / `dashboard` 块**不引入新词法符号**。`=`、`[]`、`{}`、`:` 均已支持。

---

## 4. Chart IR

载体：`crates/forma/src/visualization/mod.rs`。IR 是从 DSL 声明**降级（lower）**出来的类型化结构，与语法解耦，供 Adapter / Check / Verify 共用。

### 4.1 顶层

```rust
/// 图表语义类型（与任何图表库无关）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Line, Bar, Area, Pie, Scatter, Funnel, Gauge, Radar, Heatmap, Treemap,
}

/// 数据形状家族：决定「行数据 → option 数据」的整形规则
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeFamily {
    /// 笛卡尔：x 类目/时间 + y 数值（可多系列）
    Cartesian,
    /// 二维散点：x 数值 + y 数值
    Xy,
    /// 部分-整体：分类 + 数值（饼/漏斗/树图）
    PartToWhole,
    /// 雷达：多指标
    Radial,
    /// 仪表：单值
    Gauge,
    /// 热力：x 分类 + y 分类 + 数值
    Heatmap,
}

pub struct ChartSpec {
    pub id: String,             // 图表 id（= DSL 块名）
    pub chart_type: ChartType,
    pub shape: ShapeFamily,
    pub title: Message,         // i18n
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
}
```

### 4.2 数据绑定与编码

```rust
pub struct DataBinding {
    /// 绑定的 api 块名（如 "Enrollments"）
    pub api: String,
    /// 操作（当前仅 "list"）
    pub operation: String,
    /// 绑定的实体名（由 api 反查）
    pub entity: String,
}

pub struct Encoding {
    pub x: Option<Channel>,
    pub y: Option<Channel>,
    pub series: Option<Channel>,
    /// 数值色通道：heatmap 的单元格值（未来可扩展散点 size）
    pub color: Option<Channel>,
}

pub struct Channel {
    /// DSL 字段路径（首段是实体字段）
    pub field: String,
    /// 运行时数据键（snake_case；关系字段取 `{rel}_name`）
    pub data_key: String,
    pub value_type: ValueType,   // Temporal | Quantitative | Nominal | Ordinal
    pub format: DataFormat,      // Currency | Percent | Integer | Decimal | DateTime | Date | None
}
```

**`data_key` 的推导规则**（与后端 ListRow 的 JSON 键严格对齐，见 memory: backend runtime contract）：

| DSL 字段 | 后端 ListRow JSON 键 | chart `data_key` |
|---|---|---|
| `enrolledAt` | `enrolled_at` | `enrolled_at` |
| `amount` | `amount` | `amount` |
| `course`（关系） | `course_id` + `course_name` | `course_name`（维度用显示名） |

### 4.3 形状推导规则（Check 阶段可判定）

| ChartType | Shape | 必需 encoding | 可选 encoding |
|---|---|---|---|
| line / area / bar | Cartesian | x, y | series |
| scatter | Xy | x, y | — |
| pie / funnel / treemap | PartToWhole | x（分类）, y（数值） | — |
| gauge | Gauge | y | — |
| radar | Radial | x（指标名）, y（数值） | — |
| heatmap | Heatmap | x（分类）, y（分类）, color（数值） | — |

违反 → 诊断 `viz-shape-mismatch`（error）。

### 4.4 变换（Transform Plan）

```rust
pub enum TransformOp {
    Filter { field: String, cmp: CompareOp, value: String },
    Bucket { field: String, by: BucketUnit, as_name: String },
    Aggregate { agg: AggOp, field: Option<String>, group_by: Vec<String>, as_name: String },
    Sort { by: String, desc: bool },
    Limit { n: usize },
}

pub struct TransformPlan { pub ops: Vec<TransformOp> }
```

`TransformPlan` 编译为**可序列化的 JSON 计划**交给 Runtime 通用执行：

```json
[
  { "op": "filter", "field": "status", "cmp": "eq", "value": "active" },
  { "op": "bucket", "field": "enrolled_at", "by": "day", "as": "enrolled_at" },
  { "op": "aggregate", "agg": "count", "field": null, "groupBy": ["enrolled_at"], "as": "value" },
  { "op": "sort", "by": "enrolled_at", "desc": true },
  { "op": "limit", "n": 30 }
]
```

**为什么变换在 Runtime 而不是 Adapter**：聚合、时间分桶、Top-N 不是 ECharts 的原生语义（ECharts 没有 dataset 聚合），若塞进 Adapter，AntV/D3 适配器就要重写一遍语义。变换属于 IR 层语义，Runtime 用统一执行器实现一次，所有 Adapter 复用。

**聚合字段冲突规则**：`aggregate.op = count` 时 `field` 必须缺省；其余聚合必须给出 `field`。违规 → `viz-aggregate-conflict`。

**聚合输出字段**：聚合产生名为 `value` 的数值字段，编码/轴通道可直接引用 `field value`——Check 阶段识别为聚合输出（ quantitative），不要求它是实体字段。这让「按天计数」「按区域求和」这类规格无需伪造实体字段：

```fm
encoding {
  x { field enrolledAt type temporal }        // 分桶后的时间
  y { field value      type quantitative }    // 聚合输出
}
transform {
  bucket    { field enrolledAt by day }
  aggregate { op count groupBy [enrolledAt] }
}
```

**分组键的运行时键**：`groupBy` 输出行使用数据键（snake_case），与 `channel.dataKey` 对齐——变换输出直接对接编码通道，不做第二套命名。

### 4.5 轴 / 系列 / 提示 / 图例 / 交互 / 主题 / 响应式 / 无障碍

```rust
pub struct AxisSpec {
    pub x: Option<AxisChannel>,
    pub y: Option<AxisChannel>,
}
pub struct AxisChannel { pub label: Option<String>, pub value_type: ValueType, pub format: DataFormat }

pub struct SeriesSpec { pub grouped: bool, pub stack: Option<String> }

pub struct TooltipSpec { pub enabled: bool }
pub struct LegendSpec { pub enabled: bool }

pub struct InteractionSpec { pub zoom: bool }

pub struct ResponsiveSpec { pub min_height: u32, pub aspect: Option<String> }

pub struct A11ySpec { pub aria_label: Option<String> }
```

---

## 5. Dashboard IR（V0.3 目标）

```rust
pub struct DashboardSpec {
    pub id: String,
    pub title: Message,
    pub layout: LayoutSpec,
    pub filters: Vec<FilterSpec>,
    pub widgets: Vec<WidgetSpec>,
    pub interactions: Vec<InteractionSpec2>,
}

pub enum LayoutSpec {
    Grid { columns: u32, gap: u32 },
    Flex { direction: FlexDirection, gap: u32 },
}

pub struct FilterSpec {
    pub id: String,
    pub kind: FilterKind,      // DateRange | Select | Text | Number
    pub binding: String,       // 字段路径
}

pub enum WidgetSpec {
    Chart { id: String, chart: String, position: Option<GridPos> },
    Metric { id: String, source: String, field: String, agg: AggOp, position: Option<GridPos> },
    Table { id: String, source: String, columns: Vec<String>, position: Option<GridPos> },
}

pub struct InteractionSpec2 {
    pub source: String,   // widget id
    pub event: VizEvent,  // RowClick | Select | RangeChange
    pub target: String,   // widget id
    pub action: VizAction,// Filter | Highlight | Drilldown | Navigate
    pub field: Option<String>,
}
```

联动语义由 `VizEvent` / `VizAction` 表达，**不出现任何图表库事件名**（见 §8.4）。

---

## 6. 语义校验规则与诊断码

所有诊断走现有 `Diagnostic { severity, code, message, file, line }` 结构。

| 码 | 级别 | 触发条件 |
|---|---|---|
| `viz-unknown-chart` | error | `page` 引用了不存在的 chart |
| `viz-unknown-type` | error | `type` 不在词表内 |
| `viz-unknown-api` | error | `source` 引用的 api 块不存在 |
| `viz-unknown-operation` | error | `source` 的操作不在该 api 的 `operations` 中 |
| `viz-unknown-field` | error | `encoding` / `transform` 引用的字段不在实体上（聚合输出 `value` 除外） |
| `viz-unknown-format` | error | `format` 不在词表内 |
| `viz-type-mismatch` | error | 字段实际类型与声明的 `type` 不符（如 Timestamp 声明为 quantitative）；bucket/aggregate 的字段类型不符同理 |
| `viz-shape-mismatch` | error | 图表类型要求的 encoding 不完整/多余 |
| `viz-aggregate-conflict` | error | `count` 聚合带 field，或非 count 聚合缺 field |
| `viz-duplicate` | error | 同一 Program 内 chart id 重复 |
| `viz-unknown-transform` | error | transform 里出现未知 op / bucket 单位 / 聚合算子 |

**这些错误在 `forma build` 里硬失败**：语义检查（`check_program`）是编译管线第 3 阶段（architecture §4），Emit 之前执行——可视化错误不能只靠 `forma validate` 事后发现。

**关系字段的维度语义**：关系字段在 ListRow 里是 `{rel}_id`（UUID）+ `{rel}_name`（显示名）。维度通道默认取 `{rel}_name`；若该关系可空则可能为 `null`，Check 阶段给出 warning（不阻断）。

---

## 7. ECharts Adapter

### 7.1 适配边界

```rust
/// 纯函数：Chart IR → ECharts option 骨架（数据位置留空，由 Runtime 绑定）
pub fn chart_option(spec: &ChartSpec) -> serde_json::Value
```

**约束**：

- 输入只有 IR，**不得**读取 DOM / 环境变量 / 随机数 / 时间；
- 输出是 `serde_json::Value`，键序稳定（`serde_json` 的 `Map` 在本仓库用 `preserve_order` 特性时保序；否则显式按固定序构造）；
- 同一 IR 两次调用结果 byte 级一致（确定性铁律）。

### 7.2 形状 → option 映射表

| Shape | ECharts 结构 | 数据绑定点 |
|---|---|---|
| Cartesian | `xAxis[]` + `yAxis[]` + `series[]` | `xAxis[0].data`（类目）/ `series[i].data`（数值数组） |
| Xy | `xAxis[]`+`yAxis[]`（均 value）+ `series[0].data` | `series[0].data` = `[x, y]` 对 |
| PartToWhole | 无坐标轴 + `series[0].data` | `series[0].data` = `{name, value}` 数组 |
| Radial | `radar[0].indicator` + `series[0].data` | indicator 名 + `{value: number[]}` |
| Gauge | `series[0]`（type gauge） | `series[0].data` = `[{value}]` |
| Heatmap | `xAxis[]`+`yAxis[]`（category）+ `visualMap` + `series[0]` | `xAxis[0].data` / `yAxis[0].data` / `series[0].data` = `[xi, yi, v]` |

### 7.3 各类型细则（V0.1 已实现的部分）

| ChartType | series.type | 备注 |
|---|---|---|
| line | `line` | `smooth` / `showSymbol` 由 appearance 决定 |
| area | `line` + `areaStyle` | 堆叠由 `series.stack` 决定 |
| bar | `bar` | 横向由 `axis.orientation`（V0.3） |
| pie | `pie` | `radius` 由 appearance 决定（默认 `'62%'`） |
| scatter | `scatter` | `symbolSize` 由 size encoding（V0.3） |
| funnel | `funnel` | — |
| gauge | `gauge` | `detail` 格式由 y.format 决定 |
| radar | `radar` | — |
| heatmap | `heatmap` | 需 `visualMap` |
| treemap | `treemap` | 需层级数据（V0.3 用 flat + namePath） |

### 7.4 主题（Design Token → ECharts Theme）

`theme` 取设计 token 名（默认 `default`）。Adapter 输出一份与主题无关的 option；主题在 Runtime 侧以 `echarts.registerTheme` + `init({theme})` 应用，或用 `option.color` 覆盖。

| token | 用途 |
|---|---|
| `default` | shadcn 中性色板 |
| `dark` | 深色模式 |
| `data-screen` | 数据大屏（高饱和 + 无边框） |

### 7.5 格式化（format → ECharts formatter）

| format | formatter |
|---|---|
| `currency` | `¥{v}` / `{v} CNY`（按 locale） |
| `percent` | `{v}%` |
| `integer` | `{v}`（千分位） |
| `decimal` | 固定 2 位小数 |
| `datetime` | `YYYY-MM-DD HH:mm` |
| `date` | `YYYY-MM-DD` |

格式化在 Runtime 的 `format.ts`（既有 `fmtMoney` / `fmtDateTime`）里已有实现，Adapter 只声明 format 语义，Runtime 复用。

### 7.6 Native Extension（受控逃逸口）

```fm
chart Weird {
  type line
  ...
  native echarts {
    series { markLine { data [{ type "average" }] } }
    graphic [ ... ]
  }
}
```

规则：

- 只允许**非核心**节点（`series.*` 的样式层、`graphic`、`markLine` 等），**禁止**覆盖 `xAxis` / `yAxis` / `data` / `dataset`；
- 产物中标记 `// @native-extension: echarts — not portable`；
- Check 阶段校验白名单；
- 破坏标准规格的可移植性由使用者显式承担。

### 7.7 性能

| 手段 | 位置 |
|---|---|
| 按需注册 | Runtime 只 `echarts.use()` 本工程实际用到的图表与组件（由编译器按 chart 类型清单生成），避免把整个 ECharts 打进产物（实测 bundle 1.34 MB → 842 KB） |
| `progressive` | Adapter 输出（大数据量渐进渲染） |
| `sampling` | Adapter 输出（`series.sampling = 'lttb'`） |
| `renderer: 'canvas' \| 'svg'` | 由 `responsive.aspect` 与数据量决定，Runtime 选择 |
| 数据量上限 | `transform.limit` + warning |
| 整数刻度 | y 通道 `format integer` 时输出 `yAxis.minInterval: 1`，计数轴不出现 0.2/0.6 小数刻度 |

---

## 8. React Runtime

### 8.1 组件接口

```tsx
// 由 web-react 适配层生成的页面使用
import { FormaChart } from "../viz/runtime";
import { EnrollmentTrendSpec, EnrollmentTrendOption, EnrollmentTrendSource }
  from "../viz/charts/EnrollmentTrend";

<FormaChart
  spec={EnrollmentTrendSpec}
  option={EnrollmentTrendOption}
  source={EnrollmentTrendSource}   // { fetch: () => Promise<Row[]> }
  height={320}
  ariaLabel="报名趋势"
/>
```

通用签名：

```tsx
export interface FormaChartProps {
  spec: ChartSpec;
  option: Record<string, unknown>;
  source: { fetch: () => Promise<Array<Record<string, unknown>>> };
  height?: number | string;
  width?: number | string;
  ariaLabel?: string;
  onEvent?: (e: VizEventPayload) => void;
  className?: string;
}
```

### 8.2 数据绑定

- `source.fetch()` 由 web emitter 生成，绑定到既有 API 客户端（`EnrollmentApi.list({})`）——**不新增取数通道**；
- Dashboard 场景下，filter state 作为额外参数注入 `fetch(params)`。

### 8.3 Transform 执行器

`src/viz/transform.ts` 实现 §4.4 的计划执行器，输出整形后的 `ChartData`：

```ts
export type ChartData =
  | { kind: "cartesian"; categories: string[]; series: Array<{ name: string; data: Array<number | null> }> }
  | { kind: "xy"; points: Array<[number, number]> }
  | { kind: "pairs"; items: Array<{ name: string; value: number }> }
  | { kind: "gauge"; value: number }
  | { kind: "radar"; indicators: string[]; values: number[] }
  | { kind: "heatmap"; xLabels: string[]; yLabels: string[]; cells: Array<[number, number, number]> };
```

### 8.4 事件模型（统一，不绑定图表库事件名）

```ts
export type VizEventPayload =
  | { type: "select"; series?: string; dataIndex: number; value: unknown }
  | { type: "legend-toggle"; series: string; visible: boolean }
  | { type: "range-change"; start: number; end: number }
  | { type: "row-click"; rowIndex: number; row: Record<string, unknown> };
```

Adapter 负责把 ECharts 事件翻译成 `VizEventPayload`；Dashboard 的 `interactions` 规则用 `VizEventPayload.type` 匹配，用 `VizAction` 描述目标行为。**换 Adapter 时事件模型不变。**

### 8.5 响应式

- `ResizeObserver` 监听容器 → `chart.resize()`；
- `responsive.min_height` 保证容器最小高度；
- 断点策略（V0.3）：`responsive.breakpoints` 决定小屏下的图表堆叠方式。

### 8.6 主题 Provider

`src/viz/theme.ts` 暴露 `FormaVizThemeProvider` + `useFormaVizTheme()`，把 design token 映射到 ECharts theme 名。

### 8.7 无障碍

- 容器 `role="img"` + `aria-label`（来自 `accessibility.aria_label` 或标题）；
- 表格类 widget（V0.2）提供 `<table>` 真实数据回退；
- 键盘：图表不可聚焦，但联动筛选器可聚焦（V0.4）。

### 8.8 生成的产物

| 文件 | 内容 |
|---|---|
| `src/viz/spec.ts` | `ChartSpec` / `TransformPlan` / `ChartData` 类型（适配器无关） |
| `src/viz/transform.ts` | 变换计划执行器 + 形状整形（适配器无关） |
| `src/viz/runtime.tsx` | `FormaChart` React 组件（取数 → 变换 → 渲染 → 事件） |
| `src/viz/echarts.ts` | ECharts renderer（init / setOption / resize / dispose / bindData / 事件翻译） |
| `src/viz/charts/<Name>.ts` | 每个图表：`Spec` + `Option` + `Source` |

---

## 9. Rust 模块设计

```text
crates/forma/src/
├── dsl/
│   ├── parser.rs        # chart / dashboard 块解析 → ChartDecl / DashboardDecl
│   └── mod.rs           # Program 合并 + check_program 接线
├── visualization/       # ← 新增
│   ├── mod.rs           # IR 类型 + lower（DSL decl → ChartSpec）+ 形状推导
│   ├── validate.rs      # 语义校验（viz-* 诊断码）
│   └── echarts.rs       # ECharts Adapter（纯函数）
├── emit/
│   ├── visualization.rs # ← 新增：web 运行时与图表产物生成
│   └── web_react.rs     # 页面挂载 chart body + echarts 依赖
└── build.rs             # 编排新增 "viz-web" 步骤
```

关键接口约定：

```rust
// visualization/mod.rs
pub fn lower_chart(program: &Program, decl: &ChartDecl) -> Result<ChartSpec, Vec<Diagnostic>>;
pub fn chart_option(spec: &ChartSpec) -> serde_json::Value;      // echarts.rs
pub fn transform_plan_json(plan: &TransformPlan) -> serde_json::Value;
```

---

## 10. MCP 工具面

与现有 `forma_check` / `forma_build` / `forma_graph` 并列，DSL 工程专用：

| 工具 | 用途 |
|---|---|
| `forma_viz_catalog` | 列出支持的图表类型、形状家族、transform op、format 词表 |
| `forma_viz_describe` | 返回指定图表类型的规格定义（字段、默认值、约束） |
| `forma_viz_validate` | 校验 chart / dashboard 规格（字段映射与语义） |
| `forma_viz_graph` | 返回可视化语义摘要（图表 → 数据源 → 字段 → 联动） |

AI 的典型工作流（对应用户需求「AI 不写 ECharts 代码」）：

1. `forma_graph` 查现有实体 / api / 字段；
2. `forma_viz_catalog` 查支持的图表类型；
3. 写 `chart` 块（DSL 补丁）；
4. `forma_viz_validate` 校验；
5. `forma_build` 编译；
6. `forma_viz_graph` 复核联动关系未被破坏。

---

## 11. Verify

| 检查 | 手段 |
|---|---|
| 语义正确 | `forma_viz_validate` + `viz-*` 诊断码 |
| 编译确定性 | 同一 Program 两次编译产物 byte 级一致 |
| 增量安全 | 修改单个 chart 不影响其他 chart / 页面 / 数据绑定（增量更新测试） |
| 运行时 | `forma build` 后 `npm run build`（Vite）必须通过——这是唯一能证明 TS 产物可编译的门 |
| 浏览器 | 图表页在真实浏览器渲染无 console error（人工/脚本抽检） |

---

## 12. 验收测试矩阵

| 组 | 数量 | 内容 |
|---|---|---|
| 图表类型编译 | 10 | line/bar/area/pie/scatter/funnel/gauge/radar/heatmap/treemap 各一条 fixture，断言 option 结构 |
| 数据结构与字段类型 | 10 | temporal/quantitative/nominal/ordinal × 关系字段/可空/枚举 |
| 交互行为 | 10 | tooltip/legend/zoom 开关、事件翻译、联动过滤 |
| 仪表盘布局 | 10 | grid / flex / position / filter / interaction（V0.3） |
| 增量更新 | 20 轮 | 改一个 chart 20 次，其余产物不变 |
| 确定性 | 全部 | 同规格多次编译结果一致 |

V0.1 已落地的断言见 `crates/forma/tests/visualization.rs`。

---

## Implementation Plan（实施计划）

按 §13 分期推进：V0.1 已落地（Chart IR + 十类图表 + Adapter + React Runtime）；
V0.2（metric/table + cursor 源）与 V0.3（dashboard grid + 筛选联动）已落地；
V0.5（theme + preview + 浏览器渲染门）已落地；V0.4 / V0.6 待排期（§13）。
每个切片的验收判据见 §12 验收测试矩阵。

## Conformance Requirements（一致性验收）

- [x] chart / dashboard 语义错误（viz-* / dsl-dashboard-*）在 Emit 前硬失败（§6 / §12）
- [x] 确定性：同一 Program 两次构建 byte 级一致（`tests/visualization.rs`）
- [x] 零变化：无 chart 声明工程不产生 viz 产物与 echarts 依赖（`tests/visualization_v02.rs`）
- [x] 浏览器渲染门：preview 页面 + mock API 断言渲染状态（V0.5，`tests/visualization_v05.rs`）
- [x] 回归锚点：`tests/visualization.rs` / `visualization_v02.rs` / `dashboard.rs` / `visualization_v05.rs`

## 13. 里程碑

| 版本 | 交付内容 | 状态 |
|---|---|---|
| V0.1 | Chart IR、十种图表类型词表、ECharts Adapter（确定性投影 + 按需注册）、React Runtime、页面挂载、MCP 三工具、23 条验收测试 | **已落地** |
| V0.2 | Data Binding 完整化（分页/服务端聚合）、Metric Card、Table widget | **已落地**（metric/table + cursor 源解包；服务端聚合阈值见 §16 未决） |
| V0.3 | Dashboard IR、Grid/Flex 布局、响应式断点 | **已落地**（dashboard grid + 筛选联动；Flex / 响应式断点未做，见 §16 未决） |
| V0.4 | Tooltip/Legend/Filter/Drill-down/联动事件模型 | **已落地**（2026-11-08）：Tooltip/Legend 已有；**联动**——图表点击 → dashboard 筛选器（其他图表重取）；**Drill-down**——`drilldown "/route"` 声明 → 点击跳转列表页 + URL query 初始筛选（列表页读 URLSearchParams）；目标路由必须已声明（`dsl-dashboard-unknown-drilldown`）；回归 `tests/visualization_v04.rs` 5 项 |
| V0.5 | Theme、Preview、Runtime Verification（浏览器渲染门） | **已落地**（theme dark/data-screen + preview + 浏览器渲染门，回归 `tests/visualization_v05.rs`） |
| V0.6 | AntV Adapter、领域图表扩展（GIS / 金融 / 工业监控）、移动端图表 | **部分落地**（2026-11-08）：AntV（G2Plot）适配层（`targets.visualization.adapter: antv`，与 echarts 共享 ChartSpec/变换，验证适配层无关性）；移动端图表 bar/line/pie（fl_chart，按需依赖）；领域图表评估结论见 §16-6 |

### V0.1 落地记录（实现与 RFC 的偏差均已回写正文）

- 编码通道增加 `color`（heatmap 必需）；§4.2/§4.3 已同步。
- 聚合输出字段 `value` 可直接作为数值通道；§4.4 已同步。
- ECharts 适配层运行时按工程实际用到的图表类型生成 `echarts.use([...])`（tree-shaking）；§7.7 已同步。
- `forma build` 现在在 Emit 前执行 `check_program`（编译管线第 3 阶段补齐）——语义错误（含 viz-*）硬失败；§6 已同步。
- 验收：`crates/forma/tests/visualization.rs` 23 条测试全绿；school 示例两个图表页在真实浏览器渲染验证通过（`tsc && vite build` 无错误，无 console error，ECharts 实例数据绑定断言通过）。

---

## 14. 备选方案与取舍

| 备选 | 结论 |
|---|---|
| **A. 在 DSL 里直接写 ECharts option** | 拒绝。规格被图表库绑架，无法校验、无法多目标投影，且违背「DSL 不泄漏目标概念」 |
| **B. 只做一个 ECharts 代码生成器（无 IR）** | 拒绝。缺语义层 → 缺校验、缺复用、缺多目标 |
| **C. 让 AI 继续直接写 React 组件，只提供工具函数** | 拒绝。等于放弃「目标代码是生成物」原则，产物不可维护 |
| **D. 图表完全独立于数据模型（自描述数据源）** | 拒绝。会与业务实体脱节，无法复用 api / 权限 / 校验 |
| **E. 引入现成 BI 语义层** | 推迟。成本高、语义重；先做「形状 + 编码 + 变换」的轻量层 |

---

## 15. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| ECharts option 细节复杂，Adapter 易错 | 图表不渲染 / 渲染错 | 形状家族收敛；每类型一条 fixture + 浏览器渲染抽检；`nativeExtension` 兜底 |
| 客户端聚合与后端语义不一致 | 数字对不上 | 聚合语义在 IR 中定义并双向对照；大数据量走服务端（V0.2） |
| 运行时体积膨胀 | 首屏变慢 | Runtime 按需加载；`transform.ts` 无第三方依赖 |
| 关系字段的 `{rel}_name` 语义被误解 | 维度显示为 UUID | Check 阶段提示；spec 显式记录 `data_key` |
| AI 生成的 chart 规格与实体字段不匹配 | 编译期报错阻断 | `viz-unknown-field` / `viz-type-mismatch` 在 Check 阶段硬失败，并给出候选字段 |

---

## Performance and Reliability（性能与可靠性）

- 图表数据取数走后端 `list` 接口（复用分页 / 过滤 / 游标协议，见 §8）；服务端聚合阈值属 §16 未决问题
- 变换在运行时按视图数据量执行（filter/bucket/aggregate/sort/limit），超限由 §16 的阈值策略约束
- 可靠性：渲染门（V0.5 preview）在构建后自检；图表不进入核心编译路径，失败不影响其它产物（零变化原则）

## 16. 未决问题

> 收口（2026-11-08，RFC §18 开放问题收口）：3 项关闭、1 项保留。

1. [x] ~~**服务端聚合 vs 客户端聚合的切换阈值**~~
   **已决议**：客户端聚合（V0.2 cursor 源解包 + 运行时变换）；不做 10k 行阈值切换（避免引入服务端状态）。
2. [x] ~~**Dashboard 布局是否需要独立 DSL 词汇**（`position` vs `span`）~~
   **已定**：`grid { <Chart> w N h N }`（span 语义）已落地（V0.3）；不引入绝对定位词汇。
3. [x] ~~**多语言图表标题与图例项**~~
   **已决议**：`title` 已是 i18n `Message`；枚举变体显示名由 `@label` 承载（ListRow 的 `{rel}_name` 同源）。
4. [x] ~~**图表是否进入 Flutter 适配层**~~
   **评估完成并落地最小集（2026-11-08）**：移动端图表需求弱（屏幕小、以列表/表单为主），
   但 bar/line 两类高频图表值得支持。落地形态：
   - **支持**：`bar` / `line` + 无 transform + x/y 为实体直接字段 → 生成 `fl_chart` 图表页
     （x 为 nominal 时用索引，y 取字段值）；
   - **占位**：其余类型 / 带 transform / 字段缺失 → 保持占位页（路由与导航结构完整）；
   - 依赖：仅当存在可支持图表时 `pubspec.yaml` 引入 `fl_chart`（零变化原则）；
   - 验证：`flutter analyze` 零 issue（school 的 CourseCapacity bar 页）。
   **未支持**：pie/gauge/radar 等复杂类型与 transform 聚合——留待真实需求。

5. ~~**Drill-down（下钻明细）评估**~~ **已落地（2026-11-08）**：dashboard 条目
   `drilldown "/route"` → 点击图表元素跳转该路由，携带 x 维度值作为 URL query；
   列表页从 URLSearchParams 读取初始筛选（非 zustand 分支）。目标路由必须已声明
   （检查层 `dsl-dashboard-unknown-drilldown`）。

6. **领域图表（GIS / 金融 / 工业监控）评估（2026-11-08）**：
   - **GIS/地图**：需要 GeoJSON 数据源 + 地图渲染（ECharts map / L7）——DSL 目前无地图数据构造，
     属于 RFC 级扩展（数据从哪来、如何绑定）。**结论：v1 不做**，需先立 `geo` 数据契约。
   - **金融 K 线**：ECharts 无内置（需 custom series），G2Plot 有 Candlestick——
     需要新 chart type `candlestick` + 开高低收的数据字段约定（数据契约先行）。
     **结论：待真实场景**，触发条件明确（有金融数据源时）。
   - **工业监控（时序告警）**：本质是 line + 阈值线（markLine/markArea）——
     **可基于现有 line 扩展**（加 threshold 通道），是三者中最易落地的；待需求。

## 17. 架构上下文与边界（Architectural Context）

可视化在 Forma 中是 **Tier 2 可扩展层**的一等能力，与 Data / API / UI 并列，**不是**核心编译器的一部分。

| 位置 | 边界 |
|---|---|
| Semantic Core（Tier 1） | 只承载通用语义（实体 / 关系 / 类型 / 规则）。**新增图表类型不得修改 Core**（见 RFC-0001 §7.3、RFC-0004） |
| Visualization IR（Tier 2） | Chart / Dashboard IR——本 RFC §4 / §5 定义，承载图表语义、编码与变换 |
| Adapter | ECharts Adapter（本 RFC §7）——IR → option 的**确定性纯函数投影** |
| Runtime | React Runtime（本 RFC §8）——取数 / 变换 / 渲染 / 事件 / 响应式 |

- 依赖方向：Core ← IR ← Adapter ← Runtime，**禁止反向**。
- 能力注册与兼容性声明遵循 RFC-0004；本 RFC 只负责可视化领域自身的语义与实现。

## 18. 兼容性（Compatibility）

| 维度 | 承诺 |
|---|---|
| **Chart IR 版本** | 独立于 Specification / Capability / Adapter / Runtime 版本（见 RFC-0004） |
| **向后兼容** | 破坏语义的 Chart IR 变更须走迁移并验证；新增图表类型属增量变更 |
| **向前兼容** | 未知 `extensions.visualization.echarts` 字段**保留、不擅解释**；Adapter 只处理声明支持的扩展 |
| **渲染引擎可替换** | Chart IR 不含 ECharts 概念；换渲染引擎 = 换 Adapter + Capability，**不改 Chart IR**（renderer 为可选实现偏好） |
| **产物归属** | 生成物带 `@generated` 头；人工修改不被静默覆盖（ADR-0003） |

破坏性变更须同时具备：迁移计划（§20）+ 兼容验证 + `breaking_change: true` 声明。

## 19. 安全考量（Security Considerations）

- **受控逃逸口**：`nativeExtension` 是唯一允许直接使用目标库原生能力的通道，必须**显式标记为不可移植**，且不得覆盖核心语义（见 §1.3）。
- **数据读权限**：Capability 声明 `permissions: [data.read]`；图表只消费既有 API 的可见数据，不建立新的数据通道。
- **数据外泄面**：图表系列可暴露实体字段。**字段级可见性由后端权威校验决定**——前端按权限隐藏不构成安全边界（ADR-0004）。
- **渲染面**：option 由服务端可信数据生成；不得把未校验的用户输入直接拼入 option。
- **DSL 逃逸口**：`nativeExtension` 中的表达式与字符串须经 Check 阶段校验，避免把任意脚本带入产物。

## 20. 迁移计划（Migration Plan）

| 步骤 | 说明 |
|---|---|
| 1 | DSL 新增 `chart` / `dashboard` 顶层构造——**增量**，既有 `.fm` 无需修改即可继续编译 |
| 2 | Check 阶段引入 `viz-*` 诊断码：语义错误在 Emit 前硬失败，不产出半成品 |
| 3 | ECharts Adapter + React Runtime 生成；产物遵循统一的 `@generated` 头与归属规则 |
| 4 | 可视化产物按需注册 ECharts（`echarts.use([...])`）；无 chart 的工程零产物、零依赖 |

- **回滚**：删除 DSL 中的 `chart` / `dashboard` 块即可；涉及数据结构的变更按 `emit/migrate.rs` 的版本化机制生成 `upgrade` / `rollback` 配对。
- **不破坏无关产物**：图表语义的变更只影响可视化相关产物（Chart IR / web runtime），不触及 DDL 与后端产物。
- 既有 `diagrams` 产出（Mermaid）遵循其自身的「一渲染块一 diagram 文档」约束，不受本 RFC 影响。

---

## References（参考）

- [RFC-0001：架构总览](../rfc/RFC-0001-architecture-overview.zh-CN.md)（Tier 分层 / 依赖红线）
- [RFC-0004：能力系统](../rfc/RFC-0004-capability-system.zh-CN.md)（`extensions.visualization.echarts` Manifest）
- [ADR-0002：DSL Program 为规范 IR](../adr/accepted/ADR-0002-canonical-ir-program-over-federation-graph.zh-CN.md)
- 实现位置：`crates/forma/src/visualization/`、`crates/forma/src/emit/visualization.rs`
- 回归：`crates/forma/tests/visualization.rs` / `visualization_v02.rs` / `dashboard.rs` / `visualization_v05.rs`

## 附录 A：完整示例（V0.1 语法）

```fm
app sales {
  version = "0.4.0"
  locale = ["zh", "en"]
}

entity Order {
  id        Uuid          @id @default(uuid)
  region    String(50)    @required
  amount    Decimal(12,2) @required
  status    OrderStatus   @default(pending)
  createdAt Timestamp     @default(now) @readonly
}

enum OrderStatus { pending paid cancelled }

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [region, status] sort [createdAt desc] }
}

chart RevenueTrend {
  type   line
  title  { zh "收入趋势" en "Revenue trend" }
  source Orders.list

  encoding {
    x      { field createdAt type temporal format datetime }
    y      { field amount    type quantitative format currency }
    series { field status    type nominal }
  }

  transform {
    bucket    { field createdAt by month }
    aggregate { op sum field amount groupBy [createdAt, status] }
    sort      { by createdAt desc }
    limit     24
  }

  axis {
    x { label "月份" type datetime }
    y { label "收入" format currency }
  }

  tooltip     { enabled true }
  legend      { enabled true }
  interaction { zoom true }
  appearance  { smooth true showSymbol false }
}

page RevenueDashboard {
  route "/revenue"
  title { zh "收入分析" en "Revenue analytics" }
  chart RevenueTrend
}
```

## 附录 B：IR → ECharts option 映射全表

见 §7.2 / §7.3。`option` 生成器是纯函数：输入 `ChartSpec`，输出 `serde_json::Value`，不含任何运行时状态。

## 附录 C：变更记录（Change Log）

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-05 | 初版采纳（Accepted）：V0.1 可视化子系统落地——DSL `chart` 块 → Chart IR → ECharts Adapter → React Runtime，诊断码 `viz-*` 在 `forma build` 的 Emit 前硬失败 |
| 1.0 | 2026-11-05 | 迁入治理体系：由 `docs/rfc/0001-visualization.zh-CN.md` 迁入并统一编号为 RFC-0010 |
