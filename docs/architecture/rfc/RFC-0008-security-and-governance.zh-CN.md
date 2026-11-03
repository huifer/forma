---
id: RFC-0008
title: Security and Governance Architecture
status: Proposed
version: 1.0
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-04
domain: security
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0002]
related_adrs: [ADR-0001, ADR-0004]
related_specs: [spec:ir, spec:types, spec:protocols]
breaking_change: false
---

# RFC-0008：安全与治理架构（Security and Governance Architecture）

## 1. Metadata

见 frontmatter。本 RFC 规定 Forma 的**安全模型**与**治理机制**：权限语义（RBAC / 资源级 / 行级 / 字段级 / 租户隔离）、权限的执行原则、密钥处理、审计、依赖安全，以及验证等级（Verification Levels）与治理流程的对应关系。

## 2. Abstract

本 RFC 确立一条**安全底线**：

> **前端显隐不是权限控制。所有权限必须在权威服务端执行。**

生成式代码最危险的失败模式，是「权限只体现在按钮藏起来了，于是 Agent 以为安全了」。Forma 的权限必须**一处声明、服务端权威执行、前端仅作体验优化**——规则与权限的同源编译原则在安全维度上的延伸（[architecture.zh-CN.md §1](../../architecture.zh-CN.md) 约束 2）。

同时定义验证等级（L1–L5）与治理流程的映射：哪一级验证是哪个变更类型的**放行门槛**。

## 3. Motivation

### 3.1 Current Problem

**权限语义现状**（源码核对）：

| 机制 | DSL 位置 | 编译到后端 | 编译到前端 |
|---|---|---|---|
| 字段校验 | `rules { field X { min/max/required } }` | ✅ validator 函数 | ✅ zod / Form validator |
| 跨字段检查 | `rules { check X "msg" { expr } }` | ✅ | ✅ 表单级 |
| 守卫 | `rules { guard X.op { deny expr } }` | ✅ 前置检查 | ✅ 按钮隐藏 + 禁用态 |
| 权限（RBAC） | `rules { access X { read → … create → … } }` | ⚠️ **未实现** | ⚠️ **未实现** |
| API 鉴权 | `api X { auth { create update → role in […] } }` | ⚠️ **未实现** | ⚠️ **未实现** |

> 核对结论：`rules` 的 `access` 块与 `api` 的 `auth` 块**在 parser 中被识别**（`crates/forma/src/dsl/parser.rs` 解析 `auth`），但**检查层与 emitter 均未消费**——即这两块语义**目前是空承诺**。同时 `rules` 块的 `access` 子句在 parser 中亦未见对应处理。

这构成一个**安全性缺口**：规格作者写下 `auth { delete → role == "admin" }` 后，编译产物**不会**包含该检查——但规格读起来像是做了权限控制。

**其他缺口**：

- **无多租户**：行级隔离（`@tenant`）在 roadmap 中规划为 M10，未实现。
- **无密钥管理契约**：Secret 如何注入未成文。
- **无审计基线**：`audit { action }` 已实现（生成 `audit_logs` 表 + `tracing` 日志），但 actor 仍为 `anonymous`（待 auth 模块接入）。
- **无验证门槛定义**：L1–L5 已定义（[verification.zh-CN.md](../../verification.zh-CN.md)），但「哪类变更必须过哪一级」未成文。

### 3.2 Why Existing Architecture Is Insufficient

权限是**跨层语义**：它既要在 DSL 中表达，又要在后端权威执行，还要在前端作体验优化，还需在 Agent 协议中作为审批依据（RFC-0006 §8.6）。这些分属不同 RFC，但**权限的语义本身**必须单点定义，否则会出现「DSL 说的是 A、后端执行的是 B」。

### 3.3 Expected Outcomes

- 权限语义单点定义，一次声明、三端编译。
- 未实现的权限构造**显式报错**，而非静默忽略。
- Secret 不进入规格与生成产物。
- 每类变更都有明确的验证放行门槛。

## 4. Goals

- 定义权限模型（RBAC / 资源级 / 行级 / 字段级 / 租户）。
- 定义权限执行原则与「前端显隐 ≠ 权限控制」。
- 定义密钥处理与 Secret Provider 契约。
- 定义审计契约。
- 定义验证等级与变更类型的门槛映射。

## 5. Non-goals

- 不实现具体的认证协议（OAuth / OIDC / SAML）——由 auth 能力（RFC-0004 类别）承接。
- 不实现多租户的完整实现（属未来能力），本 RFC 只定义其语义要求。
- 不定义 Agent 补丁协议（见 RFC-0006）。
- 不定义部署与网络策略。

## 6. Terminology

- **Authority**：权限判定的权威位置——**服务端**。
- **Policy**：权限规则（谁能对什么做什么）。
- **RBAC**：基于角色的访问控制。
- **ABAC**：基于属性的访问控制。
- **Row-level / Field-level**：行级 / 字段级权限。
- **Tenant Isolation**：租户数据隔离。
- **Secret Provider**：密钥注入机制，规格与产物中只出现**引用**。

## 7. Architectural Context

### 7.1 System Context

```text
   User ──▶ Authentication（识别主体：user / role / tenant / attrs）
                │
                ▼
        Policy Evaluation（RBAC / ABAC / 行级 / 字段级）
                │
                ▼
   ┌────────────┼────────────┬──────────────┐
   ▼            ▼            ▼              ▼
Resource     Query        Command       Field（脱敏 / 过滤）
(CRUD)      (列表过滤)    (命令前置)      (输出裁剪)
                │
                ▼
     Authorization Decision（allow / deny + reason）
                │
        ┌───────┴────────┐
        ▼                ▼
   后端权威执行        前端体验优化
   （唯一强制点）      （可绕过，不构成安全边界）
```

### 7.2 Dependencies

- 依赖 RFC-0002（规格领域与语法约束）。
- 被 RFC-0006（Agent 审批门）引用。

### 7.3 Boundaries

- **安全判定只在服务端**。前端产物的权限表达**仅用于体验**，其缺失或错误不得导致越权。
- 权限语义属 Semantic Core 的**规则领域**（RFC-0002 §8.1），不是独立子系统。
- Secret Provider 属 Runtime/Deployment 关注点，不进入 Semantic Core。

## 8. Design

### 8.1 Conceptual Model

**权限四层模型**（自上而下收敛）：

| 层 | 判定对象 | 例子 | 执行位置 |
|---|---|---|---|
| 认证 Authentication | 主体是谁 | user、role、tenant、attrs | 服务端中间件 |
| 策略 Policy | 主体能否执行某操作 | `delete → role == "admin"` | 服务端 |
| 资源级 Resource | 能否操作该实体 | `Order` 的 CRUD | 服务端 |
| 行级 Row | 能否操作该行 | `tenant_id = me` | 服务端（查询过滤 + 写入校验） |
| 字段级 Field | 能否看到该字段 | 敏感字段脱敏 | 服务端（输出裁剪） |

**关键原则**：上层判定失败即拒绝；下层只能在**已授权集合内**进一步收窄，**不能放宽**。

### 8.2 Architecture

**声明形态**（既有 DSL，语义在此定稿）：

```fm
rules OrderRules {
  // 资源级 + 角色
  access Order {
    read   → any
    create → role in ["admin", "ops"]
    update → role in ["admin", "ops"] && status != "approved"
    delete → role == "admin"
  }

  // 行级（租户隔离，规划中）
  access Order {
    read → tenant_id == me
  }
}

api Orders {
  entity Order
  operations [list get create update delete]
  auth {
    create update delete → role in ["admin", "ops"]
  }
}
```

**执行契约**（后端权威）：

| 位置 | 契约 |
|---|---|
| 列表查询 | 权限条件**注入**查询（WHERE），而非查询后过滤——后者会泄露存在性与分页错误 |
| 单条读取 | 读取后校验，不匹配返回 404（不泄露存在性） |
| 写入 | 校验通过才执行；`guard` 与 `access` 同时生效（取更严格者） |
| 字段级 | 响应序列化时裁剪；不因「前端不需要」而省略服务端判定 |
| 越权响应 | 403 + 稳定错误码 + i18n 消息；不泄露内部策略细节 |

**前端契约**：

- 前端**消费**同一份规则（生成 zod / 隐藏按钮 / 禁用态），仅为体验。
- 前端**不得**成为唯一判定点（API 无 token 必须在服务端拒绝）。
- 前端隐藏**不等于**拒绝——这是本 RFC 的核心断言。

**未实现语义的显式化**（修补 §3.1 缺口）：

> 在 `access` / `auth` 块被完整实现之前，编译器**必须**对它们显式报错或明确 warning，不得静默忽略。

具体地：Check 阶段检测到 `access` / `auth` 非空时，产出 `dsl-unsupported-policy`（severity=error，阻断构建），并在错误消息中指向 auth 能力的路线图位置。这把「空承诺」变成「显式缺口」——符合设计宪法 G5（[roadmap.zh-CN.md §6.2](../../roadmap.zh-CN.md)）。

**密钥处理**：

| 规则 | 说明 |
|---|---|
| S1 规格只存引用 | 规格中出现 `secretRef` / 环境变量名，**不出现**密钥值 |
| S2 产物不含密钥 | 生成代码只含引用；密钥由运行时从 Secret Provider 注入 |
| S3 密钥不进诊断 | 诊断与错误消息不得回显密钥值 |
| S4 密钥不进日志 | 结构化日志不含密钥字段 |
| S5 审计记录密钥使用 | 密钥的使用应有审计条目（谁、何时、何资源） |

**审计契约**：

- 已有：`audit { action "…" }` → 生成 `audit_logs` 表 + `tracing::info!` 结构化日志（`emit/rust_backend.rs` 的 `audit_rs`）。
- 待补：actor 字段（当前为 `anonymous`）——auth 能力接入后填充当前用户。
- 待补：审计不可篡改（追加写 + 权限隔离）。

### 8.3 Data Model

**策略表达式**（复用既有封闭表达式语言）：

| 运算符 | 语义 | 后端 | 前端 |
|---|---|---|---|
| `role in ["a","b"]` | 角色集合 | ✅ | ✅ |
| `role == "admin"` | 角色相等 | ✅ | ✅ |
| `any` | 无限制 | ✅ | ✅ |
| `tenant_id == me` | 租户隔离 | 规划 | 规划 |
| `field == "value"` | 属性条件（ABAC） | 规划 | 规划 |

**行级权限的注入形态**（规划）：

```sql
-- 权限条件作为 WHERE 的一部分注入，而非后置过滤
SELECT ... FROM orders WHERE tenant_id = ? AND status = ?
```

### 8.4 Interfaces and Contracts

- **DSL → 后端**：权限条件编译为 Rust 判定函数 + 查询条件片段 + 序列化裁剪。
- **DSL → 前端**：同一规则编译为 zod schema / 可见性判定（体验层）。
- **规则 → 校验**：权限表达式在 Check 阶段做类型检查与运算符可编译性检查（三端一致性）。
- **验证**：权限相关变更必须通过 L2（语义）+ L4（行为：越权用例真实拒绝）。

### 8.5 Execution Flow

```text
请求到达
  ↓
Authentication：解析 token / session → Principal { user, roles, tenant, attrs }
  ↓
Route 层：资源级 access 判定（操作是否允许）
  ↓
Handler 层：
   ├─ guard 判定（状态前置条件）→ 409/403
   ├─ 行级判定（注入查询条件 / 写入校验）→ 404/403
   └─ 执行
  ↓
Response 序列化：字段级裁剪
  ↓
审计（如 spec 声明 audit）
```

### 8.6 Extension Points

- **新权限模型**：ABAC、ABAC+RBAC 混合、策略即代码（OPA / Cedar / OpenFGA 适配）——作为独立 Capability 接入（RFC-0004），不修改 Semantic Core。
- **新认证方式**：由 auth Capability 承接，产出统一 `Principal` 抽象供策略消费。
- **新密钥来源**：Secret Provider 接口扩展（env / file / vault / 云 KMS）。
- **新审计后端**：审计条目落库之外，可路由到 SIEM。

### 8.7 验证门槛映射（变更类型 → 最低验证等级）

| 变更类型 | 最低门槛 | 理由 |
|---|---|---|
| 纯文档/注释 | L1 | 无语义影响 |
| 新增实体/字段（可空） | L2 | 语义正确即可 |
| 新增字段（非空无默认） | L2 + L3 | 需真实迁移与编译 |
| 改字段类型 | L2 + L3 + **L5** | 数据需迁移，可能有损 |
| 删字段 / 删表 | L2 + L3 + **L5（人工审批）** | 破坏性 |
| **改权限 / 鉴权规则** | L2 + **L4 + L5（人工审批）** | 越权风险；需真实越权用例 |
| 改规则表达式 | L2 + L3 + L4 | 需三端一致性与行为验证 |
| 改图表 / 页面 | L2 + L3 | 语义 + 编译 |

> 现有 L1–L5 模型见 [verification.zh-CN.md](../../verification.zh-CN.md)。本表是它与**变更类型**的映射，是治理流程的落地依据。

## 9. Compatibility

- **Backward**：权限语义扩展（如新增属性条件）为 minor，既有规则语义不变。
- **Forward**：未知策略运算符必须**显式报错**，不得降级为「允许」（fail-closed 是安全默认）。
- **Version Compatibility**：策略语言版本独立于 IR 版本；规则语义变更需 major + 迁移。
- **Migration Compatibility**：权限变更不涉及 schema 迁移（除非新增租户列——那属 schema 变更，需迁移链）。
- **Deprecation Policy**：被弃用的权限构造须经一个 minor 版本的 warning 期；**安全相关的弃用不得静默降级为 allow**。

## 10. Security Considerations

本节即本 RFC 的核心，以下为必须成立的性质：

- **Fail-closed**：策略求值失败（表达式错误、上下文缺失、Provider 不可用）时**拒绝**，不得放行。
- **服务端唯一强制**：所有权限判定在服务端执行；前端是优化，不是边界。
- **不泄露存在性**：越权访问不存在的资源与无权限的资源应返回**相同**响应（404），避免通过响应差异枚举资源。
- **注入防护**：行级条件以**参数化**形式注入查询，禁止字符串拼接（既有生成器已统一使用位置占位符 `?`，见 memory 的 backend runtime contract）。
- **最小权限默认**：策略未覆盖的操作默认为**拒绝**（而非允许）。
- **审计完整性**：审计条目含主体、动作、资源、时间；payload 不得含密钥。
- **多租户隔离**：租户条件必须在**查询层**生效，且不可被客户端参数覆盖。
- **供应链**：生成产物依赖第三方库时，依赖版本须锁定（见 RFC-0007 可复现构建）。
- **敏感数据脱敏**：字段级权限在响应序列化处执行，日志与错误消息同样适用。

## 11. Performance and Reliability

- **判定开销**：RBAC 判定为内存查表，应为常数级；ABAC 表达式求值应可缓存（策略不变时）。
- **查询注入开销**：行级条件增加 WHERE 项，须确保索引可用（否则性能退化——`lint` 已有 `performance/missing-index` 规则可呼应）。
- **审计开销**：审计写库不得阻塞主流程（当前为同事务内 INSERT）。
- **故障模式**：权限 Provider 不可用时 fail-closed（拒绝服务优于放行越权）。
- **可观测性**：权限拒绝应有结构化日志（动作、主体、资源），便于审计与排障。

## 12. Alternatives Considered

- **仅前端隐藏**：拒绝——这是最常见的越权来源，前端可被绕过。
- **权限用独立配置文件表达**：拒绝——权限是业务语义，与实体/操作强相关，分离会产生第二份事实。
- **把权限交给外部策略引擎（OPA/Cedar/OpenFGA）**：不拒绝，但**推迟**——当前先把语义在本仓库定稿并实现服务端权威执行；引擎对接作为 Capability 接入（§8.6），届时策略语义仍是同一份 DSL 声明。
- **权限失败时放行（fail-open）**：拒绝——安全默认必须是拒绝。
- **让 Agent 自动批准权限变更**：拒绝——见 RFC-0006 §8.6，高风险变更必须人工审批。

## 13. Trade-offs

- 显式报错未实现的权限构造，会让写了 `auth` 块的规格暂时无法构建；换取「空承诺」显式化，避免误以为已有权限控制。
- 权限在服务端与前端双端编译增加生成物复杂度；换取前后端语义同源（避免两端规则漂移）。
- 引入外部策略引擎的对接推迟，换取先把语义与执行模型跑通。

## 14. Migration Plan

| 步 | 内容 | 完成判据 |
|---|---|---|
| **C1** | 对 `access` / `auth` 块产出显式诊断（`dsl-unsupported-policy`），杜绝静默忽略 | 写有 `auth` 块的规格在 build 时明确失败并说明原因 |
| **C2** | 实现 `access` / `auth` → 后端权威判定 + 前端体验编译 | 越权请求被服务端拒绝（403/404）；前端按钮相应隐藏 |
| **C3** | 接入 auth 能力，填充审计 actor | 审计条目 actor 不再为 `anonymous` |
| **C4** | 行级 / 字段级权限 + 多租户 | 租户条件在查询层生效；敏感字段在响应裁剪 |
| **C5** | 落地验证门槛映射到 CI | CI 按变更类型自动要求对应验证等级 |

> C1 是**立即可做**且价值高的一步：把安全相关的空承诺变成显式错误，成本极低。C2 依赖 auth 能力（roadmap M5）。

## 15. Implementation Plan

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | C1（显式诊断）+ C5 门槛表 | 无 |
| 阶段 2 | C2（权限编译） | auth 能力（M5） |
| 阶段 3 | C3（审计 actor） | 阶段 2 |
| 阶段 4 | C4（行级 / 字段级 / 多租户） | 阶段 2 |

## 16. Conformance Requirements

- [ ] `access` / `auth` 块在未实现前产出显式诊断（无静默忽略）。
- [ ] 权限判定在服务端强制：越权请求返回 403/404，且不泄露资源存在性。
- [ ] 列表查询的权限条件注入 WHERE（非后置过滤）。
- [ ] 策略求值失败时 fail-closed。
- [ ] 规格与生成产物中不含密钥值；诊断与日志不含密钥。
- [ ] 审计条目含主体、动作、资源、时间。
- [ ] 前后端权限语义同源（同一份规则编译两端）。

## 17. Acceptance Criteria

1. 写有 `auth { delete → role == "admin" }` 的规格在当前版本构建时得到**明确错误**（而非静默忽略），错误消息说明该能力尚在路线图哪个里程碑。
2. 权限能力实现后：非 admin 用户发起删除请求被服务端拒绝（403），普通用户请求不存在与不存在的资源得到**相同**响应。
3. 审计表中被拒绝的删除尝试有记录，含 actor、动作、资源。
4. 全仓扫描规格与生成产物，密钥值出现次数为 0。

## 18. Open Questions

- 行级权限的表达式是否复用同一封闭表达式语言，还是需要独立的谓词类型（避免把行级条件写进资源级规则的表达式里）？
- 多租户的隔离实现应依赖数据库级（每租户 schema）还是行级过滤 + 应用层约束？后者依赖索引正确性。
- 是否需要「策略即测试」——权限规则与规则向量一样，纳入跨语言一致性测试集？
- 字段级权限与「响应裁剪」是否需要区分（前者是禁止见，后者是可见但脱敏）？
- Secret Provider 的统一接口由谁定义（Runtime RFC 还是本 RFC 的扩展点）？

## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览（规则一处声明两端执行）：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 规格系统与领域划分：[RFC-0002-specification-system.zh-CN.md](RFC-0002-specification-system.zh-CN.md)
- Agent 审批门：[RFC-0006-agent-engineering.zh-CN.md](RFC-0006-agent-engineering.zh-CN.md)
- 验证体系 L1–L5：[verification.zh-CN.md](../../verification.zh-CN.md)
- 迁移安全与风险分级：[migration-safety.zh-CN.md](../../migration-safety.zh-CN.md)
- DSL 规则与表达式：[dsl-spec.zh-CN.md §8–§9](../../dsl-spec.zh-CN.md)
- 实现位置：`crates/forma/src/dsl/parser.rs`、`crates/forma/src/emit/rust_backend.rs`、`crates/forma/src/emit/web_react.rs`、`crates/forma/src/lint.rs`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义权限四层模型与执行原则、密钥与审计契约、验证门槛映射；把 `access`/`auth` 空承诺显式化 |
