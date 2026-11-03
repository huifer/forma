# Capability 注册表（Capability Registry）

> 本文件是 Forma 所有受管能力的索引。每个能力须有 Manifest（见 [RFC-0004](../rfc/RFC-0004-capability-system.zh-CN.md)），
> 并满足 Discovery / Registration / Versioning / Dependency / Permissions / Validation / Adapter / Runtime / Conformance / Deprecation 十项契约。
> 新增能力请在本表登记，并确保**不修改 Semantic Core**（Tier 1 红线）。

---

## 数据能力（Data Capabilities）

| ID | 类别 | 状态 | Manifest | Conformance |
|---|---|---|---|---|
| `data.mysql` | 数据库 | ✅ 已实现 | 待补 | 待补 |
| `data.postgresql` | 数据库 | 🔨 规划 | — | — |
| `data.sqlite` | 数据库 | 🔨 规划 | — | — |

## 可视化能力（Visualization Capabilities）

| ID | 类别 | 状态 | Manifest | Conformance |
|---|---|---|---|---|
| `visualization.echarts` | 图表渲染 | ✅ 已实现（RFC-0010） | 待补 | 待补 |
| `visualization.vega-lite` | 图表渲染 | 🔨 规划 | — | — |
| `visualization.maplibre` | 地图 | 🔨 规划 | — | — |
| `visualization.react-flow` | 流程图 | 🔨 规划 | — | — |

## UI 能力（UI Capabilities）

| ID | 类别 | 状态 | Manifest | Conformance |
|---|---|---|---|---|
| `ui.form` | 表单 | ✅ 已实现 | 待补 | 待补 |
| `ui.table` | 表格 | ✅ 已实现 | 待补 | 待补 |
| `ui.dialog` | 对话框 | ✅ 已实现 | 待补 | 待补 |
| `ui.navigation` | 导航 | ✅ 已实现 | 待补 | 待补 |

## 工作流能力（Workflow Capabilities）

| ID | 类别 | 状态 | Manifest | Conformance |
|---|---|---|---|---|
| `workflow.approval` | 审批 | 🔨 规划 | — | — |
| `workflow.event` | 事件 | 🔨 规划 | — | — |
| `workflow.scheduler` | 调度 | 🔨 规划 | — | — |

## 集成能力（Integrations）

| ID | 类别 | 状态 | Manifest | Conformance |
|---|---|---|---|---|
| `integration.openapi` | 导入/导出 | ✅ 已实现 | 待补 | 待补 |
| `integration.jsonschema` | 导入/导出 | ✅ 已实现 | 待补 | 待补 |
| `integration.sql` | 导入/导出 | ✅ 已实现 | 待补 | 待补 |

---

## 图例

- ✅ 已实现：已有生成器产出，待补 Manifest / Conformance 登记。
- 🔨 规划：路线图中的后续波次，尚未实现。

## 登记规则

1. 在对应类别表新增一行。
2. 在 `capability/<category>/` 下创建能力文档（含 Manifest 草案）。
3. 在 `conformance/` 下提供或规划对应测试套件。
4. 引用相关 RFC / ADR。
