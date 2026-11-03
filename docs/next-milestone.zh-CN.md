# 近期里程碑（Reliability & Verification）

> 完整路线图见 [roadmap.zh-CN.md](roadmap.zh-CN.md) · 验证体系见 [verification.zh-CN.md](verification.zh-CN.md)
> · 迁移安全见 [migration-safety.zh-CN.md](migration-safety.zh-CN.md) · Agent 工作流见 [agent-workflow.zh-CN.md](agent-workflow.zh-CN.md)

下面的八个任务来自一次外部评估（「AI-Native Specification Compiler」方向建议），经与 Forma 当前实现核对后做了状态校正。**先聚焦可信赖地生成，再扩展目标技术栈。**

| # | 任务 | 优先级 | 当前状态 | 说明 |
|---|---|---|---|---|
| F-001 | 定义 Unified IR 稳定契约（版本 / 节点 ID / 类型 / 来源映射 / 兼容策略） | P0 | 🔨 部分 | IR 已存在（`UnifiedSemanticGraph`，节点带 `NodeKind` / `EdgeKind`），但目前缺乏显式的 IR 版本与跨版本兼容策略 |
| F-002 | Provenance + 产物 Manifest（哈希 / 所有权 / 人工修改检测） | P0 | 🔨 部分 | 已有 `manifest.rs`（forma.yaml 清单）+ `@generated` 头 + `extension` 扩展点；逐文件内容哈希与所有权检测待补强 |
| F-003 | Semantic Impact Analysis（字段 / API / 规则 / 页面的依赖闭包） | P0 | ✅ 已完成 | `crates/forma/src/graph.rs` 的 `UnifiedSemanticGraph` + `forma_graph` / `forma_impact` |
| F-004 | 规则跨语言一致性测试（Rust / TS / Dart 共享语义向量） | P0 | 🔨 规划 | 规则三端同源编译已完成；缺共享测试向量与一致性断言 |
| F-005 | OpenAPI / JSON Schema 导入 | P0 | ✅ 部分 | `crates/forma/src/adapters/` 已有 openapi / jsonschema / sql / jsonforms / bpmn 适配器（导入 + 导出） |
| F-006 | Transactional Specification Patch（校验 / 预览 / 冲突检测 / 原子应用） | P0 | 🔨 部分 | `crates/forma/src/patch.rs` 已实现 AST 级语义补丁；事务式预览 / 冲突检测待补强 |
| F-007 | Agent Context / Plan / Verify 工作流 | P0 | 🔨 部分 | 上下文（graph）、验证（migrate verify / 产物编译）已具备；Plan / Report 待补 |
| F-008 | Generated Project Benchmark（真实编译 + Token / 可靠性对照） | P1 | 🔨 规划 | 待建立 100 个标准业务变更任务与 Token / 回归对照 |

## 已完成 / 已具备的能力（与评估预期一致）

- **语义依赖图与影响分析**（F-003）：`forma graph` 输出实体 / 字段 / API / 规则 / 页面的依赖闭包。
- **标准格式互通**（F-005）：OpenAPI 3.1、JSON Schema、SQL DDL、JSONForms、BPMN 的导入与导出适配器已落地（`crates/forma/src/adapters/`）。
- **多目标确定性编译**：MySQL DDL + 版本化迁移、Rust axum/sqlx、React TanStack/shadcn、Flutter、OpenAPI、Mermaid 图表。
- **迁移引擎**：不可变基线 + 升级/回滚配对 + 内容派生索引名 + 台账 + 重放模拟器 + `verify`（`crates/forma/src/emit/migrate.rs`）。
- **结构化诊断**：`Diagnostic { severity, code, ... }`，MCP 工具返回稳定错误码。
- **迁移风险分级**（本次新增）：`forma migrate risk`，见 [migration-safety.zh-CN.md](migration-safety.zh-CN.md)。
- **确定性案例数据**：`forma seed --verify`，非 AI 数据，byte 级一致（见 `docs/case-protocol.zh-CN.md`）。

## 建议开发顺序

```text
Phase 1 — 编译器可靠性（P0）
  F-003 ✅ 影响分析（已完成）
  F-001   IR 稳定契约（版本 + 兼容策略）
  F-002   Provenance + 产物 Manifest（内容哈希 + 所有权检测）
  F-006   事务式补丁（预览 + 冲突检测）
  F-004   规则跨语言一致性向量
  ── 本次交付：forma migrate risk（F-002 的迁移安全子集）──

Phase 2 — Agent 原生化（P0）
  F-007   Agent Context / Plan / Verify 闭环
  F-005   OpenAPI / JSON Schema 导入（已完成，纳入验收）

Phase 3 — 标准生态互通（P1）
  F-005   既有项目 导入 → IR → 导出 语义等价验收
  F-008   Generated Project Benchmark

Phase 4 — 目标技术扩展（P2，前序稳定后再做）
  PostgreSQL / SQLite / TypeScript-Hono / Python-FastAPI / Java-Spring / Go …
```

## 验收示例（F-007 闭环）

> 给订单增加优惠券功能，优惠券金额不能超过订单金额，且需要在后台展示优惠券使用记录。

Agent 应能在不读取全部业务源码的前提下：读取规格 → 定位订单实体 → 设计数据结构与规则 → 生成语义补丁 → 检查字段/类型/规则 → 分析迁移风险（`forma migrate risk`）→ 生成 Rust/React 产物 → 编译与测试 → 输出变更报告（影响 + 风险 + 证据）。

## 参考

- 验证体系：[verification.zh-CN.md](verification.zh-CN.md)
- 迁移安全：[migration-safety.zh-CN.md](migration-safety.zh-CN.md)
- Agent 工作流：[agent-workflow.zh-CN.md](agent-workflow.zh-CN.md)
- 完整路线图：[roadmap.zh-CN.md](roadmap.zh-CN.md)
