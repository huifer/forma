---
id: conformance:compatibility
title: Compatibility Conformance
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: governance
depends_on: [RFC-0007, conformance:overview]
related_adrs: [ADR-0003]
related_specs: [spec:compatibility, spec:serialization]
breaking_change: false
---

# 兼容性一致性（Compatibility Conformance）

> 承诺：schema 演进可重放、spec 变更可检测、人工修改不被静默覆盖。

## 迁移链（spec:compatibility §6 / migration-safety）

| 承诺 | 证明 |
|---|---|
| 前向重放 == 当前 schema；回滚到基线 | `tests/migrate.rs::migration_chain_replays_forward_to_latest_and_backward_to_baseline` |
| 迁移 verify（哈希 + 链完整性） | `tests/migrate.rs::migration_verify` |
| 索引名内容派生（多版本链不重名） | `tests/migrate.rs::index_names_are_content_derived_*` |
| 破坏性分级（Low/Medium/High/Extreme + 未知操作 fail-safe High） | `tests/migrate.rs::classify_op_assigns_expected_risk_levels` |
| FULLTEXT 迁移（add/drop 独立通道） | `tests/searchable.rs` |

## 规格兼容性检测（P1-10）

| 承诺 | 证明 |
|---|---|
| major 版本增长未标记 `breaking_change` → error | `tests/docs_validate.rs` |
| 删除 spec → error；MUST 条款弱化 → warning candidate | `tests/docs_validate.rs`（detect_spec_compatibility_changes） |
| 废弃无继任者 → warning | `tests/docs_validate.rs` |

## 产物所有权（ADR-0003 / RFC-0007）

| 承诺 | 证明 |
|---|---|
| 人工修改不被静默覆盖（构建前快照 → 构建后恢复） | `tests/artifact_ownership.rs` |
| Adopted 条目无条件保护（adopt 后不再被覆盖） | `tests/artifact_ownership.rs` |
| 来源规格（generatorVersion / irVersion / inputFingerprint / deps） | `tests/artifact_ownership.rs::manifest_entries_record_source_provenance` |
| Extension 锚点登记且永不覆盖（连 `--overwrite` 也不覆盖） | `tests/artifact_ownership.rs::extension_anchor_is_registered_and_never_overwritten` |
| patchId 幂等（重复提交零二次变更） | `tests/studio.rs::studio_v2_bidirectional_patch` |
| 失败补丁零写入（事务回滚） | `tests/dsl_patch.rs` + `tests/studio.rs` + `tests/patch_visual_nodes.rs` |
| 迁移链导出到既有体系（Flyway / Liquibase / golang-migrate） | `tests/migration_export.rs` |

## 弃用窗口（spec:compatibility §9–10）

| 承诺 | 证明 |
|---|---|
| 弃用 → 移除窗口 ≥ 2 minor 且 ≥ 6 个月（决议条目） | `docs/architecture/roadmap/gap-backlog.zh-CN.md`（P2-2 已决议） |
