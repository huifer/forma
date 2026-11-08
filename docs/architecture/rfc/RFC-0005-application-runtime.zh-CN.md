---
id: RFC-0005
title: Application Runtime
status: Implemented
version: 1.1
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-08
domain: runtime
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0003]
related_adrs: [ADR-0001]
related_specs: [spec:ir, spec:protocols]
breaking_change: false
---

> **Implementation Status（Implemented，2026-11-08）**：实现落地：统一错误响应体 E1–E5（FORMA_* 稳定错误码 + status_for 单点映射 + 内部错误不回显）+ requestId 中间件 + 结构化日志（LOG_FORMAT=json）。证据：tests/error_contract.rs / tests/structured_logging.rs。

# RFC-0005：应用运行时（Application Runtime）

## 1. Metadata

见 frontmatter。本 RFC 规定 Forma **生成物侧**的运行时契约：运行时模块边界、版本与兼容性、配置分层、错误契约、生命周期与健康检查、以及「生成物脱离 Forma 独立运行」这一根本立场的形式化。

## 2. Abstract

Forma 的产物是**普通代码**——生成的 Rust / React / Flutter 工程脱离 Forma 仍可独立构建、部署、运行。本 RFC 把这一立场形式化为**运行时契约**：

> **Runtime 不是解释器。** Forma 不在生产环境解释执行 `.fm`；Runtime 是「规格语义在目标平台的运行支撑」，其形态是**生成的产物内代码 + 可选的独立运行时库**。

这一区分关键：若 Runtime 退化为「解释 `.fm` 的服务」，则产物不再是独立资产，AI 生成的代码也无法脱离平台运行——与 Forma 的根本定位冲突。

## 3. Motivation

### 3.1 Current Problem

**现状**：生成物里已经存在事实上的运行时支撑代码，但它们**未经契约化**：

| 产物 | 事实上的运行时支撑 | 位置 | 状态 |
|---|---|---|---|
| Backend | 统一错误类型、审计钩子、鉴权骨架 | `src/error.rs`、`src/audit.rs`、`src/auth.rs` | 无版本、无契约 |
| Backend | 路由与仓储（axum + sqlx） | `src/routes.rs`、`src/repo.rs` | 依赖版本未锁定 |
| Web | API 客户端、数据 hooks、UI 组件、i18n | `src/api.ts`、`src/ui.tsx` | 无契约 |
| Web | **可视化运行时**（取数 → 变换 → 整形 → 渲染 → 事件 → 响应式） | `src/viz/*.ts(x)` | 有结构（RFC-0010 §8），无版本契约 |
| Flutter | API 客户端、格式化 | `lib/api/api.dart`、`lib/ui/format.dart` | 无契约 |

**缺口**：

1. **无版本契约**：产物运行时不声明版本，无法判断「这个产物需要哪一版运行时行为」。
2. **无配置分层约定**：Build-time / Environment / Tenant / User 四类配置的边界未定义；密钥注入方式未定义（RFC-0008 S1 引用了 Secret Provider，但未定义接口）。
3. ~~**无错误契约**~~（**已关闭 2026-11-05**：`FORMA_*` 稳定错误码贯穿后端 / Web / Flutter / OpenAPI）。
4. **无生命周期与健康检查约定**：生成的 axum 应用有无 `/health` 端点、优雅停机约定，未定义。

### 3.2 Why Existing Architecture Is Insufficient

[roadmap.zh-CN.md §6.4](../../roadmap.zh-CN.md) 明确「明确不做：运行时解释执行 DSL」「产物是普通代码，脱离 Forma 可运行」——这是**立场**，但没有**契约**。没有契约，实现者各自发明（各 emitter 生成的 `error.rs`/`auth.rs` 互不对齐），Agent 与 CI 也无从判断产物是否合规。

### 3.3 Expected Outcomes

- 运行时支撑代码有明确的模块边界与版本契约。
- 配置分层与密钥注入方式成文。
- 错误契约跨后端 / 前端 / Agent 三方一致。
- 生成物独立运行这一根本立场有可验证的判据。

## 4. Goals

- 定义运行时模块边界（Runtime Modules）与各模块契约。
- 定义运行时版本与兼容性规则。
- 定义配置分层与 Secret Provider 契约。
- 定义错误契约。
- 定义生命周期与健康检查约定。

## 5. Non-goals

- 不定义解释器或规格执行引擎（**明确排除**）。
- 不定义部署编排与容器镜像（属 Deployment 能力）。
- 不定义可观测性栈选型（仅定义契约接口）。
- 不定义具体运行时库的实现细节（属各 adapter）。

## 6. Terminology

- **Runtime Module**：产物内承担特定运行支撑职责的代码单元。
- **Runtime Contract**：该模块对外承诺的行为（接口、错误、版本、配置）。
- **Runtime Version**：运行时契约的版本，与 IR 版本、编译器版本独立。
- **Configuration Layer**：配置的作用域分层（构建期 / 环境 / 租户 / 用户）。
- **Secret Provider**：密钥供给接口，运行时按需拉取。

## 7. Architectural Context

### 7.1 System Context

```text
   规格 (.fm) ──Compiler──▶ 生成产物（独立可运行）
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
   Rust Backend            React Web              Flutter App
   ┌──────────┐            ┌──────────┐           ┌──────────┐
   │ error    │            │ api      │           │ api      │
   │ auth     │  Runtime   │ hooks    │  Runtime  │ format   │
   │ audit    │  Modules   │ ui       │  Modules  │ ...      │
   │ routes   │            │ viz      │           │          │
   │ repo     │            │ i18n     │           │          │
   └──────────┘            └──────────┘           └──────────┘
        │                       │                       │
        └───────────────────────┴───────────────────────┘
                                │
                     独立的部署产物（不依赖 Forma）
```

### 7.2 Dependencies

- 依赖 RFC-0003（IR 语义）。
- 被 RFC-0008（安全与密钥）引用。

### 7.3 Boundaries

- **Runtime 不得解释 `.fm`**。运行时**不知道**规格的存在——它只看到编译后的普通代码与配置。
- **Runtime 不得依赖 Compiler 内部实现**（RFC-0001 §7.3）。
- Runtime 模块**只消费** IR 编译出的语义，不重新解释业务语义。
- 运行时**不含**业务逻辑——业务逻辑在生成的 handler / component 中。

## 8. Design

### 8.1 Conceptual Model

**Runtime Module 分类**（与产物形态对应）：

| 类别 | 职责 | 当前形态 |
|---|---|---|
| **Data Runtime** | 连接、查询、事务、错误映射 | `repo.rs`（sqlx）+ `error.rs` |
| **Auth Runtime** | 身份解析、权限判定 | `auth.rs`（骨架，权限语义待 RFC-0008 C2） |
| **Audit Runtime** | 审计写入与结构化日志 | `audit.rs`（`audit_logs` 表 + tracing） |
| **API Runtime** | 类型化客户端、请求/响应处理 | `api.ts` / `api.dart` |
| **UI Runtime** | 基础组件、格式化、i18n | `ui.tsx`、`ui.css`、`format.dart` |
| **Visualization Runtime** | 取数 → 变换 → 整形 → 渲染 → 事件 → 响应式 | `src/viz/`（RFC-0010 §8） |
| **Observability Runtime** | 日志、指标、追踪接口 | 🔨 待定义 |

> 所有 Runtime Module 都是**产物的一部分**，随产物一同分发、独立版本化、独立升级。

### 8.2 Architecture

**Runtime Contract**（每个模块必须声明）：

```yaml
id: backend.error
version: 1.0.0
responsibilities: [error_mapping, http_status_mapping]
depends_on: [backend.auth]
config: []
errors:
  - code: FORMA_VALIDATION_FAILED
    http_status: 400
    shape: { error: String, fields: [{ field: String, message: String }] }
  - code: FORMA_POLICY_DENIED
    http_status: 403
    shape: { error: String }
compatibility:
  ir: ">=1.0.0 <2.0.0"
  runtime: ">=1.0.0 <2.0.0"
```

**配置分层**：

| 层 | 作用域 | 何时解析 | 例子 |
|---|---|---|---|
| **Build-time** | 构建期固化 | 编译时 | 目标框架、UI 库、数据层、路由方案 |
| **Environment** | 部署环境 | 启动时 | API 基址、超时、日志级别、连接串 |
| **Tenant** | 租户 | 请求时 | 租户标识、租户级开关 |
| **User** | 用户 | 请求/会话 | 主题、语言、偏好 |

契约：

- Build-time 配置进入产物（可编译期内联）；其余**不得**编译进产物。
- Environment / Tenant / User 配置**只能经运行时接口**注入，产物内不得出现硬编码值。
- **Secret 只能经 Secret Provider 注入**，规格与产物中只出现引用（RFC-0008 S1/S2）。

**Secret Provider 契约**：

```rust
pub trait SecretProvider {
    /// 按引用名解析密钥。引用名来自规格中的 secretRef。
    async fn resolve(&self, ref_name: &str) -> Result<SecretValue>;
}
```

- 至少提供 `env`（环境变量）与 `file`（挂载文件）两种实现；云 KMS 作为扩展。
- 解析失败 → **fail-closed**（RFC-0008 §10），并**不得**在错误消息或日志中回显密钥值。
- 密钥**不缓存到产物**，不写入清单（RFC-0007）。

**错误契约**（跨后端 / 前端 / Agent 一致）：

```json
{
  "error": "Validation failed",
  "code": "FORMA_VALIDATION_FAILED",
  "fields": [{ "field": "email", "message": "邮箱必填" }],
  "requestId": "..."
}
```

| 规则 | 说明 |
|---|---|
| E1 稳定错误码 | `code` 是稳定 API，Agent 可据此分支处理 |
| E2 i18n 消息 | `error` / `fields[].message` 为面向用户文案（来自规格 message） |
| E3 HTTP 状态映射 | 错误码 → 状态码由 Runtime Contract 声明，不由各处硬编码 |
| E4 结构一致 | 所有后端错误同形；前端 API 客户端按同形解析 |
| E5 不泄露内部细节 | 不返回堆栈、内部路径、SQL |

> **已落地（2026-11-05）**：`error` / `code`（`FORMA_*` 稳定错误码）/ `fields` 同形贯穿
> 后端（`error.rs`）→ Web（`api.ts`）→ Flutter（`ApiException.code`）→ OpenAPI（`Error` schema + 统一错误响应）；
> E5 已实现（内部错误只写日志，不回显 SQL）；`requestId` 为预留字段。
> 行级条件 / 资源级 / 字段级授权见 [spec:protocols](../specifications/protocols/api-ui-protocols.zh-CN.md) §7。

**生命周期与健康检查**：

| 约定 | 内容 |
|---|---|
| L1 启动顺序 | 配置加载 → 日志初始化 → 连接建立 → 开始接受请求 |
| L2 健康检查 | 生成的应用暴露 `/health`（存活）与 `/ready`（就绪，含依赖连通性） |
| L3 优雅停机 | 收到终止信号后停止接受新请求，完成在途请求后退出 |
| L4 超时与重试 | 外部调用有超时；重试策略显式（幂等才重试） |
| L5 幂等 | 命令类操作支持幂等键（RFC-0006 补丁与生产调用） |

**版本与兼容性**：

| 版本 | 管理对象 | 变更触发 |
|---|---|---|
| **Runtime Version** | 运行时契约 | Runtime Module 行为变更 |
| **IR Version** | 语义内核 | 见 RFC-0003 §9 |
| **Compiler Version** | 编译器 | 编译行为变更 |

- 产物在运行时**声明**它需要的 Runtime 版本范围（`compatibility.runtime`）。
- Runtime 版本 major 变更需提供迁移（与 IR 迁移同构：显式路径 + 验证）。

### 8.3 Data Model

Runtime Module 清单随产物分发（可置于产物目录的 `runtime.manifest.yaml`），内容：

- module id / version / 职责
- 依赖的 module 及其版本范围
- 声明的配置键与来源层
- 声明的错误码与状态码映射
- 兼容性范围（ir / runtime）

### 8.4 Interfaces and Contracts

- **产物 → Runtime Module**：模块间调用走显式接口，不共享可变全局状态。
- **产物 → 配置层**：按层读取，接口一致（`get` / `get_tenant` / `get_user`）。
- **产物 → Secret Provider**：异步解析，按引用名。
- **Runtime Module ↔ 目标生态**：适配层负责把契约映射到具体库（axum / TanStack Query / Dio）。

### 8.5 Execution Flow

```text
启动：
  读取 Environment 配置 → 初始化日志 → 建立连接 → /ready 就绪 → 开始服务
请求：
  认证 → 授权（RFC-0008）→ 路由 → handler
       → 数据运行时（查询/事务）
       → 错误映射（统一错误契约）
       → 响应（字段裁剪）
停止：
  收到信号 → 停止接受新请求 → 等待在途请求 → 关闭连接 → 退出
```

### 8.6 Extension Points

- **新 Runtime Module**：作为独立 Capability 接入（RFC-0004），声明契约与版本。
- **新配置源**：实现配置接口；不得把配置编译进产物。
- **新 Secret Provider**：实现 `SecretProvider` trait。
- **新错误类别**：由 Runtime Contract 声明（错误码 → 状态码）。

### 8.7 生成物的独立性判据（可验证）

「产物脱离 Forma 独立运行」必须可验证：

| 判据 | 验证方式 |
|---|---|
| 产物不含规格解析器 | 生成物中无 `.fm` 解析相关代码 |
| 产物不含编译器依赖 | 生成物的 `Cargo.toml` / `package.json` 不依赖 `forma` crate / 包 |
| 产物可独立构建 | 在**无 Forma 环境**下 `cargo build` / `npm run build` 通过 |
| 产物可独立部署 | 只依赖数据库等外部服务，不依赖 Forma 服务 |
| 产物无规格回读 | 运行时不读取 `.fm`（除非用户显式做配置读取） |

> 当前**已满足**大部分判据：生成物是独立工程；`src/viz/` 等运行时支撑均内嵌在产物内。**待修**的是判据 1 的显式化——目前无自动检查确保产物不含规格解析依赖。

## 9. Compatibility

- **Backward**：Runtime Module 新增可选能力为 minor；新增错误码为 minor。
- **Forward**：产物声明的 Runtime 版本范围**必须**被遵守；超出范围时运行时应显式告警而非静默降级。
- **Version Compatibility**：Runtime major 变更须提供迁移路径（显式 + 验证），与 IR 迁移同构。
- **Migration Compatibility**：Runtime 迁移与数据库迁移**独立**——schema 迁移不要求 Runtime 同步升级，反之亦然；二者通过契约版本对齐。
- **Deprecation Policy**：Runtime Module 废弃须经一个 minor 版本的 warning 期，并提供替代模块。

## 10. Security Considerations

- **配置与密钥分离**：Build-time 配置可公开；Environment 配置可能含敏感项，须区分「配置」与「密钥」——密钥只经 Secret Provider。
- **密钥不落盘**：产物、日志、审计、清单均不得含密钥值（RFC-0008 S3/S4）。
- **错误不泄露**：统一错误契约的 E5 规则是**安全边界**——不得返回堆栈、SQL、内部路径。
- **健康检查不泄露**：`/health` 与 `/ready` 只返回状态，不返回版本、连接串、依赖地址。
- **运行时最小权限**：Runtime Module 不自行提权；权限判定只在授权点（RFC-0008）。
- **供应链**：产物依赖的运行时库版本须锁定（RFC-0007 可复现构建）。
- **fail-closed**：配置解析失败、密钥解析失败、健康检查依赖不可用 → 拒绝服务，不放行。

## 11. Performance and Reliability

- **启动开销**：配置与连接初始化须有超时；健康检查反映就绪状态。
- **热路径开销**：Runtime Module 处于请求热路径（如错误映射、字段裁剪），须零分配或低分配；错误构造走 `&'static str` 而非运行时格式化。
- **连接池**：数据运行时须有明确的池大小与超时配置，且随 Environment 配置注入。
- **超时与重试**：外部调用超时是**默认行为**而非可选项；重试仅对幂等操作。
- **优雅停机**：在途请求超时须短于平台强制终止宽限期。
- **可观测性**：Runtime Module 的关键路径（认证、授权、查询、审计）须留结构化日志，字段名稳定（供日志查询与 Agent 消费）。

## 12. Alternatives Considered

- **把 `.fm` 解释器作为 Runtime 主体**：拒绝——产物不再是独立资产，AI 生成的代码无法脱离平台运行，且与「目标代码是生成物」的根本立场冲突（[architecture.zh-CN.md §1](../../architecture.zh-CN.md)）。
- **提供统一的 Runtime 框架 crate/包，生成物依赖它**：不拒绝，但须**可选**——生成物默认自带最小运行时（当前实现如此），用户可选择替换为共享库。保持产物自包含是默认。
- **运行时统一解释配置（不生成配置代码）**：拒绝——配置应可被静态检查与类型化，避免运行期才发现配置错误。
- **错误消息直接用规格原文，不做 i18n 结构**：拒绝——前端与 Agent 需要结构化错误（字段级错误定位），纯文本无法消费。
- **健康检查端点由用户自行添加**：拒绝——健康检查是部署契约的一部分，产物应默认提供。

## 13. Trade-offs

- 契约化运行时（错误码、配置层、版本）增加 emitter 的生成复杂度，换取产物可审计、Agent 可消费、CI 可校验。
- 默认自带运行时（而非依赖共享库）增大产物体积，换取产物自包含、脱离 Forma 可独立构建。
- 统一错误契约约束各 emitter 的自由实现，换取前后端与 Agent 的一致性。

## 14. Migration Plan

| 步 | 内容 | 完成判据 |
|---|---|---|
| **T1** | 统一错误契约：后端错误码 + i18n + 字段级错误；前端客户端按同形解析 | 后端 `src/error.rs` 与前端 `api.ts` 的错误结构一致，有测试覆盖 |
| **T2** | 配置分层落地：Build-time 内联；Environment/Tenant/User 走运行时接口 | 产物中无硬编码的环境相关值 |
| **T3** | Secret Provider 契约 + `env` / `file` 两种实现 | 规格含 secretRef 的工程可构建，密钥运行时注入且不入产物 |
| **T4** | Runtime Module 清单（`runtime.manifest.yaml`）随产物分发 | 产物声明 module 版本、配置键、错误码、兼容范围 |
| **T5** | 生命周期约定：`/health`、`/ready`、优雅停机 | 生成的应用具备健康检查端点 |
| **T6** | 独立性判据自动化检查 | CI 断言产物不含规格解析依赖、构建不依赖 forma 包 |

> T1 优先级最高（错误契约是 Agent 消费的接口）。T2–T5 可随各 emitter 逐步落地。T6 是对根本立场的自动化守护。

## 15. Implementation Plan

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | T1（错误契约） | RFC-0008 C2（权限错误码需先定） |
| 阶段 2 | T2 + T3（配置分层 + Secret Provider） | 无 |
| 阶段 3 | T4 + T5（清单 + 生命周期） | 阶段 1、2 |
| 阶段 4 | T6（独立性检查） | RFC-0007（产物清单） |

## 16. Conformance Requirements

> 核实日期 2026-11-08（Phase 1 收口）。

- [x] 后端与前端的错误结构一致（错误码 / i18n 消息 / 字段级错误），且有契约测试。
  证据：`tests/error_contract.rs`（E1–E5 / Web / Flutter / OpenAPI 同形）。
- [x] 错误码为稳定 API，跨版本不变（新增为 minor）。
  证据：生成的 `error.rs::error_code` 模块 + `status_for` 单点映射；`tests/error_contract.rs`。
  边界：「跨版本不变」由 spec:compatibility 的弃用窗口约束，暂无版本化快照断言。
- [x] 产物中无硬编码的环境相关配置。
  证据：`DATABASE_URL` / `PORT` / `JWT_SECRET` / `REST_BASE` 均经环境变量读取；
  `tests/runtime_probes.rs::generated_artifacts_do_not_hardcode_secret_values`。
- [ ] 密钥仅经 Secret Provider 注入，产物 / 日志 / 审计 / 清单中密钥出现次数为 0。
  ⚠️ 部分：已保证「不硬编码密钥值」（上条 + `tests/runtime_probes.rs`），但**独立 Secret Provider
  抽象未建模**（见 §18 开放问题）。已登记 gap-backlog，不假绿。
- [x] 产物声明 Runtime 版本范围与兼容矩阵。
  证据：`BuildReport.ir_version`（CLI 打印 / MCP `irVersion`）+ Capability `compatibility.ir/runtime`
  构建期校验（`tests/capability.rs::incompatible_ir_version_is_rejected`）。
- [x] 生成的应用提供 `/health` 与 `/ready`（2026-11-08 Phase 1 补齐）。
  证据：`tests/runtime_probes.rs`；两个 router 分支共用 `health_routes`，`/ready` 真实探测数据库，
  不可达返回 503；CI `generated-backend` 对两个生成工程 `cargo check`。
- [x] 产物可在无 Forma 环境下独立构建（CI 断言）。
  证据：生成工程自带 `Cargo.toml` / `package.json`；CI `generated-backend` job 与 `web` job
  在生成目录内独立执行 `cargo check` / npm build。
- [x] 配置 / 密钥解析失败时 fail-closed（2026-11-08，G-5 收口）。
  证据：`DATABASE_URL` 缺失 / 为空、连接失败均输出稳定错误码（`FORMA_CONFIG_MISSING` /
  `FORMA_DB_UNREACHABLE`）+ 非零退出，不再 panic；细节只写日志（E5）。
  回归 `tests/runtime_probes.rs::config_failures_are_structured_fail_closed_not_panics`。

## 17. Acceptance Criteria

1. 后端返回字段级校验错误时，前端能按 `fields[].field` 精确定位到输入框（当前 `ApiException.fields` 已具备基础，需契约化与测试）。
2. 规格中引用密钥的工程：产物中只出现引用名；密钥由环境注入；产物与日志中密钥出现次数为 0。
3. 在移除 Forma 的干净环境中，生成物 `cargo build` / `npm run build` 通过。
4. 生成的应用 `/ready` 在数据库不可达时返回非 2xx，可用编排系统的就绪探针正确摘除流量。

## 18. Open Questions

> 收口（2026-11-08）：2 项关闭、1 项本阶段落地、2 项保留。

- [x] ~~Runtime Module 清单的载体：随产物分发还是独立包索引？~~
  **已决议**：随产物分发——本机清单 `.forma/artifacts.json`（含来源规格）；生成工程不额外携带
  `runtime.manifest.yaml`（避免双源）。
- [ ] 配置接口是否需要跨语言同构（Rust / TS / Dart 三份 trait 定义）？
  ⏸ **保留**：当前各目标独立读取环境变量，语义一致由 spec:protocols 契约文档保证；
  trait 化属增强，待多目标配置项增多再评估。
- [x] ~~是否需要「运行时行为向量」测试集（错误映射、配置解析、字段裁剪）？~~
  **已落地（2026-11-08）**：权限行为向量从 DSL 规则派生，Web（vitest）与 Flutter（flutter_test）
  跨端同源执行（`validation_vectors::access_vectors` + `canForRole`）；错误码映射向量见
  `tests/error_contract.rs`（E1–E5 三端）。
- [ ] Secret Provider 的 `resolve` 是否需要缓存层？
  ⏸ **保留**（跟随 G-4：Secret Provider 抽象未建模，缓存无从谈起）。
- [x] ~~多租户配置（Tenant 层）在单租户部署中的默认值如何定义？~~
  **已决议（2026-11-08）**：单租户部署（无 `@tenant`）→ `tenant_arg = None`，repo **不过滤**；
  `@tenant` 工程 → 服务端绑定当前租户，`None` → `col = NULL` 恒不命中（空集，fail-closed）；
  tenant 列保持 nullable（兼容存量行）。与 `tests/tenant_admin_ui.rs` / `access_relation_depth.rs` 一致。


## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览（运行时分离立场）：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 语义内核：[RFC-0003-semantic-core.zh-CN.md](RFC-0003-semantic-core.zh-CN.md)
- 安全与密钥（Secret 规则）：[RFC-0008-security-and-governance.zh-CN.md](RFC-0008-security-and-governance.zh-CN.md)
- 产物治理（可复现构建）：[RFC-0007-build-and-artifact.zh-CN.md](RFC-0007-build-and-artifact.zh-CN.md)
- 可视化运行时范例：[RFC-0010-visualization.zh-CN.md](RFC-0010-visualization.zh-CN.md)
- 架构蓝图（明确不做清单）：[architecture.zh-CN.md](../../architecture.zh-CN.md)
- 实现位置：`crates/forma/src/emit/rust_backend.rs`、`crates/forma/src/emit/web_react.rs`、`crates/forma/src/emit/flutter.rs`、`crates/forma/src/emit/visualization.rs`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义 Runtime Module 契约、配置分层、Secret Provider、统一错误契约、生命周期约定与产物独立性判据 |
| 1.1 | 2026-11-08 | Phase 1 §16 验收核实：逐条对照回归测试与实现证据并勾选；未覆盖项保留 `[ ]` 并登记 [gap-backlog](../roadmap/gap-backlog.zh-CN.md)「Phase 1 §16 未覆盖项」；配套补齐生成后端 `/health` `/ready`（RFC-0005）与产物原子写 / 内容跳过（RFC-0007） |
