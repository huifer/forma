---
id: ADR-0001
title: Adopt RFC Governance & Extensibility Architecture v1.0
status: Accepted
date: 2026-11-04
deciders: [forma-maintainers]
domain: governance
supersedes: []
superseded_by: []
related_rfcs: [RFC-0001, RFC-0004]
related_adrs: []
---

# ADR-0001：采纳 RFC 治理与可扩展架构 v1.0

## 1. Context（背景）

Forma 已从 v0.2「标准联邦」演进为 v0.3「DSL 中间层」，并落地了可视化子系统（原 `docs/rfc/0001-visualization.zh-CN.md`）、
版本化迁移引擎、迁移风险分级、确定性案例数据等能力。随着功能增加，架构知识分散在 `architecture.zh-CN.md`、
各 spec 与历史文档中，缺少：

- 强制的 RFC 生命周期与稳定性分层；
- 功能扩展的统一 Capability 契约；
- 决策历史的不可丢失记录（ADR）；
- 兼容性保证的 Conformance 层。

项目越大，单一巨型 RFC 越难维护，每次加功能都可能牵动核心架构。

## 2. Decision（决策）

采纳 **Forma RFC Governance & Extensibility Architecture v1.0**，将架构知识拆分为五层并分离职责：

- **RFC**（边界/契约）、**Specification**（语义）、**ADR**（决策）、**Capability**（功能）、**Conformance**（兼容）；
- 建立 `docs/architecture/` 目录体系与 `rfc/TEMPLATE.md`、`adr/TEMPLATE.md`；
- 引入 7 态生命周期、`Accepted ≠ Implemented ≠ Stable` 区分、Tier 1/2/3 稳定性分层；
- 将既有可视化 RFC 迁入新体系，重编号为 **RFC-0010**，新增 **RFC-0001**（架构总览）与 **RFC-0004**（能力系统）。

## 3. Consequences（后果）

- 正面：新增能力不必改核心；新增 Adapter 不必改 IR；决策历史可追；兼容性可验证。
- 负面：引入 frontmatter / 审查清单 / 分层的治理成本。
- 中性：既有 `architecture.zh-CN.md` 保留为「详细蓝图伴侣」，不与 RFC-0001 重复。

## 4. Alternatives Considered（被否决的方案）

- **维持单一架构文档**：拒绝——无法随功能增长而扩容。
- **纯代码即文档**：拒绝——AI 无法从代码反推边界与决策历史。

## 5. References（引用）

- 治理总入口：[docs/architecture/README.md](../../README.md)
- RFC-0001：[rfc/RFC-0001-architecture-overview.zh-CN.md](../../rfc/RFC-0001-architecture-overview.zh-CN.md)
- RFC-0004：[rfc/RFC-0004-capability-system.zh-CN.md](../../rfc/RFC-0004-capability-system.zh-CN.md)
- 原可视化 RFC（迁入后）：[rfc/RFC-0010-visualization.zh-CN.md](../../rfc/RFC-0010-visualization.zh-CN.md)

## 6. Change Log（变更记录）

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版采纳治理与可扩展架构体系 |
