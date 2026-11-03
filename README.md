<div align="center">

```
 ███████╗ ██████╗ ██████╗ ███╗   ███╗ █████╗
 ██╔════╝██╔═══██╗██╔══██╗████╗ ████║██╔══██╗
 █████╗  ██║   ██║██████╔╝██╔████╔██║███████║
 ██╔══╝  ██║   ██║██╔══██╗██║╚██╔╝██║██╔══██║
 ██║     ╚██████╔╝██║  ██║██║ ╚═╝ ██║██║  ██║
 ╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝
```

# One DSL. Every platform.

**Forma is a DSL middle layer: one DSL describes your frontend, backend, database and rules — adapter layers compile it into complete code for every platform.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-48%20passing-brightgreen.svg)](#development)
[![MCP](https://img.shields.io/badge/MCP-compatible-blueviolet.svg)](#ai-integration-mcp)
[![Status](https://img.shields.io/badge/status-v0.3%20design%20%2B%20M1-blue.svg)](docs/roadmap.zh-CN.md)

**English** | [简体中文](README.zh-CN.md)

</div>

## The idea

A typical AI-coding session burns most tokens on **language tax**: semicolons, `public static void`, imports, HTML tag pairs — and worst of all, **repetition**: adding one field means re-writing the same fact six times across SQL, types, validators, routes, forms and tests.

Forma inserts a **middle layer** between AI and code:

```text
AI writes the DSL (semantics only, ~1% of the tokens)
        │
        ▼
┌─────────────────────────────────────────────┐
│           Forma Compiler (Rust)             │
│  parse → semantic graph → check → emit      │
└──────┬───────────────┬──────────────┬───────┘
       ▼               ▼              ▼
   MySQL DDL      Rust backend    React web app
   (migrations)   (axum + sqlx)   (TanStack + shadcn/ui)
                                          (+ iOS/Android later)
```

One DSL is the **single source of truth** for data, rules, API and pages. Update the DSL, run the compiler, get new code — across every platform. Rules are first-class: written once in the DSL, compiled to **authoritative backend validation (Rust)** and **same-source frontend validation (zod)**.

> Formally: **AI becomes the intent expresser. The adapter layer becomes the typewriter** — deterministic, incremental, free.

Deep dive: [Design Philosophy](docs/philosophy.zh-CN.md)（中文）· [Architecture v0.3](docs/architecture.zh-CN.md)（中文）· [DSL Spec](docs/dsl-spec.zh-CN.md)（中文）

## The DSL, in one file

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

No axum, no React, no SQL in the DSL — pages are target-agnostic primitives (`table`/`form`/`detail`); component preferences are target config. Full grammar: [DSL Spec](docs/dsl-spec.zh-CN.md)（中文）.

## Targets

| Target | Stack (now) | Generated output |
|---|---|---|
| Database | **MySQL 8** (InnoDB, utf8mb4) | DDL + migrations |
| Backend | **Rust** (axum + sqlx) | models, validators, routes, repositories |
| Web | **React + TanStack Router/Query + shadcn/ui** | typed routes, query hooks, forms, zod schemas |
| Mobile | iOS / Android | planned (same page primitives → native or Expo) |

## Status & roadmap

**v0.3 (current)** — DSL middle-layer architecture finalized; DSL spec v0.1 published. The full **adapter matrix** — databases (MySQL → PostgreSQL/SQLite), backends (Rust → NestJS/Hono/FastAPI → Spring/Go/.NET), web (React → Vue/Next.js), mobile (Expo → Flutter/native/mini-programs), API styles (REST → OpenAPI/GraphQL/gRPC), and capability modules (auth, uploads, i18n, search, jobs, realtime, audit, multi-tenant) — is mapped in waves:

- **Wave 1 (M1–M4)** end-to-end loop: DSL kernel → MySQL → Rust backend → React web
- **Wave 2 (M5–M8)** auth module · **Studio v1 visualization** · **ecosystem interop** (OpenAPI/SQL/Postman both ways) · AI semantic-patch closed loop
- **Wave 3 (M9–M10)** engineering (Docker/uploads/exports/audit) + Studio two-way editing · breadth: PostgreSQL/SQLite, Node/Python backends, Vue/Next.js, Expo, GraphQL, multi-tenant, realtime
- **Wave 4 (M11+)** enterprise: Spring/Go/.NET, native mobile, mini-programs, workflow runtime, payments

The repository currently contains a working **standards-federation prototype** (v0.2, Rust: adapters + doctor + incremental build + transactional patches + MCP, 17 tests green). Its semantic-graph, patch and build engines are reused by the DSL compiler; the standard adapters become export views (M7). History: [docs/history/](docs/history/README.md).

Full roadmap with acceptance criteria per milestone: [docs/roadmap.zh-CN.md](docs/roadmap.zh-CN.md)（中文）.

## Development (current prototype)

```bash
cargo test            # 17 integration tests
cargo build --release
cargo run -- doctor examples/order-management    # federation-era demo commands
```

## Contributing

PRs welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) (design constitution included). Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Security: [SECURITY.md](SECURITY.md). Changelog: [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE) © Forma Contributors
