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
│   ├── RFC-0002-specification-system.zh-CN.md
│   ├── RFC-0003-semantic-core.zh-CN.md
│   ├── RFC-0004-capability-system.zh-CN.md
│   ├── RFC-0005-application-runtime.zh-CN.md
│   ├── RFC-0006-agent-engineering.zh-CN.md
│   ├── RFC-0007-build-and-artifact.zh-CN.md
│   ├── RFC-0008-security-and-governance.zh-CN.md
│   ├── RFC-0009-developer-experience-and-studio.zh-CN.md
│   └── RFC-0010-visualization.zh-CN.md   ← 由 docs/rfc 迁入的既有可视化 RFC
│
├── specifications/                ← Layer 2：Specification（语义契约）
│   ├── ir/ir-contract.zh-CN.md    ← 统一语义 IR 契约（Tier 1）
│   ├── types/  protocols/  manifests/  serialization/  compatibility/
│
├── adr/                           ← Layer 3：ADR（决策记录）
│   ├── accepted/ADR-0001-adopt-rfc-governance.zh-CN.md
│   ├── accepted/ADR-0002-canonical-ir-program-over-federation-graph.zh-CN.md
│   ├── accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md
│   ├── accepted/ADR-0004-server-authoritative-authorization.zh-CN.md
│   ├── accepted/ADR-0005-list-authorization-boundary.zh-CN.md
│   ├── proposed/  rejected/  deprecated/
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

### 4.1 现有 RFC 一览

| ID | 主题 | 状态 | 一句话职责 |
|---|---|---|---|
| [RFC-0001](rfc/RFC-0001-architecture-overview.zh-CN.md) | Architecture Overview | Accepted | 全局边界、依赖方向、Tier 分层 |
| [RFC-0002](rfc/RFC-0002-specification-system.zh-CN.md) | Specification System | Implemented | 规格领域划分、语法元规则、标准互通保真度 |
| [RFC-0003](rfc/RFC-0003-semantic-core.zh-CN.md) | Semantic Core | Implemented | IR 稳定契约、类型系统、Stable Identity、依赖图 |
| [RFC-0004](rfc/RFC-0004-capability-system.zh-CN.md) | Capability System | Implemented | Capability Manifest、注册/发现/验证、三层 IR |
| [RFC-0005](rfc/RFC-0005-application-runtime.zh-CN.md) | Application Runtime | Implemented | 运行时模块契约、配置分层、错误契约、产物独立性 |
| [RFC-0006](rfc/RFC-0006-agent-engineering.zh-CN.md) | Agent Engineering | Implemented | 任务型工具面、补丁事务、风险分级、人工审批门 |
| [RFC-0007](rfc/RFC-0007-build-and-artifact.zh-CN.md) | Build & Artifact | Implemented | 产物三分类归属、产物清单、冲突检测、增量与可复现构建 |
| [RFC-0008](rfc/RFC-0008-security-and-governance.zh-CN.md) | Security & Governance | Implemented | 权限四层模型、密钥与审计契约、验证门槛映射 |
| [RFC-0009](rfc/RFC-0009-developer-experience-and-studio.zh-CN.md) | Developer Experience | Implemented | CLI 契约、诊断统一结构、Studio 阶段与边界 |
| [RFC-0010](rfc/RFC-0010-visualization.zh-CN.md) | Visualization | Accepted | Chart IR、ECharts Adapter、React Runtime（领域范例） |

### 4.2 现有 ADR 一览

| ID | 主题 | 状态 |
|---|---|---|
| [ADR-0001](adr/accepted/ADR-0001-adopt-rfc-governance.zh-CN.md) | 采纳 RFC 治理与可扩展架构 v1.0 | Accepted |
| [ADR-0002](adr/accepted/ADR-0002-canonical-ir-program-over-federation-graph.zh-CN.md) | DSL 主线 `Program` 为规范语义表示，联邦语义图为导入视图 | Accepted |
| [ADR-0003](adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md) | 产物分三类归属，人工修改不被静默覆盖 | Accepted |
| [ADR-0004](adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md) | 权限服务端权威执行，前端显隐不构成安全边界 | Accepted |

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

## 9. 自动化治理

Forma 面向 AI，其 RFC 体系也应机器可读。`forma docs` 子命令族：

```text
forma docs validate            # ✅ 校验 frontmatter 完整、ID 不重复、状态合法、引用存在、无非法循环
forma docs lint                # ✅ 模板章节完整性（核心章节不得省略）+ N/A 标注规范
forma docs graph               # ✅ 渲染 RFC/ADR/Spec 依赖图（--json 输出机器可读）
forma docs check-compatibility [--base <git-ref>] # ✅ 生命周期 + Git 基线语义 diff 检测破坏性变更
forma docs check-dependencies  # ✅ 依赖引用存在性 + 自依赖 + 循环依赖
forma docs check-links         # ✅ 内部相对链接是否指向存在的文件
```

### 各子命令的检查项

| 命令 | 诊断码 | 级别 |
|---|---|---|
| `validate` | `docs-frontmatter-missing` | error |
| `validate` | `docs-id-duplicate` | error |
| `validate` | `docs-status-invalid` | error |
| `validate` | `docs-ref-missing`（核心引用） | error |
| `validate` | `docs-cycle` | error |
| `validate` | `docs-metadata-incomplete` | warning |
| `validate` | `docs-ref-missing`（`related_specs` 等） | warning |
| `validate` | `docs-acceptance-missing` / `docs-changelog-missing` | warning |
| `validate` | `docs-breakingchange-missing` | info |
| `lint` | `docs-lint-section-missing`（核心章节不得省略） | error |
| `lint` | `docs-lint-section-missing`（其余章节） | info |
| `lint` | `docs-lint-section-empty`（核心章节为空） | error |
| `lint` | `docs-lint-na-without-reason` | warning |
| `graph` | `docs-cycle` | error |
| `check-compatibility` | `docs-compat-breaking-without-migration` / `docs-compat-major-without-marker` / `docs-compat-spec-deleted` | error |
| `check-compatibility` | `docs-compat-breaking-candidate`（Git 基线检测到 MUST 规范性条款或带反引号标识的契约表行被删除/弱化，需人工确认） | warning |
| `check-compatibility` | `docs-compat-deprecated-without-successor` / `docs-compat-dep-on-deprecated` | warning |
| `check-dependencies` | `docs-ref-missing` / `docs-dep-self` / `docs-cycle` | error / warning |
| `check-links` | `docs-link-broken` | warning |

### 严重级别策略

- `depends_on` / `related_rfcs` / `related_adrs` 缺失 → **error**（核心文档图必须闭合）；
- `related_specs` / `supersedes` / `superseded_by` 缺失 → **warning**（spec 属分阶段交付，暴露缺口但不阻塞）；
- 元数据必填项按文档类型区分（RFC 要 version/authors/created/updated；ADR 要 date/deciders；Spec 要 id/title/status）。

- P1-10 自动检测是**保守检测器**：移除/弱化 MUST 条款或删除带反引号标识的契约表行 → warning candidate（需人工确认）；major version 增长但未声明 `breaking_change: true`、直接删除 spec → error。
- 本地默认比较工作树与 `HEAD`；CI 对 PR 比较 `origin/$GITHUB_BASE_REF`，对 push 比较 `HEAD^`。手动指定：`forma docs check-compatibility --base <git-ref> [dir]`。
- 实现：`crates/forma/src/docs.rs::detect_spec_compatibility_changes`；回归测试：`crates/forma/tests/docs_validate.rs`。
- 退出码：0 error → 0；存在 error → 1。
- CI：`.github/workflows/ci.yml` 的 `governance docs` job 强制执行 docs validate / lint / capability check / Git 基线兼容性检查。

> 核心章节（边界 / 兼容性 / 安全 / 验收 / 迁移）缺失或为空在 `lint` 中是 **error**，会阻断 CI；
> 不适用某章节时，按治理规范写明 `N/A` **及原因**即可通过（`docs-lint-na-without-reason` 守这条）。

### 机器可读输出

所有 `forma docs <action>` 支持 `--json`，输出结构固定，便于 CI 直接消费：

```json
{
  "action": "validate",
  "root": ".",
  "docs_checked": 21,
  "summary": { "error": 0, "warning": 0, "info": 0 },
  "exit_code": 0,
  "diagnostics": [ { "severity": "error", "code": "docs-cycle", "message": "…" } ]
}
```

> `forma docs graph --json` 输出的是依赖图（`nodes` / `edges`），与诊断报告结构不同。

---

## 9b. Capability Registry（已实现）

RFC-0004 的可扩容机制**已落地为代码**，不是纸面设计：

| 落点 | 位置 |
|---|---|
| Manifest 类型与注册表 | `crates/forma/src/capability.rs` |
| 工程声明 | `forma.yaml` 的 `capabilities:` 段（列表 / 映射带 pin 两种形式） |
| 项目自备能力 | `<project>/capabilities/*.yaml`——**新增能力不必改核心** |
| 构建期校验 | `crates/forma/src/build.rs::resolve_capabilities`（能力错误**阻断构建**） |
| CLI | `forma capability list \| check \| show <id>` |
| 回归测试 | `crates/forma/tests/capability.rs`（9 项，含「新增能力不改核心」端到端证明） |
| 模式定义 | [spec:manifests](specifications/manifests/capability-manifest.zh-CN.md) |

注册表职责：**发现 → 注册 → 依赖闭包 → 版本兼容 → 权限汇总**。
编译器只消费注册表的结论，**不认识任何具体能力**——这是「新增能力不改 Semantic Core」的实现方式。

---

## 10. 落地顺序与 Phase 1 完成标准

按以下顺序建设，避免一次性堆砌 60 个章节：

| 顺序 | 交付物 | 目的 | 状态 |
|---|---|---|---|
| 1 | RFC Governance（本文件） | 确定 RFC 管理规则 | ✅ 已交付 |
| 2 | Architecture Overview（RFC-0001） | 确定全局架构边界 | ✅ 已交付 |
| 3 | Core Specification（RFC-0003 + `spec:ir`） | 稳定 IR / Identity / Type System | ✅ 已交付并实现（IR 1.2.0 / M1–M4 / 桥接 / 影响闭包） |
| 4 | Capability Specification（RFC-0004） | 建立能力扩展机制 | ✅ 文档已交付 |
| 5 | Adapter Contract（RFC-0003 §8.4 + RFC-0007） | 建立技术适配契约 | ✅ 文档已交付 |
| 6 | Runtime Contract（RFC-0005） | 明确软件运行模型 | ✅ 文档已交付 |
| 7 | Build and Artifact（RFC-0007） | 可复现、增量编译 | ✅ 文档已交付 |
| 8 | Agent Protocol（RFC-0006） | 安全语义操作 | ✅ 文档已交付 |
| 9 | Specification System（RFC-0002） | 规格领域与标准互通契约 | ✅ 文档已交付 |
| 10 | Security & Governance（RFC-0008） | 权限、密钥、审计、验证门槛 | ✅ 文档已交付 |
| 11 | Developer Experience（RFC-0009） | CLI 契约、Studio | ✅ 文档已交付 |
| 12 | Conformance Suite | 兼容性测试 | ✅ 已建（[conformance/](conformance/README.md)：语义/适配器/运行时/兼容性四层矩阵，承诺 → 证明测试映射；当前 76 套件 / 494 测试全绿） |
| 13 | Reference Application | 用真实应用验证架构 | ✅ 已满足：`examples/school`（多版本迁移链 + 种子 + 三端产物）+ `examples/access-depth` / `cursor-pagination` + [case-protocol](../case-protocol.zh-CN.md) V1–V16 矩阵 + CI `e2e`（真 MySQL 8）/ `flutter` / `web` / `generated-backend` |

> 基础 RFC（0001–0009）状态已于 2026-11-08 逐份核实：0001 为 `Accepted`，
> 0002–0009 为 `Implemented`；RFC-0010 的 V0.1–V0.6 实现已分期落地，状态保持 `Accepted`
> 并以其 §13 分期与 §16 未决项作为版本化实现记录（不把单份 RFC 的 `Implemented`
> 误当成所有后续版本都已冻结）。0002 的 S4、0004 的 Deprecation 等未覆盖项在各 RFC §16 保留 `[ ]`。
> **文档齐备 ≠ 架构可扩容**——Phase 1 的三条完成标准靠实现与 Conformance 测试证明：
> 2026-11-08 已把 10 份 RFC 的 §16 验收项逐条对照回归：35 项未勾项中 **28 项补上证据并勾选**
> （RFC-0007 7/7、RFC-0004 §16-2、RFC-0002 §16-1、RFC-0001 §16-2、RFC-0005 §16-8 等），
> **7 项收口、2 项显式保留**（RFC-0004 §16-3「未知扩展」无适用对象——DSL 无 extensions 语法，
> 已按 fail-closed 锁定；RFC-0008 §16-3 由 [ADR-0005](adr/accepted/ADR-0005-list-authorization-boundary.zh-CN.md) 取代）。
> gap-backlog 的 **G-1…G-11 已全部收口**（实现 / 决议 / 显式保留），`docs lint` 0 warning / 0 info。
>
> **RFC §18 开放问题收口（2026-11-08 续）**：10 份 RFC 的 §18/§16 未决项逐条处置——
> ~20 项关闭（附实现证据）、3 项本阶段落地（**策略即测试**：权限向量跨端同源执行；
> 多租户默认值决议；docs 命令 ↔ REVIEW-CHECKLIST 映射）、~8 项显式保留（含 G-4
> Secret Provider、RFC-0007 产物签名、RFC-0003 表达式级依赖）。

**发布前一致性收口（2026-11-09）**：最近 49 个本地提交已完成 Studio V3 本地历史回放、
可视化 V0.4–V0.6（Dashboard 联动/下钻、AntV/G2Plot、legend/tooltip、size/label、GIS、
工业监控阈值线、K 线）与 CLI 端到端回归；README / roadmap / RFC / Capability Registry /
Conformance 叙事已同步到该状态。当前实测 `cargo test` 494 项 / 76 套件，
本地领先 `origin/main` 49 个提交。

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

**零核心改动路径**（推荐）：只需放一个 Manifest，编译器不需重新编译。

1. 在工程的 `capabilities/<id>.yaml` 写 Manifest（模式见 [spec:manifests](specifications/manifests/capability-manifest.zh-CN.md)）；
2. 在 `forma.yaml` 的 `capabilities:` 段启用它（可带版本 pin）；
3. `forma capability check` 验证依赖闭包与版本兼容；
4. `forma build` 时能力错误会**阻断构建**（未知能力 / 依赖缺失 / 版本不兼容 / pin 不匹配）。

**编译器内置能力路径**：若能力需要编译器内的实现（而非仅声明）：

5. 在 `crates/forma/src/capability.rs::builtin_manifests()` 加内置 Manifest；
6. 在 `capability/registry.md` 登记；
7. 提供 Conformance 测试（见 `conformance/`）；
8. **不得修改 Semantic Core**，除非发现新的通用语义概念、现有 IR 无法合理表达。

### 新增一份 Specification

1. 在 `specifications/<topic>/` 下创建（`ir/` `types/` `protocols/` `manifests/` `serialization/` `compatibility/`）；
2. frontmatter 用 `id: spec:<topic>`、`status`、`version`、`depends_on`、`breaking_change`；
3. 内容 **MUST** 只固化**已核实的事实**；未实现的条目标入「已知缺口」章节并声明**不受本规范约束**；
4. 被相关 RFC 以 `related_specs` 引用——引用悬空会被 `forma docs check-dependencies` 报出。

---

## 12. 相关文档

- 架构治理总入口（本文件）：[README.md](README.md)
- **架构缺口台账**（各 spec 缺口的统一排期）：[roadmap/gap-backlog.zh-CN.md](roadmap/gap-backlog.zh-CN.md)
- 架构蓝图 v0.3（详细实现伴侣）：[../architecture.zh-CN.md](../architecture.zh-CN.md)
- DSL 语言规范：[../dsl-spec.zh-CN.md](../dsl-spec.zh-CN.md)
- 验证体系 L1–L5：[../verification.zh-CN.md](../verification.zh-CN.md)
- 迁移安全：[../migration-safety.zh-CN.md](../migration-safety.zh-CN.md)
- Agent 工作流：[../agent-workflow.zh-CN.md](../agent-workflow.zh-CN.md)
- 历史归档：[../history/README.md](../history/README.md)
