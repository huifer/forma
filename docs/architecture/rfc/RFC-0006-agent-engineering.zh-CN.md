---
id: RFC-0006
title: Agent Engineering Protocol
status: Implemented
version: 1.0
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-04
domain: agent
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0003]
related_adrs: [ADR-0001, ADR-0002]
related_specs: [spec:ir, spec:protocols]
breaking_change: false
---

> **Implementation Status（Implemented，2026-11-08）**：实现落地：agent.rs 四工具（search/get_context/plan/report）+ 事务式补丁（快照→编辑→check→重建）+ patchId 幂等 + expected 冲突检测 + 审批门 + 风险评估同源。证据：tests/agent_tools.rs / tests/dsl_patch.rs。

# RFC-0006：Agent 工程协议（Agent Engineering Protocol）

## 1. Metadata

见 frontmatter。本 RFC 规定 Agent 与 Forma 交互的**协议层**：工具面、按任务而非按命令组织的 API、补丁事务语义、影响面与风险报告、以及人工治理边界。

## 2. Abstract

Forma 的核心承诺是「AI 只面对 DSL」。但当前实现中这条承诺**只完成了一半**：DSL 工程的 Agent 只能「读 `.fm` → 改文件 → `forma_check` → `forma_build`」，**没有事务式补丁、没有变更影响面、没有冲突检测**——而这三项恰恰是联邦侧 `patch.rs` 已经具备、却被 `reject_dsl_only_tool` 对 DSL 工程显式拒绝的能力。

本 RFC 规定：把 Agent 工具面按**任务**组织（而非按 CLI 命令镜像），为 DSL 工程提供事务式补丁 + 影响闭包 + 风险分级 + 证据报告，并明确「哪些操作必须人工审批」。

## 3. Motivation

### 3.1 Current Problem

**MCP 工具面现状**（`crates/forma/src/mcp.rs`，共 10 个工具）：

| 工具 | DSL 工程 | 联邦工程 | 说明 |
|---|---|---|---|
| `forma_check` | ✅ | — | 语法 + 语义检查 |
| `forma_build` | ✅ | — | 编译产物 |
| `forma_migrate` | ✅ | — | 迁移链状态 |
| `forma_graph` | ✅ | ✅ | DSL 侧为**模型摘要**（非依赖图） |
| `forma_viz_catalog` / `_describe` / `_validate` | ✅ | — | 可视化能力目录与校验 |
| `forma_validate` | — | ✅ | 跨标准校验 + doctor |
| `forma_impact` | ❌ **拒绝** | ✅ | 影响闭包 |
| `forma_patch` | ❌ **拒绝** | ✅ | 事务式补丁 |

**三个实质缺口**：

1. **DSL 工程没有影响闭包与补丁**。`forma_impact` / `forma_patch` 对 DSL 工程直接 `bail!`（`crates/forma/src/mcp.rs` 的 `forma_impact` / `forma_patch` 分支），并提示「请直接编辑 `.fm` 后调 `forma_build`」。这让 Agent 无法回答「改这个字段会影响哪些产物？」——而这正是多轮次演进的关键问题。
2. **补丁能力已经存在但不通用**。`crates/forma/src/patch.rs` 已实现事务式补丁：快照 → 应用 → 失败回滚（`crates/forma/src/patch.rs` 的 `rollback` 闭包）、返回 `PatchReport`、失败时产出 `patch-rollback` 诊断。但它只支持 3 个操作（`add-entity-field` / `rename-entity-field` / `set-entity-field-required`），且只作用于联邦工程。
3. **工具面按 CLI 命令镜像，而非按任务组织**。`docs/agent-workflow.zh-CN.md` §5 已指出这一点并列出 15 个任务型工具（`forma_search` / `forma_get_context` / `forma_plan` / `forma_report` / `forma_explain` / `forma_adopt` 等），其中 6 个仍为「🔨 规划」。

### 3.2 Why Existing Architecture Is Insufficient

`patch.rs` 的补丁模型是**联邦中心**的：它定位的是联邦语义图中的 URI 节点，改的是标准文件。DSL 主线的语义单元不同（`.fm` 的声明块），无法直接套用同一套 patch 操作。这正是 RFC-0003 要求先建立 Stable Identity 的原因——**没有稳定标识，就没有可寻址的补丁目标**。

### 3.3 Expected Outcomes

- Agent 能在不读取目标语言源码的前提下完成完整变更闭环：定位 → 设计 → 补丁 → 校验 → 影响分析 → 风险 → 编译 → 证据。
- 所有 DSL 变更走**同一条**事务式补丁路径，而非裸编辑文件。
- 高风险操作（破坏性迁移、权限变更、生产部署）强制人工审批。

## 4. Goals

- 定义任务型 Agent 工具面（取代命令镜像式暴露）。
- 定义补丁事务语义：预览 → 校验 → 影响分析 → 授权 → 原子应用 → 验证 → 报告。
- 定义影响面与风险分级在 Agent 协议中的表达。
- 定义人工治理边界与审批门。

## 5. Non-goals

- 不定义 IR 语义（见 RFC-0003）。
- 不定义 Capability 扩展机制（见 RFC-0004）。
- 不实现多 Agent 协作与消息传递协议。
- 不定义 Studio 的图形化补丁评审（属 RFC-0009）。

## 6. Terminology

- **Agent**：以机器可读接口操作 Forma 的外部主体（模型或工具）。
- **Task-Oriented Tool**：按「任务」而非「CLI 命令」组织的工具（如 `forma_impact` 而非 `forma_graph --impact`）。
- **Patch Transaction**：带预览、影响分析、原子应用与回滚的变更序列。
- **Impact Closure**：某语义节点变更后受影响节点的传递闭包。
- **Evidence**：一次变更的机器可读证据（诊断、产物、验证结论）。

## 7. Architectural Context

### 7.1 System Context

```text
        Agent（模型 / 工具）
             │  MCP（JSON-RPC over stdio）
             ▼
    ┌────────────────────────┐
    │  Task-Oriented Tools   │  ← 本 RFC
    │  inspect/search/context│
    │  propose/plan/impact   │
    │  patch/verify/report   │
    └───────────┬────────────┘
                │ 只读 IR / 事务写规格
                ▼
        ┌───────────────┐        ┌──────────────┐
        │ Semantic Core │        │  CLI / Studio │
        │  (RFC-0003)   │        │  同一内核     │
        └───────────────┘        └──────────────┘
```

### 7.2 Dependencies

- 依赖 RFC-0003：影响闭包与可寻址补丁目标建立在 Stable Identity 与依赖图之上。
- 依赖 RFC-0001：工具面不绕过 Semantic Core，不直接写生成物。

### 7.3 Boundaries

- Agent **不得**直接写生成物（`generated/**`）——那是 emitter 的产物。
- Agent **不得**绕过补丁事务直接编辑 `.fm`（`forma init` 脚手架与人工编辑除外）。
- Agent **不得**触发高风险操作而无需人工审批（见 §8.6）。
- Agent 工具**只读** Semantic Core，不得扩展或修改 IR 结构。

## 8. Design

### 8.1 Conceptual Model

Agent 工作流是**收敛的**闭环，而非发散的对话：

```text
  ① 取上下文  →  ② 定位语义节点  →  ③ 设计变更
        ↑                                    ↓
  ⑦ 复核证据  ←  ⑥ 验证  ←  ⑤ 应用  ←  ④ 校验 + 影响 + 风险
        └──────────────── 人工审批门（高风险时）────────────────┘
```

每一步都有对应的机器可读工具，Agent 无需读取目标语言源码即可完成全流程。

### 8.2 Architecture

**任务型工具面**（在现有 10 个工具之上扩展）：

| 工具 | 任务 | 现状 | 本 RFC 目标 |
|---|---|---|---|
| `forma_inspect` | 项目概览 + 规格摘要 | `forma_graph`（DSL 侧） | 保留 `forma_graph`，补 `forma_inspect` 语义 |
| `forma_search` | 搜索实体/字段/API/规则/页面 | 🔨 规划 | 新增：跨声明块的模式搜索 |
| `forma_get_context` | 取指定任务所需的最小上下文 | 🔨 规划 | 新增：按节点收敛上下文子图 |
| `forma_check` | 语法 + 语义校验 | ✅ | 保持 |
| `forma_impact` | 变更影响闭包 | 联邦侧 ✅ / DSL 侧 ❌ | **DSL 侧补齐**（依赖 RFC-0003 M3） |
| `forma_plan` | 生成执行计划 | 🔨 规划 | 新增：受影响产物清单 + 迁移操作清单 + 风险等级 |
| `forma_patch` | 事务式语义补丁 | 联邦侧 ✅ / DSL 侧 ❌ | **DSL 侧补齐**，并扩展操作集 |
| `forma_build` | 编译产物 | ✅ | 保持 |
| `forma_verify` | 验证编译与行为 | 部分（`forma_migrate verify`） | 新增统一入口：L1–L5 分级结论 |
| `forma_report` | 完整审查报告 | 🔨 规划 | 新增：变更集 + 影响 + 风险 + 证据汇总 |
| `forma_explain` | 解释规格与产物的对应关系 | 🔨 规划 | 新增（可选） |
| `forma_adopt` | 接管人工修改的产物 | 🔨 规划 | 依赖 RFC-0007 的 Artifact 归属模型 |

> **命名约定**：沿用现有 `forma_<verb>` 下划线风格（现有 10 个工具均为此风格），不引入 `forma.project.inspect` 这类点号命名——点号命名会与既有 MCP 工具面并存两套体系。

### 8.3 Data Model

**补丁操作集**（在联邦侧 3 个操作基础上扩展）：

```rust
pub enum PatchOperation {
    // ── 既有（联邦侧）──
    AddEntityField { entity: String, field: PatchFieldSpec },
    RenameEntityField { entity: String, from: String, to: String },
    SetEntityFieldRequired { entity: String, field: String, required: bool },

    // ── 本 RFC 新增（DSL 主线）──
    AddEntity { entity: String, fields: Vec<PatchFieldSpec> },
    RemoveEntity { entity: String },
    AddEnumVariant { enum_name: String, variant: String },
    RemoveEnumVariant { enum_name: String, variant: String },
    AddFieldRule { entity: String, field: String, rule: String },
    SetFieldType { entity: String, field: String, type_name: String },
    AddApiOperation { api: String, operation: String },
    RemoveApiOperation { api: String, operation: String },
    AddPage { page: String, route: String },
}
```

**补丁文档**（每次变更必须携带的元数据）：

```json
{
  "patchId": "patch-2026-11-04-001",
  "baseRevision": "school@0.3.0",
  "reason": "为订单增加优惠券字段并限制其不超过订单金额",
  "operations": [ ... ],
  "expected": { "entity": "Order", "fields": ["amount", "couponAmount"] }
}
```

- `patchId`：幂等键，重复提交同一 patchId 不产生二次变更。
- `baseRevision`：应用前校验规格未被人改动（冲突检测）。
- `expected`：乐观并发的前置条件——目标节点的当前状态必须匹配，否则拒绝。

**影响面**：

```json
{
  "target": "school/entity/Order/field/amount",
  "affected": [
    { "kind": "table-column", "target": "orders.amount" },
    { "kind": "rust-field",  "target": "generated/backend/src/models.rs::Order" },
    { "kind": "ts-field",    "target": "generated/web/src/types.ts::Order" },
    { "kind": "chart-series","target": "RevenueTrend" },
    { "kind": "migration",   "target": "0005_upgrade.sql" }
  ],
  "risk": "Medium"
}
```

### 8.4 Interfaces and Contracts

**补丁事务协议**（每次变更必须遵循）：

```text
Read Context        →  取目标节点的当前规格与依赖
       ↓
Prepare Patch       →  生成操作序列 + expected 前置条件
       ↓
Validate Patch      →  语法 + 语义检查（复用 check_program）
       ↓
Calculate Impact    →  影响闭包 + 受影响产物 + 迁移操作
       ↓
Assess Risk         →  破坏性分级（Low/Medium/High/Extreme）
       ↓
Approve             →  高风险需人工审批（§8.6）
       ↓
Apply Transaction   →  快照 → 原子应用 → 失败回滚
       ↓
Verify              →  重校验 + 编译 + 产物验证
       ↓
Commit / Report     →  证据报告
```

**原子性契约**：

- 应用前对**所有**待改文件做快照；任一操作失败则**全部回滚**（既有实现：`crates/forma/src/patch.rs` 的 `rollback` 闭包 + `patch-rollback` 诊断）。
- 失败后磁盘状态必须与操作前**完全一致**（既有测试 `crates/forma/tests/patch.rs` 已断言「files unchanged after failed patch」）。
- 应用成功后规格内容与 patch 的 `expected` 不符时视为冲突，**拒绝**并回滚。

**风险分级契约**：

复用 `forma migrate risk` 的既有分级（Low / Medium / High / Extreme，未知操作 fail-safe 归 High），见 [migration-safety.zh-CN.md](../../migration-safety.zh-CN.md)。补丁的风险等级取以下**最高者**：

| 风险来源 | 判定 |
|---|---|
| 迁移操作破坏性 | 沿用 `forma migrate risk` 分级 |
| 删字段 / 删实体 / 删枚举变体 | ≥ High |
| 改字段类型 | ≥ High（数据需迁移） |
| 改权限（`access`）规则 | ≥ High |
| 仅新增 / 仅改约束 | Low–Medium |
| 未知操作 | High（fail-safe） |

### 8.5 Execution Flow

一次典型多轮演进（以「给订单加优惠券」为例）：

```text
1. forma_inspect        → 确认 Order 实体、amount 字段、OrderRules 规则集
2. forma_get_context    → 收敛到 Order 相关节点 + 依赖它的图表/规则
3. forma_propose_patch  → AddEntityField(couponAmount) + AddFieldRule(max <= amount)
4. forma_check          → 规则表达式三端可编译性通过
5. forma_impact         → 受影响：orders 表列、Order 模型、OrderEdit 表单、RevenueTrend 系列
6. forma_plan           → 迁移操作：ADD COLUMN couponAmount（Low）+ 提示回填
7. 人工审批             → 风险 Low，Agent 可自行继续（High+ 必须停）
8. forma_patch          → 原子应用 + 重新校验
9. forma_build          → 重新生成受影响产物
10. forma_verify        → cargo check / tsc / migrate verify 结论
11. forma_report        → 变更集 + 影响面 + 风险 + 证据
```

### 8.6 Extension Points

- **新补丁操作**：扩展 `PatchOperation` 枚举；每个操作必须实现「幂等 + 可回滚 + 可校验」。
  当前 DSL 主线操作集（2026-11-05）：
  `add-entity-field` / `set-entity-field-required` / `add-access-rule` / `add-table-page` /
  `add-entity` / `add-field-rule` / `add-form-field` / **`set-field-rule`**（改已有 min/max 值，
  message 保留）；全部经过同一事务、审批门、幂等与 expected 冲突检测路径。
- **新工具**：新增任务型工具，不得绕过补丁事务。
- **新风险维度**：扩展风险分级表，但必须保持 fail-safe 默认（未知 → High）。
- **人工审批集成**：审批门以可插拔方式接入（CI 门、人工确认、策略引擎）。

### 8.7 人工治理边界（不可省略）

以下操作**必须**经人工审批，Agent 不得自动执行：

| 操作 | 理由 | 风险等级 |
|---|---|---|
| 破坏性数据库迁移（删列/删表/改类型） | 数据不可逆 | High / Extreme |
| 权限规则变更（`access` / `auth`） | 越权风险 | High |
| 删除实体或枚举变体 | 波及面不可预知 | High |
| 生产环境部署 / 迁移 apply | 不可回滚 | Extreme |
| 接管人工修改的产物（`forma_adopt`） | 覆盖人工逻辑 | Medium |

> 依据 [RFC-0001](RFC-0001-architecture-overview.zh-CN.md)（人工治理）与 `docs/verification.zh-CN.md` L5（变更安全）：**AI 负责表达意图，Forma 负责语义约束与风险拦截**。

## 9. Compatibility

- **Backward**：现有 10 个 MCP 工具的**名称与语义不变**；新工具为增量添加。`forma_patch` 的 DSL 侧支持为新增能力，联邦侧既有 3 个操作保持不变。
- **Forward**：未知工具名返回明确错误（当前 `call_tool` 已有 `unknown tool: {other}` 分支），不静默忽略。
- **Version Compatibility**：补丁的 `baseRevision` 绑定规格版本；跨版本的补丁应用必须先走 IR 迁移（RFC-0003 §9）。
- **Migration Compatibility**：涉及 schema 变更的补丁必须生成配对的 upgrade/rollback 迁移（既有迁移引擎契约）。
- **Deprecation Policy**：补丁操作废弃须经一个 minor 版本的 warning；`PatchOperation` 的序列化格式（`op` 标签 + kebab-case）保持稳定。

## 10. Security Considerations

- **信任边界**：Agent 的补丁是**用户意图的输入**，必须经过与手工编辑同等的校验路径——不得为 Agent 开一个「快速通道」。
- **注入面**：补丁内容最终进入 DSL 源码并被编译为代码，因此：
  - 表达式类规则必须走封闭表达式语言（[dsl-spec §9](../../dsl-spec.zh-CN.md)），禁止任意代码；
  - 字符串字段需转义后再写入规格文件；
  - 类型名、路由等标识符需白名单校验，防止注入额外声明。
- **授权**：高风险补丁必须携带审批凭据，否则 `forma_patch` 拒绝执行（§8.6）。
- **审计**：每次补丁应用记录操作集、影响面、风险等级与审批来源，供 `audit { action }` 机制落库。
- **凭据**：补丁不得携带或写入任何密钥；Secret 只能通过 Secret Provider 注入。
- **DoS**：补丁操作数与影响闭包大小须设上限，避免单次补丁触发超大重编译。

## 11. Performance and Reliability

- **上下文预算**：Agent 上下文是稀缺资源。`forma_get_context` 必须返回**收敛子图**而非全量规格；目标：典型变更的上下文 ≤ 2 个实体的声明 + 其直接依赖。
- **幂等性**：同一 `patchId` 重复提交不产生二次变更。
- **确定性**：同一 patch 两次应用产生相同的规格内容（byte 级）。
- **失败隔离**：单操作失败即整体回滚，不留半应用状态。
- **超时**：影响闭包与编译步骤须有超时上限，超时按失败处理并回滚。

## 12. Alternatives Considered

- **让 Agent 直接编辑 `.fm` 文本**（当前提示的做法）：拒绝——绕过校验、无法冲突检测、无法回滚，且文本编辑易产生语法错误。
- **把 CLI 命令直接暴露为 MCP 工具**（当前形态）：拒绝——命令镜像式 API 迫使 Agent 组合多个命令来表达一个任务，且 DSL/联邦双模式造成工具语义分裂（同一工具在两类工程行为不同）。任务型 API 消除这种分裂。
- **允许 Agent 绕过语义检查直接写规格**：拒绝——违反「错误在编译期暴露」的基本原则（RFC-0003 §8.4）。
- **引入点号命名（`forma.project.inspect`）**：拒绝——与既有 10 个工具的下划线风格冲突，且不带来额外表达力。
- **为 DSL 与联邦各做一套补丁实现**：拒绝——补丁事务语义（快照/回滚/幂等/冲突检测）应当唯一；差异只在**操作集**与**目标寻址**。

## 13. Trade-offs

- 引入补丁协议层（预览、审批、回滚）增加交互往返，换取变更安全性与可审计性。
- 任务型工具面比命令镜像面「工具更多、单个更复杂」，换取 Agent 单次调用即可完成一个任务。
- 强制高风险审批会让自动化流水线在破坏性变更处停下，换取不可逆操作的人工控制。

## 14. Migration Plan

| 步 | 内容 | 完成判据 | 状态 |
|---|---|---|---|
| **P1** | 修正 CLI/MCP 语义一致性（与 RFC-0003 M4 同批） | `forma_validate` / `forma_impact` / `forma_patch` 对 DSL 工程不再误报或误拒 | ✅ 已完成（2026-11-05，`engine::check_project`） |
| **P2** | 扩展 `PatchOperation` 至 DSL 主线操作集；复用 `patch.rs` 的快照/回滚机制 | DSL 工程可经 `forma_patch` 完成「加字段 + 加规则 + 加页面」 | ✅ 已完成（2026-11-05，`dsl::patch`：`add-entity-field` / `set-entity-field-required` / `add-access-rule` / `add-table-page`；事务 = 快照 → 编辑 → `check_program` → 重建，新引入语义错误即整体回滚） |
| **P3** | 基于 RFC-0003 M3 的依赖图实现 DSL 侧 `forma_impact` | 影响闭包含表列、模型、表单、图表、迁移五类目标 | ✅ 已完成（2026-11-05，M3 桥接 + `NodeKind` 增 `chart` / `migration`：实体/字段/表/列/枚举/API 操作/命令/页面/控件/图表/迁移链；五类目标全部入闭包，回归 `impact_closure_covers_all_five_target_kinds`） |
| **P4** | 引入 `forma_plan` / `forma_report` / `forma_search` / `forma_get_context` | 一次 Agent 会话可完成 §8.5 全流程，无需读取目标源码 | ✅ 已完成（2026-11-05，`src/agent.rs` + MCP 工具面：search=语义检索；get_context=声明块摘录+依赖/引用者+影响闭包；plan=副本试运行（零副作用）+ 迁移风险分级 + 审批判定 verdict；report=模型/校验证据/待执行迁移/产物面。回归 `tests/agent_tools.rs`（8 项，含 §8.5 全流程）） |
| **P5** | 风险审批门 + 审计落库 + 补丁幂等/冲突检测 | High/Extreme 操作在无审批凭据时被拒绝；同一 patchId 不重复应用；过期 expected 被拒绝 | ✅ 已完成（2026-11-05：`dsl::patch::assess_risk` 共享风险评估（迁移 High/Extreme + 权限规则变更 §8.7）；`forma_patch` 写入前在副本试运行评估，无审批凭据即**拒绝**（`patch-approval-required`，零写入）；补丁文档支持 `approval: { by, reason }`、`patchId`（`.forma/patches.json` 幂等台账）、`expected`（声明块 sha256 冲突检测，`patch-conflict`）；审计落 `.forma/audit.jsonl`（applied/rejected/failed/idempotent + 风险 + 审批来源），`forma_plan` 回传 expected，`forma_report` 附审计尾迹。回归 `tests/dsl_patch.rs`（18 项）+ `tests/agent_tools.rs`（11 项）） |

> P2 依赖 RFC-0003 M1（line 定位）与 M2（类型清单）；P3 依赖 RFC-0003 M3（StableId + 依赖边）。**P3 是关键路径**——没有稳定标识就没有可寻址的影响面。M1–M3 已于 2026-11-05 落地（`dsl::lower`），P2/P3 随之解锁。

## 15. Implementation Plan

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | P1 + P2（CLI 一致性 + DSL 补丁操作集） | RFC-0003 M1/M2 |
| 阶段 2 | P3（DSL 影响闭包） | RFC-0003 M3 |
| 阶段 3 | P4（搜索/上下文/计划/报告） | 阶段 2 |
| 阶段 4 | P5（审批门 + 审计） | 阶段 3 |

## 16. Conformance Requirements

- [x] DSL 工程的 `forma_patch` 成功应用并通过重校验；失败时磁盘状态与操作前完全一致（P2 已回归）。
- [x] 同一 `patchId` 重复提交不产生二次变更（幂等）（2026-11-05：`.forma/patches.json` 台账 + `idempotent: true`）。
- [x] `expected`（声明块 sha256）不匹配时拒绝并零写入（冲突检测）（2026-11-05：`patch-conflict` 错误码；`forma_plan` 回传目标实体哈希供 Agent 原样提交）。
- [x] `baseRevision`（`<project>@<IR_VERSION>`）不匹配时拒绝并零写入（`patch-base-revision`）；`forma_plan` 回传当前 revision（2026-11-05，IR 1.2.0）。
- [x] `forma_impact` 对 DSL 工程返回影响闭包，且闭包包含表列、模型、表单、图表、迁移五类目标（P3）。
- [x] 未知补丁操作被显式拒绝并给出错误码（不静默忽略；serde 操作标签 + P2 操作计划失败均有回归）。
- [x] 破坏性补丁在缺少审批凭据时被 `forma_patch` 拒绝（P5：`patch-approval-required`，零写入 + 审计留痕；带凭据可执行并落审计）。
- [x] 每个 DSL 补丁操作都有对应回归测试（`tests/dsl_patch.rs`：18 项，含 P2/P5/幂等/冲突）。

## 17. Acceptance Criteria

1. Agent 在**不读取任何生成物源码**的前提下，完成「加字段 + 加跨字段校验 + 加表单字段 + 生成迁移」的完整闭环。
2. 影响面报告准确列出受影响产物，且无关模块未被重新生成（与 RFC-0007 增量断言联测）。
3. 人为构造的冲突补丁（`expected` 不匹配）被拒绝且不落盘任何变更。
4. ✅ 破坏性补丁在无审批时无法执行；提供审批凭据后可执行并留审计记录（P5：`.forma/audit.jsonl` 记录 applied/rejected + 风险 + 审批来源）。

## 18. Open Questions

> 收口（2026-11-08）：3 项关闭、1 项保留。

- [x] ~~`expected` 前置条件是否需要表达式形式？~~
  **已决议**：限定为结构化声明（`expected: [{entity, sha256}]`），支持 `patchId` 幂等；表达式形式不做。
- [x] ~~补丁是否需要支持跨实体/跨文件的原子操作？~~
  **已支持**：`DslPatchDocument.operations` 多操作按当前磁盘状态组合应用；失败整体回滚（事务）。
- [x] ~~`forma_plan` 的输出是否应直接作为 CI 的变更评审材料？~~
  **已支持**：JSON 报告含 verdict / baseRevision / operations / migrations（风险分级）/ expected / impact / build。
- [x] ~~是否需要「补丁草案」在磁盘上落盘（供人审阅后再 apply）？~~
  **已落地（2026-11-08）**：`forma patch <file> --dry-run --out <report.json>`——
  工程副本上预览（源工程零写入），报告落盘供人工审阅；端到端验证通过。


## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览与人工治理：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 语义内核（Stable Identity / 依赖图）：[RFC-0003-semantic-core.zh-CN.md](RFC-0003-semantic-core.zh-CN.md)
- Agent 工作流（任务型工具清单）：[agent-workflow.zh-CN.md](../../agent-workflow.zh-CN.md)
- 验证体系 L1–L5：[verification.zh-CN.md](../../verification.zh-CN.md)
- 迁移安全与风险分级：[migration-safety.zh-CN.md](../../migration-safety.zh-CN.md)
- 实现位置：`crates/forma/src/patch.rs`、`crates/forma/src/mcp.rs`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义任务型 Agent 工具面、补丁事务协议、风险分级与人工审批门 |
