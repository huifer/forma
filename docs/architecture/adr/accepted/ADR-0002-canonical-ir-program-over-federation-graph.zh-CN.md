---
id: ADR-0002
title: DSL 主线 Program 为规范语义表示，联邦语义图为导入视图
status: Accepted
date: 2026-11-04
deciders: [forma-maintainers]
domain: semantic-core
supersedes: []
superseded_by: []
related_rfcs: [RFC-0001, RFC-0002, RFC-0003]
related_adrs: [ADR-0001]
---

# ADR-0002：DSL 主线 `Program` 为规范语义表示，联邦语义图为导入视图

## 1. Context（背景）

仓库长期并存两套 IR：

- **v0.2 联邦引擎** `UnifiedSemanticGraph`（`crates/forma/src/graph.rs`）：带 `forma://` URI 稳定标识、`NodeKind`/`EdgeKind` 依赖边、`SourceRef` 来源追踪；被 5 个 importer、`engine.rs`、`validate.rs`、`patch.rs` 消费。
- **v0.3 DSL 主线** `Program`（`crates/forma/src/dsl/parser.rs`）：类型化声明树，被全部 9 个 emitter 消费；**无稳定标识、无依赖边、来源追踪仅覆盖 `FieldDecl.line`/`ChartDecl.line`、无版本**。

二者之间**没有 Lowering**。后果是 DSL 工程（v0.3 主线，即当前唯一主线）永远拿不到联邦侧已有的能力：影响闭包、事务式补丁、跨版本兼容判定。`forma_impact` / `forma_patch` 因此对 DSL 工程显式拒绝（`crates/forma/src/mcp.rs` 的 `reject_dsl_only_tool`）。

本 ADR 须裁决：**哪一套是规范语义表示，二者如何长期共存**。

## 2. Decision（决策）

**以 DSL 主线 `Program` 为规范语义表示（canonical representation）；联邦 `UnifiedSemanticGraph` 降级为导入视图（import view）。**

具体：

1. 新增的语义能力（Stable Identity、依赖边、IR 版本、来源追踪）**只加在 `Program` 侧**，并复用联邦侧已有的 `SourceRef` 结构与 `NodeKind`/`EdgeKind` 枚举——**不新建平行枚举**。
2. 联邦 importer 产出 `Program`（而非直接产出联邦图），联邦图仅在 importer 内部作为中间结构存在。
3. DSL 主线的 emitter 契约（`Program` → 产物）保持不变；Lowering 的引入是**渐进**的（RFC-0003 §14 的 M1–M4），不要求一次性插入新环节。

## 3. Consequences（后果）

- **正面**：DSL 工程获得影响闭包、事务补丁、版本治理能力（RFC-0006 的关键路径打通）；语义能力只有一处实现；RFC-0001 §8.3 的「Stable Identity 由 IR 节点承载」承诺得到兑现。
- **负面**：短期需维护 `Program` 与联邦图之间的桥接；联邦 importer 需改造（从「产出联邦图」改为「产出 `Program`」），工作量集中在 `adapters/*` 与 `patch.rs`。
- **中性**：既有 emitter 无需改动（`Program` 既有字段语义不变）；联邦工程继续按原路径工作。

## 4. Alternatives Considered（被否决的方案）

- **废弃联邦 `UnifiedSemanticGraph`，重写一套新 IR**：拒绝——5 个 importer 与 `patch.rs` 依赖它，重写会同时打断导入与补丁链路，代价远大于收益。
- **保持两套 IR 并列，各管一摊**：拒绝——DSL 工程将永久缺失影响分析与补丁能力，Agent 工作流只能停留在「改文件—重编译」，与「AI 只面对 DSL」的承诺不符。
- **为 DSL 侧另建平行 `IrNodeKind` / 独立标识体系**：拒绝——两套标识使 `forma_impact`、增量构建、产物追踪都需双份映射，违背「规范表示只有一个」。
- **把联邦图作为规范表示，反向把 DSL 降级为它的前端**：拒绝——DSL 主线是 v0.3 已确立的方向（`architecture.zh-CN.md` §2.1），且 DSL 的类型化声明树更贴近语义（联邦图是标准形状的泛化，丢失 DSL 的类型信息）。

## 5. References（引用）

- 语义内核：[RFC-0003-semantic-core.zh-CN.md](../../rfc/RFC-0003-semantic-core.zh-CN.md)
- 架构总览：[RFC-0001-architecture-overview.zh-CN.md](../../rfc/RFC-0001-architecture-overview.zh-CN.md)
- 规格系统：[RFC-0002-specification-system.zh-CN.md](../../rfc/RFC-0002-specification-system.zh-CN.md)
- 治理总入口：[README.md](../../README.md)
- 实现：`crates/forma/src/dsl/parser.rs`、`crates/forma/src/graph.rs`、`crates/forma/src/identity.rs`

## 6. Change Log（变更记录）

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版采纳：裁决 `Program` 为规范语义表示，联邦图降级为导入视图 |
