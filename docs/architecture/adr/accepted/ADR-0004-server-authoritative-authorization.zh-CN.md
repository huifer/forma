---
id: ADR-0004
title: 权限服务端权威执行，前端显隐不构成安全边界
status: Accepted
date: 2026-11-04
deciders: [forma-maintainers]
domain: security
supersedes: []
superseded_by: []
related_rfcs: [RFC-0001, RFC-0002, RFC-0008]
related_adrs: [ADR-0001]
---

# ADR-0004：权限服务端权威执行，前端显隐不构成安全边界

## 1. Context（背景）

DSL 规格中已存在两处权限声明语法：

- `rules { access Order { delete → role == "admin" } }`（RBAC）
- `api Orders { auth { create update delete → role in ["admin","ops"] } }`（操作级鉴权）

源码核对结论：`auth` 块**在 parser 中被识别**（`crates/forma/src/dsl/parser.rs`），但**检查层与全部 emitter 均不消费它**；`rules` 的 `access` 子句在 parser 中亦无对应处理。即两处权限语义**目前是空承诺**。

这构成一个方向明确的缺口：规格读起来像做了权限控制，而编译产物**不含**任何权限判定。生成式工作流会因此把「写了 auth 块」误当作「已有权限控制」——若不显式化，AI 生成的系统会在用户以为受保护时实际无保护。

同时，仓库既有立场已明确「规则一处声明，两端执行」（`architecture.zh-CN.md` §1 约束 2），但权限的**安全边界**未被单独表述。

## 2. Decision（决策）

**权限在服务端权威执行；前端显隐仅为体验优化，不构成安全边界。**

具体：

1. **权威判定点在服务端**：所有权限判定（认证、资源级、行级、字段级）在服务端执行并强制。前端产物的权限表达仅用于体验（隐藏按钮、禁用态、表单预校验），其缺失或错误**不得**导致越权。
2. **未实现的权限构造必须显式报错**：在 `access` / `auth` 块被完整实现之前，Check 阶段检测到它们非空时产出 `dsl-unsupported-policy`（error，阻断构建），错误消息指向 auth 能力的路线图位置。**不得静默忽略**（设计宪法 G5，[roadmap.zh-CN.md §6.2](../../../roadmap.zh-CN.md)）。
3. **Fail-closed**：策略求值失败（表达式错误、上下文缺失、Provider 不可用）一律拒绝，不得放行。
4. **行级权限在查询层注入**：作为 WHERE 条件注入，而非查询后过滤（后者泄露存在性与分页错误）。
5. **不泄露资源存在性**：越权访问不存在的资源与无权限的资源返回相同响应。
6. **最小权限默认**：策略未覆盖的操作默认拒绝。

## 3. Consequences（后果）

- **正面**：规格中的权限声明与实际执行一致；AI 生成的系统不会因「以为有权限控制」而留下越权路径；显式报错把安全缺口从「静默空承诺」变为「可见的待办」。
- **负面**：C1 步骤会让当前写有 `auth` / `access` 块的规格**暂时无法构建**——这是期望行为（暴露真实缺口），但须在 CHANGELOG 标注；权限在服务端与前端双端编译增加生成物复杂度。
- **中性**：前端按钮隐藏逻辑保留（体验），但其地位被明确降级为「非安全边界」。

## 4. Alternatives Considered（被否决的方案）

- **仅前端隐藏**：拒绝——最常见的越权来源，前端可被绕过（改请求、curl、直接调 API）。
- **权限用独立配置文件表达**：拒绝——权限是业务语义，与实体/操作强相关，分离会产生第二份事实来源，与 ADR-0001 的治理立场冲突。
- **把权限交给外部策略引擎（OPA / Cedar / OpenFGA）**：不拒绝，但**推迟**——先把语义在本仓库定稿并实现服务端权威执行；引擎作为 Capability 接入（RFC-0004 §8.6），届时策略声明仍是同一份 DSL。
- **权限失败时放行（fail-open）**：拒绝——安全默认必须是拒绝。
- **让 Agent 自动批准权限变更**：拒绝——见 RFC-0006 §8.6，高风险变更必须人工审批。
- **保持静默忽略（现状）**：拒绝——「写了 auth 但没实现」比「明确报错」危险得多。

## 5. References（引用）

- 安全与治理：[RFC-0008-security-and-governance.zh-CN.md](../../rfc/RFC-0008-security-and-governance.zh-CN.md)
- 规格系统（领域划分）：[RFC-0002-specification-system.zh-CN.md](../../rfc/RFC-0002-specification-system.zh-CN.md)
- 架构总览：[RFC-0001-architecture-overview.zh-CN.md](../../rfc/RFC-0001-architecture-overview.zh-CN.md)
- 治理总入口：[README.md](../../README.md)
- 实现：`crates/forma/src/dsl/parser.rs`、`crates/forma/src/emit/rust_backend.rs`、`crates/forma/src/emit/web_react.rs`

## 6. Change Log（变更记录）

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版采纳：裁决权限服务端权威执行、前端显隐非安全边界、未实现权限构造显式报错 |
