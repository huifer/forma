# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **gen-seed（确定性案例数据，非 AI 数据）** — `forma seed [dir]` / 自动随 `forma build` 产出：
  - 全部取值由 DSL 纯函数推导（类型/约束/枚举/关系），不经过任何模型；同一 DSL 生成结果
    byte 级一致，可 hash 校验、可跨机器重放。
  - 版本化 `db/seed/seed_{version}.sql`（INSERT IGNORE + SHA-256 派生 v5-shape 主键，幂等可重放；
    表按拓扑序先父后子，FK 引用父表同行号主键）+ `db/seed/fixtures/*.json`（实体/列表两种形态，
    键名与 Rust 后端 serde 序列化一致）。种子文件与迁移链版本一一对应：migrate up --to X
    后应用 seed_X 即得到该版本完整数据集；值满足同一套 DSL rules（min/max/枚举/必填/check）。
  - school 案例现带三轮数据链：`seed_0_1_0 → seed_0_2_0 → seed_0_3_0`（与迁移链对齐）。
- **gen-flutter（移动端适配层，Android / iOS 同源）** — `forma.yaml` 增加 `mobile: { framework: flutter }`：
  - `lib/models/models.dart`：实体/枚举/ListRow/Input 类，fromJson/toJson 键名与 Rust 后端
    serde 契约严格一致（实体/ListRow 用 `{rel}_id`、Create/Update 输入用裸关系名 `{rel}`；
    Dart 字段名 lowerCamelCase、JSON 键保持 snake_case；Decimal 走 string 保精度）。
  - `lib/api/api.dart`：REST 客户端（`/api/{plural}`，page/page_size/q/filters+_contains），
    API_BASE 经 `--dart-define` 注入（Android 模拟器默认 10.0.2.2）。
  - `lib/validation/validators.dart`：同一套 DSL rules 三端同源编译
    （Rust 权威 + Web zod + Dart），check 规则 DateTime 比较编译为 compareTo。
  - `lib/pages/*.dart` + `lib/main.dart`：页面原语 → Material 列表/编辑页，DSL 路由
    （含 `{id}` 参数路由与嵌套路由）→ 命名路由 + onGenerateRoute 模式匹配。
  - 产物是普通 Flutter 工程：`flutter pub get && flutter run` 即可运行（Flutter ≥ 3.35）。
  - 生成代码通过 `flutter analyze`（0 issues）与 `flutter build web` 生产编译。
- **案例协议文档** `docs/case-protocol.zh-CN.md`：多轮次迭代剧本（R1 基线 → R2 增量 →
  R3 演进）+ 12 项能力完整性验收矩阵（V1–V12），每轮固定跑同一组可脚本化命令。
- school 示例：v1/v2 快照目录可独立 seed；`app_mobile/` 产物入库。

### Changed
- CLI：`forma seed` 子命令；`forma build` 新增 seed/mobile-flutter 两个发射步骤。
- school 示例 `forma.yaml` 增加 `mobile` target（产物指向非 gitignored 目录，与迁移 SQL 同一可见性原则）。

### Planned
- DMN adapter (decision tables → rule execution)
- AsyncAPI adapter (event contracts → subscriptions)
- `forma patch --dry-run` (edit-plan preview without writing)
- Forma Studio (per-standard visual editors)

## [0.3.0] - 2026-11-03

First public release of the Forma DSL compiler. This release establishes the end-to-end
workflow: write one declarative DSL, compile every target deterministically, evolve the schema
across versions, and verify the migration chain before deployment — with the full SQL trail
visible in the repository.

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
- English-first public README: positioning, current status, quick start, DSL overview,
  generated targets, versioned-migration guide, CLI reference, and contribution guide.
- Release metadata: package `repository`/`keywords`/`categories`, `v0.3.0` release tag.

### Fixed
- `ADD COLUMN` on a `@unique` field dropped the unique constraint (new columns skipped the
  column-level unique diff) — now emits a named `uq_{table}_{col}` key, consistent between
  CREATE and ALTER paths.
- Positional index names (`idx_{table}_{i}`) collided across versions (baseline
  `idx_enrollments_0 (student_id, course_id)` vs upgrade `idx_enrollments_0 (status)` →
  real MySQL "Duplicate key name") — replaced with content-derived names.
- `DROP COLUMN` was emitted before its dependent `DROP INDEX` — MySQL auto-drops the index,
  making the explicit drop fail; constraints now drop first.

### Fixed (infra)
- Flaky parallel-test failure: build-test tmp projects were named with pid + a truncated
  `SystemTime` debug string; two threads could mint the same directory in one window
  ("forma.yaml already exists"). Directories now use an in-process atomic sequence plus true
  nanoseconds.
- Debug leftovers removed (`examples/dbg10.rs`, debug `eprintln!` in a migration test).

### Changed (design)
- **Architecture v0.3 — DSL middle layer.** Forma is repositioned: one DSL is the single
  source of truth for data, rules, API and pages; adapter layers compile it to MySQL,
  Rust backend (axum + sqlx) and React web (TanStack + shadcn/ui). The v0.2
  standards-federation prototype is kept as a working reference and its standard
  adapters become generated export views (M6).
- DSL spec v0.1 published (`docs/dsl-spec.zh-CN.md`): app/enum/entity/rules/api/page,
  closed rule-expression language compiled to both backend (Rust) and frontend (zod).

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

[Unreleased]: https://github.com/huifer/forma/commits/main
[0.3.0]: https://github.com/huifer/forma/commit/77899ea91d79853a63cb50171c57597445cc5dbf
[0.2.0]: https://github.com/huifer/forma/commit/5477039
