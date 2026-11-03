<div align="center">

# Forma

### One DSL. Every platform.

A Rust-based specification compiler that turns one declarative DSL into database migrations,
backend code, frontend code, APIs, validation, and diagrams.

[![CI](https://github.com/huifer/forma/actions/workflows/ci.yml/badge.svg)](https://github.com/huifer/forma/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-60%20passing-brightgreen.svg)](#development)
[![MCP](https://img.shields.io/badge/MCP-compatible-purple.svg)](#ai-integration)

[English](README.md) · [简体中文](README.zh-CN.md)

</div>

## Why Forma?

Application development repeats the same product decision across SQL schemas, backend models,
API contracts, frontend forms, validation rules, and documentation. That repetition is expensive
for humans and especially expensive for AI coding agents.

Forma introduces a semantic middle layer:

```text
Declarative Forma DSL
        │
        ▼
Forma compiler (Rust)
parse → semantic graph → validate → emit
        │
        ├── MySQL 8 schema + versioned migrations
        ├── Rust backend (axum + sqlx)
        ├── React web app (TanStack + shadcn/ui)
        ├── OpenAPI documentation
        └── ER, data-flow, and lifecycle diagrams
```

The DSL is the source of truth. You describe the domain once; deterministic emitters project it
into the target technologies. Rules are emitted to both authoritative backend validation and
same-source frontend validation.

> **AI expresses intent. Forma handles repetition.**

## Current status

Forma is an **early open-source development release**. The end-to-end DSL pipeline is working,
and the migration engine is covered by deterministic tests and a SQL replay simulator. The
adapter matrix and Studio are still under active development; see the [roadmap](docs/roadmap.zh-CN.md).

The current release focuses on:

- a typed DSL for applications, enums, entities, rules, APIs, and pages;
- deterministic MySQL, Rust backend, React frontend, OpenAPI, and diagram emitters;
- multi-version database migrations with upgrade/rollback pairs;
- migration-chain verification without requiring a live database;
- semantic diffs, linting, validation, diagrams, and MCP integration.

## Quick start

### Requirements

- Rust stable (the project is tested on Linux and macOS)
- No database is required for compilation or migration-chain verification

### Build and test Forma

```bash
git clone https://github.com/huifer/forma.git
cd forma
cargo test --locked
cargo build --release
```

### Create and build a project

```bash
cargo run -- init examples/my-app
cargo run -- validate examples/my-app
cargo run -- build examples/my-app
```

The compiler writes generated artifacts according to `forma.yaml`. The repository includes a
complete working example in [`examples/school`](examples/school).

## The DSL

A single `.fm` file can describe data, rules, APIs, and screens:

```fm
app orders {
  version = "0.1.0"
  locale = ["en"]
}

enum OrderStatus {
  pending
  approved
  cancelled
}

entity Order {
  id            Uuid          @id @default(uuid)
  customerEmail Email         @required
  amount        Decimal(12,2) @required @min(0.01)
  status        OrderStatus   @default(pending)
  createdAt     Timestamp     @default(now) @readonly
}

rules OrderRules {
  field Order.amount {
    min 0.01 { en "Amount must be positive" }
  }

  guard Order.delete {
    deny status == "approved"
      reason { en "Approved orders cannot be deleted" }
  }
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [status] search [customerEmail] sort [createdAt desc] }
}
```

The DSL does not contain axum, React, or SQL. Pages and forms use target-independent primitives;
target configuration selects the concrete output stack.

- [DSL specification](docs/dsl-spec.zh-CN.md)
- [Architecture](docs/architecture.zh-CN.md)
- [Design philosophy](docs/philosophy.zh-CN.md)
- [Roadmap](docs/roadmap.zh-CN.md)

## Generated targets

| Target | Current output |
| --- | --- |
| MySQL 8 | Immutable baseline DDL, upgrade migrations, rollback migrations |
| Rust | axum + sqlx models, repositories, routes, and validators |
| React | TanStack Router/Query, forms, types, and validation |
| OpenAPI | OpenAPI JSON and a local Swagger UI page |
| Diagrams | ER, data-flow, and enum lifecycle diagrams |

The target paths are configured in `forma.yaml`:

```yaml
targets:
  db:
    engine: mysql
    out: db
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
  web:
    framework: react
    router: tanstack
    data: tanstack-query
    ui: shadcn
    out: generated/web
```

`db` and `web` are accepted aliases for `database` and `frontend`.

## Versioned migrations

Forma treats schema evolution as a first-class development workflow. The initial schema is an
immutable baseline; every later DSL change produces a numbered upgrade and rollback pair.
Migration SQL is committed to the repository, while applied state remains machine-local.

```text
v0.1.0  →  db/migrations/0001_init.sql
v0.2.0  →  db/migrations/0002_upgrade.sql
          db/migrations/0002_rollback.sql
v0.3.0  →  db/migrations/0003_upgrade.sql
          db/migrations/0003_rollback.sql
          db/migrations/MIGRATIONS.md
```

The migration engine provides:

- structured schema diffs for tables, columns, indexes, unique keys, and foreign keys;
- inverse rollback SQL for every supported schema operation;
- MySQL-safe operation ordering;
- content-derived index names that remain collision-free across versions;
- a version ledger with hashes and applied/pending state;
- `MIGRATIONS.md` with the complete human-readable SQL trail;
- a replay simulator that validates the generated SQL subset without a database;
- CI-friendly drift and tamper detection.

```bash
# Inspect the chain
cargo run -- migrate examples/school status
cargo run -- migrate examples/school history

# Move forward
cargo run -- migrate examples/school up --to 0.2.0
cargo run -- migrate examples/school up --apply

# Roll back
cargo run -- migrate examples/school down --steps 1
cargo run -- migrate examples/school down --to 0.1.0 --apply

# Verify numbering, pairing, hashes, replay, and schema consistency
cargo run -- migrate examples/school verify
```

The three-version chain in [`examples/school/db/migrations`](examples/school/db/migrations)
shows a realistic evolution:

- `v0.1.0`: school, student, course, and enrollment CRUD baseline;
- `v0.2.0`: election, candidate, and vote-record features with foreign keys;
- `v0.3.0`: phone and remark fields, a widened course title, and a voter index.

## CLI reference

```text
forma init [dir]                          Scaffold a project
forma validate [dir]                      Validate the project
forma check [dir]                         Check DSL semantics
forma build [dir] [--force]              Generate all configured targets
forma diff <dir-a> <dir-b>                Compare two projects
forma diagrams [dir]                      Generate diagrams and HTML preview
forma lint [dir]                          Run built-in lint rules
forma migrate [dir] status               Show migration state
forma migrate [dir] history              Show migration history
forma migrate [dir] verify               Verify the migration chain
forma migrate [dir] up [--to v]          Plan/apply forward migrations
forma migrate [dir] down [--steps N]     Plan/apply rollback migrations
forma mcp [dir]                          Start the MCP stdio server
```

Run `cargo run -- --help` for the complete command help.

## AI integration

Forma includes an MCP stdio server for AI coding environments such as Codex, Claude Code, and
Pi. It exposes project inspection and build-oriented capabilities while keeping generated output
deterministic and reviewable.

```bash
cargo run -- mcp examples/school
```

## Development

```bash
cargo test --locked                         # 60 tests across 12 suites
cargo build --locked --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo run -- migrate examples/school verify
```

The same checks run in [GitHub Actions](.github/workflows/ci.yml). Test fixtures use isolated
temporary projects so the suite remains safe under Cargo's parallel test execution.

## Project layout

```text
crates/forma/src/dsl/       DSL lexer and parser
crates/forma/src/emit/      target emitters and migration engine
crates/forma/src/engine.rs  project loading and build orchestration
crates/forma/tests/         integration and regression tests
examples/school/            three-version end-to-end example
docs/                       architecture, DSL, philosophy, and roadmap
```

## Contributing

Pull requests are welcome. Please read:

- [CONTRIBUTING.md](CONTRIBUTING.md) for design rules and commit conventions;
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community expectations;
- [SECURITY.md](SECURITY.md) for vulnerability reports.

For a new feature, include deterministic tests and update the relevant documentation. Generated
migration SQL should be reviewed like application code: it is part of the public project history.

## License

Forma is released under the [MIT License](LICENSE).
