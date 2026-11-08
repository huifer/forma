---
id: conformance:semantic
title: Semantic Conformance
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: governance
depends_on: [RFC-0003, RFC-0008, conformance:overview]
related_adrs: [ADR-0002, ADR-0004]
related_specs: [spec:types, spec:ir]
breaking_change: false
---

# 语义一致性（Semantic Conformance）

> 承诺：DSL 语义检查（诊断码）、类型系统、权限语义在**检查层 / 生成层 / 前端层**一致。

## 类型系统（spec:types）

| 承诺 | 证明 | 
|---|---|
| `BUILTIN_TYPES` 单一权威清单（12 型） | `tests/dsl_ir_bridge.rs::builtin_types_have_mappings_in_all_targets` |
| 每型四目标映射（MySQL/Rust/TS/Dart）非空且为具体列类型 | 同上（expected_mysql 逐型断言） |
| 参数化边界（String/Decimal）`dsl-invalid-type-arg` | `tests/dsl_validate.rs` |
| min/max 仅数值类型 `dsl-invalid-constraint` | `tests/dsl_validate.rs` |

## IR 稳定标识与桥接（RFC-0003 / spec:ir）

| 承诺 | 证明 |
|---|---|
| DSL→图单向桥接，节点 URI 与联邦侧同构 | `tests/dsl_ir_bridge.rs` |
| 诊断全部携带 file/line（Origin 注入） | `tests/dsl_ir_bridge.rs` |
| `forma validate` 对 DSL 工程**不假绿**（错误 → 退出码 1，CLI↔MCP 一致） | `tests/dsl_validate.rs` |

## 权限语义（RFC-0008 / ADR-0004）

| 承诺 | 证明 |
|---|---|
| api 操作级 + 实体级规则服务端权威执行（401/403） | `tests/api_auth.rs`（18 项） |
| 行级条件（Timestamp 比较 / 越界诊断） | `tests/api_auth.rs` |
| 行级条件**任意深度**关系遍历 + 关系行缺失 fail-closed | `tests/access_relation_depth.rs` + 生成后端 `cargo check`（CI `generated-backend`） |
| 字段级裁剪（服务端序列化裁剪 + 前端显隐同源） | `tests/field_policies.rs` + `tests/web_access.rs` + `tests/flutter_access.rs` |
| 补丁操作语义（图表/看板节点，错误变换事务回滚） | `tests/patch_visual_nodes.rs` |
| GraphQL 逆向关系（每请求 DataLoader 防 N+1） | `tests/graphql.rs` + 生成工程 `tsc --noEmit` / SDL `buildSchema` |
| 逆向导入骨架通过 `check_program`（不引用不存在的实体/非法类型） | `tests/adapter_roundtrip.rs` + `tests/import_reverse.rs` |
| 每个规格领域都有专属诊断码并被测试覆盖（17 领域） | `tests/domain_validators.rs` |
| 未知顶层构造 fail-closed（无静默忽略） | `tests/domain_validators.rs::unknown_top_level_construct_is_fail_closed_not_preserved` |
| IR 机器可读契约与实现一致（schema 漂移锁定） | `tests/ir_schema.rs`（4 项） |
| @tenant 行级隔离 + 管理端点（未设 token → 403 关闭） | `tests/backend_contract.rs` |

## 可视化语义（RFC-0010）

| 承诺 | 证明 |
|---|---|
| chart 语义校验（viz-* 诊断码）阻断构建 | `tests/visualization.rs` |
| metric/table 形状校验（V0.2） | `tests/visualization_v02.rs` |
| theme token 白名单（V0.5） | `tests/visualization_v05.rs` |
| dashboard 条目/筛选校验（V0.3） | `tests/dashboard.rs` |
