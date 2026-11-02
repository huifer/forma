<div align="center">

```
 ███████╗ ██████╗ ██████╗ ███╗   ███╗ █████╗
 ██╔════╝██╔═══██╗██╔══██╗████╗ ████║██╔══██╗
 █████╗  ██║   ██║██████╔╝██╔████╔██║███████║
 ██╔══╝  ██║   ██║██╔══██╗██║╚██╔╝██║██╔══██║
 ██║     ╚██████╔╝██║  ██║██║ ╚═╝ ██║██║  ██║
 ╚═╝      ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝
```

# Universal Specification Compiler

**Don't pay tokens for syntax. Pay for meaning — then compile.**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-17%20passing-brightgreen.svg)](#development)
[![MCP](https://img.shields.io/badge/MCP-compatible-blueviolet.svg)](#ai-integration-mcp)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg)]()

**English** | [简体中文](README.zh-CN.md)

</div>

<!-- 🔧 After publishing to GitHub, replace YOUR_GH_USER in the CI badge below with your org/username:
[![CI](https://github.com/YOUR_GH_USER/forma/actions/workflows/ci.yml/badge.svg)](https://github.com/YOUR_GH_USER/forma/actions/workflows/ci.yml)
-->

## Why Forma exists

A typical AI-coding session burns most of its tokens on **language tax**: semicolons, paired braces, `public static void`, imports, HTML tag pairs, and — worst of all — **repetition**. Adding one field `paymentStatus` means the model re-writes the same fact six times across `schema.sql`, `types.ts`, a validator, an API route, a React form, and tests. Every copy is full-price tokens for near-zero semantic content.

Forma splits **semantics from syntax**:

- The AI produces **specifications in existing standards** (SQL DDL, JSON Schema, OpenAPI, JSON Forms, BPMN) — dense, machine-checkable, ~1% of the tokens.
- A **deterministic Rust compiler** expands those specs into a complete, standard codebase — zero tokens, byte-for-byte reproducible.
- A **Semantic Patch API** propagates one business change across every standard, validates cross-standard consistency, and rebuilds only what's affected.

> Formally: **AI becomes the intent expresser. Forma becomes the typewriter** — one that never mistypes, never hallucinates an import, and works for free.

Full reasoning: [Design Philosophy](docs/philosophy.zh-CN.md) (中文) · project history archive: [docs/history/](docs/history/README.md).

## The standards (no new DSL)

Forma does **not** invent a specification language. Teaching a model a private DSL is itself a per-session token tax. Instead, Forma organizes the standards models already speak:

| Domain | Standard (version) | Status | Forma's role |
|---|---|---|---|
| Database | SQL DDL (SQLite) | ✅ MVP | parse, validate, emit migrations |
| Data types | JSON Schema 2020-12 | ✅ MVP | cross-module contracts — single source of truth |
| API | OpenAPI 3.1 | ✅ MVP | generate Hono routes + typed validators |
| Forms | JSON Forms (UI Schema) | ✅ MVP | generate React forms |
| Workflow | BPMN 2.0 (subset) | ✅ MVP | process graph + execution skeleton |
| Decisions | DMN | 🗓 Phase 1 | decision tables → rule execution |
| Events | AsyncAPI | 🗓 Phase 1 | message contracts → subscriptions |
| Architecture | C4 / Structurizr | 🗓 Phase 2 | dependency analysis |
| Deploy | K8s / Terraform | 🗓 Phase 3 | environments |

## Quick start

```bash
# 1. Build (Rust, no heavy dependencies)
cargo build --release          # → target/release/forma (single binary: CLI + MCP server)

# 2. Run the end-to-end example
cd examples/order-management
../../target/release/forma doctor     # cross-standard consistency report
../../target/release/forma graph      # semantic graph: who defines / references Order
../../target/release/forma build      # → generated/: TS types, Hono routes, SQLite migration, React form

# 3. One patch, four standards
../../target/release/forma patch patches/add-payment-status.json
```

The patch file is pure business intent:

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

Forma applies it everywhere, transactionally:

```text
✓ sql:         add-column           orders.payment_status TEXT      (schema.sql)
✓ json-schema: add-property         Order.paymentStatus enum[3]     (order.schema.json)
✓ openapi:     add-schema-property  components.schemas.Order        (openapi.yaml)
✓ json-forms:  add-control          #/properties/paymentStatus      (order-form.ui.json)
build: 4 file(s) regenerated   ✓ no cross-standard issues
```

Any failed edit rolls back **all** files — the spec tree is never left half-written.

## How it works

```text
                ┌──────────────────────────────┐
                │      AI Specification Agent   │
                │  (Codex / Claude Code / Pi …) │
                └──────────────┬───────────────┘
                               │ Semantic Patch / MCP tools
                               ▼
┌───────────┐   ┌──────────────────────────────────────────────┐
│ specs/    │   │                Forma Engine (Rust)            │
│           │   │  ┌────────────┐  ┌────────────────────────┐  │
│ schema.sql│──▶│  │  Adapters  │─▶│  Unified Semantic Graph │  │
│ *.json    │   │  │ sql/js/oas │  │  identity · references  │  │
│ openapi   │   │  │ forms/bpmn │  │  dependency · impact    │  │
│ *.bpmn    │   │  └────────────┘  └───────────┬────────────┘  │
└───────────┘   │  ┌────────────┐  ┌───────────▼────────────┐  │
                │  │  Validate  │  │   Deterministic Build  │  │
forma.yaml ────▶│  │  (doctor)  │  │  (incremental · cache) │  │
                │  └────────────┘  └───────────┬────────────┘  │
                └──────────────────────────────┼───────────────┘
                                               ▼
                 generated/ — standard code: TypeScript · Hono · SQL · React
```

What Forma owns (and the **only** things it owns):

| Asset | What it is |
|---|---|
| `forma.yaml` Manifest | which specs exist, standards + versions, build targets — no business semantics |
| `forma://` identity protocol | stable URIs binding the same business entity across standards (`x-forma-entity` → manifest bindings → name conventions like `orders ≡ Order`) |
| Unified Semantic Graph | normalized nodes/edges from all standards; `affectedBy()` computes exact change impact |
| Semantic Patch API | the only write path: transactional, cross-standard, validated |

## AI integration (MCP)

```bash
forma mcp <project-dir>        # stdio MCP server
```

Register it with any MCP client (Codex, Claude Code, Pi, …):

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

Four tools. The AI workflow is **constrained by protocol** to `graph → impact → patch → validate` — it never reads whole files, never writes code, never guesses impact:

| Tool | Returns |
|---|---|
| `forma_validate` | all cross-standard diagnostics |
| `forma_graph` | entity-level semantic summary (not source code) |
| `forma_impact` | deterministic affected-node closure |
| `forma_patch` | transactional patch application + rebuild report |

This is where the token economics pay off: the AI's context holds semantic summaries instead of source trees, and its output is a 40-token patch instead of 4 rewritten files.

## Deterministic by construction

- Same input → byte-identical output (tested).
- Generated files carry `@generated` headers; manual edits are detected by the incremental cache.
- The review surface shrinks from a code diff to a spec diff — the patch *is* the review unit.

## Project layout

```text
Cargo.toml                  cargo workspace
crates/forma/               engine + adapters + compiler + CLI + MCP (Rust, single crate)
  src/adapters/             sql | jsonschema | openapi | jsonforms | bpmn
  src/graph.rs              Unified Semantic Graph
  src/validate.rs           cross-standard doctor rules
  src/generators.rs         ts-types | hono-routes | sqlite-migration | react-forms
  src/engine.rs             load → validate → build (incremental)
  src/patch.rs              Semantic Patch (transactional)
  src/mcp.rs                stdio MCP server
examples/order-management/  end-to-end example (5 standards + patch demo)
docs/                       architecture · philosophy · decision history
```

## Roadmap

**Phase 0 (now)** — Federation MVP: 5 standards, doctor, patch, MCP, CLI.
**Phase 1** — DMN, AsyncAPI, DDL-diff migrations, workflow runtime, more patch ops.
**Phase 2** — Studio (visual editors per standard), node-level incremental compilation, DBML input.
**Phase 3** — C4/Structurizr, Protobuf/GraphQL, K8s/Terraform targets, token-economics benchmark.

Details: [docs/roadmap.zh-CN.md](docs/roadmap.zh-CN.md) (中文) · Architecture: [docs/architecture.zh-CN.md](docs/architecture.zh-CN.md) (中文)

## Development

```bash
cargo test            # 17 integration tests: SQL parsing / graph / build / patch
cargo build --release
rustup component add rustfmt clippy   # PRs are expected to pass fmt + clippy
```

Design principles in one line each: standards over invention · AI never writes files · deterministic output · incremental everything · transactional patches · no private syntax on top of standards.

## Contributing

PRs welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Please follow our [Code of Conduct](CODE_OF_CONDUCT.md). Security issues: [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © Forma Contributors
