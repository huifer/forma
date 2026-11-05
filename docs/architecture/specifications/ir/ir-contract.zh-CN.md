---
id: spec:ir
title: Unified Semantic IR Contract
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-05
domain: semantic-core
governing_rfc: RFC-0003
supersedes: []
superseded_by: []
depends_on: []
related_adrs: [ADR-0001, ADR-0002]
related_specs: [spec:types]
breaking_change: false
---

# spec:ir — 统一语义 IR 契约（Unified Semantic IR Contract）

> **规范性**：本文件定义 IR 的**目标契约**（规范性条款用 MUST / MUST NOT / SHOULD 表述）。
> §12「现状差距」逐条列出**当前实现与本契约的偏差**——这些条目**尚未符合本规范**，
> 且**不受本规范约束**。契约获批 ≠ 实现达标；实现进度以 §12 表格为准。

> 本文档是 Layer 2（Specification）层的**语义契约**，规定 IR 节点的结构、标识、来源、版本与扩展规则。
> 边界与依赖红线由 [RFC-0003](../../rfc/RFC-0003-semantic-core.zh-CN.md) 规定；本文档只定义**语义**。
> **本文档的契约部分属 Tier 1**——修改须经正式 RFC + 迁移设计 + 兼容验证。

---

## 1. 范围

本契约适用于 Forma DSL 主线的语义内核（`Program`）及其向统一语义节点集的降级结果。

**不在范围**：DSL 语法（[dsl-spec.zh-CN.md](../../../dsl-spec.zh-CN.md)）、Capability Manifest（[RFC-0004](../../rfc/RFC-0004-capability-system.zh-CN.md)）、产物治理（[RFC-0007](../../rfc/RFC-0007-build-and-artifact.zh-CN.md)）。

---

## 2. 术语

| 术语 | 定义 |
|---|---|
| **节点（Node）** | 语义内核中的最小可寻址单元，承载稳定标识、来源与类型化载荷 |
| **边（Edge）** | 节点之间的有向语义依赖 |
| **稳定标识（Stable Identity）** | 跨重新编译、跨版本保持不变的节点标识 |
| **来源（Source Ref）** | 节点到规格源位置（file/line）的引用 |
| **扩展（Extension）** | 按能力命名空间隔离的目标专有信息 |

---

## 3. 节点结构

```rust
/// 语义内核节点。所有领域 IR 共用此信封。
pub struct IrNode<T> {
    /// 稳定标识：语义路径构成，跨版本不变（见 §4）
    pub id: StableId,
    /// 语义种类
    pub kind: NodeKind,
    /// 来源追踪：可多源（DSL 声明 + 联邦导入视图）
    pub source: Vec<SourceRef>,
    /// 类型化载荷
    pub value: T,
    /// 目标专有扩展，按 capabilityId 命名空间隔离
    pub extensions: ExtensionMap,
}

/// 稳定标识：语义路径式，人可读、可调试。
///
/// 格式：`<project>/<kind>/<path...>`
/// 例：`school/entity/Student/field/email`
/// 例：`school/api/Orders/operation/list`
/// 例：`school/page/OrderList`
pub struct StableId(String);

pub struct SourceRef {
    pub file: String,
    pub line: u32,
    pub column: Option<u32>,
}
```

### 3.1 载荷类型（`T`）

载荷按领域划分，**每个领域一个独立模块**，不合并为巨型枚举：

| 领域 | 载荷类型 | 定义位置 |
|---|---|---|
| 数据 | `EntityNode` / `FieldNode` / `EnumNode` / `EnumVariantNode` / `RelationNode` / `IndexNode` | `spec:ir`（本文档） |
| 规则 | `FieldRuleNode` / `CheckRuleNode` / `GuardNode` / `AccessNode` | `spec:ir`（本文档） |
| API | `ApiNode` / `OperationNode` / `CommandNode` | `spec:ir`（本文档） |
| UI | `PageNode` / `TableNode` / `FormNode` / `DetailNode` | `spec:ir`（本文档） |
| 可视化 | `ChartNode` | [RFC-0010](../../rfc/RFC-0010-visualization.zh-CN.md) |

> **约束**：领域载荷**不得**引入其他领域的概念。跨领域引用一律通过边表达（§5），而非在载荷里内嵌别领域的结构。

---

## 4. 稳定标识规则

| 编号 | 规则 |
|---|---|
| ID1 | 标识由**语义路径**构成（项目 / 种类 / 路径段），**不**由声明顺序、数组下标或哈希构成 |
| ID2 | 标识在**同一规格版本内**唯一，且对同一输入**可重现** |
| ID3 | 标识**不含**源码位置（行号会随编辑漂移，不属于身份） |
| ID4 | 改名会改变标识（名称是语义路径的一部分）；这与「增量以节点内容哈希为准」是两个独立机制（见 [RFC-0007](../../rfc/RFC-0007-build-and-artifact.zh-CN.md) §8.3） |
| ID5 | 跨项目引用（如多仓合并）时，`project` 段可带命名空间限定 |

### 4.1 与既有 `forma://` URI 的关系

联邦侧已有的 `forma://project/{p}/entity/{e}/field/{f}` URI（`crates/forma/src/identity.rs`）与本契约的 `StableId` 是**同一标识的两种字符串形式**：

```text
forma://project/school/entity/Student/field/email   ← 既有 URI 形式
school/entity/Student/field/email                     ← StableId 形式
```

迁移期两者共存且**必须可互换**（同一实体在两条路径下得到同一标识串）。收敛后的规范形式由 ADR-0002 裁决。

---

## 5. 依赖边

```rust
pub enum EdgeKind {
    /// 定义：实体 → 字段、页面 → 组件
    Defines,
    /// 归属：子实体 → 父实体（关系）
    BelongsTo,
    /// 绑定：字段 → 类型、页面 → API 操作
    Binds,
    /// 引用：规则 → 字段、图表 → 数据源
    References,
    /// 生成：语义节点 → 产物节点
    Generates,
}
```

> 复用联邦侧既有 `EdgeKind`（`crates/forma/src/graph.rs`）——**不新建平行枚举**（ADR-0002 硬约束）。

| 编号 | 规则 |
|---|---|
| E1 | 边是**有向**的，且**必须无环**（建图时检测环并报错） |
| E2 | 边的语义由 `EdgeKind` 决定，**不得**新增未登记的边种类而不修改本契约 |
| E3 | 影响闭包 = 沿边做**反向可达**（谁依赖我），不是正向 |
| E4 | 表达式级依赖（规则引用了哪些字段）**可选**；未实现时闭包按字段级近似（RFC-0003 §18 未决问题） |

### 5.1 必须表达的依赖

| 依赖 | 边 | 用途 |
|---|---|---|
| 实体 → 字段 | `Defines` | 字段变更 → 模型 / 表单 / 校验 |
| 实体 → 父实体 | `BelongsTo` | 关系字段变更 → JOIN / 关联选择器 |
| 字段 → 类型 | `Binds` | 类型变更 → 表列类型 / 迁移 |
| 规则 → 字段 | `References` | 规则变更 → validator / zod / 表单校验 |
| 页面 → API 操作 | `Binds` | API 变更 → 数据 hooks / 类型 |
| 图表 → 字段 | `References` | 字段变更 → 图表编码失效 |
| 语义节点 → 产物 | `Generates` | 产物级增量与冲突定位 |

---

## 6. 来源追踪

| 编号 | 规则 |
|---|---|
| S1 | 节点携带其**全部**来源：DSL 声明位置 + （若有）联邦导入视图位置 |
| S2 | 诊断必须能反向定位到规格文件与行号（`Diagnostic { file, line }`） |
| S3 | 降级过程**不得丢失**来源 |
| S4 | 无法定位的节点必须显式标记为未知来源，**不得**静默填充 |
| S5 | 来源是**元数据**，不参与稳定标识计算（见 ID3） |

> 现状差距：DSL 侧仅 `FieldDecl.line` 与 `ChartDecl.line` 携带位置；`EntityDecl` / `ApiDecl` / `PageDecl` / `CommandDecl` 无位置信息。修复路径见 [RFC-0003 §14](../../rfc/RFC-0003-semantic-core.zh-CN.md) M1。

---

## 7. 版本

```rust
/// IR 契约版本。与 Specification Version、Project Version、Compiler Version 独立。
pub const IR_VERSION: &str = "1.0.0";
```

| 编号 | 规则 |
|---|---|
| V1 | 每个 IR 产物声明其 `irVersion` |
| V2 | **minor** 变更：仅新增可选字段、新增节点种类、新增扩展命名空间、新增边种类 |
| V3 | **major** 变更：字段语义变更、节点种类移除、标识规则变更、边语义变更 |
| V4 | major 变更必须提供显式迁移路径（IR v1 → Migration → IR v2 → Validation），**禁止**静默覆盖 |
| V5 | 消费者**必须**声明其支持的 IR 版本范围；超出范围时显式失败或告警，**不得**静默降级 |

### 7.1 五个版本的职责边界

| 版本 | 消费者关心的问题 |
|---|---|
| **Specification Version** | 我能写什么语法？ |
| **IR Version** | 我能消费什么语义结构？ |
| **Capability Version** | 这个能力提供什么契约？ |
| **Adapter Version** | 这个适配器实现什么目标投影？ |
| **Runtime Version** | 这个运行时模块承诺什么行为？ |

另含 **Project Version**（业务演进）与 **Compiler Version**（编译行为），二者不混入上述五类。

---

## 8. 扩展字段

```rust
pub struct ExtensionMap {
    // key: capabilityId（如 "visualization.echarts"）
    // value: 该能力私有的结构化数据
}
```

| 编号 | 规则 |
|---|---|
| X1 | 扩展按 `capabilityId` 命名空间隔离；核心语义**不得**放入扩展 |
| X2 | 扩展**不得覆盖**核心语义字段 |
| X3 | 未知扩展**可保留**，但**不可擅解释**（前向兼容） |
| X4 | Adapter 只处理自己声明支持的扩展 |
| X5 | 扩展升级**必须**有迁移策略 |

> 与 [RFC-0004 §9](../../rfc/RFC-0004-capability-system.zh-CN.md) 的三层 IR 策略一致：**通用语义**（Entity/Relation/Type/Rule）/ **领域语义**（Chart/Map/Workflow）/ **技术实现**（ECharts/MySQL，不进核心）。

---

## 9. 类型系统

### 9.1 内建类型（权威清单）

内建类型**只有一个定义源**，检查层与全部生成层从同一处派生：

| 类型 | 语义 | MySQL | Rust | TypeScript | Dart |
|---|---|---|---|---|---|
| `Uuid` | 通用唯一标识 | `CHAR(36)` | `String` | `string` | `String` |
| `String(n)` | 变长字符串 | `VARCHAR(n)` | `String` | `string` | `String` |
| `Email` | 邮箱 | `VARCHAR(n)` | `String` | `string` | `String` |
| `Text` | 长文本 | `TEXT` | `String` | `string` | `String` |
| `Int` | 整数 | `BIGINT` | `i64` | `number` | `int` |
| `Float` | 浮点 | `DOUBLE` | `f64` | `number` | `double` |
| `Decimal(p,s)` | 定点小数 | `DECIMAL(p,s)` | `Decimal` | `string` | `String` |
| `Bool` | 布尔 | `TINYINT(1)` | `bool` | `boolean` | `bool` |
| `Timestamp` | 时间戳 | `DATETIME(3)` | `DateTime<Utc>` | `string` | `DateTime` |
| `Date` | 日期 | `DATE` | `NaiveDate` | `string` | `DateTime` |

| 编号 | 规则 |
|---|---|
| T1 | **单一权威清单**：检查层与生成层的类型集合必须一致（新增类型须三端同步，否则测试失败） |
| T2 | 新增类型为 **minor**；移除类型为 **major**（需迁移报告） |
| T3 | 每个类型对每个目标有**唯一**映射；缺失即编译期错误 |
| T4 | **`Decimal` 不得降级为浮点**：MySQL `DECIMAL(p,s)`、Rust `Decimal`、TS/Dart 用 `string` 序列化（避免 IEEE754 精度丢失） |
| T5 | 关系类型（实体名）与枚举类型不属内建集，由规格声明 |
| T6 | `Json` 若纳入内建集，须同时出现在检查层与全部生成层——**当前不一致**（见 §12） |

### 9.2 枚举与关系

- **枚举**：类型名解析到 `enum` 声明；变体受**属性**约束（`@label` 等）。
- **关系**：类型名解析到 `entity` 声明；生成侧表现为 `{rel}_id` 列 + `{rel}_name` 展示列（后端 ListRow 契约），而**输入体**使用裸关系名 `{rel}`——两套键名体系不可统一。

---

## 10. 语义检查契约

### 10.1 职责

| 编号 | 规则 |
|---|---|
| C1 | `check_program` 是 DSL 主线的**唯一**语义检查入口 |
| C2 | 检查在 **Emit 之前**执行；`error` 级诊断**阻断**构建 |
| C3 | 诊断严重度只有三档：`error`（阻断）/ `warning` / `info`（均不阻断） |
| C4 | 诊断**不早退**：聚合全部问题后一次性返回 |
| C5 | 诊断码是**稳定 API**：已发布码的含义不得改变；废弃须先经一个 minor 版本 warning |

### 10.2 诊断码命名

```text
dsl-*    DSL 主线语义诊断
viz-*    可视化领域诊断
lint-*   风格与最佳实践
```

### 10.3 不得静默忽略

未被支持的语义**必须**显式报错或明确 warning，不得静默跳过（设计宪法 G5，[roadmap.zh-CN.md §6.2](../../../roadmap.zh-CN.md)）。

已落地实例：生成器遇到 `paginate cursor`（无 emitter 实现）时显式 `bail!`。

**待落地**（见 §12）：权限构造（`access` / `auth`）当前被解析但不被消费——须产出显式诊断。

---

## 11. 序列化

| 编号 | 规则 |
|---|---|
| R1 | IR 的序列化形式仅供**工具消费**（MCP / Studio / 清单），**不是**公开契约——公开契约是本文档的语义 |
| R2 | 序列化**必须**确定性：相同 IR 两次序列化字节一致（对象键排序） |
| R3 | 序列化**不得**包含非确定性内容（时间戳、随机数）——非确定性信息（如 `generatedAt`）只在产物清单层 |
| R4 | 序列化形式随 IR Version 演进；消费者须容忍未知字段 |

---

## 12. 现状差距（实现与本契约的偏差）

以下是**当前实现**与本契约的已知差距（经源码核对与实跑复现）。它们是 [RFC-0003](../../rfc/RFC-0003-semantic-core.zh-CN.md) §14 迁移路径的实现对象。

| 编号 | 差距 | 影响 | 修复步骤 |
|---|---|---|---|
| G-1 | DSL 侧无稳定标识 | 影响闭包与可寻址补丁无法建立 | RFC-0003 M3 |
| G-2 | DSL 侧无依赖边 | 无法计算影响面 | RFC-0003 M3 |
| G-3 | 来源追踪仅覆盖字段与图表 | 诊断无法精确定位到实体 / API / 页面 | RFC-0003 M1 |
| G-4 | IR 无版本 | 无法判定契约版本与兼容范围 | RFC-0003 M2 |
| ~~G-5~~ | ~~`Json` 在生成层被识别但不在检查层内建清单~~ | **已关闭（2026-11-05）**：`Json` 已纳入 `BUILTIN_TYPES`（11 型），四目标映射齐备 | ✅ 已完成 |
| ~~G-6~~ | ~~`access` / `auth` 被解析但检查层与生成层均不消费~~ | **已关闭（2026-11-05）**：检查层新增 7 条 `dsl-auth-*` / `dsl-access-*` 诊断；生成层在服务端强制执行（401/403）；`api { auth }` 与 `rules { access }` 均已落地 | ✅ 已完成 |
| G-7 | 无 DSL → 语义图降级 | 两条路径语义能力割裂（**M4 已完成**：CLI/MCP 对 DSL 工程统一走 `check_program`，假绿已消除） | ADR-0002 / RFC-0003 M3 |

---

## 13. Conformance

实现必须通过的检查：

- [ ] 同一规格两次降级 / 检查结果**字节级一致**。
- [ ] 全部 `dsl-*` 诊断携带 `file` + `line`。
- [ ] 检查层与各生成层的类型集合一致（新增类型须三端同步，否则测试失败）。
- [ ] DSL 侧节点/边集合与联邦语义图**同构**（同一实体得到同一标识串）。
- [ ] 依赖图**无环**。
- [ ] 含 `error` 诊断时构建被阻断。
- [ ] 未支持的语义产生显式诊断（无静默忽略）。
- [ ] IR 序列化不含非确定性内容。

---

## 14. 变更

本契约的变更遵循 [RFC-0003](../../rfc/RFC-0003-semantic-core.zh-CN.md) §9 的版本规则：

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版：定义节点结构、稳定标识、依赖边、来源追踪、版本、扩展、类型系统、语义检查与序列化契约；记录 7 项现状差距 |
| 1.0.0 | 2026-11-05 | 状态提升为 Accepted（契约定稿）；关闭 G-6（权限声明不再为空承诺）、G-5（`Json` 入内建清单）；补 §15 双 IR 桥接路径；M4 完成（CLI/MCP 对 DSL 工程统一走 `check_program`，消除假绿） |

---

## 15. 双 IR 桥接路径（DSL Program → 联邦语义图）

现状是**两套互不相通的 IR**（ADR-0002 已裁定 DSL `Program` 为规范语义表示，联邦图降级为导入视图）：

| 维度 | DSL 主线 `Program` | 联邦 `UnifiedSemanticGraph` |
|---|---|---|
| 稳定标识 | ❌ 仅声明名 | ✅ `forma://` URI |
| 依赖边 | ❌ 无 | ✅ `EdgeKind`（5 类） |
| 来源追踪 | ⚠️ 仅 `FieldDecl.line` / `ChartDecl.line` | ✅ `SourceRef` |
| 语义检查 | ✅ `check_program` | ✅ `run_doctor` |

**桥接目标**：让 DSL 工程也能获得稳定标识、依赖边与影响闭包，同时**不改动既有 emitter 契约**。

### 15.1 四步渐进（与 RFC-0003 §14 的 M1–M3 对齐）

| 步 | 交付物 | 依赖 | 验收 |
|---|---|---|---|
| **M1** | 补 `EntityDecl` / `ApiDecl` / `PageDecl` / `CommandDecl` 的 `line`；诊断携带精确 file/line | 无 | 全部 `dsl-*` 诊断可定位到规格行 |
| **M2** | 引入 `IR_VERSION` 与 `Program.ir_version`；补 `Json` 等内建类型到**单一权威清单** | 无 | 检查层与生成层类型集合一致；`Json` 字段可通过 `forma check` |
| **M3** | 引入 `StableId` 与 `SourceRef`；为 DSL 实体/字段/API/页面/命令**构造节点与边** | M1、M2 | DSL 工程可导出与联邦**同构**的节点/边集合 |
| **M4** | 修正 CLI/MCP 一致性：`validate` / `doctor` / `graph` 对 DSL 工程走 `check_program` 路径 | 无 | 含语义错误的 DSL 工程 `forma validate` **必须非零退出** |

> M4 是**安全相关**修复：当前「假绿」会让 CI 在带错误的规格上通过——优先于 M3 落地。

### 15.2 桥接的落点

```text
DSL .fm ──parse──▶ Program（当前规范表示）
                      │
                      ├──▶ check_program（语义检查，L2）
                      │
                      └──▶ lower_to_graph()  ← 本桥接（M3 新增）
                                │
                                ▼
                    UnifiedSemanticGraph（稳定标识 + 依赖边）
                                │
                    ┌───────────┼───────────┐
                    ▼           ▼           ▼
              impact 闭包   patch 预览   联邦导入视图
```

- **桥接是单向的**：`Program → 图`。联邦图**不得**反向成为规范表示（ADR-0002）。
- 迁移期间既有 emitter 继续消费 `Program`（M1–M3 **不改变** `Program` 既有字段语义），
  因此**无需同步改动任何 emitter**——这是「不改核心也能补齐契约」的关键。
- 桥接完成后，`forma impact` / `forma patch` 对 DSL 工程**不再被拒绝**
  （当前被 `reject_dsl_only_tool` 显式拒绝）。

### 15.3 兼容性

- 桥接**不改变** DSL 语法、IR 语义与目标产物——因此**不是破坏性变更**。
- 新增的节点/边只影响**分析类工具**（impact / patch / 依赖图）的可用性。
- 图的 `NodeKind` / `EdgeKind` 取值集合**MUST** 与联邦侧保持同构（Conformance §13 要求）。

### 15.4 现状

- G-1（稳定标识）、G-2（依赖边）、G-3（来源追踪）、G-4（IR 版本）、G-7（无降级）**仍未关闭**。
- 本节是这些差距的**实施路径**，不是完成声明。
