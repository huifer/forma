---
id: conformance:runtime
title: Runtime Conformance
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: governance
depends_on: [RFC-0005, conformance:overview]
related_adrs: [ADR-0004]
related_specs: [spec:protocols]
breaking_change: false
---

# 运行时一致性（Runtime Conformance）

> 承诺：生成的后端运行时遵循统一契约——错误响应体、请求追踪、结构化日志、游标信封。

## 错误契约（RFC-0005 E1–E5 / spec:protocols）

| 承诺 | 证明 |
|---|---|
| E1 统一错误体 `ErrorBody { error, code, fields? }` | `tests/error_contract.rs` |
| E2 Web/Flutter/OpenAPI 跨端同形解析 | `tests/error_contract.rs` |
| E3 错误码 → 状态码单点映射（`status_for`） | `tests/error_contract.rs` |
| E4 `FORMA_*` 稳定错误码公开 API | `tests/error_contract.rs` |
| E5 内部错误不回显（只写日志） | `tests/error_contract.rs` |

## 请求追踪与日志

| 承诺 | 证明 |
|---|---|
| requestId 中间件（§4.4，`X-Request-Id`） | `tests/backend_contract.rs` + 生成的 `request_id.rs` |
| 结构化日志（RUST_LOG 过滤 + `LOG_FORMAT=json` JSON Lines） | `tests/structured_logging.rs` |

## API 语义

| 承诺 | 证明 |
|---|---|
| 游标分页 `{items, nextCursor}` 信封 + `pageSize+1` 探测 | `tests/cursor_pagination.rs` |
| offset 分页协议不变（cursor 与 offset 并存） | `tests/cursor_pagination.rs` |
| CRUD/守卫/分页端到端（真库案例协议） | `tests/backend_contract.rs` + `docs/case-protocol.zh-CN.md` |
| 能力模块端点语义（jobs/live/uploads/export） | `tests/jobs.rs` / `tests/live.rs` / `tests/storage.rs` / `tests/export.rs` |
