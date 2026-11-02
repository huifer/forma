# Forma 设计理念（Design Philosophy）

> 本文是 Forma 全部技术决策的「为什么」。架构规格见 [architecture.zh-CN.md](architecture.zh-CN.md)。
> 起点文章与完整演化史见 [history/](history/README.md)。

---

## 0. 一句话

**AI Coding 最大的浪费，是让模型用最贵的方式输出最便宜的东西：语法。**
Forma 把「语义」和「语法」拆开——AI 只生产语义（规格），编译器免费生产语法（代码）。

## 1. Token 经济学：语言关键词税

一次典型的 AI Coding 会话里，模型输出的绝大部分 Token 不是业务语义，而是**语言税**：

| 语言税 | 例子 | 语义含量 |
|---|---|---|
| 标点税 | 分号、配对括号、引号、缩进 | 0 |
| 关键词税 | `public` `static` `void` `import` `function` `return` | ≈0 |
| 样板税 | Java 的 getter/setter、构造器、包声明；TS 的类型标注重复 | ≈0 |
| 结构税 | HTML 标签配对、CSS 属性、React 组件骨架、`package.json` | ≈0 |
| 重复税 | **同一个字段在 6 个地方重复出现**（见下） | 语义只有 1 份，Token 却花 6 次 |

其中最贵的是**重复税**。加一个字段 `paymentStatus`，传统 AI Coding 要在：

```
schema.sql            ORDER.payment_status TEXT        ← 第 1 次
types.ts              paymentStatus: "unpaid" | ...    ← 第 2 次
validator.ts          if (!enum.includes(...))         ← 第 3 次
routes.ts             return c.json({...})             ← 第 4 次
OrderForm.tsx         <input value={...} />            ← 第 5 次
order.test.ts         expect(...).toBe(...)            ← 第 6 次
```

**同一个事实，AI 逐 Token 写 6 遍，每遍都要重新「理解上下文 + 生成语法」。**
而模型生成高确定性文本（分号、括号、import、样板）恰恰是它最不需要智能、但你付全价的部分。

> 起点（《我认为 AI Coding 最大的骗局：让 AI 写代码》，见 history/）里有一个更尖锐的表述：
> **100 万 Token 里，真正承载业务决策的可能只有 1 万。** 其余 99% 是把决策「翻译成语言」的成本。

## 2. Forma 的解法：语义与语法的编译级分离

```
AI 生产：  {"op":"add-entity-field", "entity":"Order",
           "field":{"name":"paymentStatus", "type":"string",
                    "enum":["unpaid","paid","refunded"]}}     ← ~40 Token，纯语义

Forma 编译：schema.sql +1 行、order.schema.json +1 个属性、
           openapi.yaml +1 个字段、order-form.ui.json +1 个控件、
           types.ts +validators.ts +routes.ts +OrderForm.tsx …   ← 0 Token，确定性展开
```

| 环节 | 传统 AI Coding | Forma |
|---|---|---|
| 表达一次业务变更 | 数千 Token（读文件 + 重写代码） | 数十 Token（语义补丁） |
| 同一事实的副本数 | 6+ 份，每份都要模型生成 | 1 份规格，编译器展开 N 份 |
| 生成高确定性样板 | 模型逐 Token 输出，付全价 | 编译器输出，零 Token |
| 变更影响分析 | 靠模型「读代码猜」 | 语义图确定性计算 |
| 校验产物 | 重新生成 + 测试 + 人工审 | 补丁 diff 即审阅面 |
| AI 上下文负担 | 全项目代码进上下文 | 实体级语义摘要（`forma_graph`） |

**要点：不是「AI 写得更快」，而是让一部分输出彻底不再经过模型。**
编译器展开语法是确定性的、可缓存的、增量的——这是把「Token 成本」变成「CPU 成本」。

## 3. 为什么是现有标准，而不是新 DSL

给 AI 发明一门新语言，等于**对每个会话重新收一次学习税**：新语法、新解析器、新报错格式，模型全都要靠上下文样本现学——这本身就是 Token 浪费，还会把用户锁进生态孤岛。

所以 Forma 选择 **Polyglot Specification**：

- **SQL / JSON Schema / OpenAPI / JSON Forms / BPMN** 都在模型的海量训练数据里，AI 天然「会说」这些语言，零学习成本；
- 它们有现成的解析器、校验器、编辑器和社区；
- Forma 不在标准文件里塞私有语法——跨标准关联只通过 `x-forma-*` 扩展字段、Manifest 绑定和名称约定实现。

**Forma 自己只保留四样东西**：`forma.yaml` Manifest、`forma://` 实体身份协议、Unified Semantic Graph、Semantic Patch API。它们是「集成与编译层」，不是「新语言」。

## 4. AI 不直接写文件

AI 的工作流被协议约束为：

```
forma_graph → forma_impact → forma_patch → forma_validate
```

- **不读全项目**：AI 拿到的是实体级语义摘要，不是几千行代码；
- **不写代码**：AI 输出的是语义补丁（几十 Token），不是文件内容；
- **不猜影响面**：`forma_impact` 返回确定性闭包，而不是模型的直觉；
- **不承担回滚**：补丁是事务式的，任一编辑失败全部还原。

这把 AI 从「全栈打字员」变成「意图表达者」——它只做它有比较优势的事：**理解需求、设计结构、表达语义**。

## 5. 确定性：可审计的生成

- 同一输入必产生 byte 级相同的输出（`forma build` 两次产物一致，有测试保证）；
- 生成文件带 `@generated` 头，人手改生成物会被增量缓存检测出来；
- 人类审阅面从「代码 diff」缩小为「规格 diff」——语义补丁本身就是 review 单位。

## 6. 诚实的边界

- **BPMN 不是「XML 转函数」**：持久化任务、重试、补偿需要工作流运行时，MVP 只做子集；
- **Token 对比数字是量级估计**，不是基准测试结果；正式 Benchmark 在路线图 Phase 3；
- **规格密度有上限**：算法、复杂 UI 交互这类「本身就是代码」的东西，Forma 不试图用规格表达（extension 机制承接）；
- **写规格也要花 Token**——但只花一次、只花在语义上，且可复用、可校验、可增量。

## 7. 设计原则速查

1. Polyglot Specification：每个领域用最成熟的标准，Forma 不重复定义。
2. AI 不直接写文件：语义补丁是唯一写入入口。
3. 确定性编译：同输入同输出，`@generated` 可检测。
4. 增量优先：一切编译从影响分析开始，只重算受影响子图。
5. 事务式补丁：规格目录永远自洽。
6. 标准之上，不做私有语法：`x-forma-*` 与 Manifest 是仅有的关联面。
