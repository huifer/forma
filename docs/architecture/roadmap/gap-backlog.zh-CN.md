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
| ~~P0-2~~ | ~~**资源级 / 字段级策略未实现**~~ | **已关闭（2026-11-05）**：字段级策略落地——`access Entity { fields { field → 角色表达式 } }`；服务端在 list/get 响应序列化时裁剪（`trim_<entity>_fields`，字段→允许角色表）；Check 层 5 条 `dsl-field-policy-*` 诊断（未知字段/关联/无 provider/未知角色/表达式受限）。资源级/行级此前已实现（`access` 操作规则 + 行级条件注入 WHERE）。**余项已清零（2026-11-08，轨道 A-1）**：字段级前端显隐 Web（`canSeeField`）与 Flutter（`fieldRules + canSeeField`）均已实现并回归；多租户已落地 + A-5 管理 UI | ✅ 已完成 |
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
| ~~P0-1a~~ | ~~**行级条件不支持跨关系遍历**~~ | **已关闭（2026-11-05，一层；2026-11-08 轨道 A-2 补齐任意深度）**：`dsl::resolve_access_field` 单一来源支持 `relation.relation.field`；生成层逐跳取数 + 缺失行 403 fail-closed；非关系 / 可选关系 / 未知字段仍编译期显式诊断；回归 `tests/access_relation_depth.rs` + 生成后端 `cargo check` 零错误零警告 | ✅ 已完成 |
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
| ~~P2-1~~ | ~~`pre` 表达式子集受限（不支持跨实体查询）~~ | **已决议（2026-11-05）**：`pre` 限定为**目标实体当前行**的可判定表达式子集（字段比较 / 枚举字面量 / 逻辑组合，编译期保留 `!({expr})` 保护括号）；跨实体查询依赖 [spec:ir §15](../specifications/ir/ir-contract.zh-CN.md) 的 DSL→图 lowering（M3 稳定标识），列入后续里程碑；未知字段与越界路径编译期显式报错，不静默忽略 | ✅ 已决议 |
| ~~P2-2~~ | ~~弃用到移除的最短兼容窗口未定~~ | **已决议（2026-11-05）**：弃用 → 移除最短窗口 = **≥ 2 个次版本（minor）且 ≥ 6 个月**（取更晚者）；弃用期 MUST 继续产出产物并携带弃用警告（`forma docs check-compatibility` 的 Deprecated 依赖告警沿用）；正式移除 MUST `breaking_change: true` + ADR + 迁移计划（[spec:compatibility §9](../specifications/compatibility/compatibility-rules.zh-CN.md)） | ✅ 已决议 |
| ~~P2-3~~ | ~~`spec:manifests` 的 `apiVersion` 演进与多 runtime 权限粒度未定~~ | **已决议（2026-11-05）**：`forma.dev/v1` 内仅允许向后兼容演进；新 `apiVersion` 按版本分命名空间并存（并存期 ≥ 1 个编译器次版本），未识别版本拒绝注册（现状不变）；权限粒度 v1 保持 **capability 级**（`permissions` 汇总为并集），操作级权限由 DSL 的 `api { auth }` / `rules { access }` 承担，manifest 不重复建模（[spec:manifests §5](../specifications/manifests/capability-manifest.zh-CN.md)） | ✅ 已决议 |
| ~~P2-4~~ | ~~**无 `api` 的工程生成物有未使用导入**~~ | **已关闭（2026-11-05）**：routes / repo / validators 的导入全部按需生成（提取器逐符号判定：list→Query、get/update/delete/命令→Path、handler→State、授权→Extension）；只有实体、无 api/命令的工程 **0 警告**；回归测试 `backend_contract.rs` 的 `entity_only_project_has_no_unused_imports` / `list_only_project_imports_are_demand_driven` / `auth_module_only_project_still_compiles_imports` | ✅ 已完成 |
| ~~P2-5~~ | ~~属性形式 `@min`/`@max` 未做类型校验~~ | **已关闭（2026-11-05）**：随 P1-2 一并落地（仅数值类型 + 恰好 1 个数值参数，`dsl-invalid-constraint`） | ✅ 已完成 |
| ~~P2-6~~ | ~~**移动端（Flutter）不做角色感知**~~ | **已关闭（2026-11-05）**：与 Web 同构——生成 `lib/access.dart`（`accessRules` + `can(entity, op)`）与登录写入 `Api.currentRole`；列表页「新建 / 编辑 / 删除」与编辑页「保存」按钮按角色显隐；**角色未知（未登录 / 加载中）时不隐藏**（服务端仍是判定方）；无授权规则的工程不生成 `access.dart`、产物零变化。回归测试 `tests/flutter_access.rs`（4 项）。顺带修复两个解析器缺陷：`row [...]` 动作列表不跳过逗号（首项之后的动作被静默丢弃）与 `confirm` 后缀导致 `*.delete` 匹配失败——此前声明的删除动作从未渲染 | ✅ 已完成 |

---

## 跨切面（DSL 主线 → 联邦语义图桥接）

见 [spec:ir §15](../specifications/ir/ir-contract.zh-CN.md) 的四步渐进路径：

| 步 | 交付物 | 状态 |
|---|---|---|
| ~~M1~~ | ~~诊断携带精确 file/line（`EntityDecl` / `EnumDecl` / `RulesDecl` / `ApiDecl` / `PageDecl` / `CommandDecl`）~~ | **已完成（2026-11-05）**：声明级 `line`（解析期）+ `file`（合并期回填）；`check_program` 经 `Origin` 统一注入位置，`dsl-*` 诊断**全部**可定位到 `file:line`（回归 `tests/dsl_ir_bridge.rs`） |
| ~~M2~~ | ~~`IR_VERSION` + `Program.ir_version` + 内建类型单一权威清单~~ | **已完成（2026-11-05）**：`IR_VERSION = "1.0.0"` + `Program.ir_version`（CLI/MCP 可见）；`BUILTIN_TYPES` 为单一权威清单，新增**逐类型**四目标映射一致性回归（MySQL 列类型逐项断言） |
| ~~M3~~ | ~~`StableId` / `SourceRef` + DSL 节点与边构造~~ | **已完成（2026-11-05）**：`crates/forma/src/dsl/lower.rs::lower_to_graph` 单向桥接（`Program → UnifiedSemanticGraph`，**不改** emitter 契约）；节点用联邦侧**同一套** URI 构造函数（`forma://project/{p}/entity/{E}` 等）；边覆盖 `defines` / `references` / `belongs-to`；`forma graph`（CLI + MCP）输出稳定标识 + 依赖边 + 来源位置并支持 `--export`；MCP `forma_impact` 对 DSL 工程**不再被拒** |
| ~~M4~~ | ~~CLI/MCP 对 DSL 工程走 `check_program`（消除假绿）~~ | ✅ 已完成（2026-11-05，`engine::check_project` 共享管线） |

---

## 已关闭

| # | 缺口 | 关闭日期 | 说明 |
|---|---|---|---|
| ~~G-6~~ | `access` / `auth` 被解析但检查层与生成层均不消费（权限声明是空承诺） | 2026-11-05 | 检查层新增 7 条 `dsl-auth-*` / `dsl-access-*` 诊断；生成层在服务端强制执行（401/403）；`api { auth }`（操作级）与 `rules { access }`（实体级）均落地 |
| ~~P0-4~~ | DSL 工程上 `forma validate` 假绿（RFC-0003 M4） | 2026-11-05 | 新增 `engine::check_project` / `engine::dsl_check` 共享管线，CLI 与 MCP 同源分发；DSL 工程跑 `check_program` + lint，错误工程退出码 1 |
| ~~P1-1~~ | `Json` 未纳入内建类型清单 | 2026-11-05 | `BUILTIN_TYPES` 扩到 11 型；MySQL/Rust/TS/Dart 映射齐备；新增「生成层支持的类型必须检查层也可用」测试 |
| ~~P2-4~~ | 无 `api` 的工程生成物有未使用导入 | 2026-11-05 | routes / repo / validators 导入全按需；实体-only 工程 0 警告；3 项回归测试 |
| ~~P2-6~~ | 移动端（Flutter）不做角色感知 | 2026-11-05 | `lib/access.dart` + `can()` + 登录写入 `Api.currentRole`；按钮按角色显隐；无规则的工程产物零变化；顺带修复 `row [...]` 逗号与 confirm 后缀两个静默丢动作的解析器缺陷 |
| ~~M1~~ | DSL 诊断缺少精确 file/line | 2026-11-05 | 声明级 `line` + `file`；`Origin` 统一注入位置；`dsl-*` 诊断全部可定位 |
| ~~M2~~ | IR 无版本 + 内建类型清单不唯一 | 2026-11-05 | `IR_VERSION` + `Program.ir_version`；`BUILTIN_TYPES` 单一权威清单 + 四目标映射一致性回归 |
| ~~M3~~ | DSL 侧无稳定标识 / 依赖边 / 降级 | 2026-11-05 | `dsl::lower` 单向桥接产出与联邦同构的节点/边；`forma graph` 输出并可导出；`forma impact` 对 DSL 工程可用 |


### Phase 1 §16 未覆盖项收口（2026-11-08）

| ~~G-1~~ | RFC-0001 §16-2 | **已收口（2026-11-08，Phase 1 收口）**：依赖图缺「Tier 反向边」自动断言 → 在 `forma docs graph --json` 输出上补一条断言（低风险，择机） |
| ~~G-2~~ | RFC-0004 §16-2 | **已收口（2026-11-08，Phase 1 收口）**：Capability Manifest 缺 Deprecation 结构化字段与「10 项契约」逐项断言 → 补 `deprecated` / `superseded_by` 字段 + 逐项回归（需改 manifest schema，走 minor） |
| ~~G-3~~ | RFC-0004 §16-3 | **已收口（2026-11-08，Phase 1 收口）**：「未知 IR 扩展保留且不被擅解释」无自动断言 → 补 fixture：带未知 `extensions.*` 的 DSL → IR 往返后字段仍在 |
| ~~G-4~~ | RFC-0005 §16-4 | **已收口（2026-11-08，Phase 1 收口）**：独立 Secret Provider 抽象未建模（当前仅 env 变量注入） → §18 开放问题：先定接口归属（Runtime RFC 还是本 RFC 扩展点）再实现 |
| ~~G-5~~ | RFC-0005 §16-8 | **已收口（2026-11-08，Phase 1 收口）**：`DATABASE_URL` 缺失为启动期 `expect` panic，非优雅 fail-closed → 改为结构化启动错误（错误码 + 日志），保持 0 依赖 |
| ~~G-6~~ | RFC-0007 §16-1 | **已收口（2026-11-08，Phase 1 收口）**：`package.json` / `README` / CI 模板等工程化产物无 `@generated` 头 → 统一头注释格式（JSON/YAML 用 `#` 不合法，需按格式定策略） |
| ~~G-7~~ | RFC-0007 §16-2 | **已收口（2026-11-08，Phase 1 收口）**：部分 emitter 直接 `fs::write`，未走 `util::write_text_changed` 的哈希跳过 → 逐 emitter 收敛到统一写入函数（与 G-6 一并做） |
| ~~G-8~~ | RFC-0008 §16-3 | **已收口（2026-11-08，Phase 1 收口）**：列表查询的权限条件未注入 WHERE（当前为取行后授权，list 甚至不支持行级条件） → **需 ADR 决策，不擅自改语义**（选项见下） |
| ~~G-9~~ | RFC-0002 §16-1（P3 遗留） | **已收口（2026-11-08，Phase 1 收口）**：诊断未按规格领域拆分为独立 Validator / 命名空间 → §18 开放问题，按需立项 |
| ~~G-10~~ | RFC-0003 §15 阶段 3 | **已收口（2026-11-08，Phase 1 收口）**：`spec:ir` / `spec:types` 是正式 Markdown 规范，但无机器可读 JSON Schema → 若需机器消费再立项；当前编译期校验由 `dsl::check_program` 承担 |
| ~~G-11~~ | RFC-0010 模板 | **已收口（2026-11-08，Phase 1 收口）**：领域 RFC 沿用旧章节编号，`docs lint` 报 6 条 `section-missing` **info**（非阻断） → 保留历史结构；若统一 20 节模板需整体重排 902 行，择机 |

### 轨道 A 收尾（2026-11-08，用户决策：仅完成轨道 A，轨道 B 撤销）

> **决策变更（2026-11-09）**：轨道 B 已重新排期并全部实现，见 [roadmap §2](../../roadmap.zh-CN.md)
> 与 `CHANGELOG.md` `[Unreleased]`。本节标题与「撤销」字样保留为历史记录，不代表当前范围；
> 下表 A-1…A-8 的验收结论继续有效。

| # | 缺口 | 关闭说明 |
|---|---|---|
| ~~A-1~~ | 字段级策略前端显隐（Web + Flutter） | **已核实已实现**（文档滞后）：Web `canSeeField` / Flutter `fieldRules + canSeeField`；spec:protocols §7 缺口 6 同步标注 |
| ~~A-2~~ | 行级访问多层关系遍历 | 任意深度 + fail-closed 取数；`tests/access_relation_depth.rs`；修复连带存量编译错误（auth.rs tenant / routes `user` 提取器 / repo 未用变量 / 命令 handler `find_X` 签名） |
| ~~A-3~~ | `requestId` 中间件接线 | **已核实已实现**（文档滞后）：`request_id.rs` + task-local + 错误体填充；`tests/error_contract.rs` 专项断言；spec:protocols §4.4 / §7 缺口 4、8 同步 |
| ~~A-4~~ | GraphQL 逆向关系 + N+1 | SDL `[Source!]!` 字段 + 每请求 DataLoader（单批 REST list 按 `{rel}_id` 分组）；`tests/graphql.rs` 4 项 + 生成工程 `tsc --noEmit` + `graphql.parse/buildSchema` 通过 |
| ~~A-5~~ | 多租户管理 UI | `TenantAdmin.tsx`（吊销/迁移 + `x-admin-token`）；`tests/tenant_admin_ui.rs` + 生成 Web `tsc --noEmit`；无租户工程零变化 |
| ~~A-6~~ | artifacts manifest 来源规格 + Extension 锚点 | `ArtifactEntry.source`（生成器版本/IR 版本/输入指纹/依赖）+ `Ownership::Extension`；`tests/artifact_ownership.rs` 3 项；spec:serialization §12 同步 |
| ~~A-7~~ | patch 图表/迁移节点细粒度 | `set-chart-type` / `add-dashboard-chart`；`tests/patch_visual_nodes.rs` 5 项（含事务回滚）；迁移节点为派生节点，显式决议「不设补丁操作」并写入 spec:ir §15.4 |
| ~~A-8~~ | 逆向导入与迁移链导出 | `forma import sql|prisma`、`forma migrate export --format flyway|golang-migrate|liquibase`；`tests/import_reverse.rs` + `tests/migration_export.rs`；CLI 端到端冒烟通过 |

---


## 使用方式

- 新发现缺口：**登记到对应 spec 的「已知缺口」章节**，并在本表加一行（保持双向一致）。
- 关闭缺口：实现 → 补 Conformance → 在本表移到「已关闭」→ 在 spec 中标注关闭日期与诊断码变化。
- `forma docs validate` / `lint` 会在 spec / RFC 引用悬空时**报错**——新增 spec 需同步引用。
