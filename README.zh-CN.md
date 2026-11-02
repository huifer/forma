<div align="center">

```
 ███████╗ ██████╗ ██████╗ ███╗   ███╗ █████╗
 ██╔════╝██╔═══██╗██╔══██╗████╗ ████║██╔══██╗
 █████╗  ██║   ██║██████╔╝██╔████╔██║███████║
 ██╔══╝  ██║   ██║██╔══██╗██║╚██╔╝██║██╔══██║
 ██║     ╚██████╔╝██║  ██║██║ ╚═╝ ██║██║  ██║
 ╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝
```

# 通用软件规格编译器

**不要为语法付 Token。为语义付费——然后编译。**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-17%20passing-brightgreen.svg)](#开发)
[![MCP](https://img.shields.io/badge/MCP-compatible-blueviolet.svg)](#AI-接入mcp)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg)]()

[English](README.md) | **简体中文**

</div>

## 为什么需要 Forma

一次典型的 AI Coding 会话，大部分 Token 消耗在**语言税**上：分号、配对括号、`public static void`、import、HTML 标签配对——以及最贵的**重复税**。给 `Order` 加一个 `paymentStatus` 字段，模型要把同一个事实在 `schema.sql`、`types.ts`、校验器、API 路由、React 表单、测试里**逐 Token 写六遍**。每一份副本都是全价的 Token、近乎零的语义含量。

Forma 把**语义与语法拆开**：

- AI 只生产**现有标准的规格**（SQL DDL、JSON Schema、OpenAPI、JSON Forms、BPMN）——稠密、可校验，约占传统方式 1% 的 Token；
- **确定性 Rust 编译器**把规格展开成完整的标准代码库——零 Token，byte 级可复现；
- **Semantic Patch API** 把一次业务变化传播到所有标准，校验跨标准一致性，只重建受影响的产物。

> 一句话说：**AI 成为意图表达者，Forma 成为打字员**——一个不会打错、不会幻觉出错误 import、而且免费的打字员。

完整论证：[设计理念](docs/philosophy.zh-CN.md) · 项目演化史：[docs/history/](docs/history/README.md)

## 标准（不发明新 DSL）

Forma **不发明**规格语言。给模型发明一门私有 DSL，等于对每个会话重收一次学习税。Forma 组织的是模型本来就「会说」的标准：

| 领域 | 标准（版本） | 状态 | Forma 的角色 |
|---|---|---|---|
| 数据库 | SQL DDL（SQLite） | ✅ MVP | 解析、校验、生成迁移 |
| 数据类型 | JSON Schema 2020-12 | ✅ MVP | 跨模块契约，唯一事实来源 |
| API | OpenAPI 3.1 | ✅ MVP | 生成 Hono 路由 + 类型化校验器 |
| 表单 | JSON Forms（UI Schema） | ✅ MVP | 生成 React 表单 |
| 流程 | BPMN 2.0（子集） | ✅ MVP | 流程图数据 + 执行骨架 |
| 决策 | DMN | 🗓 Phase 1 | 决策表 → 规则执行 |
| 事件 | AsyncAPI | 🗓 Phase 1 | 事件契约 → 订阅骨架 |
| 架构 | C4 / Structurizr | 🗓 Phase 2 | 依赖分析 |
| 部署 | K8s / Terraform | 🗓 Phase 3 | 环境与运行 |

## 快速开始

```bash
# 1. 构建（Rust，无重依赖）
cargo build --release          # → target/release/forma（单二进制：CLI + MCP server）

# 2. 跑端到端示例
cd examples/order-management
../../target/release/forma doctor     # 跨标准一致性诊断
../../target/release/forma graph      # 语义图：谁定义/引用了 Order
../../target/release/forma build      # → generated/：TS 类型、Hono 路由、SQLite 迁移、React 表单

# 3. 一条补丁，同步四处标准
../../target/release/forma patch patches/add-payment-status.json
```

补丁文件是纯业务意图：

```jsonc
{
  "operations": [{
    "op": "add-entity-field",
    "entity": "Order",
    "field": {
      "name": "paymentStatus",
      "type": "string",
      "enum": ["unpaid", "paid", "refunded"],
      "required": false
    }
  }]
}
```

Forma 以事务方式把它应用到所有标准：

```text
✓ sql:         add-column           orders.payment_status TEXT      (schema.sql)
✓ json-schema: add-property         Order.paymentStatus enum[3]     (order.schema.json)
✓ openapi:     add-schema-property  components.schemas.Order        (openapi.yaml)
✓ json-forms:  add-control          #/properties/paymentStatus      (order-form.ui.json)
build: 4 file(s) regenerated   ✓ no cross-standard issues
```

任一编辑失败会回滚**全部**文件——规格目录永远不会处于半写状态。

## 工作原理

```text
                ┌──────────────────────────────┐
                │      AI Specification Agent   │
                │  (Codex / Claude Code / Pi …) │
                └──────────────┬───────────────┘
                               │ Semantic Patch / MCP 工具
                               ▼
┌───────────┐   ┌──────────────────────────────────────────────┐
│ specs/    │   │                Forma Engine (Rust)            │
│           │   │  ┌────────────┐  ┌────────────────────────┐  │
│ schema.sql│──▶│  │  Adapters  │─▶│  Unified Semantic Graph │  │
│ *.json    │   │  │ sql/js/oas │  │  身份 · 引用 · 依赖 ·   │  │
│ openapi   │   │  │ forms/bpmn │  │  影响分析               │  │
│ *.bpmn    │   │  └────────────┘  └───────────┬────────────┘  │
└───────────┘   │  ┌────────────┐  ┌───────────▼────────────┐  │
                │  │  Validate  │  │   Deterministic Build  │  │
forma.yaml ────▶│  │  (doctor)  │  │  （增量 · 缓存）        │  │
                │  └────────────┘  └───────────┬────────────┘  │
                └──────────────────────────────┼───────────────┘
                                               ▼
                 generated/ — 标准代码：TypeScript · Hono · SQL · React
```

Forma 自身只拥有四样东西（**也仅有**这四样）：

| 资产 | 是什么 |
|---|---|
| `forma.yaml` Manifest | 项目有哪些规格、标准与版本、编译目标——不含业务语义 |
| `forma://` 实体身份协议 | 跨标准绑定同一业务实体的稳定 URI（`x-forma-entity` → Manifest 绑定 → 名称约定如 `orders ≡ Order`） |
| Unified Semantic Graph | 从所有标准归一的节点/边；`affectedBy()` 确定性计算变更影响 |
| Semantic Patch API | 唯一写入路径：事务式、跨标准、先校验 |

## AI 接入（MCP）

```bash
forma mcp <project-dir>        # stdio MCP server
```

在任何 MCP 客户端（Codex、Claude Code、Pi…）注册：

```jsonc
{
  "mcpServers": {
    "forma": {
      "command": "/path/to/forma",
      "args": ["mcp", "/path/to/project"]
    }
  }
}
```

四个工具。AI 的工作流被**协议约束**为 `graph → impact → patch → validate`——不读整个文件、不写代码、不靠猜评估影响面：

| 工具 | 返回 |
|---|---|
| `forma_validate` | 全部跨标准诊断 |
| `forma_graph` | 实体级语义摘要（不是源代码） |
| `forma_impact` | 确定性的受影响节点闭包 |
| `forma_patch` | 事务式补丁执行 + 重编译报告 |

Token 经济学在这里兑现：AI 的上下文里是语义摘要而不是源码树，输出是 40 Token 的补丁而不是重写 4 个文件。

## 确定性构造

- 同一输入 → byte 级相同输出（有测试保证）；
- 生成文件带 `@generated` 头，手改生成物会被增量缓存检出；
- 人工审阅面从「代码 diff」缩小为「规格 diff」——补丁本身就是 review 单位。

## 仓库结构

```text
Cargo.toml                  cargo workspace
crates/forma/               引擎 + 适配器 + 编译器 + CLI + MCP（Rust，单一 crate）
  src/adapters/             sql | jsonschema | openapi | jsonforms | bpmn
  src/graph.rs              Unified Semantic Graph
  src/validate.rs           doctor 跨标准校验规则
  src/generators.rs         ts-types | hono-routes | sqlite-migration | react-forms
  src/engine.rs             load → validate → build（增量）
  src/patch.rs              Semantic Patch（事务式）
  src/mcp.rs                stdio MCP server
examples/order-management/  端到端示例（5 种标准 + 补丁演示）
docs/                       架构 · 设计理念 · 决策史
```

## 路线图

**Phase 0（当前）** — 联邦 MVP：5 种标准、doctor、patch、MCP、CLI。
**Phase 1** — DMN、AsyncAPI、DDL 差异迁移、工作流运行时、更多补丁操作。
**Phase 2** — Studio（各标准可视化编辑器）、节点级增量编译、DBML 输入。
**Phase 3** — C4/Structurizr、Protobuf/GraphQL、K8s/Terraform 目标、Token 经济学 Benchmark。

详见：[docs/roadmap.zh-CN.md](docs/roadmap.zh-CN.md) · 架构：[docs/architecture.zh-CN.md](docs/architecture.zh-CN.md)

## 开发

```bash
cargo test            # 17 个集成测试：SQL 解析 / 语义图 / 构建 / 补丁
cargo build --release
rustup component add rustfmt clippy   # PR 需通过 fmt + clippy
```

设计原则一句话版：标准优先于发明 · AI 不直接写文件 · 输出确定性 · 一切增量 · 补丁事务式 · 不在标准之上加私有语法。

## 参与贡献

欢迎 PR——见 [CONTRIBUTING.md](CONTRIBUTING.md)。请遵守[行为准则](CODE_OF_CONDUCT.md)。安全问题：[SECURITY.md](SECURITY.md)。

## 开源协议

[MIT](LICENSE) © Forma Contributors
