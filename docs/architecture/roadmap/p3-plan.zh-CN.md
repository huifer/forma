---
id: roadmap:p3-plan
title: P3 阶段规划——治理收尾与发布就绪（Governance & Release Readiness）
status: Accepted
version: 1.1.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: roadmap
depends_on: [RFC-0001, RFC-0002, RFC-0009]
related_adrs: [ADR-0001, ADR-0003]
related_specs: [spec:ir, spec:protocols, spec:compatibility, spec:serialization]
breaking_change: false
---

# P3：治理收尾与发布就绪

> **执行结果（2026-11-08）**：C-1 ~ C-5 全部完成。RFC-0002 / RFC-0009 升为
> `Implemented`（附证据勾选项，S4 如实保留未勾选）；补齐 S2 机读损失报告与 S3 往返测试、
> 修复 OpenAPI `number → Decimal(p,s)` 缺陷；CI 新增 `generated-backend` 编译门与
> `examples/access-depth` 参考工程；四份 Conformance profile 证据矩阵核对（44 个引用全部存在）
> 并补 P2/P3 新证据；README 双语 / roadmap / verification / CHANGELOG 与撤销决策同步。
> 全量 `cargo test` 406 项 / 65 套件，docs validate/lint/check-links 全绿，
> CI YAML 解析通过；顺带修掉 `capability check` 在仓库根必然失败的坏命令。

> 前置：[P2 轨道 A 8/8 完成、轨道 B 撤销](p2-plan.zh-CN.md)。
> 本阶段**不新增 target、不扩栈**，只做一件事：把「已实现」变成「可验证、可发布」。
> 立项依据（2026-11-08 源码审计）：
> - RFC-0002 仍 `Proposed`，§16 验收项全部未勾选，其中 S2（Lossy 导入损失报告）/ S3（Lossless
>   往返语义等价测试）**确实未实现**；
> - RFC-0009 仍 `Proposed`，§16/§17 全部未勾选，但 `tests/dsl_validate.rs` 等已覆盖大部分；
> - CI 有 test / lint / docs / flutter / web / e2e 六道门，**唯独没有 `examples/school`
>   生成 Rust 后端的 `cargo check`**——P2 轨道 A-2 抓出的 6 类编译错误正是这道门缺失的后果；
> - `README.zh-CN.md` 的 Wave 4 仍描述已撤销的目标扩栈；
> - Conformance 四份 profile 只写了验收要求，没有「回归测试 / CI job」证据列。

## 0. 完成定义

| 项 | 完成判据 |
|---|---|
| 治理 | 全部 RFC 状态与实现证据一致；`forma docs validate / lint / check-links / check-dependencies` 全绿 |
| 工程 | CI 能拦住「cargo test 全绿但生成后端编译不过」这类回归 |
| 互通 | Lossy 导入有机读损失报告；Lossless 方向有往返语义等价回归 |
| 叙事 | README 双语 / roadmap / CHANGELOG 与撤销决策一致 |

## 1. 任务清单

### ✅ C-1 RFC-0009 验收核实与升格
逐条核对 §16 / §17 与现状：`forma validate`↔`forma check` 同源分发（F-004 假绿修复）、
错误工程退出码、Diagnostic 同形、CLI↔MCP 诊断一致性、`--version`、docs validate、Studio→IR。
**有证据才勾选**，然后把 `status: Proposed` → `Implemented`，§20 Change Log 记一行。

### ✅ C-2 RFC-0002 互通验收补齐（真工程）
- **S2 损失报告**：`forma import sql|prisma` 增加机读 `LossReport`（`--loss-report <path>`），
  逐条列出被跳过/保守降级的构造；无损失时产出空清单。OpenAPI 导入同样接入。
- **S3 往返测试**：OpenAPI → `.fm` → OpenAPI 的语义等价回归（操作路径 / 请求体字段 /
  响应结构），以及 SQL DDL → `.fm` → DDL 的字段/类型/关系对齐回归。
- **S5 版本信息**：构建报告显式打印 IR 版本。
- 逐条核对 §16 后升格 `Proposed` → `Implemented`；仍未覆盖的项**保留未勾选并写明缺口**，
  不假绿。

### ✅ C-3 CI 生成后端编译门
新增 `generated-backend` job：`cargo run --locked -- build examples/school` →
`cd examples/school/generated/backend && cargo check`（覆盖 auth + commands + 无租户列组合），
并保留已有 cursor-pagination 后端检查。这是 L3 验证门在 CI 的落点。

### ✅ C-4 Conformance 证据矩阵
`conformance/{semantic,runtime,adapter,compatibility}.md` 每条验收项补「证据」列
（回归测试文件 / CI job 名），无证据的显式标注 `needs-evidence`，不静默略过。

### ✅ C-5 文档与发布叙事同步
- `README.md` / `README.zh-CN.md` 的状态与路线图段：Wave 4 扩栈标注撤销，指向 p2/p3 plan；
- `docs/roadmap.zh-CN.md` §1 Wave 4 文字同步；
- `CHANGELOG.md` 加本阶段条目；`docs/verification.zh-CN.md` 的 L3 映射补 CI 门。

## 2. 边界与不做

- 不做轨道 B 的任何内容（新 target / 新 DSL 构造 / 工作流运行时）。
- 不做「无证据升格 RFC」：状态变更必须附带可执行的回归或实现位置。
- 不改默认 target 的产物（零变化原则）。
- 不引入新的运行时依赖（宪法：零新增依赖优先）。

## 3. 验收

1. 全量 `cargo test` 无回归；`cargo clippy -D warnings` / `cargo fmt --check` 通过。
2. `forma docs validate / lint / check-links / check-dependencies / capability check` 全绿。
3. CI 配置经 YAML 解析通过；新增 job 的步骤在本地等价命令真实跑通。
4. 生成工程验证门：school 生成后端 `cargo check` 零错误零警告；Web `tsc --noEmit`。
5. 全部 RFC 状态与 §16/§17 勾选状态自洽（人工复核 + docs 工具）。

## 4. 开放问题

- RFC-0002 S4（每个规格领域都有独立 Validator）目前由 `check_program` 集中承担，
  是否需要按领域拆分诊断码命名空间？本阶段只做「如实标注」，不重构。
- `forma import openapi` 的往返保真度边界（枚举 / 数组 / 嵌套对象降级）需人工校对，
  测试只保证「可表达部分」等价。
