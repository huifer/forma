---
id: ADR-0005
title: 列表操作不做行级 WHERE 过滤——「取行后授权」的显式语言边界
status: Accepted
date: 2026-11-08
deciders: [forma-maintainers]
domain: security
supersedes: []
superseded_by: []
related_rfcs: [RFC-0001, RFC-0008]
related_adrs: [ADR-0004]
---

# ADR-0005：列表操作不做行级 WHERE 过滤——「取行后授权」的显式语言边界

## 1. Context（背景）

[RFC-0008 §16](../../rfc/RFC-0008-security-and-governance.zh-CN.md) 的验收项要求
「列表查询的权限条件注入 WHERE（非后置过滤）」。逐条核实（2026-11-08，Phase 1 收口）发现：

- 现行行级 `access` 条件仅作用于 `get` / `update` / `delete`——**取行 → 授权 → 校验 → 变更**；
- `list` 使用行级条件会被显式拒绝（`dsl-access-row-unsupported`），并非静默忽略；
- 因此 RFC-0008 §16 该项**当前未满足**，且其字面要求与已发布的诊断契约冲突。

## 2. Alternatives Considered（备选方案）

| 选项 | 内容 | 影响 |
|---|---|---|
| **A（采纳）** | 保持现状，把「列表不做行级 WHERE 过滤」固化为显式语言边界 | 零语义变更；与 `dsl-access-row-unsupported` 诊断、`ADR-0004` 一致；只需文档 |
| B | 立即改为 WHERE 注入：放开 list 行级条件、repo SQL 注入、空集 vs 403 语义 | 改变安全语义与错误码；需同时改检查层、生成层、Web/Flutter 契约，属 breaking change |

## 3. Decision（决策）

**采用选项 A**：把现状固化为**显式语言边界**，并写入本 ADR 与 RFC-0008 §16：

1. 行级 `access` 条件**只**约束 `get` / `update` / `delete`（取行后授权，`ADR-0004` 服务端权威）；
2. `list` / `create` 上的行级条件保持编译期**显式拒绝**（`dsl-access-row-unsupported`）；
3. 列表过滤需求由**显式机制**满足：api 级 `auth`（角色白名单）+ 列表 `filters` 配置 +
   `@tenant` 行级隔离（`col = ?` 注入 WHERE，由服务端绑定当前租户，非用户输入）；
4. 若未来确需「任意行级谓词注入列表 WHERE」，必须以 **breaking change** 推进：
   放开 list 行级条件、明确空集 vs 403 的语义边界、同步检查层 / 生成层 / Web / Flutter 契约，
   并走 [spec:compatibility](../../specifications/compatibility/compatibility-rules.zh-CN.md)
   的弃用窗口（≥ 2 minor 且 ≥ 6 个月）。

**不采用选项 B（立即改为 WHERE 注入）**：它会让「列表返回空集」与「403 拒绝」的边界模糊
（权限谓词为假时，无法区分「无数据」与「越权」），且推翻现有显式诊断与已发布行为。

## 4. Consequences（影响）

- RFC-0008 §16「列表权限条件注入 WHERE」**由本 ADR 取代**为上述显式边界（不再是缺口，而是已决策）；
- 需要行级列表过滤的用户：用 `@tenant`（服务端绑定）或 `filters`（显式声明字段），
  两者都是编译期可检查的，不依赖任意表达式；
- 治理缺口台账中 G-8 关闭（决议），避免被当作「实现未完成」反复登记。

## 5. References

- [RFC-0008 §16](../../rfc/RFC-0008-security-and-governance.zh-CN.md)
- [ADR-0004](./ADR-0004-server-authoritative-authorization.zh-CN.md)
- [spec:compatibility](../../specifications/compatibility/compatibility-rules.zh-CN.md)

## 6. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-08 | 初版采纳：列表操作行级权限边界固化为显式语言边界（选项 A）；RFC-0008 §16 对应验收项由本 ADR 取代 |
