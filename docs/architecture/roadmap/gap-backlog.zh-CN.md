---
id: roadmap:gap-backlog
title: Architecture Gap Backlog
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-05
domain: governance
depends_on: [RFC-0001]
related_adrs: [ADR-0001]
related_specs: [spec:types, spec:serialization, spec:protocols, spec:compatibility, spec:ir]
breaking_change: false
---

# 架构缺口台账（Gap Backlog）

> 本文件汇总**各 Specification §「已知缺口」与 §「现状差距」**中的条目，给出统一排期。
> 规则：缺口**只在被实现并通过 Conformance 后**才能从本表移除，并在对应 spec 中标注关闭日期。
> 「静默忽略」**不是**关闭方式——未支持的语义必须产生显式诊断。

---

## 排期原则

| 级别 | 含义 | 处理 |
|---|---|---|
| **P0** | 安全 / 正确性——会放行错误结果或造成越权 | 优先，且**必须有**编译期显式诊断或 CI 门禁 |
| **P1** | 契约完整性——规范已声明但实现未达 | 排入近期，spec 中必须显式标注为「未实现」 |
| **P2** | 体验 / 效率——不影响正确性 | 择机 |

---

## P0（安全 / 正确性）

| # | 缺口 | 来源 | 现状 | 关闭条件 |
|---|---|---|---|---|
| ~~P0-1~~ | ~~**行级访问条件未实现**~~ | **已关闭（2026-11-05）**：`get`/`update`/`delete` 支持行级条件（取行 → 授权 → 校验 → 变更）；Option 字段克隆后比较；Decimal 字面量类型化 | ✅ 已完成（余 P0-1a/b 见 P1） |
| P0-2 | **资源级 / 字段级策略未实现**（ADR-0004 四层模型只落地了认证层 + 操作/实体级授权） | spec:protocols §7.6 | 无声明入口 | 权限模型补齐到四层并有 Conformance |
| ~~P0-3~~ | ~~**前端不做角色感知**~~ | **已关闭（2026-11-05）**：Web 端由授权规则编译出 `src/access.ts`（`ACCESS_RULES` + `useCan`），列表页按角色显隐；角色未知时不隐藏（服务端仍为判定方） | ✅ 已完成（Flutter 端见 P2-6） |
| ~~P0-4~~ | ~~**DSL 工程上 `forma validate` 假绿**~~ | **已关闭（2026-11-05）**：CLI 与 MCP 统一走 `engine::check_project` 分发；DSL 工程跑 `check_program` + lint；带错误工程**退出码 1** | ✅ 已完成 |

## P1（契约完整性）

| # | 缺口 | 来源 | 现状 | 关闭条件 |
|---|---|---|---|---|
| ~~P1-1~~ | ~~**`Json` 未纳入内建类型清单**~~ | **已关闭（2026-11-05）**：`Json` 已入 `BUILTIN_TYPES`（11 型），四目标映射齐备，并加测试保证「生成层支持的类型检查层也可用」 | ✅ 已完成 |
| ~~P0-3~~ | 前端不做角色感知 | 2026-11-05 | Web 端生成 `src/access.ts`（`ACCESS_RULES` + `useCurrentRole`/`useCan`），列表页按角色显隐；**角色未知时不隐藏**（服务端为判定方）；无规则的工程产物零变化 |
| ~~P1-6~~ | 游标分页未实现 | 2026-11-05 | MySQL 多键 lexicographic keyset + id tie-breaker；cursor `values[]/id`；`items/nextCursor` 响应信封；Web `useInfiniteQuery`；OpenAPI schema；Flutter target 显式拒绝；offset 协议保持不变 |
| ~~P1-7~~ | 统一错误响应体契约未落地 | 2026-11-05 | 按 RFC-0005 E1–E5 实现：`FORMA_*` 稳定错误码 + `status_for` 单点映射（E3）+ 内部错误不回显（E5，tracing 记录）；Web / Flutter / OpenAPI 同形解析（E4） |
| ~~P1-2~~ | 参数化类型边界未校验 | 2026-11-05 | `dsl-invalid-type-arg`：String 0<n≤65535 / Decimal 0<p≤65 且 s≤p / 非参数化类型带参 |
| ~~P1-3~~ | 枚举跨端一致性向量缺失 | 2026-11-05 | `tests/rule_vectors.rs` 三端字面量一致性 + 参考应用 12 条 Dart 行为向量（Flutter CI 执行）；余：Web 无测试运行器 |
| ~~P1-4~~ | 跨平台换行未归一化 | 2026-11-05 | `sha256_hex` 哈希前 CRLF/CR → LF，单元测试锁定 |
| ~~P1-5~~ | 二进制产物哈希口径未统一 | 2026-11-05 | 决议：产物全部为文本，不设二进制口径；未来引入须新增 `sha256_bytes` 并声明 |
| ~~P1-8~~ | 项目锁文件格式未定 | 2026-11-05 | v1 决议：`forma.yaml` 的 `capabilities:` 段即锁；解析闭包由 Registry 确定性导出；`.forma/capability-lock.json` 为 v2 |
| ~~P1-9~~ | Capability ↔ IR 兼容矩阵无自动校验 | 2026-11-05 | 随 Capability Registry 落地：构建期校验 ir/runtime/pin，错误阻断构建 |
| ~~P1-10~~ | 破坏性变更无自动检测 | 2026-11-05 | `forma docs check-compatibility --base <git-ref>` 检测 major bump 未标记（error）、删除 spec（error）、规范性 MUST 条款或带反引号标识的契约表行删除/弱化候选（warning candidate）；CI 对 PR 与目标分支比较 |
| ~~P2-5~~ | 属性形式 `@min`/`@max` 未做类型校验 | 2026-11-05 | 随 P1-2 落地：仅数值类型 + 恰好 1 个数值参数（`dsl-invalid-constraint`） |
| ~~P0-1~~ | 行级访问条件未实现 | 2026-11-05 | `get`/`update`/`delete` 支持行级条件；类型感知编译器（Decimal 字面量显式化、Option 克隆后比较）；越界用法（create/list 用行级、跨关系、不可比较类型）均有**编译期显式诊断** |
| ~~P0-1a~~ | ~~**行级条件不支持跨关系遍历**~~ | **已关闭（2026-11-05）**：一层 `relation.field` 遍历已实现（自动 t{N} 别名 + Option 关系 clone 后兜底）；两层以上 / Optional 关系 / 非关系遍历仍有编译期显式诊断 | ✅ 已完成 |
| ~~P0-1b~~ | ~~**行级条件不支持 Timestamp / Date / Json 比较**~~ | **已关闭（2026-11-05）**：Timestamp/Date 同类型字段对字段比较已实现（含 Option clone fallback），混比/字面量报 `dsl-access-temporal-type-mismatch`；Json 字段已入 `access_comparable`（支持 `==`/`!=`），其他组合仍 fail-closed | ✅ 已完成 |
| ~~P1-2~~ | ~~**参数化类型边界未校验**~~ | **已关闭（2026-11-05）**：`dsl-invalid-type-arg`（String 0<n≤65535 / Decimal 0<p≤65 且 s≤p / 非参数化类型带参） | ✅ 已完成 |
| ~~P1-3~~ | ~~**枚举跨端一致性向量缺失**~~ | **已关闭（2026-11-05）**：共享语义向量落地——`tests/rule_vectors.rs` 断言同一规则的边界字面量与比较方向在 Rust/TS/Dart 三端一致（Int 边界、Decimal 显式类型化字面量、required 文案）；参考应用 `app_mobile/test/rule_vector_test.dart`（12 条行为用例）在 **Flutter CI 真实执行** Dart 校验器。余项：Web 无测试运行器，TS 行为验证暂缺（结构性断言已覆盖） | ✅ 已完成 |
| ~~P1-4~~ | ~~**跨平台换行未归一化**~~ | **已关闭（2026-11-05）**：`util::sha256_hex` 统一执行 CRLF/CR → LF 归一化后再哈希，单元测试锁定 | ✅ 已完成 |
| ~~P1-5~~ | ~~**二进制 / 大文件产物哈希口径未统一**~~ | **已决议（2026-11-05）**：Forma 产物**全部是文本**，不设二进制口径；未来引入二进制产物必须新增 `sha256_bytes` 并在 spec:serialization 声明 | ✅ 已决议 |
| ~~P1-6~~ | ~~游标分页未实现~~ | **已关闭（2026-11-05）**：MySQL 多键 lexicographic keyset（全部 sort keys + id tie-breaker，游标 `{values:[...],id}`），`{items,nextCursor}` envelope；Web `useInfiniteQuery` + load-more；OpenAPI cursor 参数与响应 schema；offset API 不变 | ✅ 已完成 |
| ~~P1-7~~ | ~~**统一错误响应体契约未落地**~~ | **已关闭（2026-11-05）**：按 RFC-0005 E1–E5 实现跨后端 / Web / Flutter / OpenAPI 同形（`error`/`code`/`fields`），E5 已实现（内部错误不回显，只写日志） | ✅ 已完成（`requestId` 预留见 spec:protocols §7.8） |
| ~~P1-8~~ | ~~**项目锁文件格式未定**~~ | **已决议（2026-11-05，v1）**：`forma.yaml` 的 `capabilities:` 段即锁（声明 + pin）；解析闭包由 Registry 构建期确定性导出；`.forma/capability-lock.json`（解析快照）为 v2 增强 | ✅ 已决议 |
| ~~P1-9~~ | ~~**Capability ↔ IR 兼容矩阵无自动校验**~~ | **已关闭（2026-11-05）**：随 Capability Registry 落地——构建期校验 `compatibility.ir`/`runtime`/pin（`capability-ir-incompatible` 等），**错误阻断构建**；测试 `incompatible_ir_version_is_rejected` | ✅ 已完成 |

## P2（体验 / 效率）

| # | 缺口 | 来源 | 现状 |
|---|---|---|---|
| P2-1 | `pre` 表达式子集受限（不支持跨实体查询） | spec:protocols §7.5 | 编译期仅接受可判定表达式 |
| P2-2 | 弃用到移除的最短兼容窗口未定 | spec:compatibility §10.4 | 未约定 |
| P2-3 | `spec:manifests` 的 `apiVersion` 演进与多 runtime 权限粒度未定 | spec:manifests §5 | 开放问题 |
| P2-4 | **无 `api` 的工程生成物有未使用导入** | spec:types §7.4 | 导入逻辑由 api 操作驱动；只有实体没有 api 时产生一批 `unused_imports` |
| ~~P2-5~~ | ~~属性形式 `@min`/`@max` 未做类型校验~~ | **已关闭（2026-11-05）**：随 P1-2 一并落地（仅数值类型 + 恰好 1 个数值参数，`dsl-invalid-constraint`） | ✅ 已完成 |
| P2-6 | **移动端（Flutter）不做角色感知** | spec:protocols §7.7 | Web 端已按角色显隐；Flutter 端仍由服务端 403 兜底 |

---

## 跨切面（DSL 主线 → 联邦语义图桥接）

见 [spec:ir §15](../specifications/ir/ir-contract.zh-CN.md) 的四步渐进路径：

| 步 | 交付物 | 状态 |
|---|---|---|
| M1 | 诊断携带精确 file/line（`EntityDecl` / `ApiDecl` / `PageDecl` / `CommandDecl`） | ⏳ 未开始 |
| M2 | `IR_VERSION` + `Program.ir_version` + 内建类型单一权威清单 | ⏳ 未开始（与 P1-1 合并） |
| M3 | `StableId` / `SourceRef` + DSL 节点与边构造 | ⏳ 未开始 |
| M4 | CLI/MCP 对 DSL 工程走 `check_program`（消除假绿） | ✅ 已完成（2026-11-05，`engine::check_project` 共享管线） |

---

## 已关闭

| # | 缺口 | 关闭日期 | 说明 |
|---|---|---|---|
| ~~G-6~~ | `access` / `auth` 被解析但检查层与生成层均不消费（权限声明是空承诺） | 2026-11-05 | 检查层新增 7 条 `dsl-auth-*` / `dsl-access-*` 诊断；生成层在服务端强制执行（401/403）；`api { auth }`（操作级）与 `rules { access }`（实体级）均落地 |
| ~~P0-4~~ | DSL 工程上 `forma validate` 假绿（RFC-0003 M4） | 2026-11-05 | 新增 `engine::check_project` / `engine::dsl_check` 共享管线，CLI 与 MCP 同源分发；DSL 工程跑 `check_program` + lint，错误工程退出码 1 |
| ~~P1-1~~ | `Json` 未纳入内建类型清单 | 2026-11-05 | `BUILTIN_TYPES` 扩到 11 型；MySQL/Rust/TS/Dart 映射齐备；新增「生成层支持的类型必须检查层也可用」测试 |

---

## 使用方式

- 新发现缺口：**登记到对应 spec 的「已知缺口」章节**，并在本表加一行（保持双向一致）。
- 关闭缺口：实现 → 补 Conformance → 在本表移到「已关闭」→ 在 spec 中标注关闭日期与诊断码变化。
- `forma docs validate` / `lint` 会在 spec / RFC 引用悬空时**报错**——新增 spec 需同步引用。
