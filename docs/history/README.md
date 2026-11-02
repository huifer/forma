# Forma 历史决策记录（ChatGPT 对话归档）

> 来源：[ChatGPT 分享 · 6ae81e37-c000-83ee-9a47-77fec81cd99a](https://chatgpt.com/share/6ae81e37-c000-83ee-9a47-77fec81cd99a)
> 提取方式：从分享页 HTML 中解析 React 序列化数据（见 `raw/`），完整还原全部消息正文。
> 本目录是 Forma 项目从「一个观点」到「v0.1 技术蓝图」的完整演化史，也是当前「Specification Federation」战略调整的决策依据。

## 文档索引

| 文件 | 内容 | 状态 |
|---|---|---|
| [01-origin-article.md](01-origin-article.md) | 起点文章《我认为 AI Coding 最大的骗局：让 AI 写代码》+ 标题候选 | 思想起点，仍然有效 |
| [02-ai-spec-compiler-plan.md](02-ai-spec-compiler-plan.md) | 第一版完整方案：AI Specification Compiler（自研 YAML DSL + Rust 编译器 + MCP/Agent 协议） | DSL 部分已被取代 |
| [03-competitor-research.md](03-competitor-research.md) | 竞品研究：Spec Kit / Overplane / Prodara / Tessl / Rishon / fromSpec / Loopar / Vortex | 仍然有效 |
| [04-forma-dsl-blueprint-v0.1.md](04-forma-dsl-blueprint-v0.1.md) | Forma v0.1 技术蓝图：自研 DSL 8 类 + Canonical IR + Studio + 增量编译 + Semantic Patch | **本次调整对象** |
| [05-naming-and-brand.md](05-naming-and-brand.md) | 命名过程（SpecForge 等候选 → 定名 Forma）与品牌定位 | 已定案 |
| `raw/` | 321 段原始提取文本 + manifest.json + 分享页 HTML 源文件 | 存档 |

## 对话演化时间线

1. **起点观点**：AI Coding 的默认工作方式（AI 写代码）在浪费 Token——100 万 Token 里真正有价值的可能只有 1 万。写成争议性文章《我认为 AI Coding 最大的骗局：让 AI 写代码》，技术落点为 Specification-Driven Software Generation。
2. **升级为项目**：不再只做文章，制定完整方案。目标产品：通用 AI Coding 的规格生产引擎（支持 Codex / Claude Code / Pi 等）。
3. **第一版方案《AI Specification Compiler》**：自研 YAML DSL（spec/entities/ui/logic，`spec_version: 1`）、Rust 编译器管线（Parser → Schema/Reference/Type/Semantic Validation → AST → Dependency Graph → Codegen）、插件式目标（react/vue/svelte、hono/express/axum、sqlite/mysql/pg）、MCP `speccompiler` 服务 + Semantic Patch 协议 + 事务式构建（build 失败即回滚）、MVP 只做标准 CRUD 全栈 Web 应用。
4. **竞品研究**：确认这不是「再做一遍 Spec Kit」。最接近的产品：**Rishon**（专用 DSL + Compiler + Ontology Graph + VM）、**fromSpec**（AI 生成 spec.json → 用户审核 → 确定性编译）；Overplane / Prodara / Tessl 部分重合。用户明确区分：不要可视化 DSL 编辑器贴皮、不要图模型驱动、不要低代码脚手架。
5. **用户澄清原始意图（关键转折）**：「不要让 AI 写代码，而是让 AI 生产规格……规格变化是一个 DSL + 一个优质的可视化 + 编译系统，从而让 token 更有价值」——由此 ChatGPT 承认此前归类为 Spec-Driven Development 不准确，Forma 要改变的是 AI Coding 的**生产方式**。
6. **Forma v0.1 蓝图**：正式定义 Forma = AI-Native Software Compiler，自研 8 类 DSL（App/Data/UI/Flow/Rule/API/Theme/Test）作为软件唯一事实来源，配 Canonical IR / Semantic Graph、可视化 Studio、确定性编译器、Semantic Patch Protocol、Verification System、增量编译。
7. **定名 Forma**：Form（形式化规格）+ 科技感后缀，Slogan「Describe software. Compile reality.」

## 与当前「Specification Federation」新提案的衔接

新提案对 v0.1 蓝图的核心调整：**不再发明 Forma DSL，改为组织现有成熟标准**（SQL DDL、OpenAPI 3.1、JSON Schema 2020-12、BPMN 2.0、DMN、AsyncAPI、JSON Forms、C4/Structurizr），Forma 自身只保留四项技术资产：Manifest（forma.yaml）、实体身份协议（`forma://` URI）、语义关联模型（Unified Semantic Graph）、Semantic Patch API。

对照结论：

- **被替换**：蓝图第 4-5 章（8 类自研 DSL 的语法与示例）。
- **被强化但内核不变**：第 6 章 Canonical IR / Semantic Graph（从「自研 DSL 的 IR」升级为「跨标准语义图」）；第 10 章 Semantic Patch Protocol（AI 仍然只做语义操作，不直接写文件）；第 9 章增量编译；第 11 章 Verification System。
- **新提案的真正差异化**（竞品研究中的空白地带，没有任何产品做到）：**跨标准语义变更传播**——「Order 加一个付款状态」一条指令，同步 SQL / JSON Schema / OpenAPI / BPMN / UI Schema，再做一致性校验与增量编译。
- **BPMN 边界提醒**（v0.1 蓝图 Runtime 章与新提案一致）：BPMN 编译不是转成几个 TS 函数；涉及持久化任务、重试、补偿、长事务需要工作流运行时，MVP 先支持有限子集。

## 提取方式说明

分享页为 React 服务端序列化数据（turbo-stream），消息正文以转义字符串内嵌在 HTML 中。`raw/chatgpt-share-source.html` 为原始页面（2.3 MB），`raw/manifest.json` 为 321 段提取文本的索引（含 payload 偏移与长度），`raw/s_*.txt` 按偏移量命名的原文段落。GenUI 组件包装（`<box>`/`<text>` 等）为 ChatGPT 渲染产物，主文档均已选用无包装的干净版本。
