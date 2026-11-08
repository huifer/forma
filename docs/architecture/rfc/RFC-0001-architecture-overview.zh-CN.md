---
id: RFC-0001
title: Architecture Overview and Module Boundaries
status: Accepted
version: 1.1
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-08
domain: architecture
supersedes: []
superseded_by: []
depends_on: []
related_adrs: [ADR-0001]
related_specs: [spec:ir, spec:types]
breaking_change: false
---

# RFC-0001：架构总览与模块边界

## 1. Metadata

见 frontmatter。本 RFC 定义 Forma 的全局架构边界、依赖方向与治理规则；crate 级实现细节不在本文件范围，见伴侣文档 [architecture.zh-CN.md](../../architecture.zh-CN.md)。

## 2. Abstract

Forma 是一层 DSL 中间层：一套 DSL（`.fm`）描述前端、后端、数据库与规则，编译器（适配层）将其变为各平台的完整代码。本 RFC 规定系统的模块边界与依赖方向，使各子系统可独立演进，新增能力不必修改核心架构。

## 3. Motivation

### 3.1 Current Problem

功能增加后，架构边界容易模糊：IR 混入技术实现、规格泄漏目标概念、核心契约被随意修改。一旦核心被污染，所有生成产物都会受影响。

### 3.2 Why Existing Architecture Is Insufficient

`architecture.zh-CN.md` 已描述蓝图，但它是**实现伴侣**而非**治理契约**——缺少强制生命周期、稳定性分层、边界红线与变更规则。需要一份只定义边界的 RFC 来约束它。

### 3.3 Expected Outcomes

新增 Capability 不必改核心；新增 Adapter 不必改 IR；修改规格不破坏无关产物。

## 4. Goals

- 定义 Forma 的子系统边界与依赖方向。
- 明确 Tier 1 / Tier 2 / Tier 3 稳定性分层。
- 规定「什么变更改什么文件」（见 [README §8](../README.md) 演进规则表）。

## 5. Non-goals

- 不规定各 crate 的内部实现（属伴侣蓝图）。
- 不定义具体 IR 字段（属 RFC-0003 / Specification）。
- 不定义 Capability Manifest 字段细节（属 RFC-0004）。

## 6. Terminology

- **DSL / IR / Adapter / Runtime / Capability**：见 [README §0](../README.md)。
- **Tier 1/2/3**：见 [README §5](../README.md)。
- **Stable Identity**：实体 / 字段 / 关系的稳定标识符，跨版本不变。

## 7. Architectural Context

### 7.1 System Context

Forma 处于「AI 规格」与「多平台代码」之间：

```text
AI / Human 写 DSL（.fm，唯一事实来源）
        │
        ▼
Forma Compiler（parse → lower → IR → check → emit）
        │
        ├── IR（语义中间表示，与目标解耦）
        │
        ├── Adapters（db / backend / web / flutter / openapi / diagrams / visualization）
        │
        └── Runtime（Rust / React / Flutter / 服务端）
```

### 7.2 Dependencies

- 依赖 RFC-0003（Semantic Core）定义 IR 边界。
- 依赖 RFC-0004（Capability System）定义扩展机制。
- 依赖 RFC-0008（Security & Governance）定义权限与审查。

### 7.3 Boundaries

**依赖方向红线（不可违反）**：

```text
Tier 1（Stable Foundation）  ◀──  Tier 2（Extensible）  ◀──  Tier 3（Experimental）
      IR / Type / Identity           Capability / Adapter          新数据源 / 新 UI Runtime
      Dependency Graph              Runtime / Agent Protocol       新工作流执行器
      Compatibility Rules
```

- Semantic Core **不依赖**任何 Adapter / Runtime / Capability。
- Adapter **只消费** IR，不反向修改语义。
- Capability **不得修改** Semantic Core，除非发现新的通用语义概念、现有 IR 无法合理表达。
- DSL **不泄漏**目标概念（无 axum / React / ECharts 词汇）。

## 8. Design

### 8.1 Conceptual Model

系统由五类边界清晰的层组成：RFC（边界）/ Specification（语义）/ ADR（决策）/ Capability（功能）/ Conformance（兼容）。见 [README §0](../README.md)。

### 8.2 Architecture

核心子系统：

| 子系统 | 职责 | Tier |
|---|---|---|
| Compiler（parse/lower/check/emit） | DSL → IR → 代码 | T1/T2 |
| Visualization | 图表/看板编译（见 RFC-0010） | T2 |
| Verify（L1–L5） | 产物正确/可运行/安全 | T1 |
| Agent Protocol | 安全语义操作（graph/impact/patch） | T2 |
| Capability Registry | 能力注册/发现/验证 | T2 |

### 8.3 Data Model

Stable Identity 是跨版本稳定的标识符，由 IR 节点承载（`NodeKind` / `EdgeKind`），与源码位置（`span`）解耦。详见 RFC-0003 / `spec:ir`。

### 8.4 Interfaces and Contracts

- IR ↔ Adapter：纯函数投影（Adapter 是 IR → 目标代码的确定性函数）。
- Compiler ↔ Runtime：生成物带 `@generated` 头 + `extension` 扩展点；人工逻辑走扩展点，不被重编译覆盖。
- DSL ↔ IR：Lower 阶段单向转换，语义与语法解耦。

### 8.5 Execution Flow

`forma build`：解析 → Lower 到 IR → Check（类型/引用/规则/路由/权限）→ 影响分析（IR 节点 Hash → 受影响文件）→ 仅重生成受影响文件 → 产物校验。

### 8.6 Extension Points

- **Capability**（RFC-0004）：功能扩展主通道。
- **Adapter**：技术生态接入。
- **extension 扩展点**：生成物内的人工逻辑锚点。
- **nativeExtension**（可视化）：受控逃逸口，显式标记不可移植。

## 9. Compatibility

- **Backward**：IR 兼容性规则见 RFC-0003；破坏需 Migration 与兼容验证。
- **Forward**：未知扩展字段须保留、不得擅解释（见 RFC-0004 §IR 扩展策略）。
- **Deprecation**：进入 `Deprecated` 须提供迁移政策，不立即删除。

## 10. Security Considerations

- 信任边界：DSL 是用户内容，编译为代码——规则表达式与 patch 是语义操作，须经 Check 阶段验证。
- 权限：Capability 声明 `permissions`（如 `data.read`），运行时按声明约束。
- 详见 RFC-0008。

## 11. Performance and Reliability

- 确定性：同一 DSL → 同一产物（byte 级），全链路铁律。
- 增量：仅重编译受影响文件；失败单点不影响其余产物。

## 12. Alternatives Considered

- **单一巨型 RFC**：拒绝——项目越大越难维护，且每次改动牵动全局。
- **纯代码即文档**：拒绝——AI 无法从代码反推边界与决策历史。

## 13. Trade-offs

引入治理成本（frontmatter / 审查 / 分层），换取长期可扩容与语义统一。值得。

## 14. Migration Plan

既有 `architecture.zh-CN.md` 保留为伴侣蓝图；本 RFC 成为治理契约。既有可视化 RFC 迁入 `rfc/RFC-0010-visualization.zh-CN.md`（见 ADR-0001）。

## 15. Implementation Plan

- 阶段 1：治理总入口 + 模板 + 本 RFC + RFC-0004 + ADR-0001。（本交付）
- 阶段 2+：RFC-0002/0003/0005… 与对应 Specification / Conformance。

## 16. Conformance Requirements

> 核实日期 2026-11-08（Phase 1 收口）：有回归 / 实现证据才勾选；未覆盖项保留 `[ ]`
> 并登记 [gap-backlog](../roadmap/gap-backlog.zh-CN.md)「Phase 1 §16 未覆盖项」。

- [x] `forma docs validate` 对所有 RFC 通过（工具就绪后）。
  证据：CI `docs` job + `tests/docs_validate.rs`；当前 29 篇治理文档 0 error / 0 warning。
- [x] 依赖图无 Tier 反向边（见 §7.3）。
  证据（2026-11-08，G-1 收口）：`tests/docs_tier.rs` 解析 `forma docs graph --json`，按 Tier 映射
  断言 T1 文档不 `depends_on` T2 文档（当前 0 违规）；映射完整性由同文件第二项测试锁定。
- [x] 新增 Capability 的 PR 不修改 Semantic Core。
  证据：`tests/capability.rs::adding_a_capability_requires_no_core_change`（核心产物 byte 级不变）。

## 17. Acceptance Criteria

- 新 Capability 接入无需改核心架构；新 Adapter 接入无需改 IR；改规格不破坏无关产物（[README §10](../README.md) Phase 1 标准）。

## 18. Open Questions

- `forma docs` 自动化工具的具体实现归属（CLI crate 还是独立 bin）。
- 项目锁文件格式（`forma.yaml` 扩展 vs `.forma/lock.json`）。

## 19. References

- 伴侣蓝图：[architecture.zh-CN.md](../../architecture.zh-CN.md)
- 治理总入口：[README.md](../README.md)
- 审查清单：[REVIEW-CHECKLIST.zh-CN.md](../REVIEW-CHECKLIST.zh-CN.md)
- 验证体系：[verification.zh-CN.md](../../verification.zh-CN.md)

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版采纳，定义边界、分层与依赖红线 |
| 1.1 | 2026-11-08 | Phase 1 §16 验收核实：逐条对照回归测试与实现证据并勾选；未覆盖项保留 `[ ]` 并登记 [gap-backlog](../roadmap/gap-backlog.zh-CN.md)「Phase 1 §16 未覆盖项」；配套补齐生成后端 `/health` `/ready`（RFC-0005）与产物原子写 / 内容跳过（RFC-0007） |
