---
id: visualization.echarts
title: Capability Manifest — visualization.echarts
status: Draft
version: 0.1.0
depends_on: [RFC-0004, RFC-0010]
consumes: [ChartIR, DashboardIR, DataBinding]
---

# Capability: visualization.echarts

可视化渲染能力：将 `ChartIR` / `DashboardIR` 编译为 ECharts option + React 运行时。
本文件是**首个受管 Capability 范例**，用于验证 RFC-0004 的核心命题——**不改 Semantic Core 即可接入新渲染引擎**。

---

## Manifest（遵循 spec:manifests）

```yaml
apiVersion: forma.dev/v1
kind: Capability
metadata:
  id: visualization.echarts
  version: 0.1.0
spec:
  category: visualization
  provides:
    - chart.bar
    - chart.line
    - chart.area
    - chart.pie
    - chart.scatter
    - chart.funnel
    - chart.gauge
    - chart.radar
    - chart.heatmap
    - chart.treemap
  consumes:
    - ChartIR
    - DashboardIR
    - DataBinding
  adapters:
    - echarts-adapter            # crates/forma/src/visualization/echarts.rs
  runtime:
    - forma-web-runtime         # React + ECharts（按需 echarts.use([...]) 做 tree-shaking）
  dependencies:
    - capability.data-binding
  permissions:
    - data.read
  compatibility:
    ir: ">=1.0.0 <2.0.0"
    runtime: ">=1.0.0"
  conformance:
    suite: visualization.chart.v1
```

## 实现映射（当前落地）

| 层 | 位置 |
|---|---|
| Specification | `crates/forma/src/dsl/parser.rs`（`chart` / `dashboard` 块，无 ECharts 词汇） |
| Chart / Dashboard IR | `crates/forma/src/visualization/` |
| Adapter | `crates/forma/src/visualization/echarts.rs`（`chart_option(spec) -> serde_json::Value`，纯函数） |
| Runtime | `crates/forma/src/emit/visualization.rs` + 模板 |
| Check | `crates/forma/src/visualization/validate.rs`（`viz-*` 诊断码，`forma build` 硬失败） |
| Verify | `crates/forma/tests/visualization.rs` |

## 扩展字段（受控）

`extensions.visualization.echarts` 可携带 `animation` / `progressive` 等实现偏好；未知扩展**保留不解释**，核心语义不被覆盖。

## 待补（不影响接入）

- 正式 Manifest 文件落地（本草案经 `spec:manifests` 定稿后写入项目锁文件）。
- `conformance/visualization/` 测试套件（对齐 RFC-0004 §16）。
- 接入 PostgreSQL / Vega-Lite / 其他渲染引擎时，本能力**不应触发 Semantic Core 修改**——这是 Phase 1 完成标准之一。

## 参考

- RFC-0004：[../../rfc/RFC-0004-capability-system.zh-CN.md](../../rfc/RFC-0004-capability-system.zh-CN.md)
- RFC-0010：[../../rfc/RFC-0010-visualization.zh-CN.md](../../rfc/RFC-0010-visualization.zh-CN.md)
- Manifest 模式：[../../specifications/manifests/capability-manifest.zh-CN.md](../../specifications/manifests/capability-manifest.zh-CN.md)
- 注册表：[../registry.md](../registry.md)
