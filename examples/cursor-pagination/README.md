# Cursor Pagination Reference App

A minimal reference project for Forma's keyset cursor pagination (P1-6).

## Contract

- DSL: one or more non-null, comparable `sort` keys; duplicate/unsupported keys fail during semantic check.
- Backend: lexicographic seek predicate with per-key direction and a stable `id` tie-breaker; fetches `pageSize + 1` rows.
- Response: `{ "items": [...], "nextCursor": "{\"values\":[...],\"id\":\"...\"}" | null }`.
- Web: TanStack Query `useInfiniteQuery` and a “Load more” control.
- OpenAPI: cursor query parameter and response envelope schema.
- Offset pagination remains a bare array and is unchanged.
- Flutter and chart runtimes currently reject cursor APIs before any build emitter writes artifacts.

## Verify

```sh
cargo run -- build examples/cursor-pagination
cargo check --manifest-path examples/cursor-pagination/generated/backend/Cargo.toml
cd examples/cursor-pagination/generated/web
npm install
npx tsc --noEmit
npm run build
```

The repository CI runs this reference app after generation.
