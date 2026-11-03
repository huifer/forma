# Forma 架构治理体系（RFC Governance & Extensibility Architecture v1.0）

> 本文档是 Forma 架构治理的**总入口**。它定义 Forma 如何分离「边界 / 契约 / 决策 / 能力 / 兼容」，
> 以及任何 Agent 或开发者新增 RFC、ADR、Capability、Specification 时必须遵守的规则。
> 它先于后续所有功能 RFC 落地——否则 Forma 会在功能变多的同时逐渐失去统一的语义与架构边界。

---

## 0. 核心立场：五者必须分离

Forma 的架构知识必须由五个**职责分离**的层承载，任何一层都不允许越界吞掉另一层：

| 层 | 职责 | 回答的问题 | 物理位置 |
|---|---|---|---|
| **RFC** | 定义**边界与契约**：模块边界、依赖方向、扩展机制、治理规则 | 系统应如何组织？ | `docs/architecture/rfc/` |
| **Specification** | 定义**语义**：数据结构、语法、协议、类型、接口、序列化 | 系统具体如何表达？ | `docs/architecture/specifications/` |
| **ADR** | 记录**决策**：选型理由、被否决方案、决策影响 | 为什么选这种设计？ | `docs/architecture/adr/` |
| **Capability** | 扩展**功能**：可注册、可发现、可验证、可替换的能力模块 | 如何不改动核心而增加能力？ | `docs/architecture/capability/` |
| **Conformance** | 保证**兼容**：语义测试、契约测试、参考实现、验收标准 | 如何证明实现符合设计？ | `docs/architecture/conformance/` |

边界规则（第一条必须写入 Forma 治理规范的铁律）：

- **Architecture RFC 不描述每个 crate 的实现细节。**
- **Domain RFC 不重复定义整个系统。**
- **Specification 不依赖某个具体实现。**
- **ADR 不退化成普通开发任务列表。**
- **测试规范必须能验证 RFC 与 Specification 中的承诺。**

---

## 1. 五层文档体系

### 1.1 Layer 1 — Architecture RFC

定义系统边界、架构原则、核心模块、扩展机制与治理规则。分为**基础 RFC（RFC-0001…0009）**与**领域 RFC（RFC-0010+）**。
基础 RFC 只定义模块边界与依赖规则；领域 RFC 负责具体演进。

### 1.2 Layer 2 — Specification

定义具体的数据结构、语法、协议、类型、接口与语义。不依赖具体实现，机器可读优先。
按主题分目录：`ir/`（中间表示）、`types/`（类型系统）、`protocols/`（协议）、`manifests/`（清单格式）、
`serialization/`（序列化规则）、`compatibility/`（兼容矩阵）。

### 1.3 Layer 3 — ADR

记录具体技术决策、选型理由、被否决的方案与决策影响。分状态目录：
`accepted/`（已采纳）、`proposed/`（提案中）、`rejected/`（已否决）、`deprecated/`（已废弃）。

### 1.4 Layer 4 — Capability

承载可扩展功能。每个 Capability 有统一 Manifest（见 RFC-0004），可注册、可发现、可验证、可替换。
按类别分目录：`visualization/`、`data/`、`ui/`、`workflow/`、`integrations/`。

### 1.5 Layer 5 — Conformance

定义兼容性测试、语义测试、契约测试、参考实现与验收标准。按对象分目录：
`semantic/`（语义）、`adapter/`（适配层契约）、`runtime/`（运行环境）、`compatibility/`（兼容矩阵）、`fixtures/`（测试数据）。

---

## 2. 目录结构

```text
docs/architecture/
├── README.md                      ← 本文件（治理总入口）
├── REVIEW-CHECKLIST.zh-CN.md      ← RFC 架构审查清单
│
├── rfc/                           ← Layer 1：RFC（基础 0001–0009，领域 0010+）
│   ├── TEMPLATE.md                ← 标准 RFC 模板（强制）
│   ├── RFC-0001-architecture-overview.zh-CN.md
│   ├── RFC-0004-capability-system.zh-CN.md
│   └── RFC-0010-visualization.zh-CN.md   ← 由 docs/rfc 迁入的既有可视化 RFC
│
├── specifications/                ← Layer 2：Specification（语义契约）
│   ├── ir/  types/  protocols/  manifests/  serialization/  compatibility/
│
├── adr/                           ← Layer 3：ADR（决策记录）
│   ├── accepted/  proposed/  rejected/  deprecated/
│   └── TEMPLATE.md
│
├── capability/                    ← Layer 4：Capability（功能扩展）
│   ├── registry.md                ← Capability 注册表（索引）
│   ├── visualization/  data/  ui/  workflow/  integrations/
│
├── conformance/                   ← Layer 5：Conformance（兼容保证）
│   ├── semantic/  adapter/  runtime/  compatibility/  fixtures/
│
└── roadmap/                       ← 演进节奏
    ├── milestones.md  implementation-status.md  migration-plan.md
```

> 既有 `docs/architecture.zh-CN.md`（架构蓝图 v0.3）继续作为**详细蓝图伴侣**存在，承载 crate 级实现细节；
> 新的 `RFC-0001` 只定义边界与依赖规则，不与其重复。历史材料归档于 `docs/history/`。

---

## 3. RFC 生命周期

每一份 RFC 必须拥有明确状态。状态机：

```text
Proposed ──▶ In Review ──▶ Accepted ──▶ Implemented ──▶ Stable
                                         │
                                         └──▶ Deprecated ──▶ (Superseded / Rejected)
```

| 状态 | 含义 |
|---|---|
| **Proposed** | 已提交提案，描述问题、目标与初步方案 |
| **In Review** | 进行架构审查（兼容性、依赖、替代方案、风险） |
| **Accepted** | 设计已批准，可进入实施（**≠ 已实现**） |
| **Implemented** | 已有实现，并通过对应 Conformance Tests（**≠ 已稳定**） |
| **Stable** | 接口、语义、兼容性承诺已明确 |
| **Deprecated** | 不再推荐，但按迁移政策提供兼容支持 |
| **Superseded / Rejected** | 被新 RFC 替代，或明确否决 |

**关键区分（必须写入治理规范）**：`Accepted ≠ Implemented ≠ Stable`。批准不代表实现，实现不代表稳定。

每个 RFC 文件头部统一使用 YAML frontmatter（见 `rfc/TEMPLATE.md`）：

```yaml
---
id: RFC-0003
title: Semantic Core Architecture
status: Proposed
version: 1.0
authors: []
created: 2026-11-04
updated: 2026-11-04
domain: semantic-core
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0002]
related_adrs: []
related_specs: []
breaking_change: false
---
```

> `created` / `updated` 由文档治理流程（如 `forma docs` 或 PR 审查）管理，不应由 AI 随意改写。

---

## 4. 编号与命名约定

- **基础 RFC（RFC-0001…0009）**：全局架构与治理契约，修改需经正式 RFC 流程。
  - `RFC-0001` Architecture Overview · `RFC-0002` Specification System ·
    `RFC-0003` Semantic Core · `RFC-0004` Capability System · `RFC-0005` Application Runtime ·
    `RFC-0006` Agent Engineering · `RFC-0007` Build & Artifact · `RFC-0008` Security & Governance ·
    `RFC-0009` Developer Experience
- **领域 RFC（RFC-0010+）**：具体能力 / 子系统演进（如可视化、数据库适配、Agent 协议）。
- **文件名**：`RFC-XXXX-kebab-title.zh-CN.md`（英文编号 + 英文短名 + 语言后缀，与仓库现有 `.zh-CN.md` 约定一致）。
- **状态流转**通过修改 frontmatter 的 `status` 字段表达，不靠文件名。

---

## 5. 架构稳定性分层（Tiering）

引入三层稳定性，防止实验污染基础：

| Tier | 名称 | 包含 | 变更规则 |
|---|---|---|---|
| **Tier 1** | Stable Foundation | Core IR Contract、Type System、Stable Identity、Dependency Graph、Specification Package、Compatibility Rules | 修改须经正式 RFC + 迁移设计 + 兼容验证 |
| **Tier 2** | Extensible Architecture | Capability、Adapter、Runtime、Plugin、Agent Protocol、Data Platform | 允许独立扩展，但须遵循 Tier 1 契约 |
| **Tier 3** | Experimental Ecosystem | 新型可视化、新 Agent 工作模式、新 UI Runtime、新数据源、新工作流执行器 | 允许快速试验，不得污染 Tier 1 |

**原则**：Tier 不是永久身份。实验能力可升级为稳定能力；稳定能力也可经迁移后废弃。

---

## 6. 版本体系

Forma 至少存在五类版本，**不可混为一谈**：

| 版本类型 | 管理对象 |
|---|---|
| Specification Version | 规格格式 |
| IR Version | 语义中间表示 |
| Capability Version | 能力定义 |
| Adapter Version | 技术适配器 |
| Runtime Version | 执行环境 |

另含 Project Version 与 Compiler Version，但它们不混入上述五类。

每个项目应提供锁文件（`forma.yaml` 或 `.forma/lock.json`）记录：`compiler` / `ir` / `runtime` 版本，
已启用的 `capabilities` 与 `adapters` 版本。兼容判定**必须**通过契约测试与语义规则，而非仅凭版本号。

---

## 7. 架构审查机制

每次增加新功能，先回答（而非直接问「要不要支持」）：

1. 它属于 Core 还是 Capability？
2. 能否复用已有标准？
3. 是否需要新增 IR？
4. 是否需要新增 Adapter？
5. 是否需要 Runtime 支持？
6. 是否影响已有语义？
7. 是否破坏兼容性？
8. 是否需要新权限？
9. 是否有自动化测试？
10. 能否独立安装 / 替换 / 升级？

正式审查清单见 [REVIEW-CHECKLIST.zh-CN.md](REVIEW-CHECKLIST.zh-CN.md)。

---

## 8. 演进规则（变更类型 → 应修改什么）

| 变更类型 | 应修改 |
|---|---|
| 新增图表类型 | Visualization Capability |
| 新增图表渲染引擎 | Adapter RFC |
| 新增数据库 | Data Capability |
| 新增 API 标准 | Specification RFC |
| 新增通用语义类型 | Semantic Core RFC |
| 新增 IR 字段 | IR Specification |
| 修改 IR 语义 | IR RFC + Migration |
| 新增 Agent 工具 | Agent Protocol RFC |
| 修改编译策略 | Compiler RFC |
| 更换 UI 框架 | UI Runtime RFC |
| 新增部署平台 | Deployment Capability |
| 新增插件权限 | Security RFC |
| 修改版本兼容规则 | Compatibility RFC |

目标：**新增 Capability 不必改核心架构；新增 Adapter 不必改 IR；修改规格不破坏无关生成产物。**

---

## 9. 自动化治理（规划）

Forma 面向 AI，其 RFC 体系也应机器可读。规划 `forma docs` 子命令：

```text
forma docs validate            # 校验 frontmatter 完整、ID 不重复、状态合法
forma docs lint               # 模板合规性、章节完整、必填项（安全/兼容/边界/验收/迁移）无缺
forma docs graph              # 生成 RFC/ADR/Spec 依赖图，检测非法循环依赖
forma docs check-compatibility # 变更是否声明 breaking_change
forma docs check-dependencies  # depends_on / related_specs 引用存在
forma docs check-links         # 内部链接有效
```

检查项：RFC ID 唯一、Metadata 完整、Status 合法、Dependencies 引用存在、无非法循环、
ADR/Spec 引用有效、兼容性变更有声明、验收标准明确、重要变更有 Change Log。

> 在工具落地前，本 README 与 `REVIEW-CHECKLIST.zh-CN.md` 作为人工 + Agent 审查依据。

---

## 10. 落地顺序与 Phase 1 完成标准

按以下顺序建设，避免一次性堆砌 60 个章节：

| 顺序 | 交付物 | 目的 |
|---|---|---|
| 1 | RFC Governance（本文件） | 确定 RFC 管理规则 |
| 2 | Architecture Overview（RFC-0001） | 确定全局架构边界 |
| 3 | Core Specification | 稳定 IR / Identity / Type System |
| 4 | Capability Specification（RFC-0004） | 建立能力扩展机制 |
| 5 | Adapter Contract | 建立技术适配契约 |
| 6 | Runtime Contract | 明确软件运行模型 |
| 7 | Build and Artifact | 可复现、增量编译 |
| 8 | Agent Protocol | 安全语义操作 |
| 9 | Conformance Suite | 兼容性测试 |
| 10 | Reference Application | 用真实应用验证架构 |

**Phase 1 完成标准不是文档数量，而是**：

1. 新加入一个 Capability 时，不需要修改核心架构；
2. 新加入一个 Adapter 时，不需要修改 IR；
3. 修改已有规格时，不会破坏无关的生成产物。

这三点才是 Forma 真正具备扩容能力的标志。

---

## 11. 快速上手

### 新增一份 RFC

1. 复制 `rfc/TEMPLATE.md` → `rfc/RFC-00XX-kebab-title.zh-CN.md`；
2. 填 frontmatter（`status: Proposed`、`depends_on`、`domain`）；
3. 按模板 20 节撰写，不适用章节写明 `N/A` 及原因；
4. **安全 / 兼容性 / 边界 / 验收 / 迁移** 在相关场景下不可省略；
5. 跑 `forma docs validate`（工具就绪后）或人工对照 [REVIEW-CHECKLIST.zh-CN.md](REVIEW-CHECKLIST.zh-CN.md)；
6. 提交 PR，状态进入 `In Review`。

### 新增一份 ADR

1. 复制 `adr/TEMPLATE.md` → `adr/accepted/ADR-00XX-kebab-title.zh-CN.md`；
2. 记录 Context / Decision / Consequences / Alternatives；
3. 在相关 RFC 的 `related_adrs` 中引用。

### 新增一个 Capability

1. 在 `capability/<category>/` 下写 Manifest（见 RFC-0004）；
2. 在 `capability/registry.md` 登记；
3. 提供 Conformance 测试（见 `conformance/`）；
4. **不得修改 Semantic Core**，除非发现新的通用语义概念、现有 IR 无法合理表达。

---

## 12. 相关文档

- 架构蓝图 v0.3（详细实现伴侣）：[../architecture.zh-CN.md](../architecture.zh-CN.md)
- DSL 语言规范：[../dsl-spec.zh-CN.md](../dsl-spec.zh-CN.md)
- 验证体系 L1–L5：[../verification.zh-CN.md](../verification.zh-CN.md)
- 迁移安全：[../migration-safety.zh-CN.md](../migration-safety.zh-CN.md)
- Agent 工作流：[../agent-workflow.zh-CN.md](../agent-workflow.zh-CN.md)
- 历史归档：[../history/README.md](../history/README.md)
