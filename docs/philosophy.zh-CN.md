# Forma 设计理念（Design Philosophy）

> 架构见 [architecture.zh-CN.md](architecture.zh-CN.md) · DSL 语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md) · 演化史见 [history/](history/README.md)

## 0. 核心目标

**AI 通过持续生产 DSL，完成一整个软件系统的开发。**

三个承诺：

1. **完整**：从数据、规则、API 到页面，整个系统由一套 DSL（`.fm`）描述，适配层编译为 MySQL、Rust 后端、React Web（后续移动端）的全部代码。
2. **稳定优质**：同输入必得同输出；规则双端同源；变更经语义图校验与事务式补丁落地——质量由编译器保证，不依赖模型的临场发挥。
3. **持续**：DSL 是系统的持久化状态，随 git 版本演进。AI 的每次会话从当前状态出发做增量变更，不需要从头到尾重新说话。

围绕这个目标，Forma 做三层事情：**DSL 语言**（语义载体）、**编译器与适配层**（多目标生成）、**可视化与生态互通**（人机共同的审阅面，与 Swagger 等既有优质工具完全结合）。

## 1. Token 经济学：语言关键词税

一次典型的 AI Coding 会话里，模型输出的绝大部分 Token 不是业务语义，而是**语言税**：

| 语言税 | 例子 | 语义含量 |
|---|---|---|
| 标点税 | 分号、配对括号、引号、缩进 | 0 |
| 关键词税 | `public` `static` `void` `import` `function` `return` | ≈0 |
| 样板税 | Java 的 getter/setter 与构造器、TS 的类型标注重复、React 组件骨架 | ≈0 |
| 结构税 | HTML 标签配对、CSS 属性、路由注册、依赖清单 | ≈0 |
| 重复税 | 同一个字段在 SQL、类型、校验器、路由、表单、测试里逐 Token 写六遍 | 语义 1 份，Token 花 6 次 |

模型生成高确定性文本恰恰最不需要智能，但按全价计费。起点分析（《我认为 AI Coding 最大的骗局：让 AI 写代码》，见 history/）的结论是：**100 万 Token 里，真正承载业务决策的可能只有 1 万**，其余是把决策「翻译成语言」的成本。

## 2. 中间层：语义与语法的编译级分离

Forma 在 AI 与目标代码之间放一层 DSL 中间层：

```text
AI 生产（DSL，~40 Token，纯语义）:
  amount  Decimal(12,2) @required @min(0.01)

适配层编译（0 Token，确定性展开）:
  → MySQL:   amount DECIMAL(12,2) NOT NULL
  → Rust:    字段 + serde + sqlx 查询 + min(0.01) 校验函数
  → React:   表单控件 + zod schema + 金额展示
  → （后续）iOS/Android 原生表单
```

| 环节 | 传统 AI Coding | Forma |
|---|---|---|
| 表达一次业务变更 | 数千 Token（读文件 + 重写代码） | 数十 Token（DSL 补丁） |
| 同一事实的副本数 | 6+ 份，每份由模型生成 | 1 份 DSL，适配层展开 N 份 |
| 高确定性样板 | 模型逐 Token 输出，付全价 | 编译器输出，零 Token |
| 变更影响分析 | 模型「读代码猜」 | 语义图确定性计算 |
| 审阅面 | 代码 diff | DSL diff |
| AI 上下文 | 全项目源码 | 实体级语义摘要（`forma_graph`） |

本质：把一部分输出**彻底移出模型**。适配层展开语法是确定性的、可缓存的、增量的——「Token 成本」变成「CPU 成本」。

## 3. 持续开发模型：DSL 是系统的持久状态

传统 AI 会话的问题不只是贵，而是**无状态**：上下文一关，系统知识归零，下次从头解释。Forma 的 DSL 天然解决这个问题。

```text
会话 N:   需求 → forma_patch（DSL 变更）→ 校验 → 编译 → git commit
              │
              ▼
      .fm 文件 = 系统的当前状态（版本化、可 diff、可回滚）
              │
              ▼
会话 N+1: forma_graph（30 秒读懂系统现状）→ 新需求 → forma_patch → …
```

- **DSL 即记忆**：实体、规则、API、页面全部持久在 `.fm`，AI 的每次会话通过 `forma_graph` 摘要读取现状，通过语义补丁写入变更——会话之间互不依赖上下文。
- **稳定性机制**：确定性编译（同输入同输出）、语义图影响分析（不靠模型猜）、规则双端同源（后端权威 + 前端一致）、事务式补丁（失败全回滚）、生成代码自带测试与 CI 模板。
- **优质的结构保证**：所有目标代码从同一 IR 生成，天然一致；人只审 DSL diff，审阅面最小。

## 4. 可视化是 DSL 的一等公民

DSL 是给人和 AI 共同读写的，因此**写 `.fm` 必须能被看见**。Forma Studio 随编译器同源渲染：

- **实体关系图**：实体/字段/关系/索引，节点联动 DSL 源行
- **页面原型**：`table/form/detail` 原语用真实 shadcn 组件渲染——预览即最终形态，非手绘 mockup
- **API 文档**：Swagger UI 风格浏览（操作/请求/响应/权限）
- **规则矩阵**：字段校验、守卫、权限按实体 × 操作一览
- **影响分析图**：语义补丁的可视化评审

分阶段路线（v1 只读实时预览 → v2 双向编辑写回 `.fm` → v3 协作）见 [roadmap §3](roadmap.zh-CN.md)。

## 5. 与历史优质项目的结合

Forma 站在既有生态之上，是枢纽而非孤岛：

| 项目 | 结合方式 |
|---|---|
| **OpenAPI / Swagger** | 双向：`api` 块导出 OpenAPI 3.1 + Swagger UI 文档站；既有 OpenAPI 导入为实体与 API 骨架 |
| **SQL / 迁移工具** | 双向：DDL 逆向导入为实体；DDL 导出并对接 Flyway / Liquibase / golang-migrate 目录约定 |
| **Postman** | 导出 Collection，API 调试即用 |
| **JSON Schema** | 导出契约；导入为实体 |
| **Prisma / DBML** | 导入 schema 为实体；导出 DBML 建模 |
| **Mermaid** | 导出 ER / 流程图，文档即代码 |
| **VS Code / IDE** | 语法高亮、诊断、跳转（扩展） |

验收基线（M7）：一个既有 Swagger + MySQL 项目导入为 `.fm`，再导出 OpenAPI 与 DDL，语义等价。

## 6. 诚实的边界

- BPMN 式长事务工作流需要运行时；DSL 先覆盖状态迁移与守卫，完整流程引擎在后续里程碑。
- Token 对比数字是量级估计；正式 Benchmark 在路线图后期。
- DSL 密度有上限：算法与复杂自定义交互由 `extension` 机制承接手写代码。
- 写 DSL 也花 Token——但只花一次、只花在语义上，且可复用、可校验、可增量。

## 7. 设计原则速查

1. 一套 DSL，多端投影：Web/移动/后端/数据库皆由适配层生成。
2. DSL 是持久状态：AI 按增量会话持续开发，不从零对话。
3. 规则一等公民：写一次，后端权威执行 + 前端同源体验。
4. 可视化一等公民：DSL 与图形视图同源实时互映。
5. 生态枢纽：与 Swagger、SQL、Postman、Mermaid 等既有工具双向互通。
6. AI 不直接写文件：DSL 语义补丁是唯一写入入口。
7. 确定性编译：同输入同输出，`@generated` 可检测。
8. 事务式补丁：DSL 目录永远自洽。
