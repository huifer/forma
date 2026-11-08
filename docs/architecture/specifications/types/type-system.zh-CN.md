---
id: spec:types
title: Type System Specification
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-05
domain: semantic-core
governing_rfc: RFC-0003
depends_on: [RFC-0001]
related_adrs: [ADR-0002]
related_specs: [spec:ir]
breaking_change: false
---

# spec:types — 类型系统规范

> **规范性**：本规范描述 Forma DSL **当前实现已达成的**类型契约。
> §「已知缺口」中列出的条目**尚未实现**，不受本规范约束，也不得被当作已交付能力。
> 治理依据：[RFC-0003 语义内核](../../rfc/RFC-0003-semantic-core.zh-CN.md) · [RFC-0002 规格体系](../../rfc/RFC-0002-specification-system.zh-CN.md)。

---

## 1. 目的与范围

定义 Forma DSL 的**类型词汇表**、参数化形式、约束表达、校验入口，以及类型在各目标上的映射归属。

本规范**不**规定类型到具体数据库/语言的映射表——映射表随 Capability 演进（见 [RFC-0004](../../rfc/RFC-0004-capability-system.zh-CN.md)）。

## 2. 规范性词汇

- **MUST / MUST NOT**：强制要求，违反即编译失败。
- **SHOULD**：强烈建议，偏离需在 RFC 中说明理由。
- **MAY**：可选。

## 3. 内置类型词汇表

源码：`crates/forma/src/dsl/mod.rs::BUILTIN_TYPES`（11 种，**封闭集合**）。

> 该清单是**单一权威来源**：检查层与所有生成层共用。新增类型必须**同时**改清单 + 各 emitter 映射 + 本表。

| 类型 | 语义 | 参数化 | 可空 | 备注 |
|---|---|---|---|---|
| `Uuid` | 全局唯一标识 | — | 否（默认 `@id @default(uuid)`） | 物理表示为**文本** UUID |
| `String` | 变长字符串 | `String(n)` → 长度上限 `n` | 是 | `n` 缺省无上限 |
| `Email` | 邮箱地址 | — | 是 | 映射为 `VARCHAR(320)` |
| `Text` | 长文本 | — | 是 | 映射为 `TEXT` |
| `Int` | 64 位整数 | — | 是 | 映射为 `BIGINT` |
| `Float` | 浮点数 | — | 是 | — |
| `Decimal` | 定点小数 | `Decimal(p,s)` → 精度/标度 | 是 | **精度敏感，禁止浮点传输** |
| `Bool` | 布尔 | — | 是 | 映射为 `TINYINT(1)` |
| `Timestamp` | 时间戳 | — | 是 | 映射为 `DATETIME(3)`（毫秒精度） |
| `Date` | 日期 | — | 是 | 映射为 `DATE` |
| `Json` | 结构化 JSON | — | 是 | MySQL `JSON` / Rust `serde_json::Value` / TS `unknown` / Dart `Map<String, dynamic>` |

### 3.1 词汇表封闭性

- 类型名**MUST** 属于上述集合，否则报 `dsl-unknown-type`（`crates/forma/src/dsl/mod.rs` 字段类型检查分支）。
- 新增类型**MUST** 走正式 RFC（Semantic Core 层级，Tier 1），**MUST NOT** 由某个 Capability 私自引入。

### 3.2 参数化形式（规范性边界）

- 仅 `String(n)` 与 `Decimal(p,s)` 支持参数化；其余类型带参数**MUST**报错
  （`dsl-invalid-type-arg`——生成端无从映射）。
- `String(n)`：`0 < n ≤ 65535`（VARCHAR 上限）。`n = 0` 或越界**MUST**报错。
- `Decimal(p,s)`：**MUST** 恰好 2 个参数；`0 < p ≤ 65`（MySQL DECIMAL 精度上限）；
  `s ≤ p`。违反任一条**MUST**报错。
- 参数**MUST** 为数字字面量；解析失败即语法错误。

## 4. 枚举

- 枚举**不是**内置类型，而是**独立的一等构造**（`enum X { a b c }`）。
- 枚举在数据库侧映射为 `VARCHAR(64)` + 应用层校验（**可移植性优先**，见 [RFC-0002 §3.1](../../rfc/RFC-0002-specification-system.zh-CN.md)）。
- 枚举变体名**MUST** 稳定——枚举值落库即成为数据契约的一部分。

## 5. 约束表达

字段约束以属性或 `rules` 块声明，**MUST** 在两端同源执行：

| 约束 | DSL | 后端（权威） | 前端（体验） |
|---|---|---|---|
| 必填 | `@required` | validator 拒绝空值 | zod / Form validator |
| 范围 | `@min` / `@max`、`rules { field { min } }` | validator | 同源 zod |
| 格式 | `Email` 类型 | validator | 同源 zod |
| 唯一 | `unique [a, b]` | DB 唯一索引 | — |
| 只读 | `@readonly` | 拒绝写入 | 表单禁用 |

- 约束**MUST NOT** 只在一端实现——规则是一等公民（见 [RFC-0002 §3](../../rfc/RFC-0002-specification-system.zh-CN.md)）。

## 6. 类型映射的归属

- 本规范**只**定义语义侧词汇表与约束。
- 类型 → 目标（MySQL / Rust / React / Dart）的映射由**各目标的 emitter** 持有。
- **类型语义变更（如 `Decimal` 精度）MUST** 视为破坏性变更，走 [spec:compatibility](../compatibility/compatibility-rules.zh-CN.md)。

## 7. 已知缺口（未实现，不受本规范约束）

| # | 缺口 | 现状 |
|---|---|---|
| ~~1~~ | ~~`Json` 未纳入词汇表~~ | **已关闭（2026-11-05）**：`Json` 已纳入 `BUILTIN_TYPES`（11 型），四个目标均有映射；回归测试 `tests/dsl_validate.rs` 保证「生成层支持的类型必须检查层也可用」 |
| 2 | ~~参数化边界未校验~~ **已关闭（2026-11-05，P1-2）** | `dsl-invalid-type-arg`：String 0<n≤65535 / Decimal 0<p≤65 且 s≤p / 非参数化类型带参 |
| ~~3~~ | ~~枚举跨端一致性向量缺失~~ **已关闭（2026-11-05，F-004 全量收口）**：共享语义向量落地——`tests/rule_vectors.rs` 断言同一规则的边界字面量与比较方向在 Rust / TS / Dart 三端**一致**；行为向量从 DSL **派生**（`validation_vectors.rs`），生成两端测试文件并在各自 CI 真实执行：`validators.test.ts`（vitest，Web 测试运行器已补齐）与 `rule_vector_test.dart`（flutter_test）——由同一派生函数产出（同源保证），任何一端校验器漂移都会在 CI 变红 |
| ~~4~~ | ~~**无 `api` 的工程生成物有未使用导入**~~ **已关闭（2026-11-05，P2-4）** | routes / repo / validators 导入全按需（提取器逐符号判定）；实体-only 工程 0 警告；回归测试 `backend_contract.rs`（entity_only / list_only / auth_module_only） |
| 5 | ~~`min`/`max` 的类型约束~~ **已关闭（2026-11-05，P2-5）** | 属性形式 `@min`/`@max` 与 rules 同源校验：仅数值类型 + 恰好 1 个数值参数（`dsl-invalid-constraint`） |

## 8. 开放问题

> 收口（2026-11-08）：3 项均已决议/关闭。

- [x] ~~词表是否需要「实验类型」子集（与 §7.1 决策耦合）。~~
  **已决议**：不做。类型词表（`BUILTIN_TYPES`）保持稳定集合；新类型走 minor 版本 +
  四目标映射一致性回归（新增类型须 Rust/TS/Dart/MySQL 同步，否则测试失败）。
- [x] ~~`Decimal(p,s)` 的 `p/s` 上限是否由本规范固定（如 `p ≤ 65`）。~~
  **已定**：`p ≤ 65`、`s ≤ p`、`s ≥ 0`（P1-2，`dsl-invalid-type-arg`）。
- [x] ~~数组/复合类型是否进入 Tier 1（当前无）。~~
  **已决议**：不进 Tier 1。`Attachment` / `Json` 已覆盖主要场景；数组属未来扩展
  （需要 DSL → 关系/集合的建模决策）。

## 8. 开放问题

- 词表是否需要「实验类型」子集（与 §7.1 决策耦合）。
- `Decimal(p,s)` 的 `p/s` 上限是否由本规范固定（如 `p ≤ 65`）。
- 数组/复合类型是否进入 Tier 1（当前无）。

## 9. 参考

- RFC-0002 规格体系：[../../rfc/RFC-0002-specification-system.zh-CN.md](../../rfc/RFC-0002-specification-system.zh-CN.md)
- RFC-0003 语义内核：[../../rfc/RFC-0003-semantic-core.zh-CN.md](../../rfc/RFC-0003-semantic-core.zh-CN.md)
- `spec:ir`：[../ir/ir-contract.zh-CN.md](../ir/ir-contract.zh-CN.md)
- DSL 语言规范（用户向）：[../../../dsl-spec.zh-CN.md](../../../dsl-spec.zh-CN.md)
