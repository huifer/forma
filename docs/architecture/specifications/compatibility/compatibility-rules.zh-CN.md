---
id: spec:compatibility
title: Compatibility and Versioning Rules Specification
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-05
domain: architecture
governing_rfc: RFC-0001
depends_on: [RFC-0001]
related_adrs: [ADR-0003]
related_specs: [spec:ir, spec:manifests, spec:types, spec:serialization]
breaking_change: false
---

# spec:compatibility — 兼容性与版本规则规范

> **规范性**：本规范定义 Forma 的**版本分类、兼容判定、破坏性变更声明与分层兼容规则**。
> 治理依据：[RFC-0001 架构总览](../../rfc/RFC-0001-architecture-overview.zh-CN.md) · [RFC-0004 能力系统](../../rfc/RFC-0004-capability-system.zh-CN.md) · [ADR-0003](../../adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md)。

---

## 1. 版本分类（五类，不可混为一谈）

| 版本类型 | 管理对象 | 当前值 |
|---|---|---|
| **Specification Version** | 规格格式（`spec:*`） | `1.0.0` |
| **IR Version** | 语义中间表示（Tier 1 契约） | `1.0.0` |
| **Capability Version** | 能力定义（Manifest） | 每能力独立 |
| **Adapter Version** | 技术适配器 | 每适配器独立 |
| **Runtime Version** | 执行环境 | `1.0.0` |

另含 **Project Version**（业务项目版本）与 **Compiler Version**（工具链版本），二者**不**计入上述五类。

- **MUST NOT** 用项目版本推断语义兼容性——项目版本对能力/契约版本无约束力。

## 2. 兼容判定

- 兼容 **MUST NOT** 仅凭版本号判断，也 **MUST NOT** 仅因版本号变化就认为兼容。
- 判定依据：**契约测试（Conformance）+ 语义规则**。
- 版本范围表达（见 §3）**MUST** 用「编译器实际提供的版本」去匹配，而非反过来。

## 3. 版本范围语法

能力 Manifest 的 `compatibility` 使用空格分隔的比较器组合：

```yaml
compatibility:
  ir: ">=1.0.0 <2.0.0"
  runtime: ">=1.0.0"
```

| 比较器 | 含义 |
|---|---|
| `>=x.y.z` / `>x.y.z` | 大于等于 / 大于 |
| `<=x.y.z` / `<x.y.z` | 小于等于 / 小于 |
| `=x.y.z`（缺省） | 精确等于 |
| `^x.y.z` | caret：`>=x.y.z` 且 `<` 下一个不兼容版本（major>0 时为下一 major；major==0 时为下一 minor） |
| `~x.y.z` | tilde：`>=x.y.z` 且 `<` 下一 minor |
| 空 / `*` | 匹配任意版本 |

- 版本号 **MUST** 是 `major.minor.patch`（可省略 patch，按 `0` 处理）。
- 范围**不匹配**时，构建 **MUST** 失败（`capability-ir-incompatible` / `capability-runtime-incompatible`）。

## 4. 破坏性变更声明

- 受管文档 **MUST** 在 frontmatter 声明 `breaking_change: true|false`（RFC 与 Spec 强制；ADR 模板不含该字段）。
- 声明 `breaking_change: true` 的 RFC **MUST** 同时具备：
  1. **迁移计划**章节（[RFC 模板 §14](../../rfc/TEMPLATE.md)）——缺失即 `forma docs lint` 报错；
  2. **兼容验证**（契约测试 / 迁移链重放）。
- 破坏性变更 **MUST NOT** 只改版本号了事。

## 5. 分层兼容

| 层 | 成员 | 变更规则 |
|---|---|---|
| **Tier 1** Stable Foundation | Core IR 契约、类型系统、稳定标识、依赖图、Specification 包、兼容规则 | **MUST** 经正式 RFC + 迁移设计 + 兼容验证 |
| **Tier 2** Extensible | Capability、Adapter、Runtime、插件、Agent 协议、数据平台 | 允许独立扩展，但 **MUST** 遵循 Tier 1 契约 |
| **Tier 3** Experimental | 新型可视化、新 Agent 工作模式、新 UI Runtime、新数据源、新工作流执行器 | 允许快速试验，**MUST NOT** 污染 Tier 1 |

- **Tier 不是永久身份**：实验能力可升级为稳定能力；稳定能力可经迁移后废弃。
- 依赖方向 **MUST** 是 Tier 1 ← Tier 2 ← Tier 3，**MUST NOT** 反向。

## 6. IR 层兼容

### 6.1 向后兼容

- 破坏语义的变更 **MUST** 走**迁移**（`crates/forma/src/emit/migrate.rs`）并验证。
- 迁移链为 `000N_upgrade.sql` + `000N_rollback.sql` **配对**；`0001_init.sql` 基线**不可变**。
- 迁移链重放 **MUST** 满足：前向全链 == 最新 schema；反向回到基线。

### 6.2 向前兼容

- 未知扩展字段（`extensions.<capabilityId>`）**MUST** 被**保留**、**MUST NOT** 被擅解释。
  **现状（2026-11-08，G-3）**：DSL v0.3 尚无 `extensions` 语法——未知顶层构造按**fail-closed**
  显式报错（`dsl-parse`，回归 `tests/domain_validators.rs`）。本条的「保留」语义待 extensions
  语法引入时生效，届时的实现必须附带保留性回归。
- Adapter **MUST** 只处理自己声明支持的扩展。
- 扩展字段 **MUST NOT** 覆盖核心语义；扩展升级 **MUST** 有迁移策略。

### 6.3 迁移风险分级

| 等级 | 操作 | 说明 |
|---|---|---|
| **Low** | `create_table` / `add_column` / `add_index` / `add_unique` / `add_fk` | 纯增量、可逆 |
| **Medium** | `drop_index` / `drop_unique` / `drop_fk` | 可逆但影响查询计划 |
| **High** | `modify_column` / `drop_column` | 数据兼容风险 / 回滚丢数据 |
| **Extreme** | `drop_table` | 数据销毁 |
| **High**（fail-safe） | 未识别的操作 | **未知即高危**，避免静默放过破坏性变更 |

- High / Extreme **MUST** 经人工审批（`requires_approval`）。

## 7. 依赖闭包与能力兼容

- 能力启用 **MUST** 解析其**依赖闭包**；闭包内任一能力不兼容 → 整体不可用。
- 依赖闭包 **MUST** 无环；有环 → 报错。
- 项目 **MAY** 对能力加版本 pin（`forma.yaml` 的 `capabilities: {id: "约束"}`）；pin 与已注册版本不符 → 报错。
- 未在编译器中实现的 adapter **MUST** 给出警告而非静默忽略（声明被接受，但不产生额外产物）。

## 8. 产物兼容

- 产物分三类：**Generated**（可无条件覆盖）/ **Extension**（人工扩展点）/ **Adopted**（人工改写）。
- **Adopted** 产物被检测到冲突时 **MUST NOT** 静默覆盖（[ADR-0003](../../adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md)）。
- 同一 DSL 输入 **MUST** 产出字节级一致的产物。

## 9. 弃用（Deprecation）

- 文档状态进入 `Deprecated` **MUST** 通过 `superseded_by` 指向继任文档——否则 `forma docs check-compatibility` 警告。
- 被 `Deprecated` 的文档若仍被其他文档 `depends_on` → 警告（依赖方应迁移）。
- 弃用 **MUST** 提供迁移政策，**MUST NOT** 立即移除支持。

## 10. 已知缺口

| # | 缺口 |
|---|---|
| ~~1~~ | ~~项目锁文件格式未定~~ **已决议（2026-11-05，v1）**：`forma.yaml` 的 `capabilities:` 段即为锁（声明 + 可选版本 pin）；解析闭包由 Registry 在构建期**确定性导出**（同编译器版本 → 同闭包）。`.forma/capability-lock.json`（解析快照，用于跨编译器版本比对）作为 v2 增强，需要时再立项 |
| ~~2~~ | ~~Capability ↔ IR 兼容矩阵无自动校验~~ **已关闭（2026-11-05）**：随 Capability Registry 落地——构建期 `registry.resolve()` 校验 `compatibility.ir` / `compatibility.runtime` / 版本 pin（`capability-ir-incompatible` / `capability-runtime-incompatible` / `capability-pin-mismatch`），**错误阻断构建**；回归测试 `incompatible_ir_version_is_rejected` |
| ~~3~~ | ~~破坏性变更的自动检测未实现~~ **已部分关闭（2026-11-05）**：`forma docs check-compatibility --base <git-ref>` 自动检测 major version bump 未声明（error）、spec 删除（error）、规范性 MUST 条款或带反引号标识的契约表行删除/弱化候选（warning）；CI 对 PR 基线执行检测。自然语言语义等价仍需人工审查——启发式只发候选，不替代架构审批 |
| ~~4~~ | ~~弃用到移除的最短兼容窗口未定~~ **已决议（2026-11-05，P2-2）**：弃用 → 移除的最短窗口 = **≥ 2 个次版本（minor）且 ≥ 6 个月**（取更晚者）。弃用期 MUST：继续产出产物、在构建输出中携带弃用警告、保留迁移路径；正式移除 MUST `breaking_change: true` + ADR + 迁移计划。文档层沿用本规范 §9（`Deprecated` + `superseded_by` + 依赖告警） | ✅ 已决议 |

## 11. 开放问题

> 收口（2026-11-08）：3 项中 1 项此前已决议、1 项本阶段落地、1 项收窄决议。

- [x] ~~是否为 Capability 引入语义版本化的弃用窗口（建议 ≥ 2 个次版本）~~ **已决议（2026-11-05，见 §10.4）**：弃用窗口 = ≥ 2 个次版本且 ≥ 6 个月，适用于 Capability 与受管文档。
- [x] 语义 diff 如何与迁移风险分级联动（自动给出 requires_approval 建议）。
  **已落地（2026-11-08）**：`forma_plan` 报告含 `risk.overall` / `approvalRequired` /
  `accessRuleChange`（P5），并把语义变更（实体/字段增删改 → 迁移操作）纳入同一分级；
  补丁审批门与 `assess_risk` 同源（不漂移）。
- [x] 多 IR 版本并存（v1 / v2）时的产物兼容矩阵表达。
  **收窄决议**：v0.3 单版本（`IR_VERSION = 1.2.0`）；v2 未至。届时按 `apiVersion` 分命名空间并存
  （与 `spec:manifests` 的 P2-3 决议同构），兼容矩阵按版本对声明。

## 11. 开放问题

- ~~是否为 Capability 引入语义版本化的弃用窗口（建议 ≥ 2 个次版本）~~ **已决议（2026-11-05，见 §10.4）**：弃用窗口 = ≥ 2 个次版本且 ≥ 6 个月，适用于 Capability 与受管文档。
- 语义 diff 如何与迁移风险分级联动（自动给出 requires_approval 建议）。
- 多 IR 版本并存（v1 / v2）时的产物兼容矩阵表达。

## 12. 参考

- RFC-0001 架构总览：[../../rfc/RFC-0001-architecture-overview.zh-CN.md](../../rfc/RFC-0001-architecture-overview.zh-CN.md)
- RFC-0004 能力系统：[../../rfc/RFC-0004-capability-system.zh-CN.md](../../rfc/RFC-0004-capability-system.zh-CN.md)
- RFC-0007 构建与产物：[../../rfc/RFC-0007-build-and-artifact.zh-CN.md](../../rfc/RFC-0007-build-and-artifact.zh-CN.md)
- `spec:manifests`：[../manifests/capability-manifest.zh-CN.md](../manifests/capability-manifest.zh-CN.md)
- 迁移安全：[../../../migration-safety.zh-CN.md](../../../migration-safety.zh-CN.md)
