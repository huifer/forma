---
id: conformance:overview
title: Conformance Suite Overview
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: governance
depends_on: [RFC-0001]
related_adrs: [ADR-0001]
related_specs: [spec:types, spec:serialization, spec:protocols, spec:compatibility]
breaking_change: false
---

# Conformance Suite（一致性套件总纲）

> 治理铁律（[README §0](../README.md)）：**测试规范必须能验证 RFC 与 Specification 中的承诺**。
> 本套件把这一铁律形式化：每一条「承诺（RFC/Spec 条款）」都必须指向一个**可运行的证明**（测试），
> 缺口在 [roadmap/gap-backlog](../roadmap/gap-backlog.zh-CN.md) 排期。

## 运行方式

```bash
cargo test          # 全量 67 套件 / 414 测试（CI 强制）
cargo test --test <suite>   # 单套件
```

矩阵文档（承诺 → 证明 → 运行命令）：

| 层 | 文档 | 覆盖 |
|---|---|---|
| 语义 | [semantic.md](semantic.md) | DSL 检查诊断 ↔ 权限 ↔ 可视化 ↔ 行为向量 |
| 适配器 | [adapter.md](adapter.md) | Emitter 契约：确定性 / 零变化 / 编译验证门 |
| 运行时 | [runtime.md](runtime.md) | 错误契约 E1–E5 / requestId / 结构化日志 |
| 兼容性 | [compatibility.md](compatibility.md) | 迁移链重放 / spec 兼容性检测 / 产物所有权 |

## 门禁规则（Phase 1 三条完成标准）

1. **新 Capability 不改核心**：`tests/capability.rs`（加能力 → 核心产物 byte 级不变）。
2. **新 Adapter 不改 IR**：新增 target 只新增 emitter（postgres/sqlite/…），IR 与其余产物零变化——
   由各 target 的「默认零变化」回归锁定（见 [adapter.md](adapter.md)）。
3. **改规格不破坏无关产物**：产物所有权 + 零变化原则——`tests/artifact_ownership.rs` +
   各 emitter 的「无声明/未 opt-in 工程 byte 级不变」测试（见 [adapter.md](adapter.md)）。

## fixtures

测试夹具：`examples/`（school / cursor-pagination 等参考工程）+ 各测试内联构造（tmp_dir 隔离）。
共享 fixture 目录 `conformance/fixtures/` 暂未启用（v1 决议：夹具随测试内联，避免双源）。
