---
id: conformance:adapter
title: Adapter Conformance
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: governance
depends_on: [RFC-0001, conformance:overview]
related_adrs: [ADR-0003]
related_specs: [spec:serialization]
breaking_change: false
---

# 适配器一致性（Adapter Conformance）

> 承诺：每个 target（Emitter）输出**确定性**、**可编译**、且**不影响无关产物**（零变化原则）。

## 确定性与可编译（RFC-0001 §6.1 适配层契约）

| 承诺 | 证明 |
|---|---|
| 同 DSL 两次构建 byte 级一致 | `tests/build.rs`（连续两次构建零变更） |
| seed 数据确定性（SHA-256 派生主键 + 幂等） | `tests/seed_flutter.rs` + `forma seed --verify` |
| 迁移链重放（前向 == 当前 schema） | `tests/migrate.rs::migration_chain_replays_*` |

## 编译验证门（L3）

| 承诺 | 证明 |
|---|---|
| Rust 后端 `cargo check` 零错误零警告 | `tests/backend_contract.rs`（结构断言）+ **CI `generated-backend` job**（school + access-depth 两个生成工程真实编译）+ 本地生成端验证流程 |
| Web `tsc --noEmit` / `vite build` | CI `web` job（cursor-pagination / school）+ 本地验证流程 |
| GraphQL SDL `parse`/`buildSchema` + `tsc --noEmit` | `tests/graphql.rs`（4 项）+ 生成工程真实验证 |
| Flutter `flutter analyze` | `tests/flutter_*.rs`（结构断言）+ 本地验证流程 |
| 跨端行为向量（同一规则三端字面量一致） | `tests/rule_vectors.rs`（Rust/TS/Dart） |

## 零变化原则（多目标互不污染）

| 承诺 | 证明 |
|---|---|
| 默认 engine = mysql；postgres/sqlite/redis 不改变 mysql 产物 | `tests/postgres.rs` / `tests/sqlite.rs` / `tests/redis.rs` |
| 默认前端 = react（受控表单）；RHF/Zustand 仅 opt-in 生效 | `tests/web_rhf.rs` / `tests/web_zustand.rs` |
| 无 Attachment 工程：storage.rs / multipart / uploadFile 零产物 | `tests/storage.rs::no_attachment_project_has_zero_storage_artifacts` |
| 无 export 声明工程：export.rs / 路由 / 按钮 零产物 | `tests/export.rs::no_export_project_has_zero_export_artifacts` |
| 非 Web 目标显式拒绝 dashboard（宪法 6.2 不静默降级） | `tests/dashboard.rs::non_web_targets_reject_dashboard_pages` |
| 仅 `@tenant` 工程生成 `TenantAdmin.tsx`（普通工程零变化） | `tests/tenant_admin_ui.rs` |
| 迁移链导出 SQL 正文逐字复制（三种目录约定） | `tests/migration_export.rs` |
| 非 JSON 产物全部带 `@generated` 标记（JSON 按决议豁免） | `tests/generated_headers.rs` |
| 产物写入统一走内容哈希跳过 + 原子写 | `tests/artifact_atomic_write.rs` |
| NestJS/Hono/FastAPI 对不支持的语义显式拒绝 | `tests/nestjs.rs` / `tests/hono.rs` / `tests/fastapi.rs` |

## 能力扩展不改核心（Phase 1 标准一）

| 承诺 | 证明 |
|---|---|
| 新 Capability → 核心产物 byte 级不变 | `tests/capability.rs` |
| Capability 版本/pin 不兼容 → 构建阻断 | `tests/capability.rs::incompatible_ir_version_is_rejected` |
