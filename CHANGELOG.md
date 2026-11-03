# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Versioned migration engine（多版本迁移）** — schema evolution like a real project:
  - Structured diff → paired `{index:04}_upgrade.sql` + `{index:04}_rollback.sql` per change;
    rollback is the inverse sequence (add column ↔ drop column, create table ↔ drop table,
    add index/fk ↔ drop index/fk, modify ↔ modify-back).
  - MySQL-safe op ordering per table (drop constraints → add/modify columns → add constraints
    → drop columns) and content-derived index names (`idx_{table}_{cols}`) — long chains
    replay without duplicate-key or missing-column errors.
  - Immutable baseline: `0001_init.sql` is written once by gen-mysql and never rewritten;
    every later DSL change becomes an incremental migration.
  - Version ledger `.forma/migrations.json` (per-migration version, changes, up/down hashes,
    applied flag) + generated `MIGRATIONS.md` making the whole SQL trail visible.
  - CLI `forma migrate {status|history|verify|up|down} [--sql] [--apply] [--to v] [--steps N]`:
    step to a target version, roll back N steps, and `verify` the chain (numbering, pairing,
    hash tamper-detection, forward-replay == current DSL schema; CI-ready exit codes).
  - SQL replay simulator (`emit/replay.rs`): executes the generated SQL subset with real
    MySQL semantics — no database needed — proving forward-replay to latest and
    backward-rollback to baseline.
  - School example now ships a committed **three-version chain**:
    `examples/school/db/migrations/` — v0.1.0 (CRUD baseline) → v0.2.0 (election feature)
    → v0.3.0 (phone/remark/title-widen/voter-index). Versioned DSL snapshots in `v1/`, `v2/`.
  - manifest: `targets.db` / `targets.web` accepted as aliases of `database` / `frontend`.

### Fixed
- `ADD COLUMN` on a `@unique` field dropped the unique constraint (new columns skipped the
  column-level unique diff) — now emits a named `uq_{table}_{col}` key, consistent between
  CREATE and ALTER paths.
- Positional index names (`idx_{table}_{i}`) collided across versions (baseline
  `idx_enrollments_0 (student_id, course_id)` vs upgrade `idx_enrollments_0 (status)` →
  real MySQL "Duplicate key name") — replaced with content-derived names.
- `DROP COLUMN` was emitted before its dependent `DROP INDEX` — MySQL auto-drops the index,
  making the explicit drop fail; constraints now drop first.

### Changed (design)
- **Architecture v0.3 — DSL middle layer.** Forma is repositioned: one DSL is the single
  source of truth for data, rules, API and pages; adapter layers compile it to MySQL,
  Rust backend (axum + sqlx) and React web (TanStack + shadcn/ui). The v0.2
  standards-federation prototype is kept as a working reference and its standard
  adapters become generated export views (M6).
- DSL spec v0.1 published (`docs/dsl-spec.zh-CN.md`): app/enum/entity/rules/api/page,
  closed rule-expression language compiled to both backend (Rust) and frontend (zod).

### Planned
- DMN adapter (decision tables → rule execution)
- AsyncAPI adapter (event contracts → subscriptions)
- `forma patch --dry-run` (edit-plan preview without writing)
- Forma Studio (per-standard visual editors)

## [0.2.0] - 2026-11-02

First public implementation. Full rewrite in Rust (the earlier TypeScript prototype was superseded).

Specification Federation: Forma organizes existing standards instead of inventing a DSL.

### Added
- `forma.yaml` manifest: spec inventory, standards/versions, build targets, identity bindings.
- Entity identity protocol: stable `forma://` URIs; binding priority `x-forma-entity` → manifest `bindings` → name conventions (`orders ≡ Order`).
- Adapters: SQL DDL (SQLite subset, hand-written parser), JSON Schema 2020-12, OpenAPI 3.1 (with `$ref` resolution), JSON Forms UI Schema, BPMN 2.0 subset (hand-written XML parser).
- Unified Semantic Graph with `affectedBy()` impact closure (forward `defines/binds/generates`, reverse `references/belongs-to`).
- Cross-standard doctor rules: `field-type-mismatch`, `field-required-mismatch`, `field-schema-missing`, `field-column-missing`, `field-name-style`, `form-binding-missing`, `workflow-reference-soft`, `table-entity-unbound`.
- Generators (deterministic, `@generated`-stamped): TypeScript interfaces + zero-dependency validators, Hono routes with request validation, SQLite baseline migration, React forms from JSON Forms layout.
- File-level incremental builds (`.forma/build.json`); manual edits to generated files are detected.
- Semantic Patch API (transactional): `add-entity-field`, `rename-entity-field`, `set-entity-field-required` — each propagated across SQL DDL, JSON Schema, OpenAPI and JSON Forms, then re-validated and incrementally rebuilt.
- CLI: `init` / `validate` / `doctor` / `graph` / `build` / `patch` / `mcp`.
- MCP stdio server (self-implemented JSON-RPC): `forma_validate`, `forma_graph`, `forma_impact`, `forma_patch`.
- End-to-end example project `examples/order-management` (5 standards + patch demo).
- 17 integration tests (`cargo test`).

### Changed
- Project language: TypeScript prototype → **Rust** (single binary serves CLI and MCP).

[Unreleased]: https://github.com/YOUR_GH_USER/forma/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/YOUR_GH_USER/forma/releases/tag/v0.2.0
