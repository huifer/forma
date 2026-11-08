# 验证体系（L1–L5）：从「能编译」到「可信赖地生成」

> 架构见 [architecture.zh-CN.md](architecture.zh-CN.md) · 迁移安全见 [migration-safety.zh-CN.md](migration-safety.zh-CN.md)
> · Agent 工作流见 [agent-workflow.zh-CN.md](agent-workflow.zh-CN.md)

Forma 的核心价值不是「能生成代码」，而是「**可信赖地生成**」。把 AI 从逐 Token 写代码的劳动中解放出来之后，编译器必须能回答一组比「是否编译通过」更难的问题：

- AI 改了什么？影响了哪些模块？
- 生成的代码与规格是否一致？
- 这次变更是否破坏了既有行为？
- 数据库升级是否安全、能否回滚？
- 重新编译会不会覆盖人工扩展？

为此 Forma 把验证拆成五个层级。**关键纪律：通过编译不等于业务正确，通过测试也不等于形式化证明。** 每一层解决一类不同的问题，且每一层都有独立的、机器可读的结论——不要用单一的 `success: true` 代替所有验证结论。

## 1. 五级验证模型

```text
L1 语法验证   Parse           规格能不能被解析？
   │
L2 语义验证   Semantic        规格本身有没有逻辑问题？
   │
L3 产物验证   Artifact        编译产物是否正确、能否编译？
   │
L4 行为验证   Behavioral      生成的软件行为是否符合规格？
   │
L5 变更安全   Change Safety   这次变更能不能安全交付？
```

| 层级 | 名称 | 检查内容 | Forma 当前能力 |
|---|---|---|---|
| **L1** | Syntax / Parse | 语法正确性、格式、引用解析、文件结构 | `forma check` / `forma validate`：Lexer → Parser → AST（span 保留，诊断带行号 + did-you-mean） |
| **L2** | Semantic | 类型检查、字段关系、约束检查、权限规则、依赖关系、路由冲突 | `forma check`：类型兼容、跨实体引用、规则表达式可编译性、路由冲突、权限完整性 |
| **L3** | Artifact | 生成的 SQL / Rust / TS / Dart 是否符合目标规范 | 迁移重放模拟器（`emit/replay.rs`，无需真库）；生成 Rust 工程 `cargo check`；生成 Web `tsc`；生成 Flutter `flutter analyze` |
| **L4** | Behavioral | 跨语言规则测试、API 契约测试、迁移测试、集成测试 | OpenAPI 契约导出；迁移链前向重放 == 当前 schema；school 案例真实 MySQL 8 端到端（CRUD/守卫/分页） |
| **L5** | Change Safety | 兼容性分析、破坏性变更检测、迁移风险、影响范围、人工审批 | `forma migrate risk`（破坏性分级）；`forma graph` / `forma impact`（语义影响闭包）；`@generated` 与 `extension` 所有权边界 |

## 2. 生命周期状态机

每一个生成的产物都应该能定位到自己处于下面哪一步。**不要用一个布尔值概括全部验证结论**——不同阶段失败意味着不同的处理方式：

```text
  Parsed ──▶ Valid ──▶ Generated ──▶ Compiled ──▶ Tested ──▶ Verified ──▶ Approved
    │         │          │             │            │           │            │
    │         │          │             │            │           │            └─ 人审业务意图与变更证据
    │         │          │             │            │           └─ 独立验证器证明行为符合规格
    │         │          │             │            └─ 生成工程真实构建 + 集成测试通过
    │         │          │             └─ 目标语言可编译（cargo check / tsc / analyze）
    │         │          └─ IR → Emitter 确定性产出（同输入同输出）
    │         └─ 语义检查零 error（类型/引用/约束/权限/路由）
    └─ DSL 可被解析（语法 + 结构）
```

| 状态 | 含义 | 对应工具 |
|---|---|---|
| `Parsed` | 规格能被解析 | `forma check` 语法阶段 |
| `Valid` | 语义检查零错误 | `forma check` 语义阶段 |
| `Generated` | 确定性产出已落盘 | `forma build` |
| `Compiled` | 目标语言可编译 | 生成工程 `cargo check` / `tsc` / `flutter analyze` |
| `Tested` | 构建 + 集成测试通过 | school E2E、契约测试 |
| `Verified` | 独立验证器证明行为符合规格 | `forma migrate verify`、规则一致性向量 |
| `Approved` | 人审业务意图与变更证据 | Forma Studio 变更评审（规划中） |

## 3. 各层级如何落地（当前实现）

### L1 / L2 — `forma check` 与结构化诊断

诊断是机器可读的，而非一段日志文本。每条诊断带稳定 `code`、严重度、位置与建议，Agent 可以直接据此做局部修复，而不必反复重读整份文档：

```json
{
  "code": "E_REF_NOT_FOUND",
  "severity": "error",
  "message": "引用的字段不存在",
  "location": { "file": "forma/orders.fm", "line": 28, "column": 14 },
  "subject": "Order.customer",
  "suggestions": [{ "type": "similar_field", "value": "customerId" }]
}
```

（诊断体系见 `crates/forma/src/validate.rs` 的 `Diagnostic { severity, code, ... }`。）

### L3 — 产物编译验证

生成物不是「写出来就算数」，而是必须能在目标语言里真实编译：

- **MySQL**：`emit/replay.rs` 是 Forma SQL 子集的 MySQL 语义模拟器，`forma migrate verify` 在 **无数据库** 情况下重放整条迁移链，并与当前 DSL 派生的 schema 逐项比对；真实 MySQL 8 端到端作为最终验收。
- **Rust**：生成 `axum + sqlx` 工程，`cargo check` / `cargo build` 通过。
  CI 的 `generated-backend` job 对 **school + access-depth** 两个生成工程真实执行 `cargo check`
  （覆盖 auth / 命令 / 无租户列 / 两层关系遍历组合）——这是 P2 轨道 A-2 抓出「`cargo test` 全绿但
  生成后端编译不过」后补上的机器防线。
- **Web**：生成 React + TanStack 工程，`tsc` 零错误、`vite build` 通过。
- **Flutter**：生成 Dart 工程，`flutter analyze` 零 issue（`flutter build web` 生产编译）。

### L4 — 行为验证与规则一致性

规则是 Forma 的一等公民，但「Rust 与 TypeScript 能生成相同的表达式」不代表两端执行结果一定相同（Decimal 精度、Null 处理、时区、日期边界、除零等）。因此 Forma **已落地**跨语言共享的测试向量（F-004）：同一组 `(输入, 期望)` 在 Rust、TypeScript、Dart 三端得到一致结果（见 [next-milestone.zh-CN.md](next-milestone.zh-CN.md) 的 F-004；回归 `tests/rule_vectors.rs`、生成 Web `validators.test.ts`、Dart `rule_vector_test.dart`，CI 执行 `npx vitest run` + `flutter test`）。

### L5 — 变更安全与迁移风险

每一次 schema 变更都要能回答「安不安全、能不能回滚」。详见 [migration-safety.zh-CN.md](migration-safety.zh-CN.md)：`forma migrate risk` 把每条迁移操作按破坏性分级（Low / Medium / High / Extreme），删列 / 删表 / 改类型等高风险变更在执行 `up --apply` 前必须经过人工审批。

## 4. 与「普通代码生成器」的差异化

普通代码生成器只验证到 L3（产物能编译）。Forma 的护城河在 L4 / L5：

```text
普通生成器：  AI 写代码 ──▶ 能编译 ──▶ 交付
Forma：       DSL 规格 ──▶ 确定性编译 ──▶ L3 产物可编译 ──▶ L4 行为验证 ──▶ L5 变更安全 ──▶ 人审
```

最想看到、也最难被复制的 Benchmark 是：**同一个业务需求连续进行 20 次规格变更，Forma 能否始终保持数据库、后端、前端、API 与业务规则的一致性？** 这是 Forma 区别于低代码平台、ORM、OpenAPI Generator 和传统代码生成器的地方。

## 5. 参考

- 架构：[architecture.zh-CN.md](architecture.zh-CN.md)
- 迁移安全：[migration-safety.zh-CN.md](migration-safety.zh-CN.md)
- Agent 工作流：[agent-workflow.zh-CN.md](agent-workflow.zh-CN.md)
- 近期里程碑：[next-milestone.zh-CN.md](next-milestone.zh-CN.md)
