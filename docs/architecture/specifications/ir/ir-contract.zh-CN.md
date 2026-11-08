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
| 数据 | `EntityNode` / `FieldNode` / `EnumNode` / `EnumVariantNode` / `RelationNode` / `IndexNode` / `MigrationNode`（版本化迁移链，schema 变更的物化载体） | `spec:ir`（本文档） |
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
pub const IR_VERSION: &str = "1.2.0";
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

## 7.2 机器可读契约（G-10）

本规范的机器可读版本：[program.schema.json](program.schema.json)（JSON Schema draft-07）。

- 覆盖 IR 的**稳定字段面**：`ir_version`、`app` / `entities` / `enums` / `rules` / `apis` / `pages` /
  `jobs` / `lives` / `exports` / `charts` / `commands` / `auth`；
- 内建类型词表（`BUILTIN_TYPES`，12 型）作为 `$defs.BuiltinType.enum` 内嵌，与代码单一来源；
- 一致性回归：`tests/ir_schema.rs` 用真实 `Program` 的契约视图校验 schema（schema 漂移会被测试抓住）。

> 说明：`Program` 未启用 serde 直出（避免把内部声明结构暴露为公开契约），
> schema 描述的是 IR 契约视图（stable field surface），而非 `Program` 的完整内存布局。

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
| ~~G-1~~ | ~~DSL 侧无稳定标识~~ **已关闭（2026-11-05，M3）**：桥接产出 `forma://project/{p}/entity/{E}` 等稳定 URI，与联邦侧**同一套** URI 构造函数（`identity.rs`） | ~~影响闭包与可寻址补丁无法建立~~ → 影响闭包已可用 | ✅ 已完成 |
| ~~G-2~~ | ~~DSL 侧无依赖边~~ **已关闭（2026-11-05，M3）**：桥接产出 `defines` / `references` / `belongs-to` 边，覆盖实体↔字段、字段↔关系/枚举、操作↔实体、页面↔操作/实体、页面↔控件 | ~~无法计算影响面~~ → `forma impact` 对 DSL 工程可用 | ✅ 已完成 |
| ~~G-3~~ | ~~来源追踪仅覆盖字段与图表~~ **已关闭（2026-11-05，M1）**：声明级 `file` + `line` 覆盖实体 / 枚举 / 规则 / api / 命令 / 页面，诊断逐条携带位置 | ~~诊断无法精确定位~~ → `dsl-*` 诊断全部可定位到 `file:line` | ✅ 已完成 |
| ~~G-4~~ | ~~IR 无版本~~ **已关闭（2026-11-05，M2）**：`IR_VERSION = "1.0.0"` + `Program.ir_version`，CLI/MCP 可见 | ~~无法判定契约版本~~ → 版本可观测、可比对 | ✅ 已完成 |
| ~~G-5~~ | ~~`Json` 在生成层被识别但不在检查层内建清单~~ | **已关闭（2026-11-05）**：`Json` 已纳入 `BUILTIN_TYPES`（11 型），四目标映射齐备 | ✅ 已完成 |
| ~~G-6~~ | ~~`access` / `auth` 被解析但检查层与生成层均不消费~~ | **已关闭（2026-11-05）**：检查层新增 7 条 `dsl-auth-*` / `dsl-access-*` 诊断；生成层在服务端强制执行（401/403）；`api { auth }` 与 `rules { access }` 均已落地 | ✅ 已完成 |
| ~~G-7~~ | ~~无 DSL → 语义图降级~~ **已关闭（2026-11-05，M3 + M4）**：M3 落地 `Program → UnifiedSemanticGraph` 单向桥接（`dsl::lower`），M4 已让 CLI/MCP 对 DSL 工程统一走 `check_program`（假绿消除） | ~~两条路径语义能力割裂~~ → DSL 工程与联邦工程共用同一套图 API | ✅ 已完成 |

---

## 13. Conformance

实现必须通过的检查：

- [ ] 同一规格两次降级 / 检查结果**字节级一致**。
- [x] 全部 `dsl-*` 诊断携带 `file` + `line`（M1；回归 `tests/dsl_ir_bridge.rs`）。
- [x] 检查层与各生成层的类型集合一致（新增类型须三端同步，否则测试失败；M2 回归逐类型断言四目标映射）。
- [x] DSL 侧节点/边集合与联邦语义图**同构**（同一实体得到同一标识串；M3 回归断言 URI 与边集合）。
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
| 1.1.0 | 2026-11-05 | **M1–M3 落地**：M1 声明级来源位置（`file`/`line`）与逐条定位的诊断；M2 `IR_VERSION` + `Program.ir_version` + 内建类型单一权威清单（四个目标映射一致性回归）；M3 `Program → UnifiedSemanticGraph` 单向桥接（稳定标识 + 依赖边 + 来源追踪），`forma graph` / `forma impact` 对 DSL 工程可用。关闭 G-1 / G-2 / G-3 / G-4 / G-7；**不改变** DSL 语法、IR 语义与目标产物（非破坏性变更） |
| 1.2.0 | 2026-11-05 | **P3 收尾（RFC-0006）**：`NodeKind` 增加 `chart`（可视化域 `ChartNode`）与 `migration`（数据域 `MigrationNode`）；桥接图并入图表节点（chart → api 操作 / 编码通道字段，页面 → chart）与迁移链节点（表 → 迁移链，`generates` 向前传播）。**影响闭包含齐五类目标**：表列 / 模型 / 表单 / 图表 / 迁移。`IR_VERSION` → `1.2.0`（节点结构变更 ⇒ minor 递增，spec:ir §7）。回归 `tests/dsl_ir_bridge.rs`（chart_and_migration / impact_closure_covers_all_five_target_kinds） |

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

| 步 | 交付物 | 依赖 | 验收 | 状态 |
|---|---|---|---|---|
| **M1** | 补 `EntityDecl` / `EnumDecl` / `RulesDecl` / `ApiDecl` / `PageDecl` / `CommandDecl` 的 `line` + `file`；诊断携带精确 file/line | 无 | 全部 `dsl-*` 诊断可定位到规格行 | ✅ 已完成（2026-11-05） |
| **M2** | 引入 `IR_VERSION` 与 `Program.ir_version`；内建类型**单一权威清单**（`BUILTIN_TYPES`）与四目标映射一致性回归 | 无 | 检查层与生成层类型集合一致；`Json` 字段可通过 `forma check` | ✅ 已完成（2026-11-05） |
| **M3** | `StableId`（`forma://` URI）与 `SourceRef`；`dsl::lower` 为 DSL 实体/字段/枚举/API/命令/页面**构造节点与边** | M1、M2 | DSL 工程可导出与联邦**同构**的节点/边集合 | ✅ 已完成（2026-11-05） |
| **M4** | 修正 CLI/MCP 一致性：`validate` / `doctor` / `graph` 对 DSL 工程走 `check_program` 路径 | 无 | 含语义错误的 DSL 工程 `forma validate` **必须非零退出** | ✅ 已完成（2026-11-05，`engine::check_project` 共享管线） |

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
- 桥接完成后，`forma impact` 对 DSL 工程**不再被拒绝**（已实现：MCP `forma_impact`
  对 DSL 工程走 `dsl::lower` 投影的图）。
- `forma patch` 对 DSL 工程**仍限联邦工程**——补丁需要声明级可寻址定位与快照/回滚，
  属后续里程碑（见 §15.4）。

### 15.3 兼容性

- 桥接**不改变** DSL 语法、IR 语义与目标产物——因此**不是破坏性变更**。
- 新增的节点/边只影响**分析类工具**（impact / patch / 依赖图）的可用性。
- 图的 `NodeKind` / `EdgeKind` 取值集合**MUST** 与联邦侧保持同构（Conformance §13 要求）。

### 15.4 现状

- **M1–M4 已全部落地**（2026-11-05）：G-1（稳定标识）、G-2（依赖边）、G-3（来源追踪）、
  G-4（IR 版本）、G-7（无降级）**均已关闭**。
- 桥接落点：`crates/forma/src/dsl/lower.rs`（`lower_to_graph`）——
  `Program → UnifiedSemanticGraph` **单向**投影，不改动 `Program` 既有字段语义，
  因此**不改动任何 emitter 契约**。
- 效果：DSL 工程的 `forma graph` 输出稳定标识 + 依赖边 + 来源位置；
  `forma impact`（MCP）对 DSL 工程**不再被拒**（与联邦工程共用同一套图 API）。
- **`forma patch` 已支持 DSL 工程**（2026-11-05，RFC-0006 P2）：`dsl::patch` 提供
  `add-entity-field` / `set-entity-field-required` / `add-access-rule` / `add-table-page`
  四个主线操作，事务形状与联邦侧一致（快照 → 编辑 → `check_program` 重校验 → 重建）；
  定位基于 M1 的声明级 `file`/`line`。
- **P3 五类目标全部入图**（2026-11-05，IR 1.2.0）：表列（`Table` / `TableColumn`）、
  模型（`Entity` / `EntityField`）、表单（`UiForm` / `UiControl`）、图表（`Chart`：
  chart → api 操作 / 编码通道字段，页面 → chart）、迁移（`Migration` 链：表 → 迁移链
  `generates` 向前传播）。回归 `impact_closure_covers_all_five_target_kinds`。
  **P2 轨道 A 收尾（2026-11-08）**：补丁操作集已有 `add-entity` / `add-field-rule` /
  `set-field-rule` / `add-form-field`（改规则约束与加实体已覆盖），并新增
  **`set-chart-type` / `add-dashboard-chart`** 两个图表/看板节点细粒度操作（回归
  `tests/patch_visual_nodes.rs`，错误变换沿补丁事务整体回滚）。迁移节点是**派生节点**——
  没有可编辑的 DSL 声明，故显式决议「不设迁移节点补丁操作」而非造假接口；如需按迁移版本
  操作，应先在 DSL 引入对应声明并走 RFC 流程。
