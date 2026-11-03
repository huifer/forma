---
id: RFC-0004
title: Capability System and Extension Architecture
status: Proposed
version: 1.0
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-04
domain: capability
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0003]
related_adrs: [ADR-0001]
related_specs: [spec:manifests, spec:ir, spec:compatibility]
breaking_change: false
---

# RFC-0004：能力系统与扩展架构

## 1. Metadata

见 frontmatter。本 RFC 定义 Forma 的功能扩展机制：所有功能扩展都设计为**可注册、可发现、可验证、可替换**的 Capability，而非散落在核心的特例代码。

## 2. Abstract

Forma 的长期扩容能力取决于一条铁律：**新增功能不应要求修改 Semantic Core**。本 RFC 规定 Capability 的统一 Manifest、注册/发现/验证/替换契约，以及「通用语义 / 领域语义 / 技术实现」三层 IR 分类，使新图表库、新数据库、新 Agent 协议、新 UI Runtime 都能在不触动核心的前提下接入。

## 3. Motivation

### 3.1 Current Problem

当前可视化（RFC-0010）、数据库、Agent 工具均为核心内特例实现。新增一类能力往往需要改 `dsl`、`ir`、`emit` 多处，且缺乏统一 Manifest 与兼容声明。

### 3.2 Why Existing Architecture Is Insufficient

`architecture.zh-CN.md` 描述了多目标编译，但未规定「能力如何被注册、发现、验证、替换」，也未区分「通用语义」与「技术实现」。这正是 Forma 能否长期扩容的关键缺口。

### 3.3 Expected Outcomes

- 新增 Capability 不必改核心架构（[README §10](README.md) Phase 1 标准之一）。
- 新增 Adapter 不必改 IR。
- 能力可独立安装、替换、升级、废弃。

## 4. Goals

- 定义 Capability Manifest（统一元数据契约）。
- 定义发现 / 注册 / 验证 / 替换机制。
- 定义 IR 三层分类与扩展策略。
- 规定 Tier 1 红线：Capability 不得修改 Semantic Core。

## 5. Non-goals

- 不定义具体某一能力（可视化见 RFC-0010；数据库/UI 后续领域 RFC）。
- 不规定 Manifest 字段的最终版本规则（属 `spec:manifests`）。
- 不实现 `forma docs` 自动化（属 RFC-0008 / 工具层）。

## 6. Terminology

- **Capability**：一类可扩展功能（数据 / 可视化 / UI / 工作流 / 集成）。
- **Adapter**：某 Capability 的具体技术实现（如 ECharts / MySQL）。
- **Registry**：Capability 与 Adapter 的注册与发现中心。

## 7. Architectural Context

### 7.1 System Context

```text
Forma Core
   └── Capability Registry
         ├── Data Capabilities      (MySQL / PostgreSQL / SQLite)
         ├── Visualization Caps     (ECharts / Vega-Lite / MapLibre / React Flow)
         ├── UI Capabilities        (Form / Table / Dialog / Navigation)
         └── Workflow Capabilities  (Approval / Event / Scheduler)
```

### 7.2 Dependencies

依赖 RFC-0001（边界）、RFC-0003（IR 不可被 Capability 修改）。

### 7.3 Boundaries

- Capability **只能消费** IR，不得修改 Semantic Core。
- Adapter **只实现**某 Capability 声明的能力，不引入新通用语义。
- 技术实现（ECharts/React Flow/MySQL）**不得进入通用语义层**。

## 8. Design

### 8.1 Conceptual Model

每个 Capability 是一个带 Manifest 的一等实体：声明它 `provides` 什么能力、`consumes` 什么 IR、`adapters` 有哪些、`runtime` 是什么、`dependencies` 与 `permissions` 为何、`compatibility` 与 `conformance` 如何。

### 8.2 Architecture

```yaml
apiVersion: forma.dev/v1
kind: Capability
metadata:
  id: visualization.echarts
  version: 1.0.0
spec:
  category: visualization
  provides: [chart.bar, chart.line, chart.pie, chart.scatter]
  consumes: [ChartIR, DataBinding]
  adapters: [echarts-adapter]
  runtime: [forma-web-runtime]
  dependencies: [capability.data-binding]
  permissions: [data.read]
  compatibility:
    ir: ">=1.0.0 <2.0.0"
    runtime: ">=1.0.0"
  conformance:
    suite: visualization.chart.v1
```

> 此例为建议方向，最终字段与版本规则由 `spec:manifests` 正式定义。

### 8.3 Data Model

Manifest 是结构化 YAML/JSON；能力标识 `category.id`（如 `visualization.echarts`）。具体 schema 见 `spec:manifests`。

### 8.4 Interfaces and Contracts

每个 Capability 必须提供统一的扩展契约：

| 能力 | 必须要求 |
|---|---|
| Discovery | 可被 Registry 发现 |
| Registration | 可注册 |
| Versioning | 有版本 |
| Dependency | 可声明依赖 |
| Permissions | 可声明权限 |
| Validation | 可验证输入 |
| Adapter | 有具体实现 |
| Runtime | 声明运行环境 |
| Conformance | 有兼容测试 |
| Deprecation | 有废弃与迁移策略 |

### 8.5 Execution Flow

1. 项目声明启用的 Capabilities（锁文件）。
2. `forma build` 加载 Registry，解析依赖闭包。
3. 每个 Capability 的 Adapter 消费 IR 生成产物。
4. Conformance Suite 验证实现符合 Manifest 承诺。

### 8.6 Extension Points

- **新 Capability 类别**：在 Registry 增加 `category`，不碰核心。
- **新 Adapter**：实现某 Capability 的 `provides`，不碰 IR。
- **IR 扩展字段**（见 §9）：`extensions.{capabilityId}` 命名空间，不污染核心语义。

## 9. Compatibility（IR 扩展策略）

IR 分三层，**技术实现不进入通用语义**：

| 类型 | 是否属于 Core | 示例 |
|---|---|---|
| 通用语义 | 是 | Entity、Relation、Type、Rule |
| 领域语义 | 独立 IR 模块 | Chart、Map、Workflow |
| 技术实现 | 否 | ECharts、React Flow、MySQL |

不推荐：`kind: echarts / type: bar`。推荐：核心语义 `kind: chart / type: bar`，实现偏好走可选 `renderer`：

```yaml
kind: chart
type: bar
renderer:
  capability: visualization.echarts
```

`renderer` 应为可选实现偏好，非 ChartIR 必需字段——同一份 ChartIR 可由其他 Adapter 处理。

**扩展字段规则**：

- 未知扩展**可保留**，但**不可擅自解释**。
- Adapter **只处理**自己声明支持的扩展。
- 扩展字段**不得覆盖**核心语义。
- 扩展升级**必须有迁移策略**。

```yaml
kind: chart
id: dashboard.sales
type: bar
extensions:
  visualization.echarts:
    animation: true
    progressive: true
```

## 10. Security Considerations

- `permissions` 声明运行时约束（如 `data.read`），执行环境按声明限制。
- 第三方 Adapter 执行风险须评估（代码执行 / 网络访问 / 文件访问）。
- 详见 RFC-0008。

## 11. Performance and Reliability

- 能力注册为静态解析，构建期确定依赖闭包，避免运行时动态加载不确定性。
- 未知扩展保留不报错，保证向前兼容。

## 12. Alternatives Considered

- **插件动态加载任意代码**：拒绝——破坏确定性与 Tier 1 安全。
- **每个能力硬编码进核心**：拒绝——违背本 RFC 目标。

## 13. Trade-offs

引入 Manifest 与 Registry 的复杂度，换取真正的可扩容。值得。

## 14. Migration Plan

- 既有可视化实现（RFC-0010）作为首个受管 Capability 范例，补 Manifest 与 Conformance。
- 既有数据库 / 后端 / web / flutter emitter 逐步登记为 Data / UI Capability。

## 15. Implementation Plan

- 阶段 1：本 RFC + `capability/registry.md` 索引 + 可视化 Capability 范例。
- 阶段 2：`spec:manifests` 字段定稿 + `forma docs` 校验。
- 阶段 3：数据库 / UI Capability 登记。

## 16. Conformance Requirements

- [ ] 新增 Capability 的 PR 不修改 Semantic Core（Tier 1 红线）。
- [ ] 每个 Capability 提供 Manifest 且含 §8.4 全部 10 项契约。
- [ ] 未知 IR 扩展被保留且不被擅解释（测试覆盖）。

## 17. Acceptance Criteria

- 在不修改核心的前提下，接入一个新的可视化渲染引擎（Adapter）并通过 Conformance。
- 在不修改 IR 的前提下，接入一个新的数据库（Data Capability）。

## 18. Open Questions

- Registry 是编译期静态 Manifest 还是运行时发现？本 RFC 倾向编译期静态。
- Capability 版本与 IR 版本的兼容矩阵如何自动化校验？

## 19. References

- 治理总入口：[README.md](README.md)
- 架构总览：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 可视化范例：[RFC-0010-visualization.zh-CN.md](RFC-0010-visualization.zh-CN.md)
- 迁移安全：[../migration-safety.zh-CN.md](../migration-safety.zh-CN.md)

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案，定义 Capability Manifest / 三层 IR / 扩展策略 |
