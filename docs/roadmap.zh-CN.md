# Forma 完整路线图（v0.3 — DSL 中间层）

> 架构见 [architecture.zh-CN.md](architecture.zh-CN.md) · DSL 语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)
> 本文档回答两个问题：**接什么**（适配层完整矩阵）、**按什么顺序接**（里程碑与验收）。

---

## 0. 接入决策原则

一个目标技术是否接入、何时接入，按三个维度打分：

1. **真实业务覆盖面**：CRUD 管理系统 → 中后台 → 多端产品 → 企业级，逐波覆盖。
2. **LLM 生态成熟度**：模型对该栈的训练数据越多，生成的目标代码越可靠（Rust/TS/Python/Java 是第一梯队）。
3. **适配层成本**：IR → 该目标的映射复杂度；与已有 emitter 的复用率。

**当前首批目标（用户定案）**：Rust（axum + sqlx）后端、Web（React + TanStack + shadcn/ui）、MySQL。其余按下方矩阵分波接入。

---

## 1. 里程碑总览

```text
Wave 1 — 端到端闭环（单栈纵深）
M1 DSL 内核 → M2 db-mysql → M3 backend-rust → M4 web-react
                                                │
Wave 2 — 业务必备 + AI 闭环                     ▼
M5 Auth 模块 → M6 语义补丁(AI 闭环) → M7 标准视图 + 工程化产物

Wave 3 — 横向扩栈（同一 IR，多 emitter）
M8 数据库: PostgreSQL / SQLite ｜ 后端: Node(NestJS/Hono) / Python(FastAPI)
M9 前端: Vue 3 生态 ｜ 移动: Expo (React Native) ｜ GraphQL

Wave 4 — 企业级与长尾
M10 Java(Spring Boot) / Go / .NET / PHP ｜ 原生移动(SwiftUI/Compose) ｜ 小程序
M11 数据工程与高阶能力（搜索/队列/多租户/审计完善/数据仓库导出）
```

---

## 2. 适配层完整矩阵

> ✅ 已实现 · 🔨 规划中（里程碑标注）· 👁 观察（有需求再排期）
> 每个目标 = 一个 emitter crate（`forma-gen-*`），实现统一接口契约（见 §5）。

### 2.1 数据库

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **MySQL 8** | M2 | InnoDB/utf8mb4；DDL + 基线迁移 → IR diff 增量迁移 |
| 🔨 P1 | **PostgreSQL** | M8 | jsonb/数组/丰富约束；与 MySQL 共享 90% 映射逻辑 |
| 🔨 P1 | **SQLite** | M8 | 本地/嵌入式场景；复用联邦期解析经验 |
| 👁 P2 | SQL Server / Oracle | M11 | 企业遗留系统 |
| 👁 P2 | MongoDB | M11 | 文档模型与实体映射需专项设计 |
| 🔨 P1 | **Redis**（非持久化目标） | M7 | 会话/缓存/限流的键设计从 DSL 派生 |

### 2.2 后端

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **Rust：axum + sqlx** | M3 | models / validators / routes / repositories |
| 🔨 P1 | **Node：NestJS** | M8 | 企业 TS 后端主流；TypeORM/Prisma 二选一 |
| 🔨 P1 | **Node：Hono** | M8 | 轻量边缘友好；复用联邦期 Hono 生成器经验 |
| 🔨 P1 | **Python：FastAPI** | M8 | SQLAlchemy 2.0 + Pydantic（Pydantic 与规则校验天然同构） |
| 👁 P2 | **Java：Spring Boot** | M10 | JPA/Spring Security；企业市场最大 |
| 👁 P2 | **Go：Gin + GORM** | M10 | |
| 👁 P2 | PHP：Laravel / .NET：ASP.NET Core | M11 | |

### 2.3 API 风格（与后端 emitter 正交）

| 优先级 | 风格 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | REST | M3 | `api` 块 → REST 路由 |
| 🔨 P1 | **OpenAPI 规范导出** | M7 | `openapi.yaml` 生成视图（生态互通） |
| 🔨 P1 | **GraphQL** | M9 | schema + resolver 从实体/api 派生 |
| 👁 P2 | gRPC（Protobuf 导出） | M10 | 服务间通信 |
| 👁 P2 | tRPC | M9 | TS 全栈场景 |

### 2.4 前端 Web

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **React + TanStack Router/Query/Table + shadcn/ui + Tailwind** | M4 | 表单校验 zod（与后端同源） |
| 🔨 P1 | **React Hook Form 适配** | M7 | 复杂表单场景；zod resolver |
| 🔨 P1 | **Vue 3 + Vue Router + Pinia** | M9 | 组件库：Element Plus / Naive UI 二选一 |
| 🔨 P1 | **Next.js** | M9 | React 的 SSR/全栈变体（app router） |
| 👁 P2 | SvelteKit / Angular | M11 | |
| 🔨 P1 | 状态管理生成 | M7 | Zustand（React）/ Pinia（Vue）：从 `page` 数据依赖派生 |
| 🔨 P0 | **i18n 生成** | M4 | DSL 内 message → react-i18next / vue-i18n 资源文件 |
| 👁 P2 | 桌面：**Tauri**（配 Rust 后端天然）/ Electron | M10 | |

### 2.5 移动端

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| 🔨 P1 | **Expo（React Native）** | M9 | 与 React emitter 复用度最高；同一页面原语 |
| 👁 P2 | Flutter（Dart emitter） | M10 | 自绘 UI，组件映射独立 |
| 👁 P2 | 原生：SwiftUI / Jetpack Compose | M11 | 高保真双 emitter |
| 👁 P2 | **微信小程序 / uni-app** | M11 | 国内场景重要；table/form 原语可映射 |

### 2.6 认证与安全（能力模块）

| 优先级 | 能力 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **内置 Auth 模块** | M5 | DSL 声明 `auth module` → 双端生成：User/Role/Session 实体、登录/注册/改密页面、JWT/session 中间件、`access` 规则落地 |
| 🔨 P1 | OAuth2 / OIDC 接入 | M7 | 对接 Keycloak/Auth0/企业微信/飞书 |
| 🔨 P1 | 多租户 | M9 | 实体级 `@tenant` → 行级隔离 + 查询注入 |
| 👁 P2 | SSO / SAML | M11 | |

### 2.7 能力模块（业务常见件，全部双端生成）

| 优先级 | 能力 | 里程碑 | DSL 形态（草案） |
|---|---|---|---|
| ✅ 首批 | 文件上传 | M7 | `field Attachment` + `storage s3\|oss\|local` |
| ✅ 首批 | 导入导出 | M8 | `export Order as csv\|xlsx` → 后端端点 + 前端按钮 |
| 🔨 P1 | 消息通知 | M9 | `notify email\|sms\|webhook` 事件绑定 |
| 🔨 P1 | 全文搜索 | M9 | `searchable [customerEmail, note]` → Meilisearch/ES |
| 🔨 P1 | 定时任务 | M9 | `job { cron "…" { … } }` |
| 🔨 P1 | 实时推送 | M9 | `live OrderList` → WebSocket/SSE + 前端订阅 |
| ✅ 首批 | 审计日志 | M7 | `audit { … }`（已在 DSL 草案） |
| 👁 P2 | 工作流运行时 | M11 | 长事务/补偿（BPMN 经验迁移） |
| 👁 P2 | 支付 | M11 | Stripe/微信支付适配 |

### 2.8 工程化与质量

| 优先级 | 项 | 里程碑 |
|---|---|---|
| ✅ 首批 | 生成代码可编译验证（Rust `cargo check`、TS `tsc`） | M3/M4 |
| ✅ 首批 | CI 模板生成（每个目标工程自带 lint/test/build） | M4 |
| 🔨 P1 | **Docker / docker-compose 产物** | M7 |
| 🔨 P1 | K8s / Helm / Terraform 产物 | M9 |
| 🔨 P1 | OpenTelemetry + 结构化日志注入 | M9 |
| 👁 P2 | e2e 测试生成（Playwright：页面原语 → 选择器稳定） | M10 |
| 👁 P2 | 性能基准与 Token 经济学 Benchmark 报告 | M10 |

---

## 3. 里程碑详情与验收标准

### Wave 1 — 端到端闭环

**M1 DSL 内核**（进行中）
- `forma-syntax`：Lexer + Parser + AST（span 保留）
- `forma-ir`：AST → 语义图 + 类型检查 + 规则表达式检查
- `forma check`：全量诊断（行号 + did-you-mean）
- 验收：`examples/order-management/forma/*.fm` 全量通过；错误诊断样例 20+ 个

**M2 db-mysql 适配层**
- MySQL 8 DDL emitter（类型映射见架构 §3.1）+ 基线迁移
- 验收：生成的 DDL 在 MySQL 8 直接执行成功；第二次 `forma build` 零变更

**M3 backend-rust 适配层**
- 实体 → struct/serde；枚举 → Rust enum；规则 → 校验函数；`api` → axum 路由 + sqlx 仓库
- 验收：`cargo build && cargo test` 通过；CRUD 端到端（curl 可跑）；规则在 API 层生效（400 + i18n 消息）

**M4 web-react 适配层**
- TanStack Router 路由树 + Query hooks + shadcn 页面 + zod
- 验收：`pnpm dev` 跑通列表（过滤/搜索/分页）、创建/编辑表单（前端校验即时反馈）、详情；`tsc` 零错误

### Wave 2 — 业务必备 + AI 闭环

**M5 Auth 模块**：`auth module` DSL → 双端生成（实体 + 中间件 + 登录注册页 + access 规则落地）。验收：登录 → 带 token CRUD → 无权限操作被拒（后端 403 + 前端按钮隐藏）。

**M6 语义补丁（AI 闭环）**：AST 级 patch（add-entity-field / rename / add-rule / add-page…）→ 影响闭包 → 全目标增量重编；MCP 四工具切至 DSL。验收：**「Order 加付款状态」一条补丁，MySQL + Rust + React 三处同步 + 测试绿，全程 AI 不接触目标代码**。

**M7 标准视图 + 工程化**：OpenAPI 导出、Docker 产物、上传/导出/审计模块、RHF 适配、Zustand 生成、Redis 键设计。

### Wave 3 — 横向扩栈

**M8**：PostgreSQL / SQLite / NestJS / Hono / FastAPI / 导入导出。
**M9**：Vue 3 生态 / Next.js / GraphQL / Expo / 多租户 / 通知 / 搜索 / 定时任务 / 实时 / K8s 产物 / OTel。

### Wave 4 — 企业级与长尾

**M10**：Spring Boot / Go / .NET / Tauri / Flutter / 原生移动 / Playwright e2e / Benchmark。
**M11**：SQL Server / Oracle / 小程序 / SSO / 工作流运行时 / 支付 / MongoDB。

---

## 4. 治理

### 4.1 适配层接口契约

每个 emitter crate 实现：

```rust
trait Emitter {
    fn id(&self) -> &'static str;                 // "gen-mysql" / "gen-rust-axum" / "gen-web-react"
    fn version(&self) -> &'static str;            // 产物缓存失效键
    fn emit(&self, ir: &SemanticGraph, cfg: &TargetConfig) -> Result<Vec<GeneratedFile>>;
}
```

约束：确定性输出、`@generated` 头、参与增量缓存、产物内不回读业务语义（一切以 IR 为准）。

### 4.2 DSL 版本演进

- `app.version` + DSL 语法版本双轨；语法变更加 `forma migrate`（AST 级重写，非正则）。
- 弃用周期：标记 → 下一里程碑报 warning → 再下一里程碑移除。
- emitter 按 `dsl-spec` 的次版本兼容：minor 新增构造不破坏旧 emitter（未识别构造显式报错，不静默跳过）。

### 4.3 生成代码升级路径

生成物整体重生成（不手改）；自定义逻辑通过**扩展点**承接：`extension` 块引用手写代码（后端 handler 钩子、前端组件覆写），升级时扩展点文件不动。

### 4.4 明确不做（Non-goals）

- ❌ 运行时低代码引擎（Forma 是编译期方案，产物是普通代码，脱离 Forma 也能跑）
- ❌ 在 DSL 里表达像素级 UI（页面原语之上由目标生态自治）
- ❌ 通用编程语言（算法/复杂逻辑走 extension 手写）
- ❌ 运行时解释执行 DSL（解释器形态仅限规则表达式在特殊场景的候选实现）

---

## 5. 遗留代码处置

- `crates/forma`（v0.2 联邦引擎，17 测试绿）：`graph / patch / engine` 模块被 DSL 编译器复用；SQL/JSON Schema/OpenAPI/JSON Forms/BPMN 适配器 → M7 改造为标准导出视图；Hono 生成器经验并入 Node emitter。
- `docs/history/`：历史决策归档，只读。
