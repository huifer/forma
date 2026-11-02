# Forma

**Universal Specification Compiler — The integration and compilation layer for AI-native software development.**

Forma 不发明新的软件规格语言。它把现有成熟标准组织成一套 **AI 可操作、可视化、可编译** 的软件规格系统：

| 领域 | 标准 | Forma 的处理 |
|---|---|---|
| 数据库 | SQL DDL（SQLite） | 解析、校验、生成迁移 |
| 数据类型 | JSON Schema 2020-12 | 跨模块数据契约（唯一事实来源） |
| API | OpenAPI 3.1 | 生成 Hono 路由 + 客户端类型 |
| UI 表单 | JSON Forms | 生成 React 表单 |
| 业务流程 | BPMN 2.0（子集） | 流程图数据与执行骨架 |

Forma 自身只拥有四样东西：**Manifest（forma.yaml）**、**实体身份协议（`forma://` URI）**、**Unified Semantic Graph**、**Semantic Patch API**。

核心差异化能力：**跨标准语义变更传播** —— 「Order 增加一个付款状态」一条语义补丁，同步 SQL / JSON Schema / OpenAPI / JSON Forms 四处规格，校验一致性，增量编译受影响产物。

## 快速开始

```bash
# 1. 构建（Rust，无重依赖）
cargo build --release
# 二进制：target/release/forma

# 2. 在示例项目上体验全流程
cd examples/order-management
../../target/debug/forma doctor     # 跨标准一致性诊断
../../target/debug/forma graph      # 语义图：谁定义/引用了 Order
../../target/debug/forma build      # 生成 TS 类型 / Hono 路由 / SQLite 迁移 / React 表单
../../target/debug/forma patch patches/add-payment-status.json
#   → 一条补丁同步四处标准 + 重校验 + 增量重编译

# 3. 起一个新项目
../../target/debug/forma init my-app
```

## AI 接入（MCP）

```bash
forma mcp <project-dir>   # stdio MCP server
```

四个工具，AI 的工作流被约束为 `graph → impact → patch → validate`，全程不接触文件路径与具体语法：

| 工具 | 作用 |
|---|---|
| `forma_validate` | 项目跨标准诊断 |
| `forma_graph` | 语义图摘要（实体 × 关联标准） |
| `forma_impact` | 变更影响闭包 |
| `forma_patch` | 事务式语义补丁 |

适用于 Codex / Claude Code / Pi 等任何支持 MCP 的 Agent。

## 架构一览

```
specs/（标准文件）→ Adapters（逐标准解析）→ Unified Semantic Graph
                                              ↓
              Semantic Patch API ← AI Agent   doctor（跨标准校验）
                                              ↓
                              Deterministic Build（增量编译）→ generated/
```

- 架构蓝图（v0.2，Specification Federation）：[ARCHITECTURE.md](ARCHITECTURE.md)
- 路线图：[ROADMAP.md](ROADMAP.md)
- 项目演化史与 v0.1 自研 DSL 蓝图（已废弃的决策档案）：[docs/history/](docs/history/)

## 仓库结构

```
Cargo.toml                 cargo workspace
crates/forma/              引擎 + 适配器 + 编译器 + CLI + MCP（Rust，单一 crate）
apps/studio/               Forma Studio（React，最小可用：语义图浏览 + 诊断）
examples/order-management/ 端到端示例（五种标准 + 语义补丁演示）
docs/history/              全部历史决策归档
```

## 设计原则

1. **Polyglot Specification**：每个领域用最成熟的标准，Forma 不重复定义。
2. **AI 不直接写文件**：AI 通过 Semantic Patch 表达业务变化，Forma 负责跨标准传播、校验、编译。
3. **确定性编译**：同一输入必产生同一输出；生成文件带 `@generated` 头。
4. **增量优先**：一切编译以变更影响分析为入口，只重算受影响的子图。
5. **事务式补丁**：任一编辑失败 → 全部回滚，规格目录永远自洽。

## 开发

```bash
cargo test        # 17 个集成测试：SQL 解析 / 语义图 / 构建 / 补丁
cargo build --release
```

License: MIT
