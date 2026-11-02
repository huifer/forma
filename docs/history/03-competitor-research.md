# 竞品研究合集

> 来源：[ChatGPT 分享 6ae81e37](https://chatgpt.com/share/6ae81e37-c000-83ee-9a47-77fec81cd99a)
> 覆盖：Spec Kit / OpenSpec 等规格驱动工具、Overplane、Prodara、Tessl、Rishon、fromSpec、Loopar、Sirius Web、Vortex 等。
> 关键结论：Rishon（专用 DSL + Compiler + Ontology Graph + VM）与 fromSpec 在理念上最接近；但没有任何产品做「跨标准语义变更传播」，这是 Forma 的差异化空间。



---

## 第一轮：五类相邻产品（Spec-driven / AI-to-DSL / 低代码 / 工作流编译 / 可视化图模型）

有，而且不只是一个。

我重新审视了一下你的产品构想，结论是：**你想做的东西已经有多个相邻产品，甚至有产品已经实现了其中一部分核心技术。**

但我认为，目前需要重点研究的不是普通 AI Coding 工具，而是以下五类产品：

1. **Spec-driven Development**：AI 先生成规格，再开发。
2. **AI-to-DSL / Compiler**：AI 生成结构化描述，由编译器生成代码。
3. **Visual / Schema-driven App Builder**：通过结构化模型生产完整应用。
4. **Code Generation Infrastructure**：定义统一协议，生成不同语言的代码。
5. **Agent Coding Infrastructure**：为 Codex、Claude Code、Pi 提供统一的工程执行和验证能力。

你的产品恰好处于这些领域的交叉位置。

但要区分一个关键问题：

**有人做过 AI 生成 Spec，有人做过 DSL 编译器，也有人做过增量构建。你要验证的是，有没有人把这些能力组合成一个 Agent 无关、Spec 为核心、可增量编译、可验证和回滚的软件生产引擎。**

# 一、最值得关注的竞品：已经发现两个非常接近的产品

**1\. entity["software","Overplane","Spec-driven AI coding build and verification system"]** (高度重合)

image_group{"query":["Overplane AI coding spec driven development software website screenshot"],"aspect_ratio":"16:9","layout":"carousel","num_per_query":1}

官网：urloverplane.devhttps://www.overplane.dev/ · 开源 Apache-2.0

它已经实现了你方案中非常多的关键设计：

- 支持 Codex、Claude Code、OpenCode。
- Markdown Spec 作为规格来源。
- Spec 转换成 IR。
- 使用 Z3 做规格一致性验证。
- Agent 在隔离容器中运行。
- 内容 Hash 和增量缓存。
- 构建结果可追踪、可重放。
- AI 负责生成软件，系统负责管理生产流程。

它甚至已经有 `raise → verify → codegen` 三阶段编译流程。citeturn293874search0turn293874search2turn293874search4turn293874search5

**判断：这是你必须第一个下载、安装、实际运行的项目。**

不过有一个重要区别：它的 IR 是由 AI 从自然语言规格中提取的形式化模型，后续代码生成仍由 Coding Agent 完成。它不是完全由确定性 Target Compiler 根据统一业务 AST 生成全部源代码。citeturn293874search4turn293874search9

**2\. entity["software","Prodara","AI product specification compiler and incremental code generation"]** (高度重合)

GitHub：urlprodara-ai/prodarahttps://github.com/prodara-ai/prodara

它的项目描述几乎直接命中了你最初的设想：

- AI 生成 `.prd` 规格文件。
- 编译为 Typed Product Graph。
- 13 阶段验证流水线。
- 语义 Diff。
- 自动分析变更影响。
- 结构化、增量代码生成。
- Agent 根据编译器提供的影响范围执行开发。

它描述的核心理念是：Give your AI a compiler, not a pile of markdown。citeturn293874search7

**判断：这是最值得研究的 DSL + IR + Semantic Graph + Incremental Code Generation 方向的直接竞品。**

特别需要研究它的 Product Graph 和 13 阶段 Pipeline，而不是只看它的宣传页面。

这两个项目会显著改变我的建议：你不能再把「Agent + Spec + IR + 增量编译」整体当成尚未有人涉足的创新。

---

# 二、其他值得研究的具体产品

以下按照与你的技术路线的接近程度排列。

image_group{"query":["Tessl AI software specification framework website logo"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**3\. entity["software","Tessl","Specification-driven AI software development platform"]**

(Spec-as-Source)

url官网https://tessl.io/ · url技术分析https://www.martinfowler.com/articles/exploring-gen-ai/sdd-3-tools.html

核心思路是规格长期作为主要资产，代码可以标记为由 Spec 生成，提供 MCP 和规格注册机制。

重点研究：Spec 与代码映射、规格版本化、测试关联。

citeturn293874search11turn293874search13

---

image_group{"query":["AWS Kiro IDE specification driven development screenshot"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**4\. entity["software","AWS Kiro","AI development IDE with spec-driven workflows"]**

url官网https://kiro.dev/

Requirements → Design → Tasks → Implementation。

重点研究：需求结构化、验收标准、规格和实现之间的关联。

citeturn725161search2turn725161search0

---

image_group{"query":["GitHub Spec Kit logo specification driven development"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**5\. entity["software","GitHub Spec Kit","Open-source specification-driven development toolkit"]**

urlGitHubhttps://github.com/github/spec-kit

Constitution → Specify → Plan → Tasks → Implement。

重点研究：跨 Agent 工作流、规格组织和变更管理。

citeturn725161search0turn725161search1

---

image_group{"query":["OpenSpec AI specification driven development GitHub logo"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**6\. entity["software","OpenSpec","Open-source change-based specification development toolkit"]**

urlGitHubhttps://github.com/Fission-AI/OpenSpec

通过变更提案、规格差异、设计和任务文件组织开发。

重点研究：Spec Patch、Change Lifecycle、规格归档。

citeturn725161search0turn725161search11

---

image_group{"query":["Rishon declarative programming language software compiler website"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**7\. entity["software","Rishon","Declarative business software language and compiler"]**

url官网https://rishon.com/

声明式、静态验证、类型安全的多语言 DSL，编译成业务本体图，并配备 AI 产品和开发 Agent。

重点研究：业务语义 DSL、Ontology、编译器与 AI 协作。

citeturn293874search8

---

image_group{"query":["Ryngo AI codebase graph compiler interface"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**8\. entity["software","Ryngo","Deterministic codebase graph compiler for AI agents"]**

url官网https://ryngo.ai/

把现有代码库编译成类型化图、紧凑 IR 和局部子图，通过 MCP 提供给 Agent。

重点研究：代码语义图、上下文压缩、局部依赖查询。

citeturn725161search4

---

image_group{"query":["Telos AI software requirements goal based engineering platform"],"aspect_ratio":"1:1","layout":"carousel","num_per_query":1}

**9\. entity["software","Telos","Goal-driven AI software build and maintenance platform"]**

url官网https://usetelos.ai/

把 Goal 作为持久化的软件契约，连接 Claude Code、Codex 等 Agent，管理 Build、Test、Deploy 和持续维护。

重点研究：长期需求契约、软件持续验证。

citeturn293874search10

# 三、竞品能力矩阵：到底谁已经做到了什么？

这是我认为最有价值的对比。

| 核心能力 | Overplane | Prodara | Tessl | Spec Kit |
|--------|--------|--------|--------|--------|
| AI 生成规格 | 是 | 是 | 是 | 是 |
| 结构化 DSL | 部分 | 是 | 是 | 主要 Markdown |
| Canonical IR | 是 | Product Graph | 有规格模型 | 非核心 |
| 语义验证 | 是 | 是 | 规格约束 | 有分析流程 |
| 依赖图 | 部分 | 是 | 有映射关系 | 非核心 |
| 增量构建 | 是 | 是 | 有相关能力 | 非核心 |
| 确定性代码生成 | 部分 | 目标之一 | 目标之一 | 否 |
| Agent 无关 | 是 | 需要进一步核查 | 是 | 是 |
| MCP | 需要核查具体版本 | 需要核查 | 是 | 依赖 Agent 集成 |
| 独立构建验证 | 是 | 需要核查 | 测试关联 | 主要由 Agent 执行 |
| 回滚与事务构建 | 需进一步验证 | 需进一步验证 | 需进一步验证 | 非核心 |

说明：表格中的「部分」「非核心」不代表产品完全不具备该功能，而是其公开资料没有把它作为核心能力。对于 Prodara、Overplane 等新项目，具体功能和实现成熟度还需要源码级审计，不能把宣传描述等同于已经通过生产验证。

## 四、最重要的发现：你原本的产品定义需要调整

我会把原本的构想分成三个层次：

## Layer 1：Spec-driven Agent Workflow

(竞争已经比较充分)

- AI 生成规格。
- 规格驱动开发。
- 多 Agent 接入。
- 任务管理。
- 测试与验证。

代表：Spec Kit、OpenSpec、Kiro、Overplane。

---

## Layer 2：Semantic Specification Compiler

(已有直接竞品)

- DSL。
- AST / IR。
- Semantic Graph。
- 影响分析。
- Incremental Compilation。
- 结构化代码生产。

代表：Prodara、Overplane、Tessl。

---

## Layer 3：Universal Deterministic Software Production

(建议重点研究)

- AI 输出最小化 Patch。
- 统一类型化软件模型。
- 确定性 Target Compiler。
- 多语言、多平台生成。
- 生成代码和原生代码混合管理。
- 独立验证生成结果。
- 跨 Agent 的软件语义操作协议。

我认为第三层才值得你继续寻找差异化。

但必须强调：这不是已经证明没有竞争者的空白市场。它是从现有产品中提炼出来的差异化假设，需要逐项做竞品验证。

# 五、我建议你重点拆解的三个项目

不要同时研究十几个项目。先深入这三个。

(第一优先级)

**Overplane**

研究它的整个生产流水线：

1. Spec 如何组织？
2. Raise 如何生成 IR？
3. Z3 如何验证？
4. Agent 如何在容器里执行？
5. 如何进行 Hash 缓存？
6. 如何控制生成结果的回写？

urlIR Generation 文档https://www.overplane.dev/guide/advanced-topics/ir-generation · urlCodegen 文档https://www.overplane.dev/guide/advanced-topics/codegen

---

(第二优先级)

**Prodara**

研究它的：

1. `.prd` DSL。
2. Typed Product Graph。
3. 13 阶段编译流水线。
4. Semantic Diff。
5. 影响范围分析。
6. Incremental Code Generation。

urlGitHub 源码https://github.com/prodara-ai/prodara

---

(第三优先级)

**Tessl**

研究它的：

1. Spec-as-Source。
2. Spec 与代码映射。
3. Spec 与测试关联。
4. MCP 集成。
5. 规格版本与复用。

urlTessl 官网https://tessl.io/ · urlMartin Fowler 技术分析https://www.martinfowler.com/articles/exploring-gen-ai/sdd-3-tools.html

# 六、最终创业决策

我会修改之前给你的产品战略。

原来的：

> 做一个通用 AI Specification Compiler。

现在我认为还不够差异化。

我建议改为：

# Agent Software Compiler

The deterministic execution layer for AI coding agents.

**核心定位：**

让 Codex、Claude Code、Pi 不再通过大量源代码操作来完成所有软件开发任务，而是通过结构化语义操作，调用确定性编译器完成软件生产。

**三个核心产品创新：**

1. Semantic Patch Protocol：Agent 只提交最小业务变更。
2. Deterministic Target Compiler：真正由编译器生成标准代码，而不是每次再让 Agent 写代码。
3. Verified Change Engine：每次修改都有影响分析、构建证据和回滚能力。

我会给这个方向一个明确的研发优先级：

| 研发方向 | 优先级 |
|--------|--------|
| Semantic Patch Protocol | P0 |
| Typed Canonical IR | P0 |
| 确定性代码生成器 | P0 |
| Incremental Compilation | P0 |
| Independent Verification | P0 |
| Agent Adapter | P1 |
| 多语言 Target | P2 |
| 可视化 IDE | P3 |

**最关键的差异：Overplane 更偏向让 Agent 在受控环境中生产和验证代码；你可以尝试让编译器直接生产代码，让 Agent 主要操作软件语义。**

Prodara 已经覆盖了这条路线的相当一部分，因此必须先通过源码和实际运行验证它的实现边界。

---

## 七、我建议下一步做一次真正的竞品源码审计

不要再继续凭概念设计产品了。现在最有价值的是验证 Overplane 和 Prodara 到底实现到了什么程度。

我建议审计这六项：

1. **DSL**：是否真正有类型系统，还是仅仅结构化 Markdown/YAML？
2. **IR**：是否是可独立操作、可查询、可转换的语义模型？
3. **Compiler**：是否由确定性生成器产出代码，还是 Agent 仍然承担主要代码生成？
4. **Incremental**：是否有真实的依赖 DAG、内容寻址和局部重编译？
5. **Reliability**：是否有事务式生成、独立测试、失败回滚？
6. **Agent Protocol**：能否让不同 Agent 对同一份 Spec 进行并发、可控的语义修改？

最终输出应该是：逐项源码证据、已实现功能、缺失功能、架构缺陷，以及你可以切入的独立创新点。

**我的建议是：先审计 Overplane 和 Prodara。尤其是 Prodara。它可能比你之前设想的 SpecForge 更接近你真正要做的产品。**

如果审计发现它们已经把核心能力做得足够成熟，那么就不应该再重复造一个，而应该寻找它们没有解决的可靠性和通用软件生产问题。

---

## Overplane 与 Prodara：最接近的两个产品

# 一、最值得关注的竞品：已经发现两个非常接近的产品

<box border radius="lg" padding={3} gap={2}>
  <row align="center" justify="between">
    **1. <Entity value="Overplane" category="software" disambig="Spec-driven AI coding build and verification system"/>**
    <badge color="danger">高度重合</badge>
  </row>
  <AsyncImage query="Overplane AI coding spec driven development software website screenshot" aspectRatio="16:9" width="100%" fit="contain"/>
  <text color="secondary" size="xs">官网：<Link url="https://www.overplane.dev/" title="overplane.dev"/> · 开源 Apache-2.0</text>
  它已经实现了你方案中非常多的关键设计：

  - 支持 Codex、Claude Code、OpenCode。
  - Markdown Spec 作为规格来源。
  - Spec 转换成 IR。
  - 使用 Z3 做规格一致性验证。
  - Agent 在隔离容器中运行。
  - 内容 Hash 和增量缓存。
  - 构建结果可追踪、可重放。
  - AI 负责生成软件，系统负责管理生产流程。

  它甚至已经有 `raise → verify → codegen` 三阶段编译流程。<Cite refs={["turn293874search0","turn293874search2","turn293874search4","turn293874search5"]}/>

  **判断：这是你必须第一个下载、安装、实际运行的项目。**

  不过有一个重要区别：它的 IR 是由 AI 从自然语言规格中提取的形式化模型，后续代码生成仍由 Coding Agent 完成。它不是完全由确定性 Target Compiler 根据统一业务 AST 生成全部源代码。<Cite refs={["turn293874search4","turn293874search9"]}/>
</box>
<box border radius="lg" padding={3} gap={2}>
  <row align="center" justify="between">
    **2. <Entity value="Prodara" category="software" disambig="AI product specification compiler and incremental code generation"/>**
    <badge color="danger">高度重合</badge>
  </row>
  <text color="secondary" size="xs">GitHub：<Link url="https://github.com/prodara-ai/prodara" title="prodara-ai/prodara"/></text>
  它的项目描述几乎直接命中了你最初的设想：

  - AI 生成 `.prd` 规格文件。
  - 编译为 Typed Product Graph。
  - 13 阶段验证流水线。
  - 语义 Diff。
  - 自动分析变更影响。
  - 结构化、增量代码生成。
  - Agent 根据编译器提供的影响范围执行开发。

  它描述的核心理念是：Give your AI a compiler, not a pile of markdown。<Cite refs={["turn293874search7"]}/>

  **判断：这是最值得研究的 DSL + IR + Semantic Graph + Incremental Code Generation 方向的直接竞品。**

  特别需要研究它的 Product Graph 和 13 阶段 Pipeline，而不是只看它的宣传页面。
</box>

这两个项目会显著改变我的建议：你不能再把「Agent + Spec + IR + 增量编译」整体当成尚未有人涉足的创新。

---

# 二、其他值得研究的具体产品

以下按照与你的技术路线的接近程度排列。

<box gap={3}>
  <row align=start gap={3}>
    <AsyncImage query="Tessl AI software specification framework website logo" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **3. <Entity value="Tessl" category="software" disambig="Specification-driven AI software development platform"/>**
      <badge color="danger">Spec-as-Source</badge>
      <text color="secondary" size="xs"><Link url="https://tessl.io/" title="官网"/> · <Link url="https://www.martinfowler.com/articles/exploring-gen-ai/sdd-3-tools.html" title="技术分析"/></text>
      核心思路是规格长期作为主要资产，代码可以标记为由 Spec 生成，提供 MCP 和规格注册机制。

      重点研究：Spec 与代码映射、规格版本化、测试关联。
      <Cite refs={["turn293874search11","turn293874search13"]}/>
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="AWS Kiro IDE specification driven development screenshot" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **4. <Entity value="AWS Kiro" category="software" disambig="AI development IDE with spec-driven workflows"/>**
      <text color="secondary" size="xs"><Link url="https://kiro.dev/" title="官网"/></text>
      Requirements → Design → Tasks → Implementation。

      重点研究：需求结构化、验收标准、规格和实现之间的关联。
      <Cite refs={["turn725161search2","turn725161search0"]}/>
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="GitHub Spec Kit logo specification driven development" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **5. <Entity value="GitHub Spec Kit" category="software" disambig="Open-source specification-driven development toolkit"/>**
      <text color="secondary" size="xs"><Link url="https://github.com/github/spec-kit" title="GitHub"/></text>
      Constitution → Specify → Plan → Tasks → Implement。

      重点研究：跨 Agent 工作流、规格组织和变更管理。
      <Cite refs={["turn725161search0","turn725161search1"]}/>
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="OpenSpec AI specification driven development GitHub logo" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **6. <Entity value="OpenSpec" category="software" disambig="Open-source change-based specification development toolkit"/>**
      <text color="secondary" size="xs"><Link url="https://github.com/Fission-AI/OpenSpec" title="GitHub"/></text>
      通过变更提案、规格差异、设计和任务文件组织开发。

      重点研究：Spec Patch、Change Lifecycle、规格归档。
      <Cite refs={["turn725161search0","turn725161search11"]}/>
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="Rishon declarative programming language software compiler website" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **7. <Entity value="Rishon" category="software" disambig="Declarative business software language and compiler"/>**
      <text color="secondary" size="xs"><Link url="https://rishon.com/" title="官网"/></text>
      声明式、静态验证、类型安全的多语言 DSL，编译成业务本体图，并配备 AI 产品和开发 Agent。

      重点研究：业务语义 DSL、Ontology、编译器与 AI 协作。
      <Cite refs={["turn293874search8"]}/>
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="Ryngo AI codebase graph compiler interface" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **8. <Entity value="Ryngo" category="software" disambig="Deterministic codebase graph compiler for AI agents"/>**
      <text color="secondary" size="xs"><Link url="https://ryngo.ai/" title="官网"/></text>
      把现有代码库编译成类型化图、紧凑 IR 和局部子图，通过 MCP 提供给 Agent。

      重点研究：代码语义图、上下文压缩、局部依赖查询。
      <Cite refs={["turn725161search4"]}/>
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="Telos AI software requirements goal based engineering platform" aspectRatio="1:1" maxWidth="88px"/>
    <box flex="1" gap={1}>
      **9. <Entity value="Telos" category="software" disambig="Goal-driven AI software build and maintenance platform"/>**
      <text color="secondary" size="xs"><Link url="https://usetelos.ai/" title="官网"/></text>
      把 Goal 作为持久化的软件契约，连接 Claude Code、Codex 等 Agent，管理 Build、Test、Deploy 和持续维护。

      重点研究：长期需求契约、软件持续验证。
      <Cite refs={["turn293874search10"]}/>
    </box>
  </row>
</box>

---

## 竞品能力矩阵：Overplane / Prodara / Tessl / Spec Kit

# 三、竞品能力矩阵：到底谁已经做到了什么？

这是我认为最有价值的对比。

| 核心能力 | Overplane | Prodara | Tessl | Spec Kit |
|---|:---:|:---:|:---:|:---:|
| AI 生成规格 | 是 | 是 | 是 | 是 |
| 结构化 DSL | 部分 | 是 | 是 | 主要 Markdown |
| Canonical IR | 是 | Product Graph | 有规格模型 | 非核心 |
| 语义验证 | 是 | 是 | 规格约束 | 有分析流程 |
| 依赖图 | 部分 | 是 | 有映射关系 | 非核心 |
| 增量构建 | 是 | 是 | 有相关能力 | 非核心 |
| 确定性代码生成 | 部分 | 目标之一 | 目标之一 | 否 |
| Agent 无关 | 是 | 需要进一步核查 | 是 | 是 |
| MCP | 需要核查具体版本 | 需要核查 | 是 | 依赖 Agent 集成 |
| 独立构建验证 | 是 | 需要核查 | 测试关联 | 主要由 Agent 执行 |
| 回滚与事务构建 | 需进一步验证 | 需进一步验证 | 需进一步验证 | 非核心 |

说明：表格中的「部分」「非核心」不代表产品完全不具备该功能，而是其公开资料没有把它作为核心能力。对于 Prodara、Overplane 等新项目，具体功能和实现成熟度还需要源码级审计，不能把宣传描述等同于已经通过生产验证。

## 四、最重要的发现：你原本的产品定义需要调整

我会把原本的构想分成三个层次：

<box border radius="lg" padding={3} gap={3}>
  <title size="lg">Layer 1：Spec-driven Agent Workflow</title>
  <badge color="warning">竞争已经比较充分</badge>
  - AI 生成规格。
  - 规格驱动开发。
  - 多 Agent 接入。
  - 任务管理。
  - 测试与验证。

  代表：Spec Kit、OpenSpec、Kiro、Overplane。
  <divider color="subtle"/>
  <title size="lg">Layer 2：Semantic Specification Compiler</title>
  <badge color="warning">已有直接竞品</badge>
  - DSL。
  - AST / IR。
  - Semantic Graph。
  - 影响分析。
  - Incremental Compilation。
  - 结构化代码生产。

  代表：Prodara、Overplane、Tessl。
  <divider color="subtle"/>
  <title size="lg">Layer 3：Universal Deterministic Software Production</title>
  <badge color="success">建议重点研究</badge>
  - AI 输出最小化 Patch。
  - 统一类型化软件模型。
  - 确定性 Target Compiler。
  - 多语言、多平台生成。
  - 生成代码和原生代码混合管理。
  - 独立验证生成结果。
  - 跨 Agent 的软件语义操作协议。
</box>

我认为第三层才值得你继续寻找差异化。

但必须强调：这不是已经证明没有竞争者的空白市场。它是从现有产品中提炼出来的差异化假设，需要逐项做竞品验证。

---

## 我们的方向与现有产品的真正区别

## 1. 你的方向与现有产品的真正区别

我把它们重新放在一起比较：

| 方向 | AI 生产什么 | 谁负责生成程序 | 核心载体 |
|---|---|---|---|
| 传统 AI Coding | 源代码 | AI | TS、Python、HTML 等 |
| Spec Kit / Kiro | Markdown 需求、设计和任务 | AI | Markdown |
| Overplane | 结构化规格、形式化约束 | 主要仍由 AI Agent 编写代码 | YAML、SMT-LIB 等 |
| 传统 Low-Code | 组件、流程、数据模型 | 平台编译器或运行时 | 可视化编辑器 |
| **你的方向** | **可编译的 DSL 规格** | **确定性编译器** | **DSL + 可视化编辑器** |

关键差异在于：**你不是让 AI 阅读规格后继续写代码，而是让规格本身成为程序的源代码。**

这意味着你做的不是一个更好的 AI Coding Agent，而是一种新的软件生产范式。

## 2. 我建议你采用的产品模型

<box border radius="lg" gap={3} padding={3}>
  <title size="lg">AI-Native Software Compiler</title>
  <text color="secondary" size="xs">从自然语言到可执行程序，AI 不直接编写业务源代码</text>
  <box background="rgba(59,130,246,0.08)" radius="md" padding={3} align="center" gap={1}>
    **用户需求 / 自然语言**
    <text color="secondary" size="xs">What should the software do?</text>
  </box>
  <box align="center">
    <icon name="arrow-down" color="secondary" size="lg"/>
  </box>
  <box background="rgba(59,130,246,0.08)" radius="md" padding={3} align="center" gap={1}>
    **AI Specification Engine**
    <text color="secondary" size="xs">理解需求、拆解业务、生成 DSL、修改语义节点</text>
  </box>
  <box align="center">
    <icon name="arrow-down" color="secondary" size="lg"/>
  </box>
  <box border={{size:1,color:"#3B82F6"}} radius="lg" padding={3} gap={2}>
    <row align="center" justify="between">
      **Canonical DSL**
      <badge color="success">核心资产</badge>
    </row>
    <grid columns={2} gap={2}>
      <grid-item background="surface-secondary" radius="md" padding={2}>
        <icon name="database" size="lg"/>
        **Data DSL**
        <caption>数据模型、关系、约束</caption>
      </grid-item>
      <grid-item background="surface-secondary" radius="md" padding={2}>
        <icon name="layout" size="lg"/>
        **UI DSL**
        <caption>页面、组件、布局</caption>
      </grid-item>
      <grid-item background="surface-secondary" radius="md" padding={2}>
        <icon name="workflow" size="lg"/>
        **Flow DSL**
        <caption>事件、流程、状态</caption>
      </grid-item>
      <grid-item background="surface-secondary" radius="md" padding={2}>
        <icon name="shield-check" size="lg"/>
        **Rule DSL**
        <caption>权限、校验、业务规则</caption>
      </grid-item>
    </grid>
    <divider color="subtle"/>
    <row align="center" gap={2}>
      <icon name="monitor" color="secondary"/>
      **Visual Specification Editor**
      <spacer/>
      <badge>DSL 的可视化投影</badge>
    </row>
    <caption>画布、流程图、组件树、属性面板、关系图；可视化修改与 DSL 修改保持同步</caption>
  </box>
  <box align="center">
    <icon name="arrow-down" color="secondary" size="lg"/>
  </box>
  <box background="rgba(16,185,129,0.08)" radius="md" padding={3} align="center" gap={1}>
    **Compiler Engine**
    <text color="secondary" size="xs">Parser → Type Checker → Semantic IR → Dependency Graph → Code Generator</text>
  </box>
  <box align="center">
    <icon name="arrow-down" color="secondary" size="lg"/>
  </box>
  <box background="surface-secondary" radius="md" padding={3} align="center" gap={1}>
    **Executable Software**
    <text color="secondary" size="xs">React / TypeScript / API / Database / Tests / Deployment Artifacts</text>
  </box>
</box>

这里有一个非常重要的架构决定：

**可视化界面不是 DSL 的替代品，而是 DSL 的可视化编辑和展示层。**

DSL 才是唯一的事实来源（Single Source of Truth）。

例如用户在画布上添加一个订单审批节点，系统修改对应 DSL；AI 要增加一个审批条件，也修改相同的 DSL。两者最后进入同一个编译器。

不能让画布保存一套数据、YAML 保存另一套数据，否则迟早出现状态不一致。

## 3. DSL 应该长什么样？

我不建议你从 Markdown 开始，也不建议直接采用通用 YAML 作为最终产品语言。

你应该设计一种面向软件语义的 DSL。

例如用户要求：

> 做一个订单管理系统，支持创建订单、审批订单、取消订单，审批通过后生成发货任务。

AI 不应该输出 React、HTML、SQL，而应该生成类似这样的规格：

```yaml
app:
  id: order-management
  name: Order Management

models:
  Order:
    fields:
      id: uuid
      amount: decimal
      status:
        type: enum
        values: [pending, approved, rejected, cancelled]

    transitions:
      - from: pending
        to: approved
        event: approve
        guard: amount <= approval_limit

      - from: pending
        to: rejected
        event: reject

      - from: pending
        to: cancelled
        event: cancel

workflows:
  order_approval:
    trigger: Order.approved
    steps:
      - create: Shipment
        values:
          order_id: $event.order.id

pages:
  orders:
    route: /orders
    layout: table
    source: Order
    actions:
      - approve
      - reject
      - cancel
```

这只是概念示例，真正的 DSL 需要类型系统、引用约束、事件语义、版本管理和静态检查。

最重要的是：这份 DSL 不应该只是代码生成模板。

它需要表达完整的业务语义。

例如：

- `Order` 是业务实体，而不只是一个 TypeScript interface。
- `approve` 是状态迁移，而不只是一个按钮点击事件。
- `guard` 是可以静态检查的业务约束，而不只是随意嵌入的一段代码。
- `Shipment` 是一个领域对象，而不是某个数据库表名。

这样编译器才能理解业务，而不是机械地替换模板。

## 4. 可视化应该如何设计？

我认为你真正有机会做出差异化的地方就在这里。

不要把它做成传统的拖拽式 Low-Code 编辑器。传统低代码通常要求人手动搭建页面，你的产品应该让 AI 和人共同操作同一个语义模型。

我建议至少设计四种视图：

<box gap={3}>
  <row align=start gap={3}>
    <AsyncImage query="modern visual workflow builder node based automation canvas interface" aspectRatio="5:4" maxWidth="116px"/>
    <box flex="1" gap={1}>
      **1. Flow View — 流程视图**

      用节点和连线表达业务流程、条件分支、事件和状态迁移。适合 Workflow DSL。
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="modern visual database entity relationship diagram editor interface" aspectRatio="5:4" maxWidth="116px"/>
    <box flex="1" gap={1}>
      **2. Data View — 数据视图**

      用 ER 图表达实体、字段、关系、索引和约束。适合 Data DSL。
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="modern visual web app page builder component tree properties panel interface" aspectRatio="5:4" maxWidth="116px"/>
    <box flex="1" gap={1}>
      **3. UI View — 界面视图**

      通过组件树、布局画布和属性面板定义页面结构、交互和数据绑定。适合 UI DSL。
    </box>
  </row>
  <divider color="subtle"/>
  <row align=start gap={3}>
    <AsyncImage query="modern software architecture diagram editor service dependency graph interface" aspectRatio="5:4" maxWidth="116px"/>
    <box flex="1" gap={1}>
      **4. Architecture View — 架构视图**

      展示模块、服务、依赖、接口、数据流和权限边界。
    </box>
  </row>
</box>

这四种视图并不是四套规格，而是同一个 DSL Graph 的不同 Projection。

用户可以看流程图，也可以看结构化 DSL，还可以看最终生成的页面预览。它们必须保持双向同步。

## 5. Token 经济学：这才是你最值得研究的地方

你提到的 `Token is dollar` 是一个很好的产品切入点。

但是要注意：DSL 不一定天然比代码省 Token。设计得不好的 DSL 也可能产生大量重复字段。

真正的优化是：

**让 AI 只表达变化，让编译器负责重复劳动。**

举个例子：

传统 AI Coding：

1. 理解需求。
2. 生成组件。
3. 写 HTML。
4. 写 CSS。
5. 写状态管理。
6. 写接口。
7. 写类型。
8. 写测试。
9. 修改时重新理解和修改多个文件。

DSL 方式：

1. 理解需求。
2. 添加一个业务节点。
3. 修改两个关系。
4. 编译。
5. 验证。

例如，已经有一个订单管理系统，用户只要求增加一个审批条件：

传统模式可能要修改多个文件，重新检查相关代码。

你的模式应该只修改：

```yaml
guard: amount <= 50000
```

然后编译器自动完成相关实现更新。

当然，真正节省多少 Token，需要用相同需求、相同测试标准做 Benchmark，而不是只比较输出文本长度。

我建议你把产品核心指标定义成：

| 指标 | 衡量什么 |
|---|---|
| Tokens per Feature | 完成一个业务功能所需 Token |
| Spec-to-Code Ratio | 规格输入与代码输出的成本关系 |
| Change Propagation Cost | 一次规格变化需要修改多少内容 |
| Build Success Rate | 编译成功率 |
| Semantic Fidelity | 生成程序是否符合规格 |
| Recovery Cost | 出错后恢复和修复所需成本 |

## 6. 与传统 Low-Code 的区别

你应该研究 <Entity value="OutSystems" category="software" disambig="Low-code application development platform"/>、<Entity value="Mendix" category="software" disambig="Low-code application development platform"/>、<Entity value="Retool" category="software" disambig="Low-code application platform"/>、<Entity value="FlutterFlow" category="software" disambig="Visual application builder"/>，但不是照搬它们。

| 对比 | 传统 Low-Code | 你的产品 |
|---|---|---|
| 主要操作者 | 人 | AI + 人 |
| 规格输入 | 拖拽、表单、配置 | DSL + 可视化 + 自然语言 |
| 规格生成 | 人工搭建 | AI 生成 |
| 程序生成 | 平台引擎 | DSL Compiler |
| 修改方式 | 人工操作界面 | AI Semantic Patch |
| 核心价值 | 降低开发门槛 | 提高每个 Token 的软件产出效率 |
| 代码控制权 | 受平台限制 | 应支持导出、版本管理和独立运行 |

你需要避免成为一个封闭的低代码平台。最好让编译产物是标准代码，用户可以拿走、部署、测试和继续维护。

## 7. 我建议第一版只做什么？

不要一开始设计一个能编译所有软件的通用 DSL。

我会选择一个具体目标：

<badge color="success">建议的 MVP</badge> **AI CRUD / Business Application Compiler**

第一版支持：

| DSL | 功能 | 优先级 |
|---|---|---|
| Data DSL | 实体、字段、关系、约束 | P0 |
| UI DSL | 页面、表格、表单、详情页 | P0 |
| Flow DSL | CRUD、事件、状态迁移、审批流程 | P0 |
| Rule DSL | 条件、校验、权限 | P1 |
| API DSL | REST API、请求、响应 | P1 |

编译目标先固定为：

- React + TypeScript
- Hono API
- SQLite
- 标准测试代码

技术实现可以考虑 Rust Compiler Core + TypeScript Runtime。

第一版不需要支持任意复杂算法。复杂算法、特殊集成可以通过受控的原生代码扩展实现，但必须明确标记为外部扩展，不能让 AI 偷偷绕过 DSL 直接生成一大堆代码。

---

## 8. 我对这个创业方向的最终判断

我会把它定义为：

<box border radius="lg" padding={3} gap={2}>
  <title size="2xl">AI Software Compiler</title>
  **An executable specification system for AI-native software development.**

  AI 生产结构化软件规格，可视化界面负责理解和编辑规格，确定性编译器负责生成程序。

  <divider color="subtle"/>
  **不是：** AI Coding Agent、Markdown Spec 管理器、传统拖拽式 Low-Code。

  **而是：** 用 DSL 取代 AI 直接编写大部分重复性源代码。
</box>

我认为你真正需要建立的壁垒有三个：

1. **DSL Language Design**：一套足够表达软件业务语义，同时对 AI 友好的语言。
2. **Visual Specification Editor**：让人和 AI 都能高效读写同一份规格。
3. **Deterministic Compiler**：让规格真正成为可执行的软件源代码，而不是 AI 继续编程的参考文档。

还有一个非常关键的战略问题：不要把 DSL 做成一套更短的 YAML。它必须有自己的语义、类型系统、依赖关系、版本演进机制和编译规则。

**我建议下一步先不要写完整产品 PRD，而是先做 DSL 语言设计研究。** 这决定了整个产品的上限：DSL 如果设计错了，后面的可视化编辑器和编译器都会变成补丁工程。

---

## 为什么不是 Spec-Driven Development：改变 AI Coding 的生产方式

是的。你这次补充后，我认为之前把你的方向归类为 **Spec-Driven Development（规格驱动开发）**，确实不够准确。

你真正想做的不是让 AI 先写一份更好的 Markdown 规格，再让 AI 根据规格写代码。

而是要改变 AI Coding 的生产方式：

**AI 不再直接生产源代码，而是生产一种可视化、结构化、可编译的程序规格。规格经过 DSL 编译器，直接生成可运行的软件。**

这和目前主流的 AI Coding、Spec Kit、Kiro、OpenSpec 有本质区别。你想做的更接近：

**AI-Native Visual Programming + DSL Compiler + Low-Code Runtime。**

核心价值不是让 AI 少写几行代码，而是让每一个 Token 都尽可能转化为可执行的业务语义。

## 1\. 你的方向与现有产品的真正区别

我把它们重新放在一起比较：

| 方向 | AI 生产什么 | 谁负责生成程序 | 核心载体 |
|--------|--------|--------|--------|
| 传统 AI Coding | 源代码 | AI | TS、Python、HTML 等 |
| Spec Kit / Kiro | Markdown 需求、设计和任务 | AI | Markdown |
| Overplane | 结构化规格、形式化约束 | 主要仍由 AI Agent 编写代码 | YAML、SMT-LIB 等 |
| 传统 Low-Code | 组件、流程、数据模型 | 平台编译器或运行时 | 可视化编辑器 |
| **你的方向** | **可编译的 DSL 规格** | **确定性编译器** | **DSL + 可视化编辑器** |

关键差异在于：**你不是让 AI 阅读规格后继续写代码，而是让规格本身成为程序的源代码。**

这意味着你做的不是一个更好的 AI Coding Agent，而是一种新的软件生产范式。

## 2\. 我建议你采用的产品模型

## AI-Native Software Compiler

从自然语言到可执行程序，AI 不直接编写业务源代码

**用户需求 / 自然语言**

What should the software do?

**AI Specification Engine**

理解需求、拆解业务、生成 DSL、修改语义节点

**Canonical DSL** (核心资产)

**Data DSL**

数据模型、关系、约束

**UI DSL**

页面、组件、布局

**Flow DSL**

事件、流程、状态

**Rule DSL**

权限、校验、业务规则

---

**Visual Specification Editor**  (DSL 的可视化投影)

画布、流程图、组件树、属性面板、关系图；可视化修改与 DSL 修改保持同步

**Compiler Engine**

Parser → Type Checker → Semantic IR → Dependency Graph → Code Generator

**Executable Software**

React / TypeScript / API / Database / Tests / Deployment Artifacts

这里有一个非常重要的架构决定：

**可视化界面不是 DSL 的替代品，而是 DSL 的可视化编辑和展示层。**

DSL 才是唯一的事实来源（Single Source of Truth）。

例如用户在画布上添加一个订单审批节点，系统修改对应 DSL；AI 要增加一个审批条件，也修改相同的 DSL。两者最后进入同一个编译器。

不能让画布保存一套数据、YAML 保存另一套数据，否则迟早出现状态不一致。

## 3\. DSL 应该长什么样？

我不建议你从 Markdown 开始，也不建议直接采用通用 YAML 作为最终产品语言。

你应该设计一种面向软件语义的 DSL。

例如用户要求：

> 做一个订单管理系统，支持创建订单、审批订单、取消订单，审批通过后生成发货任务。

AI 不应该输出 React、HTML、SQL，而应该生成类似这样的规格：

```yaml
app:
  id: order-management
  name: Order Management

models:
  Order:
    fields:
      id: uuid
      amount: decimal
      status:
        type: enum
        values: [pending, approved, rejected, cancelled]

    transitions:
      - from: pending
        to: approved
        event: approve
        guard: amount <= approval_limit

      - from: pending
        to: rejected
        event: reject

      - from: pending
        to: cancelled
        event: cancel

workflows:
  order_approval:
    trigger: Order.approved
    steps:
      - create: Shipment
        values:
          order_id: $event.order.id

pages:
  orders:
    route: /orders
    layout: table
    source: Order
    actions:
      - approve
      - reject
      - cancel
```

这只是概念示例，真正的 DSL 需要类型系统、引用约束、事件语义、版本管理和静态检查。

最重要的是：这份 DSL 不应该只是代码生成模板。

它需要表达完整的业务语义。

例如：

- `Order` 是业务实体，而不只是一个 TypeScript interface。
- `approve` 是状态迁移，而不只是一个按钮点击事件。
- `guard` 是可以静态检查的业务约束，而不只是随意嵌入的一段代码。
- `Shipment` 是一个领域对象，而不是某个数据库表名。

这样编译器才能理解业务，而不是机械地替换模板。

## 4\. 可视化应该如何设计？

我认为你真正有机会做出差异化的地方就在这里。

不要把它做成传统的拖拽式 Low-Code 编辑器。传统低代码通常要求人手动搭建页面，你的产品应该让 AI 和人共同操作同一个语义模型。

我建议至少设计四种视图：

image_group{"query":["modern visual workflow builder node based automation canvas interface"],"aspect_ratio":"5:4","layout":"carousel","num_per_query":1}

**1\. Flow View — 流程视图**

用节点和连线表达业务流程、条件分支、事件和状态迁移。适合 Workflow DSL。

---

image_group{"query":["modern visual database entity relationship diagram editor interface"],"aspect_ratio":"5:4","layout":"carousel","num_per_query":1}

**2\. Data View — 数据视图**

用 ER 图表达实体、字段、关系、索引和约束。适合 Data DSL。

---

image_group{"query":["modern visual web app page builder component tree properties panel interface"],"aspect_ratio":"5:4","layout":"carousel","num_per_query":1}

**3\. UI View — 界面视图**

通过组件树、布局画布和属性面板定义页面结构、交互和数据绑定。适合 UI DSL。

---

image_group{"query":["modern software architecture diagram editor service dependency graph interface"],"aspect_ratio":"5:4","layout":"carousel","num_per_query":1}

**4\. Architecture View — 架构视图**

展示模块、服务、依赖、接口、数据流和权限边界。

这四种视图并不是四套规格，而是同一个 DSL Graph 的不同 Projection。

用户可以看流程图，也可以看结构化 DSL，还可以看最终生成的页面预览。它们必须保持双向同步。

## 5\. Token 经济学：这才是你最值得研究的地方

你提到的 `Token is dollar` 是一个很好的产品切入点。

但是要注意：DSL 不一定天然比代码省 Token。设计得不好的 DSL 也可能产生大量重复字段。

真正的优化是：

**让 AI 只表达变化，让编译器负责重复劳动。**

举个例子：

传统 AI Coding：

1. 理解需求。
2. 生成组件。
3. 写 HTML。
4. 写 CSS。
5. 写状态管理。
6. 写接口。
7. 写类型。
8. 写测试。
9. 修改时重新理解和修改多个文件。

DSL 方式：

1. 理解需求。
2. 添加一个业务节点。
3. 修改两个关系。
4. 编译。
5. 验证。

例如，已经有一个订单管理系统，用户只要求增加一个审批条件：

传统模式可能要修改多个文件，重新检查相关代码。

你的模式应该只修改：

```yaml
guard: amount <= 50000
```

然后编译器自动完成相关实现更新。

当然，真正节省多少 Token，需要用相同需求、相同测试标准做 Benchmark，而不是只比较输出文本长度。

我建议你把产品核心指标定义成：

| 指标 | 衡量什么 |
|--------|--------|
| Tokens per Feature | 完成一个业务功能所需 Token |
| Spec-to-Code Ratio | 规格输入与代码输出的成本关系 |
| Change Propagation Cost | 一次规格变化需要修改多少内容 |
| Build Success Rate | 编译成功率 |
| Semantic Fidelity | 生成程序是否符合规格 |
| Recovery Cost | 出错后恢复和修复所需成本 |

## 6\. 与传统 Low-Code 的区别

你应该研究 entity["software","OutSystems","Low-code application development platform"]、entity["software","Mendix","Low-code application development platform"]、entity["software","Retool","Low-code application platform"]、entity["software","FlutterFlow","Visual application builder"]，但不是照搬它们。

| 对比 | 传统 Low-Code | 你的产品 |
|--------|--------|--------|
| 主要操作者 | 人 | AI + 人 |
| 规格输入 | 拖拽、表单、配置 | DSL + 可视化 + 自然语言 |
| 规格生成 | 人工搭建 | AI 生成 |
| 程序生成 | 平台引擎 | DSL Compiler |
| 修改方式 | 人工操作界面 | AI Semantic Patch |
| 核心价值 | 降低开发门槛 | 提高每个 Token 的软件产出效率 |
| 代码控制权 | 受平台限制 | 应支持导出、版本管理和独立运行 |

你需要避免成为一个封闭的低代码平台。最好让编译产物是标准代码，用户可以拿走、部署、测试和继续维护。

## 7\. 我建议第一版只做什么？

不要一开始设计一个能编译所有软件的通用 DSL。

我会选择一个具体目标：

(建议的 MVP) **AI CRUD / Business Application Compiler**

第一版支持：

| DSL | 功能 | 优先级 |
|--------|--------|--------|
| Data DSL | 实体、字段、关系、约束 | P0 |
| UI DSL | 页面、表格、表单、详情页 | P0 |
| Flow DSL | CRUD、事件、状态迁移、审批流程 | P0 |
| Rule DSL | 条件、校验、权限 | P1 |
| API DSL | REST API、请求、响应 | P1 |

编译目标先固定为：

- React + TypeScript
- Hono API
- SQLite
- 标准测试代码

技术实现可以考虑 Rust Compiler Core + TypeScript Runtime。

第一版不需要支持任意复杂算法。复杂算法、特殊集成可以通过受控的原生代码扩展实现，但必须明确标记为外部扩展，不能让 AI 偷偷绕过 DSL 直接生成一大堆代码。

---

## 8\. 我对这个创业方向的最终判断

我会把它定义为：

# AI Software Compiler

**An executable specification system for AI-native software development.**

AI 生产结构化软件规格，可视化界面负责理解和编辑规格，确定性编译器负责生成程序。

---

**不是：** AI Coding Agent、Markdown Spec 管理器、传统拖拽式 Low-Code。

**而是：** 用 DSL 取代 AI 直接编写大部分重复性源代码。

我认为你真正需要建立的壁垒有三个：

1. **DSL Language Design**：一套足够表达软件业务语义，同时对 AI 友好的语言。
2. **Visual Specification Editor**：让人和 AI 都能高效读写同一份规格。
3. **Deterministic Compiler**：让规格真正成为可执行的软件源代码，而不是 AI 继续编程的参考文档。

还有一个非常关键的战略问题：不要把 DSL 做成一套更短的 YAML。它必须有自己的语义、类型系统、依赖关系、版本演进机制和编译规则。

**我建议下一步先不要写完整产品 PRD，而是先做 DSL 语言设计研究。** 这决定了整个产品的上限：DSL 如果设计错了，后面的可视化编辑器和编译器都会变成补丁工程。

---

## 建议重点拆解的三个项目（第一优先级 Overplane）

# 五、我建议你重点拆解的三个项目

不要同时研究十几个项目。先深入这三个。

<box border radius="lg" padding={3} gap={2}>
  <badge color="danger">第一优先级</badge>
  **Overplane**

  研究它的整个生产流水线：

  1. Spec 如何组织？
  2. Raise 如何生成 IR？
  3. Z3 如何验证？
  4. Agent 如何在容器里执行？
  5. 如何进行 Hash 缓存？
  6. 如何控制生成结果的回写？

  <Link url="https://www.overplane.dev/guide/advanced-topics/ir-generation" title="IR Generation 文档"/> · <Link url="https://www.overplane.dev/guide/advanced-topics/codegen" title="Codegen 文档"/>
  <divider color="subtle"/>
  <badge color="danger">第二优先级</badge>
  **Prodara**

  研究它的：

  1. `.prd` DSL。
  2. Typed Product Graph。
  3. 13 阶段编译流水线。
  4. Semantic Diff。
  5. 影响范围分析。
  6. Incremental Code Generation。

  <Link url="https://github.com/prodara-ai/prodara" title="GitHub 源码"/>
  <divider color="subtle"/>
  <badge color="info">第三优先级</badge>
  **Tessl**

  研究它的：

  1. Spec-as-Source。
  2. Spec 与代码映射。
  3. Spec 与测试关联。
  4. MCP 集成。
  5. 规格版本与复用。

  <Link url="https://tessl.io/" title="Tessl 官网"/> · <Link url="https://www.martinfowler.com/articles/exploring-gen-ai/sdd-3-tools.html" title="Martin Fowler 技术分析"/>
</box>

# 六、最终创业决策

我会修改之前给你的产品战略。

原来的：

> 做一个通用 AI Specification Compiler。

现在我认为还不够差异化。

我建议改为：

<box border radius="lg" padding={3} gap={2}>
  <title size="2xl">Agent Software Compiler</title>
  <text color="secondary" weight="medium">The deterministic execution layer for AI coding agents.</text>
  **核心定位：**

  让 Codex、Claude Code、Pi 不再通过大量源代码操作来完成所有软件开发任务，而是通过结构化语义操作，调用确定性编译器完成软件生产。

  **三个核心产品创新：**

  1. Semantic Patch Protocol：Agent 只提交最小业务变更。
  2. Deterministic Target Compiler：真正由编译器生成标准代码，而不是每次再让 Agent 写代码。
  3. Verified Change Engine：每次修改都有影响分析、构建证据和回滚能力。
</box>

我会给这个方向一个明确的研发优先级：

| 研发方向 | 优先级 |
|---|---|
| Semantic Patch Protocol | P0 |
| Typed Canonical IR | P0 |
| 确定性代码生成器 | P0 |
| Incremental Compilation | P0 |
| Independent Verification | P0 |
| Agent Adapter | P1 |
| 多语言 Target | P2 |
| 可视化 IDE | P3 |

**最关键的差异：Overplane 更偏向让 Agent 在受控环境中生产和验证代码；你可以尝试让编译器直接生产代码，让 Agent 主要操作软件语义。**

Prodara 已经覆盖了这条路线的相当一部分，因此必须先通过源码和实际运行验证它的实现边界。

---

## 七、我建议下一步做一次真正的竞品源码审计

不要再继续凭概念设计产品了。现在最有价值的是验证 Overplane 和 Prodara 到底实现到了什么程度。

我建议审计这六项：

1. **DSL**：是否真正有类型系统，还是仅仅结构化 Markdown/YAML？
2. **IR**：是否是可独立操作、可查询、可转换的语义模型？
3. **Compiler**：是否由确定性生成器产出代码，还是 Agent 仍然承担主要代码生成？
4. **Incremental**：是否有真实的依赖 DAG、内容寻址和局部重编译？
5. **Reliability**：是否有事务式生成、独立测试、失败回滚？
6. **Agent Protocol**：能否让不同 Agent 对同一份 Spec 进行并发、可控的语义修改？

最终输出应该是：逐项源码证据、已实现功能、缺失功能、架构缺陷，以及你可以切入的独立创新点。

**我的建议是：先审计 Overplane 和 Prodara。尤其是 Prodara。它可能比你之前设想的 SpecForge 更接近你真正要做的产品。**

如果审计发现它们已经把核心能力做得足够成熟，那么就不应该再重复造一个，而应该寻找它们没有解决的可靠性和通用软件生产问题。

---

## 第二轮检索：按新定义重新筛选（AI 输出规格 + 可视化 + 确定性编译）

这次我会按照你刚刚明确的产品定义重新筛选，不再把普通的 Spec-Driven Development 工具算作直接竞品。

我重点寻找的是同时具备以下特征的产品：

1. AI 主要输出结构化 DSL、JSON、图结构或其他机器可读规格，而不是源代码。
2. 有可视化编辑器，能够把规格呈现为流程图、数据模型、页面结构等。
3. 规格是程序的真实来源（Source of Truth），而不只是开发文档。
4. 有自己的解释器、运行时或编译器，将规格转换成可运行的软件。
5. 目标是减少 AI 直接编写重复性代码，让 Token 更多用于业务语义。

我会把产品分成三类：**直接竞品、技术路线相似的产品、容易被误认为竞品但实际上不同的产品。**

## 一、先给结论：有，而且有几个比之前提到的案例更接近

这次检索发现了几个值得重点关注的产品。其中有两个尤其重要：

- **Rishon**：在产品理念上非常接近你，已经采用专用 DSL、Compiler、Ontology Graph 和 Virtual Machine。
- **fromSpec**：在 AI 生成结构化规格、用户审核规格、确定性编译为完整代码这条链路上非常接近你。

另外，**Loopar、OpenPage、Graphical Agents** 分别覆盖了你构想中的可视化建模、JSON 作为事实来源、图结构编译代码等部分。

这意味着你的想法不是没有先例。但它也不意味着市场上已经有一个产品完整实现了你想做的东西。

## 二、最值得研究的六个产品

### 1\. entity["software","Rishon","AI-native custom business software platform"]：理念上最接近

(高度重合)

image_group{"query":["Rishon AI native business software platform interface","Rishon language IDE visual interface demo"],"aspect_ratio":"5:4","layout":"carousel"}

官网：urlRishonhttps://rishon.com/

它的架构非常值得你研究。

Rishon 公开描述的完整流程是：

```text
自然语言需求
    ↓
AI Product Agent
    ↓
详细产品规格
    ↓
AI Developer Agent
    ↓
Rishon Language（专用 DSL）
    ↓
Rishon Compiler
    ↓
Ontology Graph
    ↓
Rishon Virtual Machine
    ↓
业务软件 / 自动化 / UI
```

几个关键点：

- 有专门为 AI 和人设计的 Rishon Language。
- 语言描述业务语义，而不是传统编程语言的实现细节。
- 有静态校验和专门的 Compiler。
- 编译成 Ontology Graph。
- Virtual Machine 根据 Graph 决定数据结构、访问逻辑、用户界面和自动化行为。
- 不是简单地让 AI 生成 React、SQL 或 Python。citeturn427041view2turn452057search14

**与你的差别：**

Rishon 更像一个完整的 AI-native Business Software Platform。它有自己的语言和运行时，重点是让软件在自己的平台中运行。

你现在想要的则更偏向：

> AI 生产 DSL → 可视化编辑 DSL → Compiler 编译成标准程序代码 → 用户拥有程序。

这两者非常接近，但运行时和代码交付策略不同。

**我的判断：这是第一个必须深入研究的产品。**

### 2\. entity["software","fromSpec","Spec-driven AI application builder"]：AI 规格到代码最接近

(高度重合)

image_group{"query":["fromSpec spec driven AI app builder spec json interface","fromSpec AI app builder interactive prototype screens"],"aspect_ratio":"5:4"}

官网：urlfromSpechttps://fromspec.io/

它的产品逻辑是：

```text
用户与 AI 对话
    ↓
生成 spec.json
    ↓
用户审核规格
    ↓
生成可交互原型
    ↓
用户批准
    ↓
Deterministic Build
    ↓
React + FastAPI + PostgreSQL
```

它的规格包括：

- Entities
- Flows
- Screens
- Roles
- Branding
- API
- Stack

还有一个非常重要的产品设计：先从规格生成交互式原型，用户确认后才生成代码。修改需求时，修改规格，再重新编译。citeturn427041view0turn634844search1

**与你的差别：**

| 维度 | fromSpec | 你的构想 |
|--------|--------|--------|
| AI 输出 | spec.json | 专用 DSL |
| 规格载体 | 结构化 JSON | DSL + 多种可视化视图 |
| 可视化 | 交互式原型 | 原生规格编辑器 |
| 核心模型 | 实体、流程、页面等 | 统一语义模型 / DSL Graph |
| 输出 | 标准代码项目 | 标准代码项目 |
| 编译策略 | 确定性构建 | 可扩展的语义编译系统 |

fromSpec 已经很接近你的商业价值主张：不让用户反复 Prompt 改代码，而是让用户修改规格。

不过，从其公开介绍看，它目前是 Early Access 产品，我没有足够证据确认它是否已经具备你想要的多视图、图形化 DSL 双向编辑能力，也没有独立验证其编译器覆盖范围。citeturn427041view0turn634844search1

**我的判断：这是第二个必须深入研究的产品。**

### 3\. entity["software","Loopar","Open-source visual full-stack application framework"]：传统 Low-Code 路线非常接近

(架构部分重合)

image_group{"query":["Loopar build visual low code framework editor","Loopar visual application builder entity model interface"],"aspect_ratio":"5:4"}

官网：urlLooparhttps://loopar.build/
GitHub：urlLoopar Frameworkhttps://github.com/alphabit-technology/loopar-framework

Loopar 的模式：

```text
可视化设计 / JSON
       ↓
Entity Definition
       ↓
Loopar Engine
       ↓
Database + REST API
       ↓
React Forms + Views + Controllers
       ↓
可运行应用
```

它有几个与你的设想高度重合的点：

- 一个 Entity 定义可以生成多个程序层。
- 可以用可视化界面或者 JSON 定义模型、页面和工作流。
- 引擎负责解析定义、生成数据库、API、表单、视图和 Controller。
- 支持保留完整代码访问能力。
- 开源 MIT。citeturn427041view3turn452057search18

**差别：**

Loopar 更接近传统的可视化全栈开发框架。它的核心不是 AI 产生业务 DSL，而是人通过可视化界面或 JSON 进行建模。

但它证明了一个重要的商业方向：

**一个业务定义可以驱动整个应用的多个层，而不是只生成一个页面。**

### 4\. entity["software","OpenPage","Open-source JSON-first AI website builder"]：AI + JSON + 可视化编辑器

(UI 领域高度重合)

image_group{"query":["OpenPage open source AI website builder visual drag drop editor","OpenPage JSON first visual website builder interface"],"aspect_ratio":"5:4"}

GitHub：urlOpenPagehttps://github.com/buildingopen/openpage

它的核心架构：

```text
       AI Agent
          ↕
Visual Editor ↔ JSON Config
          ↓
       Renderer
          ↓
       Website
```

特点：

- JSON 是唯一事实来源。
- AI 根据自然语言生成 JSON。
- 可视化编辑器读取和修改相同 JSON。
- JSON 可以版本管理和 Diff。
- 支持导出独立 HTML。citeturn427041view1turn452057search1

但有个重要区别：它主要是网站搭建器，JSON 交给 Renderer 渲染，并不等于一个面向完整软件系统的通用 DSL Compiler。

**值得借鉴的是：AI 和可视化编辑器共同操作同一份 JSON。**

### 5\. entity["software","Graphical Agents","Open-source visual AI workflow graph compiler"]：图结构编译程序的直接案例

(技术路线高度重合)

image_group{"query":["Graphical Agents visual workflow builder React Flow editor","Graphical Agents LangGraph Google ADK graph compiler interface"],"aspect_ratio":"5:4"}

GitHub：urlGraphical Agentshttps://github.com/neo-fetch/draw-your-agents

这个项目很值得你看源码，因为它有一套清晰的编译架构：

```text
Visual Graph / draw.io
         ↓
Canonical JSON Graph IR
         ↓
Validator
         ↓
Compiler
      ↙     ↘
Google ADK  LangGraph
      ↓       ↓
 Runnable Python Projects
```

它已经实现：

- React Flow 可视化图编辑。
- Canonical Graph IR。
- JSON Schema 验证。
- 从同一份 IR 编译到 Google ADK 和 LangGraph。
- 实时生成代码预览。
- 导出可运行 Python 项目。
- Golden-file 测试验证生成结果。citeturn634844search2turn634844search3

它和你的方向很接近，但范围只聚焦 AI Agent Workflow，不是通用业务软件 DSL。

**它特别值得借鉴的是：图形编辑器不直接生成代码，而是统一写入 IR，编译器只认 IR。**

### 6\. entity["software","Eclipse Sirius","Open-source graphical DSL modeling framework"]：可视化 DSL 编辑器的底层基础设施

(技术基础设施参考)

image_group{"query":["Eclipse Sirius Web graphical DSL editor diagram modeling studio","Eclipse Sirius Web node edge diagram visual modeling interface"],"aspect_ratio":"5:4"}

官网：urlEclipse Sirius Webhttps://eclipse.dev/sirius/sirius-web.html

Sirius Web 不是一个直接与你竞争的 AI 软件生成产品，而是一个用于构建 DSL 可视化建模工具的框架。

它支持：

- 定义自己的领域语言。
- 自定义图形符号和节点。
- 用节点、连线表达模型关系。
- 使用表单编辑对象属性。
- 用多个图形视图呈现同一领域模型。
- 构建可部署在 Web 上的建模工作室。citeturn427041view4turn452057search3

这对你的意义很大：你不一定需要从零研究所有可视化 DSL 编辑器技术，但需要评估这类建模框架的架构。

---

## 三、还有几个值得纳入技术研究的产品

它们不是完整竞品，但覆盖了你的某个关键技术环节。

| 产品 | 值得研究的能力 | 为什么不是完整竞品 |
|--------|--------|--------|
| entity["software","Vortex","Visual JSON DSL workflow automation engine"] | 可视化工作流与 JSON DSL 双向同步、版本管理、执行引擎 | 主要编排工作流，不编译完整业务应用。citeturn427041view5 |
| entity["software","NAIV","Open-source TypeScript DSL API framework"] | DSL Editor、数据库结构、API 规格、数据库可视化 | 更聚焦 REST API 和后端框架。citeturn452057search15 |
| entity["software","A2UI","Declarative JSON UI protocol"] | AI 友好的 JSON UI 协议、增量生成、跨框架渲染 | 主要表达和渲染 UI，不是完整应用编译器。citeturn634844search5 |
| entity["software","Scafld","Visual backend architecture specification tool"] | 自然语言生成后端架构、可视化 Schema、导出 JSON/YAML | 规格交给 Claude Code、Cursor 等继续生成代码，不是自己完成编译。citeturn452057search16 |
| entity["software","Elll","Visual development environment"] | 可视化编辑、AST 变换、TypeScript 源码同步、AI Agent | 明确保留源代码作为开发方式，不是用 DSL 取代代码。citeturn634844search0 |

特别提一下 A2UI。它值得关注，因为它说明 AI 生成结构化 UI 描述、由确定性 Renderer 渲染，是一条独立于 HTML/JSX 生成的技术路线。但它只解决 UI 表达，不解决数据、业务流程和整个软件系统的编译问题。

## 四、把竞品放在同一张图上

下面是我根据公开架构做的主观匹配度评估，不代表产品成熟度或市场份额。

| 产品 | AI 生成规格 | 专用 DSL / IR | 原生可视化编辑 | 编译完整程序 | 与你的匹配程度 |
|--------|--------|--------|--------|--------|--------|
| Rishon | 是 | 是 | 公开资料未充分证明图形化 DSL 双向编辑 | 通过 VM 运行软件 | 极高 |
| fromSpec | 是 | JSON Spec | 有交互原型，非完整图形 DSL 编辑器 | 是 | 极高 |
| Loopar | 非核心 | JSON 定义 | 是 | 是 | 高 |
| OpenPage | 是 | JSON Config | 是 | 网站 Renderer / HTML 导出 | 高，但范围窄 |
| Graphical Agents | 非核心 | Graph IR | 是 | 是 | 高，但只做 Agent 工作流 |
| Sirius Web | 否 | 可自定义 DSL | 是 | 取决于自建语言和后端 | 技术基础设施 |
| Vortex | 非核心 | JSON DSL | 是 | 工作流执行，不是完整应用代码 | 中 |

## 五、真正值得你研究的技术路线

我认为你需要把以上产品拆成四条技术路线，而不是只研究 AI Coding 竞品。

**路线 A：专用语言与语义编译**

(最重要)

Rishon Language

Compiler

Ontology Graph

研究重点：语义类型、领域模型、静态校验、编译器和运行时。

---

**路线 B：AI 生成规格再生成程序**

(最重要)

AI

Spec

Code

研究重点：fromSpec 的规格设计、审核机制、原型生成、确定性重编译。

---

**路线 C：图形化 DSL 编辑器**

Visual Editor

Canonical IR

Compiler

研究重点：Sirius、Graphical Agents、Loopar 的建模与同步机制。

---

**路线 D：声明式 UI 与运行时**

UI DSL

Renderer

UI

研究重点：A2UI、OpenPage、Vortex 的结构化表达和渲染。

## 六、我的最终判断：你还有没有机会？

**有，但不能再把“AI + DSL + 可视化 + 编译器”本身作为创新点。**

Rishon 已经覆盖了专用语言、Compiler、语义图和 VM；fromSpec 已经覆盖了 AI 规格到确定性代码生成；Loopar 和 OpenPage 又覆盖了可视化编辑与 JSON Source of Truth。

真正值得你验证的差异化是：

### 1\. AI 原生的通用软件 DSL

不是为某一种业务写 DSL，也不是只表达页面和工作流。

让 AI 可以通过统一语义模型描述：

- 数据结构
- 页面结构
- 用户交互
- 业务逻辑
- 状态机
- 权限体系
- API
- 自动化流程

而且这些 DSL 可以组合、引用、继承和演进。

### 2\. 一个真正面向 AI 的多模态规格编辑器

我认为这是你最应该投入设计的产品界面。

同一份 DSL 可以切换为：

- 数据关系图
- 业务流程图
- 页面结构树
- UI 设计画布
- 状态机图
- 架构依赖图
- 结构化 DSL 文本

AI 不需要操作鼠标，也不需要输出大量代码。它直接修改语义节点；用户可以在可视化界面理解、检查和调整。

### 3\. 规格直接编译为标准代码

这一点要和 VM 路线区分开。

我更建议你第一阶段以标准代码交付为目标：

```text
DSL
 ↓
Canonical IR
 ↓
Semantic Validation
 ↓
Dependency Graph
 ↓
Incremental Compiler
 ↓
React / TypeScript / API / Database
 ↓
Standard Project
```

不把用户锁定在你自己的运行时里。

### 4\. Token 成本成为核心竞争指标

让 AI 只生成业务差异，而不是重复的实现细节。

例如新增一个订单审批规则，AI 只修改一个语义节点，编译器自动完成所有相关代码变更。

这才是你与传统 AI Coding 最大的差异。

---

## 七、建议的下一步

我会优先做三个产品的深度拆解：

1. **Rishon**：研究它的 DSL 语法、Compiler、Ontology Graph、VM，判断它的语言边界和可扩展性。
2. **fromSpec**：研究它的 Spec Schema、规格修改、原型与代码一致性、代码生成机制。
3. **Graphical Agents**：研究它的 IR、图形化编辑器、验证和多 Target Compiler 实现。

最终要回答的不是它们有哪些功能，而是：

- 它们的 DSL 到底表达了什么？
- 是否真正不需要 AI 写业务代码？
- 哪些功能由 Compiler 完成，哪些依赖 Runtime？
- 可视化画布如何与规格保持双向同步？
- 规格修改如何精确映射到代码增量？
- 哪些复杂业务无法被它们的 DSL 表达？
- 能否把它们的架构抽象成一个通用软件生产系统？

**我的建议：先把 Rishon 当作理念上的最大竞争参照，把 fromSpec 当作产品流程上的最大竞争参照，把 Graphical Agents 当作工程实现上的参照。**

如果你能做出一个真正通用、AI 原生、图形化、可编译为标准代码的 DSL 系统，仍然有独立产品的空间。但在投入完整开发之前，必须对这三个产品做源码和实际产物审计，而不能仅凭官网介绍判断技术差异。

---

## Rishon 与 fromSpec：理念最接近的产品

## 一、先给结论：有，而且有几个比之前提到的案例更接近

这次检索发现了几个值得重点关注的产品。其中有两个尤其重要：

- **Rishon**：在产品理念上非常接近你，已经采用专用 DSL、Compiler、Ontology Graph 和 Virtual Machine。
- **fromSpec**：在 AI 生成结构化规格、用户审核规格、确定性编译为完整代码这条链路上非常接近你。

另外，**Loopar、OpenPage、Graphical Agents** 分别覆盖了你构想中的可视化建模、JSON 作为事实来源、图结构编译代码等部分。

这意味着你的想法不是没有先例。但它也不意味着市场上已经有一个产品完整实现了你想做的东西。

## 二、最值得研究的六个产品

### 1. <Entity value="Rishon" category="software" disambig="AI-native custom business software platform"/>：理念上最接近

<badge color="danger">高度重合</badge>

<AsyncImageGroup query={["Rishon AI native business software platform interface","Rishon language IDE visual interface demo"]} aspectRatio="5:4" layout="carousel"/>

官网：<Link url="https://rishon.com/" title="Rishon"/>

它的架构非常值得你研究。

Rishon 公开描述的完整流程是：

```text
自然语言需求
    ↓
AI Product Agent
    ↓
详细产品规格
    ↓
AI Developer Agent
    ↓
Rishon Language（专用 DSL）
    ↓
Rishon Compiler
    ↓
Ontology Graph
    ↓
Rishon Virtual Machine
    ↓
业务软件 / 自动化 / UI
```

几个关键点：

- 有专门为 AI 和人设计的 Rishon Language。
- 语言描述业务语义，而不是传统编程语言的实现细节。
- 有静态校验和专门的 Compiler。
- 编译成 Ontology Graph。
- Virtual Machine 根据 Graph 决定数据结构、访问逻辑、用户界面和自动化行为。
- 不是简单地让 AI 生成 React、SQL 或 Python。<Cite refs={["turn427041view2","turn452057search14"]}/>

**与你的差别：**

Rishon 更像一个完整的 AI-native Business Software Platform。它有自己的语言和运行时，重点是让软件在自己的平台中运行。

你现在想要的则更偏向：

> AI 生产 DSL → 可视化编辑 DSL → Compiler 编译成标准程序代码 → 用户拥有程序。

这两者非常接近，但运行时和代码交付策略不同。

**我的判断：这是第一个必须深入研究的产品。**

### 2. <Entity value="fromSpec" category="software" disambig="Spec-driven AI application builder"/>：AI 规格到代码最接近

<badge color="danger">高度重合</badge>

<AsyncImageGroup query={["fromSpec spec driven AI app builder spec json interface","fromSpec AI app builder interactive prototype screens"]} aspectRatio="5:4"/>

官网：<Link url="https://fromspec.io/" title="fromSpec"/>

它的产品逻辑是：

```text
用户与 AI 对话
    ↓
生成 spec.json
    ↓
用户审核规格
    ↓
生成可交互原型
    ↓
用户批准
    ↓
Deterministic Build
    ↓
React + FastAPI + PostgreSQL
```

它的规格包括：

- Entities
- Flows
- Screens
- Roles
- Branding
- API
- Stack

还有一个非常重要的产品设计：先从规格生成交互式原型，用户确认后才生成代码。修改需求时，修改规格，再重新编译。<Cite refs={["turn427041view0","turn634844search1"]}/>

**与你的差别：**

| 维度 | fromSpec | 你的构想 |
|---|---|---|
| AI 输出 | spec.json | 专用 DSL |
| 规格载体 | 结构化 JSON | DSL + 多种可视化视图 |
| 可视化 | 交互式原型 | 原生规格编辑器 |
| 核心模型 | 实体、流程、页面等 | 统一语义模型 / DSL Graph |
| 输出 | 标准代码项目 | 标准代码项目 |
| 编译策略 | 确定性构建 | 可扩展的语义编译系统 |

fromSpec 已经很接近你的商业价值主张：不让用户反复 Prompt 改代码，而是让用户修改规格。

不过，从其公开介绍看，它目前是 Early Access 产品，我没有足够证据确认它是否已经具备你想要的多视图、图形化 DSL 双向编辑能力，也没有独立验证其编译器覆盖范围。<Cite refs={["turn427041view0","turn634844search1"]}/>

**我的判断：这是第二个必须深入研究的产品。**

### 3. <Entity value="Loopar" category="software" disambig="Open-source visual full-stack application framework"/>：传统 Low-Code 路线非常接近

<badge color="warning">架构部分重合</badge>

<AsyncImageGroup query={["Loopar build visual low code framework editor","Loopar visual application builder entity model interface"]} aspectRatio="5:4"/>

官网：<Link url="https://loopar.build/" title="Loopar"/>  
GitHub：<Link url="https://github.com/alphabit-technology/loopar-framework" title="Loopar Framework"/>

Loopar 的模式：

```text
可视化设计 / JSON
       ↓
Entity Definition
       ↓
Loopar Engine
       ↓
Database + REST API
       ↓
React Forms + Views + Controllers
       ↓
可运行应用
```

它有几个与你的设想高度重合的点：

- 一个 Entity 定义可以生成多个程序层。
- 可以用可视化界面或者 JSON 定义模型、页面和工作流。
- 引擎负责解析定义、生成数据库、API、表单、视图和 Controller。
- 支持保留完整代码访问能力。
- 开源 MIT。<Cite refs={["turn427041view3","turn452057search18"]}/>

**差别：**

Loopar 更接近传统的可视化全栈开发框架。它的核心不是 AI 产生业务 DSL，而是人通过可视化界面或 JSON 进行建模。

但它证明了一个重要的商业方向：

**一个业务定义可以驱动整个应用的多个层，而不是只生成一个页面。**

### 4. <Entity value="OpenPage" category="software" disambig="Open-source JSON-first AI website builder"/>：AI + JSON + 可视化编辑器

<badge color="warning">UI 领域高度重合</badge>

<AsyncImageGroup query={["OpenPage open source AI website builder visual drag drop editor","OpenPage JSON first visual website builder interface"]} aspectRatio="5:4"/>

GitHub：<Link url="https://github.com/buildingopen/openpage" title="OpenPage"/>

它的核心架构：

```text
       AI Agent
          ↕
Visual Editor ↔ JSON Config
          ↓
       Renderer
          ↓
       Website
```

特点：

- JSON 是唯一事实来源。
- AI 根据自然语言生成 JSON。
- 可视化编辑器读取和修改相同 JSON。
- JSON 可以版本管理和 Diff。
- 支持导出独立 HTML。<Cite refs={["turn427041view1","turn452057search1"]}/>

但有个重要区别：它主要是网站搭建器，JSON 交给 Renderer 渲染，并不等于一个面向完整软件系统的通用 DSL Compiler。

**值得借鉴的是：AI 和可视化编辑器共同操作同一份 JSON。**

### 5. <Entity value="Graphical Agents" category="software" disambig="Open-source visual AI workflow graph compiler"/>：图结构编译程序的直接案例

<badge color="warning">技术路线高度重合</badge>

<AsyncImageGroup query={["Graphical Agents visual workflow builder React Flow editor","Graphical Agents LangGraph Google ADK graph compiler interface"]} aspectRatio="5:4"/>

GitHub：<Link url="https://github.com/neo-fetch/draw-your-agents" title="Graphical Agents"/>

这个项目很值得你看源码，因为它有一套清晰的编译架构：

```text
Visual Graph / draw.io
         ↓
Canonical JSON Graph IR
         ↓
Validator
         ↓
Compiler
      ↙     ↘
Google ADK  LangGraph
      ↓       ↓
 Runnable Python Projects
```

它已经实现：

- React Flow 可视化图编辑。
- Canonical Graph IR。
- JSON Schema 验证。
- 从同一份 IR 编译到 Google ADK 和 LangGraph。
- 实时生成代码预览。
- 导出可运行 Python 项目。
- Golden-file 测试验证生成结果。<Cite refs={["turn634844search2","turn634844search3"]}/>

它和你的方向很接近，但范围只聚焦 AI Agent Workflow，不是通用业务软件 DSL。

**它特别值得借鉴的是：图形编辑器不直接生成代码，而是统一写入 IR，编译器只认 IR。**

### 6. <Entity value="Eclipse Sirius" category="software" disambig="Open-source graphical DSL modeling framework"/>：可视化 DSL 编辑器的底层基础设施

<badge color="info">技术基础设施参考</badge>

<AsyncImageGroup query={["Eclipse Sirius Web graphical DSL editor diagram modeling studio","Eclipse Sirius Web node edge diagram visual modeling interface"]} aspectRatio="5:4"/>

官网：<Link url="https://eclipse.dev/sirius/sirius-web.html" title="Eclipse Sirius Web"/>

Sirius Web 不是一个直接与你竞争的 AI 软件生成产品，而是一个用于构建 DSL 可视化建模工具的框架。

它支持：

- 定义自己的领域语言。
- 自定义图形符号和节点。
- 用节点、连线表达模型关系。
- 使用表单编辑对象属性。
- 用多个图形视图呈现同一领域模型。
- 构建可部署在 Web 上的建模工作室。<Cite refs={["turn427041view4","turn452057search3"]}/>

这对你的意义很大：你不一定需要从零研究所有可视化 DSL 编辑器技术，但需要评估这类建模框架的架构。

---

---

## 值得纳入技术研究的相邻产品（Vortex 等）

## 三、还有几个值得纳入技术研究的产品

它们不是完整竞品，但覆盖了你的某个关键技术环节。

| 产品 | 值得研究的能力 | 为什么不是完整竞品 |
|---|---|---|
| <Entity value="Vortex" category="software" disambig="Visual JSON DSL workflow automation engine"/> | 可视化工作流与 JSON DSL 双向同步、版本管理、执行引擎 | 主要编排工作流，不编译完整业务应用。<Cite refs={["turn427041view5"]}/> |
| <Entity value="NAIV" category="software" disambig="Open-source TypeScript DSL API framework"/> | DSL Editor、数据库结构、API 规格、数据库可视化 | 更聚焦 REST API 和后端框架。<Cite refs={["turn452057search15"]}/> |
| <Entity value="A2UI" category="software" disambig="Declarative JSON UI protocol"/> | AI 友好的 JSON UI 协议、增量生成、跨框架渲染 | 主要表达和渲染 UI，不是完整应用编译器。<Cite refs={["turn634844search5"]}/> |
| <Entity value="Scafld" category="software" disambig="Visual backend architecture specification tool"/> | 自然语言生成后端架构、可视化 Schema、导出 JSON/YAML | 规格交给 Claude Code、Cursor 等继续生成代码，不是自己完成编译。<Cite refs={["turn452057search16"]}/> |
| <Entity value="Elll" category="software" disambig="Visual development environment"/> | 可视化编辑、AST 变换、TypeScript 源码同步、AI Agent | 明确保留源代码作为开发方式，不是用 DSL 取代代码。<Cite refs={["turn634844search0"]}/> |

特别提一下 A2UI。它值得关注，因为它说明 AI 生成结构化 UI 描述、由确定性 Renderer 渲染，是一条独立于 HTML/JSX 生成的技术路线。但它只解决 UI 表达，不解决数据、业务流程和整个软件系统的编译问题。

## 四、把竞品放在同一张图上

下面是我根据公开架构做的主观匹配度评估，不代表产品成熟度或市场份额。

| 产品 | AI 生成规格 | 专用 DSL / IR | 原生可视化编辑 | 编译完整程序 | 与你的匹配程度 |
|---|---|---|---|---|---|
| Rishon | 是 | 是 | 公开资料未充分证明图形化 DSL 双向编辑 | 通过 VM 运行软件 | 极高 |
| fromSpec | 是 | JSON Spec | 有交互原型，非完整图形 DSL 编辑器 | 是 | 极高 |
| Loopar | 非核心 | JSON 定义 | 是 | 是 | 高 |
| OpenPage | 是 | JSON Config | 是 | 网站 Renderer / HTML 导出 | 高，但范围窄 |
| Graphical Agents | 非核心 | Graph IR | 是 | 是 | 高，但只做 Agent 工作流 |
| Sirius Web | 否 | 可自定义 DSL | 是 | 取决于自建语言和后端 | 技术基础设施 |
| Vortex | 非核心 | JSON DSL | 是 | 工作流执行，不是完整应用代码 | 中 |

## 五、真正值得你研究的技术路线

我认为你需要把以上产品拆成四条技术路线，而不是只研究 AI Coding 竞品。

<box border radius="lg" padding={3} gap={3}>
  **路线 A：专用语言与语义编译**

  <badge color="success">最重要</badge>

  <row align="center" gap={2}>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Rishon Language
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Compiler
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Ontology Graph
    </box>
  </row>
  研究重点：语义类型、领域模型、静态校验、编译器和运行时。

  <divider color="subtle"/>
  **路线 B：AI 生成规格再生成程序**

  <badge color="success">最重要</badge>

  <row align="center" gap={2}>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      AI
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Spec
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Code
    </box>
  </row>
  研究重点：fromSpec 的规格设计、审核机制、原型生成、确定性重编译。

  <divider color="subtle"/>
  **路线 C：图形化 DSL 编辑器**

  <row align="center" gap={2}>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Visual Editor
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Canonical IR
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Compiler
    </box>
  </row>
  研究重点：Sirius、Graphical Agents、Loopar 的建模与同步机制。

  <divider color="subtle"/>
  **路线 D：声明式 UI 与运行时**

  <row align="center" gap={2}>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      UI DSL
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      Renderer
    </box>
    <icon name="arrow-right" color="secondary"/>
    <box background="surface-secondary" radius="md" padding={2} flex="1" align="center">
      UI
    </box>
  </row>
  研究重点：A2UI、OpenPage、Vortex 的结构化表达和渲染。
</box>

## 六、我的最终判断：你还有没有机会？

**有，但不能再把“AI + DSL + 可视化 + 编译器”本身作为创新点。**

Rishon 已经覆盖了专用语言、Compiler、语义图和 VM；fromSpec 已经覆盖了 AI 规格到确定性代码生成；Loopar 和 OpenPage 又覆盖了可视化编辑与 JSON Source of Truth。

真正值得你验证的差异化是：

### 1. AI 原生的通用软件 DSL

不是为某一种业务写 DSL，也不是只表达页面和工作流。

让 AI 可以通过统一语义模型描述：

- 数据结构
- 页面结构
- 用户交互
- 业务逻辑
- 状态机
- 权限体系
- API
- 自动化流程

而且这些 DSL 可以组合、引用、继承和演进。

### 2. 一个真正面向 AI 的多模态规格编辑器

我认为这是你最应该投入设计的产品界面。

同一份 DSL 可以切换为：

- 数据关系图
- 业务流程图
- 页面结构树
- UI 设计画布
- 状态机图
- 架构依赖图
- 结构化 DSL 文本

AI 不需要操作鼠标，也不需要输出大量代码。它直接修改语义节点；用户可以在可视化界面理解、检查和调整。

### 3. 规格直接编译为标准代码

这一点要和 VM 路线区分开。

我更建议你第一阶段以标准代码交付为目标：

```text
DSL
 ↓
Canonical IR
 ↓
Semantic Validation
 ↓
Dependency Graph
 ↓
Incremental Compiler
 ↓
React / TypeScript / API / Database
 ↓
Standard Project
```

不把用户锁定在你自己的运行时里。

### 4. Token 成本成为核心竞争指标

让 AI 只生成业务差异，而不是重复的实现细节。

例如新增一个订单审批规则，AI 只修改一个语义节点，编译器自动完成所有相关代码变更。

这才是你与传统 AI Coding 最大的差异。

---

## 七、建议的下一步

我会优先做三个产品的深度拆解：

1. **Rishon**：研究它的 DSL 语法、Compiler、Ontology Graph、VM，判断它的语言边界和可扩展性。
2. **fromSpec**：研究它的 Spec Schema、规格修改、原型与代码一致性、代码生成机制。
3. **Graphical Agents**：研究它的 IR、图形化编辑器、验证和多 Target Compiler 实现。

最终要回答的不是它们有哪些功能，而是：

- 它们的 DSL 到底表达了什么？
- 是否真正不需要 AI 写业务代码？
- 哪些功能由 Compiler 完成，哪些依赖 Runtime？
- 可视化画布如何与规格保持双向同步？
- 规格修改如何精确映射到代码增量？
- 哪些复杂业务无法被它们的 DSL 表达？
- 能否把它们的架构抽象成一个通用软件生产系统？

**我的建议：先把 Rishon 当作理念上的最大竞争参照，把 fromSpec 当作产品流程上的最大竞争参照，把 Graphical Agents 当作工程实现上的参照。**

如果你能做出一个真正通用、AI 原生、图形化、可编译为标准代码的 DSL 系统，仍然有独立产品的空间。但在投入完整开发之前，必须对这三个产品做源码和实际产物审计，而不能仅凭官网介绍判断技术差异。