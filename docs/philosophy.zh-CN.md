# Forma 设计理念（Design Philosophy）

> 本文是 Forma 全部技术决策的「为什么」。架构规格见 [architecture.zh-CN.md](architecture.zh-CN.md)，DSL 语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)。
> 起点文章与完整演化史见 [history/](history/README.md)。

---

## 0. 一句话

**AI Coding 最大的浪费，是让模型用最贵的方式输出最便宜的东西：语法。**
Forma 在 AI 和目标代码之间放一层 **DSL 中间层**——AI 只生产语义（DSL），适配层把语义编译成前端、后端、数据库的完整代码，零 Token、确定性、可增量。

## 1. Token 经济学：语言关键词税

一次典型的 AI Coding 会话里，模型输出的绝大部分 Token 不是业务语义，而是**语言税**：

| 语言税 | 例子 | 语义含量 |
|---|---|---|
| 标点税 | 分号、配对括号、引号、缩进 | 0 |
| 关键词税 | `public` `static` `void` `import` `function` `return` | ≈0 |
| 样板税 | Java 的 getter/setter、构造器、包声明；TS 的类型标注重复；React 组件骨架 | ≈0 |
| 结构税 | HTML 标签配对、CSS 属性、路由注册、依赖清单 | ≈0 |
| 重复税 | **同一个字段在 6 个地方重复出现**（见下） | 语义只有 1 份，Token 却花 6 次 |

其中最贵的是**重复税**。加一个字段 `paymentStatus`，传统 AI Coding 要在：

```
schema.sql            payment_status VARCHAR(64)              ← 第 1 次
models.ts             paymentStatus: "unpaid" | ...           ← 第 2 次
validator.ts          if (!enum.includes(...))                ← 第 3 次
routes.ts             return c.json({...})                    ← 第 4 次
OrderForm.tsx         <input value={...} />                   ← 第 5 次
order.test.ts         expect(...).toBe(...)                   ← 第 6 次
```

**同一个事实，AI 逐 Token 写 6 遍，每遍都要重新「理解上下文 + 生成语法」。**
而模型生成高确定性文本（分号、括号、import、样板）恰恰是它最不需要智能、但你付全价的部分。

> 起点（《我认为 AI Coding 最大的骗局：让 AI 写代码》，见 history/）里有一个更尖锐的表述：
> **100 万 Token 里，真正承载业务决策的可能只有 1 万。** 其余 99% 是把决策「翻译成语言」的成本。

## 2. 中间层：语义与语法的编译级分离

Forma 的答案是**一层中间层**：

```
AI 生产（DSL，~40 Token，纯语义）:
  amount  Decimal(12,2) @required @min(0.01)

适配层编译（0 Token，确定性展开）:
  → MySQL:   `amount DECIMAL(12,2) NOT NULL` + CHECK 语义
  → Rust:    字段 + serde + sqlx 查询 + min(0.01) 校验函数
  → React:   表单控件 + zod schema + 金额展示
  → （未来）iOS/Android 原生表单
```

| 环节 | 传统 AI Coding | Forma（DSL 中间层） |
|---|---|---|
| 表达一次业务变更 | 数千 Token（读文件 + 重写代码） | 数十 Token（DSL 补丁） |
| 同一事实的副本数 | 6+ 份，每份都要模型生成 | 1 份 DSL，适配层展开 N 份 |
| 生成高确定性样板 | 模型逐 Token 输出，付全价 | 编译器输出，零 Token |
| 变更影响分析 | 靠模型「读代码猜」 | 语义图确定性计算 |
| 校验产物 | 重新生成 + 测试 + 人工审 | DSL diff 即审阅面 |
| AI 上下文负担 | 全项目代码进上下文 | 实体级语义摘要（`forma_graph`） |

**要点：不是「AI 写得更快」，而是让一部分输出彻底不再经过模型。**
适配层展开语法是确定性的、可缓存的、增量的——这是把「Token 成本」变成「CPU 成本」。

## 3. 为什么 DSL 是正确的中间层（v0.3 定案）

### 3.1 中间层的本质

前端（Web/移动）、后端、数据库是**同一套业务语义的不同投影**。让 AI 直接写任何一层的代码，它都要：
(a) 花大量 Token 生成那一层的语法；(b) 人工维护各层之间的一致性。
中间层把「业务语义」收敛为单一事实来源，各层由适配层投影生成——**一次声明，处处生成；一处修改，处处同步**。

### 3.2 为什么自研 DSL 而不是标准组合（对 v0.2 方案的修正）

v0.2 曾采用「现有标准联邦」（SQL + OpenAPI + JSON Schema…）。实践推演后的修正：

- **多端投影是标准组合给不了的**：OpenAPI 只管 API，页面/规则/权限/移动端在标准体系里没有统一归属；规则校验要做到「后端权威 + 前端同源」，必须有一个统一的语义归属地。
- **Token 密度**：为 AI 设计的 DSL 一行一个完整事实，比三种标准的组合更稠密；单一文件即可整体进入上下文。
- **LLM 亲和可控**：语法刻意贴近 Prisma/GraphQL/HCL 公共模式，语法面积小、规则封闭（每个运算符有双端映射），学习税一次性且极低。
- **标准不废弃，降级为视图**：`openapi.yaml`、JSON Schema、DDL 由 DSL 编译产生，供生态工具消费——既有适配器代码转为 importer/exporter，不是沉没成本。

### 3.3 DSL 的设计红线

- **不泄漏目标概念**：`.fm` 里没有 axum、React、SQL；页面是 `table/form/detail` 抽象原语。
- **规则表达式封闭**：每个运算符有 Rust/TS 双向映射，Check 阶段静态保证双端可编译。
- **AI 不直接写文件**：语义补丁是唯一写入入口（AST 级编辑 → 重编译）。

## 4. 确定性：可审计的生成

- 同一 DSL 必产生 byte 级相同的目标代码（测试保证）；
- 生成文件带 `@generated` 头，增量缓存检出手改；
- 人类审阅面从「代码 diff」缩小为「DSL diff」——语义补丁本身就是 review 单位。

## 5. 诚实的边界

- **BPMN 式长事务工作流**需要运行时，DSL 先覆盖状态迁移与守卫，完整流程引擎在后续里程碑；
- **Token 对比数字是量级估计**，正式 Benchmark 在路线图后期；
- **DSL 密度有上限**：算法、复杂自定义交互这类「本身就是代码」的东西，由 extension 机制承接（目标语言手写 + DSL 引用）；
- **写 DSL 也要花 Token**——但只花一次、只花在语义上，且可复用、可校验、可增量。

## 6. 设计原则速查

1. 一套 DSL，多端投影：Web/移动/后端/数据库皆由适配层生成。
2. 规则一等公民：写一次，后端权威执行 + 前端同源体验。
3. AI 不直接写文件：DSL 语义补丁是唯一写入入口。
4. 确定性编译：同输入同输出，`@generated` 可检测。
5. 增量优先：一切编译从影响分析开始，只重算受影响目标。
6. 事务式补丁：DSL 目录永远自洽。
