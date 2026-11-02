<div align="center">

```
 ███████╗ ██████╗ ██████╗ ███╗   ███╗ █████╗
 ██╔════╝██╔═══██╗██╔══██╗████╗ ████║██╔══██╗
 █████╗  ██║   ██║██████╔╝██╔████╔██║███████║
 ██╔══╝  ██║   ██║██╔══██╗██║╚██╔╝██║██╔══██║
 ██║     ╚██████╔╝██║  ██║██║ ╚═╝ ██║██║  ██║
 ╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝
```

# 一套 DSL，所有平台

**Forma 是一层 DSL 中间层：一套 DSL 描述前端、后端、数据库与规则，适配层（编译器）把它变成各平台的完整代码。**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-17%20passing-brightgreen.svg)](#开发)
[![MCP](https://img.shields.io/badge/MCP-compatible-blueviolet.svg)](#AI-接入mcp)
[![Status](https://img.shields.io/badge/status-v0.3%20%E8%AE%BE%E8%AE%A1%20%2B%20M1-blue.svg)](docs/roadmap.zh-CN.md)

[English](README.md) | **简体中文**

</div>

## 核心思想

一次典型的 AI Coding 会话，大部分 Token 消耗在**语言税**上：分号、`public static void`、import、HTML 标签配对——以及最贵的**重复税**：加一个字段，要把同一个事实在 SQL、类型、校验器、路由、表单、测试里逐 Token 写六遍。

Forma 在 AI 和代码之间放一层**中间层**：

```text
AI 只写 DSL（纯语义，约占传统方式 1% 的 Token）
        │
        ▼
┌─────────────────────────────────────────────┐
│           Forma 编译器（Rust）                │
│  parse → 语义图 → check → emit               │
└──────┬───────────────┬──────────────┬───────┘
       ▼               ▼              ▼
   MySQL DDL      Rust 后端        React Web
   （含迁移）      （axum + sqlx）  （TanStack + shadcn/ui）
                                     （iOS/Android → 后续适配层）
```

**一套 DSL 是数据、规则、API、页面的唯一事实来源。更新 DSL → 执行编译器 → 得到新代码**——所有平台同步。规则是一等公民：DSL 里写一次，编译为**后端权威校验（Rust）**与**同源前端校验（zod）**。

> 一句话说：**AI 成为意图表达者，适配层成为打字员**——确定性的、增量的、免费的。

深入阅读：[设计理念](docs/philosophy.zh-CN.md) · [架构蓝图 v0.3](docs/architecture.zh-CN.md) · [DSL 语言规范](docs/dsl-spec.zh-CN.md)

## 一个文件看懂 DSL

```fm
entity Order {
  id            Uuid          @id @default(uuid)
  customerEmail Email         @required
  amount        Decimal(12,2) @required @min(0.01)
  status        OrderStatus   @default(pending)
  createdAt     Timestamp     @default(now) @readonly
}

rules OrderRules {
  field Order.amount {
    min 0.01 { zh "金额必须大于 0" en "Amount must be positive" }
  }
  guard Order.delete {
    deny status == "approved"
      reason { zh "已审批订单不可删除" en "Approved orders cannot be deleted" }
  }
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [status] search [customerEmail] sort [createdAt desc] }
}

page OrderList {
  route "/orders"
  title { zh "订单列表" en "Orders" }
  table {
    query   Orders.list
    columns [createdAt: datetime, customerEmail, amount: money, status: badge]
    actions { create → OrderEdit, row [edit, delete confirm] }
  }
}
```

DSL 里没有 axum、没有 React、没有 SQL——页面是目标无关的抽象原语（`table`/`form`/`detail`），组件偏好是 target 配置。完整语法：[DSL 规范](docs/dsl-spec.zh-CN.md)。

## 适配层目标

| 目标 | 技术栈（当前） | 生成产物 |
|---|---|---|
| 数据库 | **MySQL 8**（InnoDB, utf8mb4） | DDL + 迁移 |
| 后端 | **Rust**（axum + sqlx） | models、validators、routes、repositories |
| Web | **React + TanStack Router/Query + shadcn/ui** | 类型化路由、Query hooks、表单、zod schema |
| 移动端 | iOS / Android | 规划中（同一页面原语 → 原生或 Expo） |

## 状态与路线图

**v0.3（当前）**——DSL 中间层架构定案；DSL 规范 v0.1 发布；编译器里程碑推进中：

- **M1** DSL 内核（lexer/parser/AST/IR/check）← *进行中*
- **M2** MySQL 适配层 · **M3** Rust 后端适配层 · **M4** React 适配层
- **M5** DSL 语义补丁（AI 闭环）· **M6** OpenAPI/JSON Schema 导出

仓库当前含一个可运行的**标准联邦原型**（v0.2，Rust：适配器 + doctor + 增量构建 + 事务式补丁 + MCP，17 个测试全绿）。其语义图、补丁与构建引擎被 DSL 编译器复用；标准适配器转为导出视图（M6）。演化史：[docs/history/](docs/history/README.md)。

完整路线图：[docs/roadmap.zh-CN.md](docs/roadmap.zh-CN.md)。

## 开发（当前原型）

```bash
cargo test            # 17 个集成测试
cargo build --release
cargo run -- doctor examples/order-management    # 联邦时期示例命令
```

## 参与贡献

欢迎 PR——见 [CONTRIBUTING.md](CONTRIBUTING.md)（含设计宪法）。请遵守[行为准则](CODE_OF_CONDUCT.md)。安全问题：[SECURITY.md](SECURITY.md)。变更记录：[CHANGELOG.md](CHANGELOG.md)。

## 开源协议

[MIT](LICENSE) © Forma Contributors
