---
id: RFC-0003
title: Semantic Core Architecture
status: Implemented
version: 1.1
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-08
domain: semantic-core
supersedes: []
superseded_by: []
depends_on: [RFC-0001]
related_adrs: [ADR-0001, ADR-0002]
related_specs: [spec:ir, spec:types]
breaking_change: false
---

> **Implementation Status（Implemented，2026-11-08）**：实现落地：IR_VERSION 1.2.0 + Program.ir_version；BUILTIN_TYPES 12 型四目标映射回归；Stable Identity（identity.rs 同构构造）；DSL→图单向桥接（dsl::lower）；声明级 file/line。证据：tests/dsl_ir_bridge.rs / tests/dsl_validate.rs（CLI↔MCP 漂移防线）。

# RFC-0003：语义内核架构（Semantic Core）

## 1. Metadata

见 frontmatter。本 RFC 定义 Forma 的**语义内核**：IR 的稳定契约、类型系统、Stable Identity、依赖图与语义检查。本 RFC 是 Tier 1 契约，修改须经正式 RFC + 迁移设计 + 兼容验证（[RFC-0001 §5](RFC-0001-architecture-overview.zh-CN.md)）。

> 本 RFC 被 [RFC-0001](RFC-0001-architecture-overview.zh-CN.md) 与 [RFC-0004](RFC-0004-capability-system.zh-CN.md) 以 `depends_on` 引用，此前为悬空引用；本文件补齐该契约。

## 2. Abstract

Forma 当前存在**两套互不相通的 IR**：v0.2 联邦引擎的 `UnifiedSemanticGraph`（`crates/forma/src/graph.rs`，带 `forma://` URI 稳定标识与 `NodeKind`/`EdgeKind` 边），与 v0.3 DSL 主线的 `Program`（`crates/forma/src/dsl/parser.rs`，类型化声明树，无稳定标识、无边、无版本）。二者之间**没有 Lowering**，语义能力因此割裂：DSL 工程没有依赖图、没有影响闭包、没有跨版本兼容策略。

本 RFC 规定：**以 DSL 主线 `Program` 为语义内核的唯一实现目标**，定义其稳定契约（Stable Identity、来源追踪、IR 版本）、类型系统与语义检查规则，并给出向统一语义图演进的**兼容路径**——不是推倒重来，而是在既有实现上补齐契约。

## 3. Motivation

### 3.1 Current Problem

仓库当前的实际状态（经源码核对）：

| 维度 | v0.3 DSL 主线（`Program`） | v0.2 联邦引擎（`UnifiedSemanticGraph`） |
|---|---|---|
| 载体 | `crates/forma/src/dsl/parser.rs` | `crates/forma/src/graph.rs` |
| 稳定标识 | ❌ 只有声明名（`String`） | ✅ `forma://project/{p}/entity/{e}` URI |
| 依赖边 | ❌ 无 | ✅ `EdgeKind`（5 类） |
| 来源追踪 | ⚠️ 仅 `FieldDecl.line` / `ChartDecl.line` | ✅ `SourceRef { file, line }` |
| IR 版本 | ❌ 无版本字段 | ❌ 无版本字段 |
| 语义检查 | ✅ `check_program`（401 行，16 类 `dsl-*` 码） | ✅ `run_doctor` |
| 消费方 | 全部 9 个 emitter | 联邦 importer、patch |

由此产生的**实际缺口**（非推测，均可复现）：

1. **DSL 工程没有影响闭包**。`forma_impact` / `forma_patch` 被 `reject_dsl_only_tool` 显式拒绝（`crates/forma/src/mcp.rs` 的 `reject_dsl_only_tool`），DSL 工程的 Agent 只能「改 `.fm` → `forma_check` → `forma_build`」，无事务式补丁、无变更影响面。
2. **CLI 与 MCP 语义不一致**。`forma graph` / `forma validate` / `forma doctor` 三个 CLI 子命令走联邦 `engine::load_project`，对 DSL 工程返回**空图 + 假绿**——`forma validate examples/school` 报「✓ valid — 0 nodes, 0 diagnostics」，而同一工程 `forma check` 报「7 entities」。对含真实语义错误的 DSL 工程，`forma validate` 同样报「✓ valid」（已实测复现）。
3. **`Json` 类型在检查层与生成层不一致**。`BUILTIN_TYPES`（`crates/forma/src/dsl/mod.rs`）只有 10 个类型，不含 `Json`；但 `crates/forma/src/emit/mysql.rs` 与 `crates/forma/src/emit/rust_backend.rs` 都为 `Json` 准备了映射。实际结果：`field meta Json` 被 `dsl-unknown-type` 拒绝（已实测复现）。
4. **IR 无版本**，无法回答「这份 IR 是哪个契约版本产的」。

### 3.2 Why Existing Architecture Is Insufficient

`docs/architecture.zh-CN.md` §1 声明 IR 层为「语义图（节点/边/影响闭包）」，但那是**联邦引擎的**语义图。DSL 主线从未获得同等语义能力。RFC-0001 §8.3 承诺「Stable Identity 由 IR 节点承载（`NodeKind`/`EdgeKind`）」，而 DSL 主线的 IR 节点既无 `NodeKind` 也无 `EdgeKind`——承诺与实现之间存在缺口。

### 3.3 Expected Outcomes

- DSL 工程获得稳定标识、来源追踪与依赖边，从而支持影响闭包与事务式补丁。
- IR 具备版本化契约与跨版本兼容判定。
- CLI 与 MCP 对同一工程给出一致的语义结论（消除假绿）。
- 类型系统在内建类型、检查层与生成层三者间无歧义。

## 4. Goals

- 定义 DSL 主线 IR 的稳定契约：Stable Identity、Source Ref、IR Version、扩展字段。
- 定义类型系统与跨目标映射规则（MySQL / Rust / TypeScript / Dart）。
- 定义语义检查的职责边界与诊断码稳定性。
- 规定向统一语义图演进的兼容路径（不破坏既有 emitter 契约）。

## 5. Non-goals

- 不定义**具体某个能力**的 IR（可视化见 RFC-0010）。
- 不定义 Capability Manifest 字段（见 RFC-0004）。
- 不定义 DSL **语法**（见 [dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md)）——本 RFC 只定义语法降级后的**语义契约**。
- 不实现 Agent 补丁协议（见 RFC-0006）。
- 不改动 emitter 的产物格式（见 RFC-0007）。

## 6. Terminology

- **Semantic Core**：Tier 1 的语义内核，含 IR 契约、类型系统、Stable Identity、依赖图、语义检查。
- **Stable Identity**：跨重新编译、跨版本保持不变的节点标识。
- **Lowering**：把 DSL 声明树降级为语义内核节点的**单向**转换。
- **IR Version**：语义内核契约的版本，与 Specification Version（DSL 语法版本）独立。
- **Source Ref**：节点到规格源位置（file/line）的引用。

## 7. Architectural Context

### 7.1 System Context

```text
            DSL 规格 (.fm)                 联邦标准文件
                 │                              │
                 ▼                              ▼
          ┌─────────────┐                ┌─────────────┐
          │  Parser     │                │ SpecAdapter │
          └──────┬──────┘                └──────┬──────┘
                 │  Lowering                    │
                 ▼                              │
        ┌──────────────────────┐               │
        │   Semantic Core      │◀──────────────┘
        │  ┌────────────────┐  │
        │  │ Stable Identity│  │
        │  │ Type System    │  │
        │  │ Source Ref     │  │
        │  │ Dependency     │  │
        │  │ IR Version     │  │
        │  └────────────────┘  │
        └──────────┬───────────┘
                   │  只读投影
      ┌────────────┼────────────┬─────────────┐
      ▼            ▼            ▼             ▼
  gen-mysql   gen-rust     gen-web      gen-flutter …
```

### 7.2 Dependencies

- 依赖 RFC-0001（边界与 Tier 红线）。
- 被 RFC-0004（Capability 系统）依赖：Capability 消费 IR，IR 不得被 Capability 修改。
- 被 RFC-0006（Agent 工程）依赖：影响闭包建立在依赖图之上。

### 7.3 Boundaries

**依赖方向红线**：

- Semantic Core **不依赖**任何 Adapter / Runtime / Capability。
- Adapter **只读** IR，不得回写 IR（RFC-0001 §7.3）。
- Capability **不得修改** Semantic Core（RFC-0004 §7.3）。
- 联邦 importer（`adapters/*`）**降级到** Semantic Core，**不反向**要求核心迁就联邦图结构。

> **关键决策**：联邦 `UnifiedSemanticGraph` 是**导入视图**（v0.2 遗产），Semantic Core 是**规范表示**（v0.3 主线）。二者长期并存，但规范表示只有一个。

## 8. Design

### 8.1 Conceptual Model

语义内核由五个正交切面组成：

| 切面 | 回答的问题 | 当前状态 |
|---|---|---|
| Stable Identity | 「这是哪个业务对象？」 | 仅联邦侧有（`forma://` URI） |
| Type System | 「它是什么类型？」 | 内建 10 型 + 枚举 + 关系，映射到 4 目标 |
| Source Ref | 「它来自规格的哪一行？」 | 仅 `FieldDecl.line` / `ChartDecl.line` |
| Dependency | 「它依赖/被依赖什么？」 | 仅联邦侧有边 |
| IR Version | 「它符合哪个契约版本？」 | 缺失 |

五者缺一，IR 就无法支撑影响分析、溯源与版本治理。

### 8.2 Architecture

**目标形态**（Tier 1 契约）：

```rust
/// 语义内核节点契约。稳定标识与源码位置解耦：
/// 改名不改变 Identity；移动代码不改变 Source Ref。
pub struct IrNode<T> {
    pub id: StableId,          // 跨版本稳定
    pub kind: NodeKind,         // 语义种类
    pub source: Vec<SourceRef>, // 来源追踪（可多源：DSL + 导入视图）
    pub value: T,               // 类型化载荷
    pub extensions: ExtensionMap, // 目标专有信息，命名空间隔离
}

/// 跨版本稳定的标识。路径式，便于人工阅读与调试。
pub struct StableId(String);   // "project/school/entity/Student/field/email"

pub struct SourceRef {
    pub file: String,
    pub line: u32,
    pub column: Option<u32>,
}
```

**与既有实现的对齐关系**：

| 契约要素 | 联邦侧已有 | DSL 侧需补 | 复用方式 |
|---|---|---|---|
| 稳定标识 | `forma://` URI | `StableId` | 统一为 `StableId`；URI 作为其字符串形式 |
| 节点种类 | `NodeKind`（11 类） | 需新增 | 扩展 `NodeKind`，**不新建平行枚举** |
| 边 | `EdgeKind`（5 类） | 需新增 | 复用 `EdgeKind`，补 DSL 侧边构造 |
| 来源 | `SourceRef{file,line}` | 仅部分 decl 有 line | 补 `EntityDecl`/`ApiDecl`/`PageDecl`/`CommandDecl` 的 `line` |
| 版本 | 无 | 无 | 新增 `IR_VERSION` 常量 + `Program.ir_version` |

> **不新建平行枚举**是硬约束：若 DSL 侧另起 `IrNodeKind`，则 `forma_impact` 等工具将需要维护两套映射，违背「规范表示只有一个」。

### 8.3 Data Model

**Stable Identity 规则**：

1. 标识由**语义路径**构成（项目 / 实体 / 字段 …），不由声明顺序或数组下标构成。
2. 改名（如 `customerEmail` → `email`）**改变** StableId（它是语义路径的一部分）；但**不改变**受影响产物的增量判定粒度——增量以 IR 节点 Hash 为准（见 RFC-0007）。
3. 标识在**同一规格版本内**唯一且可重现。

**Source Ref 规则**：

1. 节点携带其**全部**来源：DSL 声明位置 + （若有）联邦导入视图位置。
2. 诊断必须能反向定位到规格文件与行号（当前 `Diagnostic` 已支持 `file`/`line`，见 `crates/forma/src/validate.rs` 的 `Diagnostic`）。
3. 降级过程**不得丢失**来源；无法定位的节点必须显式标记为 `SourceRef::unknown()`，不得静默填充。

**类型系统**：

内建类型集必须与**检查层**与**生成层**三方一致。当前三方不一致（`Json` 只在生成层被认识），是 §3.1 所列缺口 3 的根因。契约要求：

| 规则 | 说明 |
|---|---|
| R1 单一权威清单 | 内建类型只有一个定义源，检查层与生成层从同一处派生 |
| R2 新增类型为 minor | 新增内建类型不破坏既有规格 |
| R3 移除类型为 major | 移除需 major 版本 + 迁移报告 |
| R4 目标映射显式 | 每个内建类型对每个目标有唯一映射，缺失即编译期错误 |

**跨目标类型映射**（当前实现的实际取值，契约化后不得随意变动）：

| DSL 类型 | MySQL | Rust | TypeScript | Dart |
|---|---|---|---|---|
| `Uuid` | `CHAR(36)` | `String` | `string` | `String` |
| `String(n)` / `Text` / `Email` | `VARCHAR(n)` / `TEXT` | `String` | `string` | `String` |
| `Int` | `BIGINT` | `i64` | `number` | `int` |
| `Float` | `DOUBLE` | `f64` | `number` | `double` |
| `Decimal(p,s)` | `DECIMAL(p,s)` | `Decimal` | `string` | `String` |
| `Bool` | `TINYINT(1)` | `bool` | `boolean` | `bool` |
| `Timestamp` | `DATETIME(3)` | `DateTime<Utc>` | `string` | `DateTime` |
| `Date` | `DATE` | `NaiveDate` | `string` | `DateTime` |

> **Decimal 一律不降级为浮点**：MySQL 保持 `DECIMAL(p,s)`，Rust 用 `Decimal`，TS/Dart 用 `string` 序列化（避免 IEEE754 精度丢失）。这是 RFC-0001 §7.3「同一语义多种投影」在类型层的具体化。

### 8.4 Interfaces and Contracts

**Lowering 契约**（DSL → Semantic Core）：

```rust
pub trait Lower {
    type Output;
    fn lower(&self, program: &Program) -> Result<Self::Output, Vec<Diagnostic>>;
}
```

- **单向**：Lowering 不可逆地丢弃语法细节（关键字、大括号、顺序），但**保留**语义与来源。
- **确定性**：同一 `Program` 两次 Lowering 结果 byte 级一致（与 RFC-0001 §11 一致）。
- **幂等**：Lowering 结果再 Lowering（作为输入）语义等价。

**语义检查契约**：

- `check_program` 是 DSL 主线的**唯一**语义检查入口（当前已实现，`crates/forma/src/dsl/mod.rs` 的 `check_program`）。
- 诊断码是**稳定 API**：一旦发布，`dsl-*` 码的含义不得改变；废弃须先经一个 minor 版本 warning。
- 诊断严重度只有三档：`error`（阻断 build）/ `warning` / `info`（均不阻断）。

**CLI / MCP 一致性契约**（修复 §3.1 缺口 2）：

> 对同一工程，`forma <cmd>`（CLI）与 `forma_<tool>`（MCP）必须给出**同一语义结论**。当前 `forma validate` / `doctor` / `graph` 对 DSL 工程假绿，必须修正为：DSL 工程走 `check_program` 路径，联邦工程走 `run_doctor` 路径。

### 8.5 Execution Flow

```text
1. 解析 .fm → Program（保留 line）
2. Lowering → Semantic Core（补 StableId / SourceRef / 边 / IR 版本）
3. check_program → 语义诊断（error 阻断）
4. 构建依赖图 → 影响闭包（供 RFC-0006 使用）
5. Adapter 只读投影 → 产物
```

第 2 步是本 RFC 的新增环节。当前第 2 步不存在，emitter 直接消费 `Program`——因此本 RFC 采用**渐进路径**（见 §14），不要求一次性插入新环节。

### 8.6 Extension Points

- **扩展字段**（`extensions`）：按 `capabilityId` 命名空间隔离（如 `visualization.echarts`），语义与 RFC-0004 §9 的三层 IR 策略一致——**核心语义**（Entity/Relation/Type/Rule）/ **领域语义**（Chart/Map/Workflow，独立 IR 模块）/ **技术实现**（ECharts/MySQL，不进核心）。
- **未知扩展可保留，不可擅解释**：前向兼容的基本要求（RFC-0004 §9）。
- **新节点种类**：扩展 `NodeKind` 而非新建枚举（§8.2 硬约束）。

## 9. Compatibility

- **Backward**：新增 IR 字段/节点种类为 minor，既有 emitter 不受影响（emitter 只读自己关心的字段）。
- **Forward**：未知扩展保留但不被解释；未知节点种类不得导致构建失败。
- **Version Compatibility**：`IR_VERSION` 独立于 Specification Version。IR 版本变更需：
  - minor：仅新增可选字段 / 新增节点种类 / 新增扩展命名空间；
  - major：字段语义变更 / 节点种类移除 / 标识规则变更（需迁移报告，RFC-0001 §9）。
- **Migration Compatibility**：IR 迁移路径必须显式提供（IR v1 → Migration → IR v2 → Validation），禁止静默覆盖（RFC-0001 §9）。
- **Deprecation Policy**：被弃用的 IR 字段/节点种类须经历「标记 → 一个 minor 版本 warning → 移除」，与 DSL 语法弃用周期一致（[roadmap.zh-CN.md §6.2](../../roadmap.zh-CN.md)）。

## 10. Security Considerations

- **信任边界**：DSL 是用户内容。Lowering 与语义检查是**唯一**把用户文本转成可执行代码之前的关口，因此必须做到：
  - 检查在 Emit **之前**执行，且 `error` 阻断构建（当前 `build.rs:load_dsl_program` 已接线，见 `crates/forma/src/build.rs` 的 `load_dsl_program`）；
  - 表达式语言是封闭子集，每个运算符有 Rust/TS/Dart 三端映射（[dsl-spec §9](../../dsl-spec.zh-CN.md)），杜绝「单端可编译、另一端不可编译」的规则。
- **规则表达式**：不得引入 `eval` 类动态执行；所有条件编译为静态代码。
- **权限**：`access` 规则（RBAC）编译到后端权威校验，前端显隐**不是**权限控制。
- **Capability 越权**：Capability 不得修改 Semantic Core；`extensions` 不得覆盖核心语义（RFC-0004 §9）。

## 11. Performance and Reliability

- **确定性**：Lowering 与检查必须无随机、无时间依赖、无 HashMap 迭代序依赖（当前 `util::stable_stringify` 对对象键排序，是既有正确做法）。
- **增量**：StableId 使「按语义路径定位节点」成为 O(1)，替代按名字扫描。
- **失败隔离**：单个节点 Lowering 失败不得影响其余节点；失败以诊断形式聚合。
- **性能目标**：语义检查 + Lowering 对 100 实体工程应在百毫秒级完成（当前 `check_program` 为单趟遍历，已满足）。

## 12. Alternatives Considered

- **废弃联邦 `UnifiedSemanticGraph`，重写为新 IR**：拒绝——联邦 importer（`adapters/sql.rs`、`openapi.rs`、`jsonschema.rs`、`jsonforms.rs`、`bpmn.rs`）与 `patch.rs` 依赖它；重写会同时打断导入器与补丁链路。改为**规范表示收敛 + 联邦图降级为视图**。
- **为 DSL 侧另建平行 IR 枚举与标识体系**：拒绝——两套标识会让 `forma_impact`、增量构建、Artifact 追踪都需要双份映射，违背「规范表示只有一个」。
- **保持现状，仅补文档说明两套 IR 并存**：拒绝——DSL 工程因此永远没有影响闭包与事务补丁（§3.1 缺口 1），Agent 工作流只能停留在「改文件—重编译」。
- **把语义检查完全交给运行时**：拒绝——错误必须在编译期暴露（RFC-0001 §7.3「DSL 不泄漏目标概念」的对偶要求：错误也不得泄漏到运行时）。

## 13. Trade-offs

- 引入 Stable Identity 与来源追踪的成本（每个节点多几个字段、Lowering 多一趟），换取跨版本可追溯与影响分析能力。
- 收敛到单一规范表示，短期内联邦图与 DSL IR 并存需要维护桥接代码；长期换取工具链统一。
- 不要求立即把 Lowering 插入所有 emitter 管线，换取 emitter 契约零破坏。

## 14. Migration Plan

渐进四步，每步独立可发布、独立可验证：

| 步 | 内容 | 完成判据 |
|---|---|---|
| **M1** | 补 `EntityDecl`/`ApiDecl`/`PageDecl`/`CommandDecl` 的 `line`；诊断携带精确 file/line | 全部 `dsl-*` 诊断可定位到规格行 |
| **M2** | 引入 `IR_VERSION` 与 `Program.ir_version`；补 `Json` 等内建类型到 `BUILTIN_TYPES` 的单一权威清单 | 检查层与生成层类型集合一致；`Json` 字段可通过 `forma check` |
| **M3** | 引入 `StableId` 与 `SourceRef`（复用联邦 `SourceRef` 结构）；为 DSL 实体/字段/API/页面/命令构造节点与边 | DSL 工程可导出与联邦同构的节点/边集合 |
| **M4** | 修正 CLI/MCP 一致性：`validate`/`doctor`/`graph` 对 DSL 工程走 `check_program` 路径 | 含语义错误的 DSL 工程 `forma validate` 必须非零退出 |

> M4 是**安全相关**修复：当前假绿会让 CI 在带错误的规格上通过。优先于 M3 落地。

迁移期间，既有 emitter 继续消费 `Program`（M1–M3 不改变 `Program` 的既有字段语义），无需同步改动。

## 15. Implementation Plan

> **实施进度（2026-11-08 逐份核实）**：M1–M4 **全部完成**——声明级 `file`/`line`、
> `IR_VERSION` 1.2.0、`BUILTIN_TYPES` 12 型四目标映射、`dsl::lower` 单向桥接（节点 URI 与联邦同构）、
> CLI/MCP 统一经 `engine::check_project` 分发；阶段 4 的影响闭包已接入 Agent 协议
> （`forma_get_context` / `forma_plan`）。
> **阶段 3 未单独交付**：`spec:ir` / `spec:types` 是正式 Markdown 规范，但没有机器可读
> JSON Schema 文件；已登记 [gap-backlog](../roadmap/gap-backlog.zh-CN.md)（G-10）。

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | M1 + M2 + M4（诊断定位、类型清单、CLI 一致性） | 无 |
| 阶段 2 | M3（StableId / SourceRef / 依赖边） | 阶段 1 |
| 阶段 3 | `spec:ir` 与 `spec:types` 正式化（机器可读 schema） | 阶段 2 |
| 阶段 4 | 影响闭包接入 RFC-0006 的 Agent 协议 | 阶段 3 |

## 16. Conformance Requirements

> 核实日期 2026-11-08（Phase 1 收口）：逐条对照实现与回归。

- [x] `forma check` 与 `forma validate` 对同一 DSL 工程给出一致结论（含退出码）。
  证据：`engine::check_project` 单一分发点；`tests/dsl_validate.rs`。
- [x] 含 `error` 级诊断的 DSL 工程，`forma validate` 必须非零退出（反假绿断言）。
  证据：`tests/dsl_validate.rs::dsl_project_with_semantic_error_is_reported`（P0-4 修复）。
- [x] `BUILTIN_TYPES` 与各 emitter 的类型映射集合一致（新增类型须三端同步，否则测试失败）。
  证据：`tests/dsl_ir_bridge.rs::builtin_types_have_mappings_in_all_targets`（逐型四目标断言）。
- [x] 全部 `dsl-*` 诊断码携带 `file` + `line`。
  证据：声明级 `Origin`；`tests/dsl_ir_bridge.rs::every_dsl_diagnostic_has_a_location`。
- [x] 同一 `Program` 两次 Lowering / 检查结果 byte 级一致。
  证据：`tests/dsl_ir_bridge.rs::lowering_output_is_byte_identical_across_runs` +
  `tests/adapter_roundtrip.rs::repeated_check_program_is_deterministic`。
- [x] DSL 侧节点/边集合与联邦 `UnifiedSemanticGraph` 同构（同一实体得到同一 StableId 字符串）。
  证据：`tests/dsl_ir_bridge.rs::bridge_produces_stable_ids_and_source_refs` /
  `bridge_produces_dependency_edges`。
- [x] 新增 Capability / Adapter 的 PR 不修改 Semantic Core（Tier 1 红线，RFC-0004 §16）。
  证据：`tests/capability.rs` + 各 target 的「默认零变化」回归（`tests/postgres.rs` / `sqlite.rs` / `vue.rs` / `expo.rs` 等）。

## 17. Acceptance Criteria

可量化判据：

1. `Json` 字段的 DSL 工程通过 `forma check` 且生成的 SQL 为 `JSON` 列、Rust 侧为 `serde_json::Value`。
2. 含未知类型的 DSL 工程，`forma validate` 退出码非 0 且诊断含 `file`/`line`。
3. DSL 工程导出的节点集合中，每个实体/字段均带 StableId，且与联邦导入同一实体时 StableId 字符串相同。
4. 同一规格连续两次 `forma build`，产物 byte 级一致且第二次无文件写入（增量断言，已有 `tests/build.rs:incremental_build_skips_unchanged_outputs` 覆盖同源断言）。

## 18. Open Questions

> 收口（2026-11-08）：2 项关闭、2 项显式保留。

- [x] ~~StableId 是否需要在**跨项目**场景下唯一？~~
  **已决议**：项目内唯一（`forma://project/{p}/…`）；多仓库合并同一业务对象不在 v0.3 范围。
- [ ] 依赖边的粒度：字段级边是否足够，还是需要表达式级依赖（规则引用了哪些字段）？
  ⏸ **保留**：字段级边已满足影响闭包主用例（`tests/dsl_ir_bridge.rs`）；表达式级依赖成本高，
  待「补丁影响面误报/漏报」真实案例出现再评估。
- [x] ~~联邦 importer 降级到 Semantic Core 后，`adapters/*` 是否应逐步删除对 `graph.rs` 的直接依赖？~~
  **已决议**：保留 `graph.rs` 为内部表示（桥接已让 DSL 与联邦两条路径共用同一套图 API，见 spec:ir §15）。
- [ ] 表达式级依赖是否需要独立的 `spec:dependencies` 文档？
  ⏸ **保留**（跟随上一条）。


## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览与依赖红线：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 能力系统与三层 IR：[RFC-0004-capability-system.zh-CN.md](RFC-0004-capability-system.zh-CN.md)
- 可视化范例（领域 IR 参照）：[RFC-0010-visualization.zh-CN.md](RFC-0010-visualization.zh-CN.md)
- 治理决策：[ADR-0001-adopt-rfc-governance.zh-CN.md](../adr/accepted/ADR-0001-adopt-rfc-governance.zh-CN.md)
- DSL 语法：[dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md)
- 验证体系 L1–L5：[verification.zh-CN.md](../../verification.zh-CN.md)
- 迁移安全：[migration-safety.zh-CN.md](../../migration-safety.zh-CN.md)
- 实现位置：`crates/forma/src/dsl/parser.rs`、`crates/forma/src/dsl/mod.rs`、`crates/forma/src/graph.rs`、`crates/forma/src/identity.rs`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义语义内核契约（Stable Identity / Type System / Source Ref / Dependency / IR Version）、双 IR 收敛决策与四步迁移路径 |
| 1.1 | 2026-11-08 | Phase 1 §16 验收核实：逐条对照回归测试与实现证据并勾选；未覆盖项保留 `[ ]` 并登记 [gap-backlog](../roadmap/gap-backlog.zh-CN.md)「Phase 1 §16 未覆盖项」；配套补齐生成后端 `/health` `/ready`（RFC-0005）与产物原子写 / 内容跳过（RFC-0007） |
