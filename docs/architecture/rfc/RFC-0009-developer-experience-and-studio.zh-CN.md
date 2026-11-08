---
id: RFC-0009
title: Developer Experience and Studio
status: Implemented
version: 1.1
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-08
domain: dx
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0002]
related_adrs: [ADR-0001]
related_specs: [spec:manifests, spec:compatibility]
breaking_change: false
---

# RFC-0009：开发者体验与 Studio（Developer Experience and Studio）

## 1. Metadata

见 frontmatter。本 RFC 规定**面向人类开发者**的体验面：CLI 契约、诊断与错误消息、文档体系、Forma Studio（规格的可视化审阅面），以及「DSL 优先」工作流对日常开发效率的保障。

## 2. Abstract

Forma 的用户有两类：**AI Agent**（走 MCP 协议，RFC-0006）与**人类开发者**（走 CLI 与 Studio）。本 RFC 规定后者的体验面。

核心立场：

> **Studio 是规格与软件系统的可视化审阅面，不是低代码拖拽工具。**
> 写 `.fm` 必须能被看见；看懂一个陌生项目的现状，不应需要读几百行规格。

同时确立一条**CLI 契约红线**：

> **CLI 与 MCP 必须对同一工程给出一致的语义结论。**（当前存在反例，见 §3.1）

## 3. Motivation

### 3.1 Current Problem

**问题一：CLI 与 MCP 语义不一致（假绿）**（已实测复现）

CLI 的 `graph` / `validate` / `doctor` 三个子命令走**联邦** `engine::load_project`，而 DSL 工程没有 `specifications` 段 → 联邦加载器返回空图。对含真实语义错误的 DSL 工程：

```text
$ forma validate examples/school
✓ valid — 0 nodes, 0 diagnostics (0 errors)     # 退出码 0（假绿）

$ forma check examples/school
✗ 1 entities, 0 enums, 0 apis, 0 pages — 1 diagnostics (1 errors)
```

`forma check` 走 DSL 路径（正确），`forma validate` 走联邦路径（对 DSL 工程无意义）。这在 CI 中意味着：**一条带语义错误的规格可以「验证通过」**。

**问题二：命令面按实现组织，而非按任务组织**

当前 CLI 动词：`check` `lint` `seed` `diagrams` `diff` `init` `validate` `doctor` `graph` `build` `migrate` `patch` `mcp`。其中：

- `validate` / `doctor` / `graph` 是**联邦时代**的遗留（对 DSL 工程无效或空转）；
- `diff` 需要**两个目录**（`forma diff <dirA> <dirB>`），不是「对比当前工程与上一版本」；
- `lint` 有 13 条规则（naming 2 / completeness 3 / security 4 / performance 2 / convention 2），但其输出与 `check` 的诊断结构**不同**（lint 用自己的 `rule`/`entity`/`field`/`fix_hint` 字段，`check` 用统一的 `Diagnostic { severity, code, message, file, line }`）。

**问题三：Studio 尚未落地**，roadmap 规划为 M6（只读）/ M9（双向编辑）/ M11（协作）。

### 3.2 Why Existing Architecture Is Insufficient

[architecture.zh-CN.md §7](../../architecture.zh-CN.md) 已描述 Studio 阶段（M6/M9/M11），但那是路线图描述，不是契约。本 RFC 规定 Studio 的**数据来源**（必须与编译器同源）、**能力边界**（审阅面 vs 编辑面）、以及 CLI 的**一致性契约**——后者是安全相关（CI 假绿）。

### 3.3 Expected Outcomes

- CLI 与 MCP 语义一致；DSL 工程的 `validate` 不再假绿。
- 诊断结构统一（CLI / MCP / Studio 消费同一份）。
- Studio 以只读审阅为第一阶段，与编译器同源渲染。
- 新人仅通过 Studio 即可完整理解陌生项目现状。

## 4. Goals

- 定义 CLI 契约与命令面组织原则。
- 定义诊断（Diagnostic）的统一结构与呈现规范。
- 定义 Studio 的能力边界、数据来源与阶段划分。
- 定义文档体系与机器可读治理入口。

## 5. Non-goals

- 不定义 Studio 的具体技术栈与实现细节。
- 不定义 Agent 工具面（见 RFC-0006）。
- 不定义产物治理（见 RFC-0007）。
- 不做低代码拖拽编排平台（明确排除）。

## 6. Terminology

- **CLI Contract**：命令的行为、输出结构与退出码约定。
- **Diagnostic**：统一的机器可读诊断结构。
- **Studio**：规格的可视化审阅面。
- **Projection**：从 IR 渲染出的图形视图（与「产物」不同：投影不落盘为可执行代码）。

## 7. Architectural Context

### 7.1 System Context

```text
   人类开发者 ──▶ CLI（forma check/build/migrate/diff/lint）
        │                    │
        │                    ▼
        │             ┌─────────────┐
        │             │ Semantic    │
        │             │ Core (IR)   │
        │             └──────┬──────┘
        │                    │
        ▼                    ▼
   ┌──────────┐      ┌──────────────┐
   │  Studio  │◀─────│  IR 投影      │
   │ (审阅面)  │      │ ER/页面/API/ │
   └──────────┘      │ 规则/图表     │
                     └──────────────┘
```

### 7.2 Dependencies

- 依赖 RFC-0002（规格体系与领域划分）。
- 依赖 RFC-0001（Studio 消费 IR，不维护第二套模型）。

### 7.3 Boundaries

- **Studio 不维护第二套模型**——它消费 IR 渲染，与编译器同源。
- **Studio 的编辑能力写回 `.fm`**——等价于一次语义补丁，走同一校验管线（RFC-0006）。
- **CLI 与 MCP 共享实现**——不允许各自实现一套语义判定（当前 `mcp.rs` 与 `main.rs` 存在重复实现，是 §3.1 问题的根因）。
- Studio 属**独立产品模块**，不进入编译器核心。

## 8. Design

### 8.1 Conceptual Model

**命令面组织原则**（取代「按实现历史遗留」）：

| 原则 | 说明 |
|---|---|
| C1 按任务组织 | 每个命令回答一个开发者问题，而不是暴露一个内部函数 |
| C2 双模式显式 | 命令若支持 DSL / 联邦双模式，必须**按工程类型分派**，且对不适用的模式**明确报错**，不得静默空转 |
| C3 退出码有意义 | 0 = 成功；1 = 检查失败；2 = 用法错误。语义失败必须非零退出 |
| C4 共享实现 | CLI 与 MCP 调用**同一个**库函数（既有 `build.rs::run_build` 已做到，CLI 与 MCP 共享同一管线） |
| C5 结构化输出 | `--json` 输出机器可读结构，与人类输出同源 |

**CLI 命令面（目标形态）**：

| 命令 | 任务 | 现状 |
|---|---|---|
| `forma check` | 语法 + 语义检查（DSL 权威路径） | ✅ 权威 |
| `forma lint` | 风格与最佳实践检查 | ✅ 但输出结构与 `check` 不同 |
| `forma build` | 编译产物 | ✅ |
| `forma migrate {status\|history\|verify\|risk\|up\|down}` | 迁移链生命周期 | ✅ |
| `forma diff <dirA> <dirB>` | 两份规格的语义差异 | ✅ 但只能比目录 |
| `forma seed [--verify]` | 确定性案例数据 | ✅ |
| `forma diagrams` | 生成图（ER / 流程） | ✅ |
| `forma init` | 脚手架 | ✅ |
| `forma mcp` | 以 MCP 服务运行 | ✅ |
| `forma docs {validate\|lint\|graph}` | 治理文档校验 | 🔨 规划（README §9） |
| `forma validate` | **跨工程类型**的统一验证入口 | ⚠️ 当前对 DSL 工程假绿 |
| `forma doctor` | 联邦跨标准体检 | ⚠️ 当前对 DSL 工程空转 |
| `forma graph` | 语义图 / 依赖图 | ⚠️ 当前对 DSL 工程空转 |

> **修正方向**：`validate` / `doctor` / `graph` 三个命令按**工程类型分派**——DSL 工程走 `check_program` 路径，联邦工程走 `run_doctor` 路径；不适用时明确报错并提示替代命令。

### 8.2 Architecture

**诊断统一结构**（当前 `Diagnostic` 已具备大部分）：

```rust
pub struct Diagnostic {
    pub severity: String,     // error | warning | info
    pub code: String,         // 稳定错误码（dsl-* / viz-* / lint-*）
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub nodes: Option<Vec<String>>,  // 涉及的语义节点（StableId）
}
```

契约：

- 同一结构喂 CLI（人类）、MCP（AI）、Studio（图形）——**一份诊断，三种呈现**。
- `code` 是**稳定 API**（见 RFC-0003 §8.4）。
- 诊断必须带 `file` + `line`（RFC-0003 §14 M1）。
- `lint` 须改用同一结构（当前 lint 自有一套 `rule`/`entity`/`field`/`fix_hint` 字段——`fix_hint` 可保留为附加建议字段，但主体须统一）。

**Studio 的 IR 投影**（与编译器同源）：

| 视图 | 内容 | IR 来源 |
|---|---|---|
| 实体关系图（ER） | 实体 / 字段 / 关系 / 索引 | Data 领域节点 + 关系边 |
| 页面原型 | `table`/`form`/`detail`/`chart` 的真实组件渲染 | UI + Visualization 领域节点 |
| API 文档 | 操作 / 请求 / 响应 / 权限 | API 领域节点 + 导出视图 |
| 规则矩阵 | 字段校验 / 守卫 / 权限，按实体 × 操作 | Rules 领域节点 |
| 影响分析图 | 语义补丁的可视化评审 | 依赖图（RFC-0003） |
| 迁移链视图 | 版本链与破坏性操作 | 迁移台账 |

**Studio 阶段**（与 roadmap M6/M9/M11 对齐）：

| 阶段 | 能力 | 验收 |
|---|---|---|
| **V1 只读** | 项目浏览、规格编辑（只读高亮）、ER 图、页面原型、API 文档、图表预览、规则矩阵、诊断跳转 | 仅通过 Studio 即可完整理解一个陌生项目的现状 |
| **V2 双向编辑** | ER 图增删字段/关系 → 写回 `.fm`；页面原型调整列/控件 → 写回 `page` 块；补丁审阅 | 写回等价于一次语义补丁，走同一校验管线 |
| **V3 协作** | 多人实时协作、变更评审视图（DSL diff 的图形化）、历史版本回放 | 变更历史可视化且可回放 |

> **明确排除**：Studio 不做运行时拖拽编排、不做可视化编程。页面由 `page` 块的抽象原语表达，视觉自由度由目标生态（shadcn 等）承担。

### 8.3 Data Model

Studio 的模型**就是 IR**——不新建第二套模型。渲染层订阅 IR 变更事件（文件变更 → 重新 Lowering → 增量更新投影）。

投影缓存须失效于：IR 节点 Hash 变化、IR 版本变化、Adapter 版本变化。

### 8.4 Interfaces and Contracts

- **CLI → 核心**：CLI 只做参数解析与呈现；判定逻辑在库函数中，与 MCP 共享。
- **Studio → IR**：只读消费 IR；写回时经补丁协议（RFC-0006）。
- **诊断 → 呈现**：CLI 文本呈现、MCP JSON 呈现、Studio 图形呈现，三者来自同一 `Diagnostic` 列表。

### 8.5 Execution Flow

```text
开发者：forma check
  → load_dsl_program（DSL 工程）
  → check_program → Vec<Diagnostic>
  → 按 severity 分组呈现（error → warning → info）
  → 退出码：任一 error → 1
```

### 8.6 Extension Points

- **新 CLI 命令**：须按任务组织（C1），并与已有命令共享实现（C4）。
- **新 Studio 视图**：只允许从 IR 投影；新增视图须声明数据来源节点类型。
- **新诊断类别**：新增 `code` 前缀族（如 `lint-*`），须稳定化。
- **新 lint 规则**：属 `lint.rs` 的 `builtin_rules()`，当前 13 条；新增须给出 `fix_hint`。

### 8.7 文档体系与治理入口

文档按 [RFC-0001](../README.md) 的五层组织（RFC / Specification / ADR / Capability / Conformance）。开发者入口：

| 需求 | 入口 |
|---|---|
| 我要写规格 | [dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md) |
| 我要理解架构 | [architecture.zh-CN.md](../../architecture.zh-CN.md)（蓝图）+ `docs/architecture/README.md`（治理） |
| 我要查某个决策为什么这么定 | `docs/architecture/adr/` |
| 我要查某个能力怎么扩展 | `docs/architecture/capability/registry.md` + RFC-0004 |
| 我要确认我的实现符合契约 | `docs/architecture/conformance/` + 对应 RFC §16 |
| 我要查下一步做什么 | [roadmap.zh-CN.md](../../roadmap.zh-CN.md) + [next-milestone.zh-CN.md](../../next-milestone.zh-CN.md) |

`forma docs` 工具（规划中，README §9）把这些入口机器化：校验 frontmatter、ID 唯一、状态合法、依赖存在、链接有效、兼容性声明。

## 9. Compatibility

- **Backward**：现有 CLI 动词与参数保持可用；`validate` / `doctor` / `graph` 的**行为**修正为按工程类型分派——对联邦工程行为不变（向后兼容），对 DSL 工程从「空转假绿」变为「正确判定」（**行为修正**，须在 CHANGELOG 标注）。
- **Forward**：未知 CLI 参数明确报错，不静默忽略。
- **Version Compatibility**：CLI 与 MCP 的 `forma --version` 输出须包含编译器版本与 IR 版本（后者待 RFC-0003 M2）。
- **Migration Compatibility**：CLI 行为修正不涉及规格迁移；文档路径变更（如 `docs/rfc/` → `docs/architecture/rfc/`）须保留旧路径的重定向说明。
- **Deprecation Policy**：命令重命名须经一个 minor 版本的 warning 期并保留别名。

## 10. Security Considerations

- **CLI 不应成为绕过校验的通道**：任何写入规格的路径（CLI、patch、Studio 写回）必须经同一 `check_program`。
- **Studio 的本地服务边界**：Studio v1 是本地 Web 服务（`forma studio`），须绑定本地地址（不默认监听 `0.0.0.0`），避免规格内容被同网段读取。
- **诊断中的敏感信息**：诊断消息不得回显密钥值或完整敏感字段内容（RFC-0008 §10）。
- **文件写入**：Studio 写回 `.fm` 须走原子写与补丁事务（RFC-0006 / RFC-0007），避免半写文件。
- **脚本注入**：文档中的代码示例在 Studio 中渲染时须转义，避免把规格内容当 HTML 执行。

## 11. Performance and Reliability

- **增量渲染**：Studio 须按 IR 节点 Hash 增量更新投影，而非每次全量重渲染。
- **CLI 启动开销**：CLI 应避免为一个 `check` 加载完整 emitter 依赖；当前 `build.rs` 与 `check` 已分离路径，须保持。
- **诊断输出量**：大量错误时应聚合展示并支持过滤，避免淹没关键信息。
- **可靠性**：Studio 的写回与 CLI 的写盘使用同一原子写路径（RFC-0007 §8.3）。
- **一致性**：CLI 与 MCP 对同一工程必须产出**相同**的诊断集合——这是可测试的不变量（回归测试断言）。

## 12. Alternatives Considered

- **Studio 做低代码拖拽平台**：拒绝——会把「规格是事实来源」变成「图形是事实来源」，产生双份事实；且目标生态（shadcn 等）的组件自由度会被 Studio 的拖拽模型限制。
- **Studio 维护独立模型**：拒绝——必然与 IR 漂移；已有明确教训（见 §7.3）。
- **CLI 命令镜像内部函数**：拒绝——联邦时代的遗留（`validate`/`doctor`/`graph`）已证明会与主线脱节，产生假绿。
- **诊断分两套结构（check 用 Diagnostic，lint 用自有字段）**：拒绝——AI 与人类看到的问题集合不一致，工具无法统一消费。
- **保留 `forma diff <dirA> <dirB>` 之外再增「与上一版本比」的独立命令**：拒绝——同一语义两套入口；应由 `forma diff` 内部支持「当前 vs 台账上一版本」模式。

## 13. Trade-offs

- 修正 `validate` / `doctor` / `graph` 的 DSL 工程行为是**行为变更**（从假绿变为正确报错），可能让既有 CI 突然失败——这是**期望的**（暴露真实问题），但须在 CHANGELOG 显著标注。
- Studio 推迟到 M6，先做 CLI 修正——因为假绿是当下就在伤害 CI 的问题，成本极低而收益立即。
- 统一诊断结构需要改造 `lint.rs` 输出——一次性成本。

## 14. Migration Plan

| 步 | 内容 | 完成判据 |
|---|---|---|
| **D1** | `validate` / `doctor` / `graph` 按工程类型分派；DSL 工程走 `check_program` | 含语义错误的 DSL 工程 `forma validate` 非零退出 |
| **D2** | `lint` 改用统一 `Diagnostic` 结构（保留 `fix_hint` 为附加字段） | 两种检查的输出结构一致 |
| **D3** | `forma diff` 增加「当前 vs 台账上一版本」模式 | 无需准备第二个目录即可做版本差异 |
| **D4** | `forma --version` 输出编译器 + IR 版本 | 版本信息完整 |
| **D5** | 实现 `forma docs validate`（frontmatter / ID / 状态 / 依赖 / 链接） | 治理文档机检通过 |
| **D6** | Studio v1（只读投影） | 仅通过 Studio 可理解陌生项目现状 |

> D1 是**立即可做且安全相关**的一步；D5 与 RFC-0003/0006/0007 的文档产出配套。

## 15. Implementation Plan

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | D1 + D2（CLI 一致性 + 诊断统一） | 无 |
| 阶段 2 | D3 + D4（diff 模式 + 版本输出） | RFC-0003 M2 |
| 阶段 3 | D5（`forma docs` 校验） | 各 RFC 文档稳定 |
| 阶段 4 | D6（Studio v1） | RFC-0003 M3（依赖图投影） |

## 16. Conformance Requirements

> 核实日期 2026-11-08（P3/C-1）：逐条对照实现与回归，**有证据才勾选**。

- [x] `forma check` 与 `forma validate` 对同一 DSL 工程产出一致结论与退出码。
  证据：`engine::check_project` 单一分发点（CLI 与 MCP 共用）；`tests/dsl_validate.rs`。
- [x] 含 `error` 诊断的 DSL 工程，`forma validate` 退出码非 0。
  证据：`tests/dsl_validate.rs::dsl_project_with_semantic_error_is_reported`（P0-4 假绿修复）。
- [x] `lint` 与 `check` 输出使用同一 `Diagnostic` 结构。
  证据：`crates/forma/src/validate.rs` 的 `Diagnostic`，lint / check / docs 全线复用。
- [x] CLI 与 MCP 对同一工程产出相同诊断集合（回归断言）。
  证据：`tests/dsl_validate.rs::cli_and_mcp_agree_on_dsl_errors`。
- [x] `forma --version` 包含编译器版本与 IR 版本。
  证据：本次修复（v0.2 联邦时代字符串 → `forma <pkg> (IR <IR_VERSION>, …)`）；`tests/cli_version.rs`。
- [x] `forma docs validate` 对全部治理文档通过（frontmatter 完整、ID 唯一、状态合法、依赖存在、链接有效）。
  证据：CI `docs` job + `tests/docs_validate.rs`；`docs check-links` 亦纳入本地验证门。
- [x] Studio 的任一视图均可追溯到 IR 节点（无独立模型）。
  证据：`crates/forma/src/studio.rs` 的视图数据由 `load_program` 派生，`/api/graph` 直接输出
  `engine::dsl_graph(&program)`（DSL→联邦语义图桥接）；Studio 不维护第二套业务模型。

## 17. Acceptance Criteria

1. 含语义错误的 DSL 工程：`forma validate` 退出码非 0，错误含 `file` + `line`，与 `forma check` 结论一致。
   ✅ `tests/dsl_validate.rs`（声明级 `Origin` 注入 `file`/`line`）。
2. 联邦工程：`forma validate` / `doctor` / `graph` 行为不变（回归通过）。
   ✅ `tests/dsl_validate.rs::federal_project_still_takes_federal_path`。
3. `forma docs validate` 能检出：重复 ID、缺失 frontmatter 字段、悬空 `depends_on`、失效相对链接。
   ✅ `tests/docs_validate.rs`（14 项，含「仓库自身治理文档零 error」防线）；
   失效链接由 `forma docs check-links` 检出（本次 P3 修复了 `conformance/README.md` 的存量断链）。
4. `forma lint` 与 `forma check` 的 JSON 输出可用同一解析器消费。
   ✅ 同形 `Diagnostic`（`severity` / `code` / `message` / `file` / `line`）。

## 18. Open Questions

- Studio 的本地服务是否需要鉴权（即使仅绑定 localhost）？多人场景下 V3 协作需要。
- `forma diff` 的「当前 vs 上一版本」应基于迁移台账还是独立的版本快照（当前有 `.forma/ir-snapshot.json` 与 `migrations.json` 两条线）？
- `forma docs` 的规则集是否应与 [REVIEW-CHECKLIST.zh-CN.md](../REVIEW-CHECKLIST.zh-CN.md) 逐条对应？
- 诊断的 i18n：CLI 输出是否需要多语言（当前诊断消息为英文，规格内 message 支持 zh/en）？

## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览与 Studio 定位：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 规格系统：[RFC-0002-specification-system.zh-CN.md](RFC-0002-specification-system.zh-CN.md)
- Agent 协议（CLI/MCP 共享实现）：[RFC-0006-agent-engineering.zh-CN.md](RFC-0006-agent-engineering.zh-CN.md)
- 产物治理（原子写）：[RFC-0007-build-and-artifact.zh-CN.md](RFC-0007-build-and-artifact.zh-CN.md)
- 审查清单：[REVIEW-CHECKLIST.zh-CN.md](../REVIEW-CHECKLIST.zh-CN.md)
- 架构蓝图 §7（Studio）：[architecture.zh-CN.md](../../architecture.zh-CN.md)
- 路线图（Studio M6/M9/M11）：[roadmap.zh-CN.md](../../roadmap.zh-CN.md)
- 实现位置：`crates/forma/src/main.rs`、`crates/forma/src/mcp.rs`、`crates/forma/src/lint.rs`、`crates/forma/src/validate.rs`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义 CLI 契约与命令面组织原则、诊断统一结构、Studio 阶段与能力边界、`forma docs` 治理校验 |
| 1.1 | 2026-11-08 | P3/C-1 验收核实：§16 七项逐条附实现证据并勾选；修复 `--version`（补 IR 版本、移除 v0.2 联邦字符串，`tests/cli_version.rs`）；状态升为 Implemented |
