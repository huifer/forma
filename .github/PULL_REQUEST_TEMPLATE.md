<!-- Title: feat|fix|docs(scope): summary -->

## What & why

<!-- What semantics changed? Link the issue. -->

## Constitution check

<!-- CONTRIBUTING.md rules — confirm each that applies. Delete the rest. -->

- [ ] No new DSL syntax inside standard files
- [ ] Semantic Patch API is the only write path to `specs/`
- [ ] Deterministic output preserved (`build_generates_all_four_targets_deterministically` green)
- [ ] Incremental build participation for new generators
- [ ] Transactional patch behavior preserved (rollback on failure)

## Standards affected

<!-- e.g. SQL DDL + JSON Schema; or "none (engine only)" -->

## Test evidence

```text
(paste `cargo test` summary — new tests for new behavior)
```

## Token-economics impact

<!-- Optional: does this reduce AI token spend? Bigger/smaller patch payloads? -->
