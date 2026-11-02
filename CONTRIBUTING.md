# Contributing to Forma

Thanks for your interest in contributing! Forma is a **Specification Federation** compiler: it organizes existing standards (SQL DDL, JSON Schema, OpenAPI, JSON Forms, BPMN) into a unified semantic graph and compiles them deterministically.

> 文档与讨论同时支持中文。Issues 和 PR 请随意使用中文或英文。

## The constitution (non-negotiable design rules)

These rules are why Forma exists. A PR that violates them will be asked to change:

1. **No new DSL.** Never add private syntax inside standard files. Cross-standard linking happens only via `x-forma-*` extensions, manifest bindings, or name conventions.
2. **AI never writes files.** The Semantic Patch API is the only write path to `specs/`. No feature should require an agent to hand-edit specs.
3. **Deterministic output.** Same input → byte-identical output. Every generator change must keep `build_generates_all_four_targets_deterministically` green.
4. **Incremental by default.** New generators must participate in the build cache (`.forma/build.json`).
5. **Transactional patches.** Any failure in a patch rolls back everything. Partial writes are a bug.
6. **Standards own semantics.** If a fact can live in the standard file (SQL column, JSON Schema property), it must not be duplicated in the manifest or elsewhere.

## Development setup

```bash
git clone https://github.com/YOUR_GH_USER/forma.git
cd forma
cargo build            # Rust stable (1.75+); no heavy dependencies
cargo test             # 17 integration tests must pass
```

Useful during development:

```bash
cargo test --test patch                     # only patch tests
cargo run -- doctor examples/order-management
cargo run -- graph examples/order-management --mermaid
```

## Workflow

1. Open an issue first for anything that changes behavior or adds a standard adapter — describe the semantics, not just the code.
2. Fork, create a feature branch from `main` (`feat/...`, `fix/...`, `docs/...`).
3. Make your change. Add or update tests — bug fixes need a regression test.
4. Run the full gate locally:

   ```bash
   cargo fmt
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

5. Open a PR with a clear description: **what semantics changed**, which standards are affected, and the test evidence.
6. Keep PRs small and single-purpose.

## Adding a new standard adapter

This is the highest-value contribution path. Checklist (see `crates/forma/src/adapters/mod.rs` for the trait):

- [ ] `src/adapters/<standard>.rs` implementing `SpecAdapter` (parse → `AdapterResult` with nodes + edges).
- [ ] Register in `AdapterRegistry::default_registry()` and in `manifest.rs::spec_group` (domain group name → format → load order).
- [ ] Semantic graph integration: what does one node of this standard *mean* for `affectedBy()`?
- [ ] Doctor rules where this standard overlaps existing ones (e.g. field consistency).
- [ ] Generator only if the standard should produce code (deterministic + cached).
- [ ] Tests: parsing, graph integration, patch propagation (if writable), generated output.
- [ ] Update `docs/architecture.zh-CN.md` standard table and `CHANGELOG.md`.

## Patch operations

New semantic patch operations (`crates/forma/src/patch.rs`) must:

- produce a typed `EditKind` plan (no string-parsed edits),
- cover **all** standards that express the changed fact,
- fail loudly (`not_implemented` report) rather than silently skip a standard,
- roll back cleanly.

## Commit messages

Conventional style, imperative mood:

```text
feat(adapters): add AsyncAPI adapter skeleton
fix(patch): roll back openapi edits when json-schema fails
docs: explain token economics in philosophy
```

## Reporting bugs

Open a GitHub issue with: minimal repro (spec files + command), expected vs actual, and `forma doctor`/`forma validate` output. See [SECURITY.md](SECURITY.md) for security-sensitive reports.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
