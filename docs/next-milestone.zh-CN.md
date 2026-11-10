# 近期里程碑（Reliability & Verification）

> **P3 治理收尾与发布就绪已完成（2026-11-08）**：RFC-0002 / RFC-0009 升为 Implemented、
> S2 机读损失报告 + S3 往返测试补齐、CI 新增生成后端编译门、Conformance 证据矩阵核对、
> 发布叙事同步。详见 [P3 规划](architecture/roadmap/p3-plan.zh-CN.md)。

> **发布前一致性收口（2026-11-09）**：P3 之后新增的 Studio V3 本地历史回放、
> 可视化 V0.4–V0.6（Dashboard 联动/下钻、AntV/G2Plot、legend/tooltip、size/label、GIS、
> 工业监控阈值线、K 线）与 CLI 端到端回归已提交但尚未同步到治理叙事。
> 本阶段只校准文档状态，不新增 target、不改 emitter 契约。
> 当前实测：`cargo test` 565 项 / 93 套件（版本以 commit + CHANGELOG 追踪，不使用 tag）。

> **全面收口（2026-11-09）**：治理层 5 项 RFC 未勾选项全部关闭（RFC-0003 §18 表达式级依赖、
> RFC-0004 §16-3 未知 IR 扩展、RFC-0005 §16-4 Secret Provider、RFC-0008 §16-3 由
> ADR-0005 取代）+ spec:ir §13 五项验收全勾；新增 IR 扩展语法、Secret Provider file
> 实现、Next.js 关系选择器、Vue target 功能对齐（列表/表单/详情 + Pinia）；修复
> IR 桥接前向引用漏边与 Web 编辑页关系初始值丢失两个真实缺陷。详见 CHANGELOG。

> **P2 轨道 A 已全面收尾（2026-11-08）**：F-001…F-008 之后，P2 的 8 项余项
> （多层关系遍历 / GraphQL 逆向关系 / 多租户管理 UI / 产物清单来源规格与锚点 /
> 补丁图表看板操作 / SQL·Prisma 逆向导入与迁移链导出 / 字段级显隐与 requestId 核实）
> 全部关闭；目标扩栈（轨道 B）在 2026-11-08 曾按当时决策撤销，**2026-11-09 用户改变决策，
> 重新排期并全部实现**（见 [roadmap §2](roadmap.zh-CN.md) 与 `CHANGELOG.md` `[Unreleased]`）。
> [P2 规划](architecture/roadmap/p2-plan.zh-CN.md) 保留历史记录并已加「决策变更」注记。

> 完整路线图见 [roadmap.zh-CN.md](roadmap.zh-CN.md) · 验证体系见 [verification.zh-CN.md](verification.zh-CN.md)
> · 迁移安全见 [migration-safety.zh-CN.md](migration-safety.zh-CN.md) · Agent 工作流见 [agent-workflow.zh-CN.md](agent-workflow.zh-CN.md)

下面的八个任务来自一次外部评估（「AI-Native Specification Compiler」方向建议），经与 Forma 当前实现核对后做了状态校正。**先聚焦可信赖地生成，再扩展目标技术栈。**

| # | 任务 | 优先级 | 当前状态 | 说明 |
|---|---|---|---|---|
| F-001 | 定义 Unified IR 稳定契约（版本 / 节点 ID / 类型 / 来源映射 / 兼容策略） | P0 | ✅ 已完成 | IR 版本（`IR_VERSION` + `Program.ir_version`）、稳定标识（DSL 与联邦**同一套** `forma://` URI 构造函数）、类型单一权威清单（`BUILTIN_TYPES` + 四目标映射一致性回归）、来源映射（声明级 `file`/`line`）、兼容策略（弃用窗口 ≥ 2 个次版本且 ≥ 6 个月，见 spec:compatibility §10）全部落地；DSL → 语义图**单向桥接**（`dsl::lower`）使两条路径语义能力合流（spec:ir §15 M1–M4） |
| F-002 | Provenance + 产物 Manifest（哈希 / 所有权 / 人工修改检测） | P0 | ✅ 已完成 | **产物清单落地（2026-11-05，ADR-0003 实现版）**：`.forma/artifacts.json` 记录逐文件哈希 / 归属（Generated/Adopted）/ 生成器；构建前快照受保护产物、构建后恢复——**人工修改不被静默覆盖**（`artifact-modified` 警告 + 保留人工版本）；裁决旗标 `--adopt`（登记 Adopted，永不覆盖）/ `--overwrite`（显式覆盖 + 审计）/ `--strict`（冲突提升 error）。**余项已清零（2026-11-08，轨道 A-6）**：`ArtifactEntry.source`（生成器版本 / IR 版本 / 输入指纹 / 依赖）+ `Ownership::Extension` 锚点（`artifact-extension-preserved`，`--overwrite` 也不得覆盖）；回归 `tests/artifact_ownership.rs` |
| F-003 | Semantic Impact Analysis（字段 / API / 规则 / 页面的依赖闭包） | P0 | ✅ 已完成 | `crates/forma/src/graph.rs` 的 `UnifiedSemanticGraph` + `forma_graph` / `forma_impact` |
| F-004 | 规则跨语言一致性测试（Rust / TS / Dart 共享语义向量） | P0 | ✅ 已完成 | **行为向量从 DSL 派生（2026-11-05）**：`validation_vectors.rs` 从规则约束（min/max/required）派生边界用例，生成两端行为测试——`validators.test.ts`（vitest，**Web 测试运行器补齐**）与 `rule_vector_test.dart`（flutter_test），由同一派生函数产出（同源保证）；CI 执行 `npx vitest run` + `flutter test`；Rust 侧保留三端字面量一致性断言（`tests/rule_vectors.rs`） |
| F-005 | OpenAPI / JSON Schema 导入 | P0 | ✅ 已完成 | `adapters/` 已有 openapi / jsonschema / sql / jsonforms / bpmn 适配器（导入 + 导出）；**`forma import openapi <spec> [--out]` 逆向生成 `.fm` 源文件**（components.schemas → 实体、paths → api、类型映射 + @id 主键保证，生成物通过 check_program，P1-#1） |
| F-006 | Transactional Specification Patch（校验 / 预览 / 冲突检测 / 原子应用） | P0 | ✅ 已完成 | **RFC-0006 §16 全部兑现**：`dsl::patch` 事务（快照 → 编辑 → 重校验 → 重建，错误整体回滚）；`forma_plan` 在**临时副本**试运行即预览；`expected:[{entity,sha256}]` 冲突检测 + `patchId` 幂等（重复提交零变更）+ `baseRevision` 基线绑定 + 审批门（High/Extreme 需 approval）；Studio v2 的 `POST /api/patch` 走同一事务写回 `.fm`（P1-#10） |
| F-007 | Agent Context / Plan / Verify 工作流 | P0 | ✅ 已完成 | **Agent 会话闭环落地**：`forma_search`（语义检索）/ `forma_get_context`（声明块摘录 + 依赖/引用者 + 影响闭包）/ `forma_plan`（副本试运行 + 迁移风险分级 + 审批判定 + **expected 回传**）/ `forma_report`（模型 + 校验证据 + 待执行迁移 + 产物面 + 审计尾迹）；`forma_patch` 支持 **patchId 幂等 + expected 冲突检测 + 审批凭据**（RFC-0006 §16 全部兑现）——一次 Agent 会话可完成 RFC-0006 §8.5 全流程，无需读取目标源码 |
| F-008 | Generated Project Benchmark（真实编译 + Token / 可靠性对照） | P1 | ✅ 已完成 | **10×10 确定性任务矩阵落地（2026-11-05，`forma bench [dir] --json`）**：10 个标准业务变更场景 × 10 个确定性变体 = 100 cases；每个 case 全量 DSL 检查 + 产物生成；每场景变体 0 额外执行生成 backend `cargo check`（10 个真实依赖编译代表样本）；输出 JSON 报告（`forma-benchmark/v1` schema，camelCase）：`checksPassed` / `buildsPassed` / `backendCompilesPassed` / `reliability` / `estimatedInputTokens`（明确标注 4 chars/token proxy，非 LLM tokenizer 实测）/ 逐 case 诊断。基准：**100/100 checks、100/100 builds、10/10 backend cargo check、可靠性 100%** |

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
  F-003 ✅ 影响分析（已完成；DSL 工程经 M3 桥接同样可用）
  F-001 ✅ IR 稳定契约（版本 + 兼容策略，已完成）
  F-002 ✅ Provenance + 产物 Manifest（内容哈希 + 所有权检测，已完成）
  F-006 ✅ 事务式补丁（预览 + 冲突检测 + 幂等，已完成）
  F-004 ✅ 规则跨语言一致性向量（已完成）
  ── 本次交付：forma migrate risk（F-002 的迁移安全子集）──

Phase 2 — Agent 原生化（P0）
  F-007 ✅ Agent Context / Plan / Verify 闭环（已完成）
  F-005 ✅ OpenAPI / JSON Schema 导入（已完成，纳入验收）

Phase 3 — 标准生态互通（P1）
  F-005 ✅ 既有项目 导入 → IR → 导出 语义等价验收（回归 `tests/adapter_roundtrip.rs`：
       OpenAPI / SQL DDL 往返 + 损失报告 + 确定性）
  F-008 ✅ Generated Project Benchmark（已完成）

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
