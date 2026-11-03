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

### Fixed
- **forma_patch：多行 required 数组漏改**——字段改名后 `required` 里遗留旧名（旧 fixture 为
  单行数组未暴露）。现在单行/多行两种排版都能正确改名。
- **gen-rust 产物的运行时契约（真实 MySQL 8 端到端抓出）**：
  - SELECT 列表尾逗号/缺换行 → MySQL 1064；命名占位符 `?name` → sqlx 运行时不支持 → 改位置参数；
  - 搜索条件三值逻辑：多字段应为 OR、NULL 列需 COALESCE（此前搜索恒空）；
  - 关系列名：DDL 是 `{rel}_id`，INSERT/UPDATE 曾用裸名 → 1054 Unknown column；
  - JOIN 表名曾用原始实体名（`Schools`）→ 与 gen-mysql 的 snake_case 复数不一致 → 1064；
  - UUID 物理表示：列是 CHAR(36)，sqlx 的 `Uuid` 仅解 16 字节二进制 → Rust 侧统一用文本 UUID；
  - 仓储函数/校验器按 api op 裁剪、导入按需生成 → 生成后端 66 warning → 0 warning。
- **跨端端口一致性**：后端默认 8080（可用 `PORT` 覆盖）、Web vite proxy 8080、Flutter 默认
  `API_BASE=http://10.0.2.2:8080/api`（此前 Flutter 默认 3000，连不上后端）。

### Added
- **`forma seed --verify`**：确定性校验——磁盘种子与当前 DSL 的纯函数推导 byte 级一致
  （SHA-256 报告；手工改动 / DSL 演进后未重建 → 非 0 退出）。「案例数据不是 AI 数据」
  从设计承诺变成 CI 可执行的命令。
- **Flutter Detail 原语**（此前只有占位注释，与 web 不对称）：section → Card、行 → 标签/值，
  与列表页同一套类型驱动渲染；`{id}` 参数路由接入。真实工具链 analyze 0 issues。
- **MCP 认识 v0.3 DSL 工程**（此前全部工具走联邦引擎，对 DSL 工程返回空）：
  - 新工具 `forma_check`（DSL 诊断 + lint + 结构摘要）、`forma_build`（与 CLI `forma build`
    同一条管线的产物清单）、`forma_migrate`（迁移链状态）；
  - `forma_graph` 双模式：DSL 工程 → 实体/枚举/API/页面/规则摘要；联邦工程 → 原语义图；
  - `forma_impact` / `forma_patch` 在 DSL 工程上给出带替代工具指引的明确报错；
  - AI 工作流闭环：读模型（graph）→ 改 .fm → 校验（check）→ 重建（build）→ 看迁移链（migrate）。
- **`forma init` 对齐 v0.3**：脚手架从联邦原型改为 DSL 工程（forma/app.fm 起步示例 +
  forma.yaml 含 mobile target + README + .gitignore）——init 后 validate/build/migrate verify
  开箱全绿（集成测试锁定）。
- **多字段排序**：`sort [createdAt desc, amount]` 逐项生成 ORDER BY（此前静默截断到第一个）。
- **cursor 分页显式报错**：按设计宪法 6.2，未实现的构造不再静默降级（写盘前失败并说明支持范围）。
- **CI 补齐案例协议 V13–V16**：flutter（analyze + 契约测试）、web（tsc + vite）、
  e2e（MySQL 8 service + 迁移链 + seed + API 冒烟：list/search/create/校验 400/守卫 409）。
- **测试体系适配**：build/patch 套件的联邦 fixture 从旧脚手架迁到 `examples/order-management`
  （`forma init` 现在生成 DSL 工程）；新增 `tests/scaffold_mcp.rs`（脚手架开箱全绿 + MCP 工具面）
  与 backend_contract 的多字段排序/cursor 报错断言；README badge 更新至 92 tests。
- **gen-flutter 契约测试**（`app_mobile/test/live_api_test.dart`）：离线 fromJson/toJson 往返 +
  在线「创建→读取→更新→删除」全链路（后端未启动时自动跳过，不阻塞 CI）。
- **gen-rust 运行时契约回归测试**（`crates/forma/tests/backend_contract.rs`）：把上述 8 类
  bug 固化为源码结构断言，防止回归。

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
