# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **状态管理生成 · Zustand（M9 / P1）**：
  - manifest opt-in：`targets.frontend.usesZustand: true`（缺省 = 局部 useState，零变化）；
  - `src/store.ts`：**从列表页数据依赖派生**——每个列表页一个 store（filters 键取自
    api.list filters / search），setter 与受控模板调用签名兼容（setFilters 收 updater、
    setSearch 直传），列表页仅替换状态声明行，queryKey / 输入绑定零变化；
  - package.json 按需引入 zustand；无列表页或未启用时零产物；
  - 回归 `tests/web_zustand.rs`（2 项：派生 store + 缺省零变化）；
  - 生成端验证：`npx tsc --noEmit` 0 错 + `npm run build`（vite）通过。
  - 注：Pinia（Vue 侧）留待 Vue 列表页引入过滤/搜索依赖面后跟进（当前 Vue 列表页
    无 filters/search 状态可派生，不造无源之 store）。

### Added
- **React Hook Form 适配（M9 / P1）**：
  - manifest opt-in：`targets.frontend.usesRhf: true`（缺省 = 受控 state 表单，产物零变化）；
  - 表单页改用 `useForm` + `register`（文本）/ `useController`（Select 关系与枚举）/ `setValue`
    （Attachment 上传回填）；`handleSubmit` 内仍调用 validators.ts 的 validateCreate/Update
    并手动 `setError`——**校验单一来源不变，不引入第二套 zod schema**；
  - package.json 按需引入 `react-hook-form`（无 opt-in 零依赖）；
  - 回归 `tests/web_rhf.rs`（2 项：opt-in 生成 RHF 页面 + 缺省零变化）；
  - 生成端验证：`npx tsc --noEmit` 0 错 + `npm run build`（vite）通过。

### Added
- **导入导出（M9 / P1）**：
  - DSL `export <Api> as csv|xlsx`（引用已声明 api；检查层 `dsl-export-unknown-api` /
    `dsl-export-no-list` / `dsl-export-unknown-format` + merge 期 (api,format) 去重）；
  - Rust 后端：`src/export.rs`（EXPORT_TARGETS 静态展开，列名取自 plan_tables 与 DDL 一致）
    + `GET /api/{plural}/export.csv` 全量 `SELECT *` 流式 CSV（try_get 类型降级解码 +
    RFC4180 转义）+ `export.xlsx` 显式 501（FORMA_EXPORT_DRIVER，外部驱动集成点）；
  - OpenAPI：`/api/{plural}/export.csv`（text/csv）+ `.xlsx`（501）；Web 列表页「导出 CSV」按钮；
  - 无 export 声明的工程全链路零产物；
  - 验证：回归 `tests/export.rs`（3 项）+ 生成端真实 `cargo check`（0 错误）+
    `npx tsc --noEmit`（0 错误）。

### Added
- **文件上传（M9 / P1）**：
  - 内建字段类型 `Attachment`（存储文件 URL 引用；MySQL→VARCHAR(500)、Rust/TS/Dart→String），
    四目标映射齐备（`BUILTIN_TYPES` 11→12，一致性回归更新）；
  - 字段属性 `@storage(provider, bucket?)`（provider 白名单 local/s3/oss），检查层 3 条诊断：
    `dsl-storage-non-attachment` / `dsl-storage-unknown-provider` / `dsl-storage-missing-bucket`（s3/oss 必填 bucket）；
  - Rust 后端：`src/storage.rs`（@storage 静态展开 STORAGE_FIELDS 表 + `POST /api/uploads`
    multipart 上传 + `GET /api/uploads/:name` 下载，含路径穿越防护）；local 真实落盘
    `./uploads/<uuid>.<ext>`，s3/oss 显式 501（`FORMA_STORAGE_DRIVER`，外部驱动集成点，
    不静默降级）；存在 Attachment 时 Cargo.toml 启用 axum `multipart` feature；
  - OpenAPI：`/api/uploads`（POST multipart）+ `/api/uploads/{name}`（GET binary）；
  - Web：`uploadFile(file, field?)` 助手（FormData + Bearer）+ 表单文件输入（选文件即上传、
    字段值存返回 URL、回显可点击链接）；
  - 无 Attachment 的工程全链路零产物（storage.rs / multipart / uploadFile 均不生成）；
  - 验证：回归 `tests/storage.rs`（3 项）+ 生成端真实 `cargo check`（0 错误）+
    `npx tsc --noEmit`（0 错误）。

### Added
- **Redis 键设计派生（M9 / P1）**：
  - 新增 `targets.db.engine: redis` → `db-redis` 构建步骤，从 DSL 实体/索引/唯一约束/auth/jobs/live **纯函数派生** Redis 键设计；
  - 产出 `redis/key-design.md`（人读）+ `redis/key-design.json`（机读），键名统一带 `forma:{project}:{version}` 边界前缀；
  - 覆盖：单实体/列表缓存（Hash+TTL）、索引（ZSET）、唯一约束（SET 存在性反查）、会话（STRING+TTL=tokenHours）、任务锁、SSE 订阅集合、限流计数器；
  - Redis 非关系存储——不做 DDL/迁移，只做键设计派生（零新增依赖、确定性输出）；
  - 默认 mysql 与 postgres/sqlite 工程**零 Redis 产物**（回归锁定）。
  - 回归 `tests/redis.rs`（2 项：键设计派生正确 + 默认/关系型 engine 零产物）。

- **OpenAPI 3.x 导入 → Forma DSL（M7 / P1）**：
  - 新增 `forma import openapi <spec.yaml> [--out <path>]`，将 OpenAPI 规范逆向工程为 `.fm` 骨架；
  - 实体来自 `components.schemas`，API 来自 `paths`（操作按 HTTP method 推导 list/get/create/update/delete，无路径的实体默认全量 CRUD）；
  - 类型映射：string(email/uuid/date-time/date/maxLength) / integer→Int / number→Decimal（含 @min/@max）/ boolean→Bool / `$ref`→关系；
  - 保证每个生成实体有 `@id` 主键（名为 id 的字段自动作为主键，Uuid/String 补 `@default(uuid)`），满足关系目标 PK 约束；
  - 数组 / 裸 object 在 v1 保守降级（跳过或 Text），生成物保证通过 `forma check`，需人工校对业务规则；
  - 回归 `tests/openapi_import.rs`（2 项：类型映射正确 + 生成 `.fm` 通过 `check_program`）。

- **结构化日志基础（M10/P1）**：
  - 生成后端统一初始化 `tracing-subscriber`（features: `env-filter` + `json`）；
    默认 human-readable，`LOG_FORMAT=json` 输出 JSON Lines。
  - `RUST_LOG` 控制 log filter（默认 info）。
  - `tracing` 始终存在（错误 / 审计 / 请求追踪统一走 `tracing::` 宏）——不再由 has_commands 条件决定。
  - **余项**：OTLP exporter 接入（外部 collector 依赖，按需立项）。
  - 回归 `tests/structured_logging.rs`（2 项）。

- **FastAPI backend target（M10 第三后端扩栈）**：
  - `targets.backend.language: python` + `framework: fastapi` 路由到 FastAPI + SQLAlchemy 2.0 +
    Pydantic 2 + PyMySQL 后端；
  - 生成 `src/database.py`（engine/SessionLocal/Base/get_db）、`src/models.py`
    （`Mapped`/`mapped_column` 声明式模型）、`src/schemas.py`（`Create*`/`Read*` DTO，
    min/max → `ge`/`le`）、`src/routes.py`（CRUD router）、`src/main.py`、
    `pyproject.toml`、`README.md`；
  - auth/access/tenant/commands/cross-field rules 显式拒绝（不静默降级安全语义）；
  - 生成产物通过 `python -m compileall` 语法校验；Rust/NestJS/Hono 主线零变化。
  - 回归 `tests/fastapi.rs`（2 项：CRUD 生成结构 + 不支持安全语义显式拒绝）。

- **Hono backend target（M10 第二后端扩栈）**：
  - `targets.backend.language: typescript` + `framework: hono` 路由到 Hono 4 + @hono/node-server +
    mysql2 + zod 轻量后端；
  - 生成 `src/index.ts`（Hono app + CRUD routes + pagination）、`src/types.ts`、`src/validators.ts`、
    `package.json`、`tsconfig.json`、`README.md`；
  - 支持 list/get/create/update/delete、offset 分页、字段 required/min/max/Decimal 基础校验；
  - auth/access/tenant/commands/cross-field rules 显式拒绝（安全语义不静默降级）；
  - Rust/axum/NestJS 主线产物零变化；`npm run build` + tsc 通过。
  - 回归 `tests/hono.rs`（2 项：CRUD 生成 + 不支持安全语义显式拒绝）。

- **NestJS backend target（M10 第一后端扩栈）**：
  - `targets.backend.language: typescript` + `framework: nestjs` 路由到 NestJS 10 + TypeORM 0.3 +
    MySQL + class-validator CRUD 后端；
  - 每实体生成 `*.entity.ts`（TypeORM `@Entity` + `@PrimaryColumn` + `@Column`），
    `*.dto.ts`（class-validator `@IsString` / `@IsOptional` / `@Min` / `@Max`），
    `*.service.ts` / `*.controller.ts` / `*.module.ts`，
    `app.module.ts`（TypeOrmModule.forRoot synchronize: false）、`main.ts`（globalPrefix `api` +
    ValidationPipe）；
  - **显式拒绝不支持的 DSL 特性**（不静默降级安全语义）：auth module、命令操作、
    access/row/field policies、跨字段 rules、tenant isolation；
  - Rust/axum 主线产物零变化（build 路由按 `backend.language` 分发）。
  - 验证：`npm run build` + `npx tsc --noEmit` 通过（NestJS 10 + TypeScript 5.5）。
  - 回归 `tests/nestjs.rs`（2 项：CRUD 生成结构 + 不支持语义显式拒绝）。

- **Vue 3 前端 target（M10 前端扩栈）**：
  - `targets.frontend.framework: vue`（或 `vue3`）路由到 Vue 3 + Vite + TypeScript + Vue Router +
    Pinia + Element Plus 前端；
  - 生成 `package.json` / `vite.config.ts` / `tsconfig.json` / `src/env.d.ts` / `src/App.vue` /
    `index.html` / `src/main.ts` / `src/api.ts` / `src/types.ts` / `src/validators.ts` /
    `src/format.ts` / `src/access.ts`（按需）/ `src/router.ts` / `src/pages/*.vue` / `README.md`；
  - **复用 web_react 的同一套契约**：`types_ts` / `validators_ts` / `format_ts` / `access` 直接复用，
    保证与 React 端类型与校验同源；
  - `src/api.ts`：每实体生成 `{Entity}Api`（list/get/create/update/remove），并注入 `AuthApi`
    （auth module 存在时）与命令操作 `{Cmd}Api.execute`；
  - `src/access.ts`：**Vue 原生**角色感知——用 `@tanstack/vue-query` 的 `useQuery` 实现 `useCurrentRole` /
    `useCan`（React 版用 react-query hook，不可直接复用），`canSeeField` 为纯函数；
  - **默认（无 framework 或 `react`）仍生成 React 前端**，产物零变化（build 路由按 `frontend.framework` 分发）。
  - 验证：生成项目 `vue-tsc --noEmit && vite build` 通过（含 auth + 命令 + 访问规则组合工程）。
  - 回归 `tests/vue.rs`（3 项：脚手架/路由 + 默认 React 不变 + auth/命令/Vue 原生 access）。

  - **Flutter 端订阅（补全）**：`lib/live_api.dart`（Dart SSE 订阅器——dart:io HttpClient 流式读取
    text/event-stream + 自动重连）+ 列表页 `LiveSubscriber? _live` + `_subscribeLive()`（initState 订阅
    → 事件到达 invalidate → dispose stop）；无 live → 零 live_api.dart / 零订阅代码 / 零变化；
    flutter analyze 通过（含 live + 无 live 两种工程）；school 工程（无 live）零变化；
  - **后端 + Flutter 端到端**：live.rs SSE 端点 ↔ Flutter LiveSubscriber，publish 事件到达即时刷新；
- **实时推送（live → SSE）能力（M10/P1 能力模块）**：
  - DSL：`live <name> { api <Api>.list; events [create update delete] }`——每个 live 块
    是一条 SSE 通道；事件 = 目标实体的 create/update/delete；
  - 检查层：`dsl-live-unknown-api`（api 必须已声明）、`dsl-live-unknown-event`
    （事件白名单 create/update/delete）；
  - Rust 后端 `src/live.rs`：`GET /api/live/{name}` SSE 端点（axum SSE + tokio broadcast，
    按表广播；keep-alive 默认）+ `publish(table, event)`；CRUD handler 成功后自动广播
    （按实体是否挂 live 通道条件生成——无通道零调用）；
  - 依赖：tokio-stream（sync feature）——live 声明存在时才加入 Cargo.toml（零新增依赖
    宪法的最小例外：SSE 流适配必需）；
  - 无 live 声明 → 零 live.rs 产物、零 publish 调用（产物零变化）；
  - 验证：生成后端 `cargo check` 零错误（含 live+auth+@tenant 组合工程）；
    回归 `tests/live.rs`（2 项：SSE 生成/publish 钩子/零产物 + 检查诊断）。

- **多租户（@tenant 行级隔离 + 管理）能力（M10 收尾）**：
  - DSL `@tenant` 字段检查已有（dsl-tenant-multiple/optional/type/no-auth）；本次补齐 Rust 后端执行；
  - **服务端租户隔离（ADR-0004）**：JWT claim 含 tenant → CurrentUser.tenant →
    list/get/update/delete 查询按租户过滤（`WHERE {col} = ?`；None → NULL 恒不命中→空集，
    无绕过路径）；create 时租户由服务端绑定（覆盖输入体）；
  - **租户管理**：`POST /api/admin/tenants/:tenant/revoke`（吊销：停用租户全部用户 +
    login 拒绝 disabled）+ `POST /api/admin/tenants/:tenant/transfer`（迁移：业务行改绑新租户）；
    授权走 TENANT_ADMIN_TOKEN env + x-admin-token 头；未设置 env → 403 关闭（默认安全）；
  - users 表增加 disabled 列（tenant_enabled 时）；无 @tenant → 全部产物零变化；
  - **顺带修复**：git checkout 意外丢失了之前 session 的多个未提交修复（request_id.rs 生成、
    字段级策略裁剪 trim/field_policy、结构化日志 tracing 恒存在、按需 imports P2-4、
    validate_create/Update 的 models import、has_notify_rules handler 钩子、expr_to_rust_mode
    的 too_many_arguments allow）——全部已重新实现并通过测试验证。

- **全文搜索（searchable）能力（M10/P1 能力模块）**：
  - DSL：实体级 `searchable [title, body]` → MySQL `FULLTEXT KEY ft_{table}_{cols}
    (cols) WITH PARSER ngram`（MySQL 8 内置 ngram 分词器，中文 bigram 可用）；
  - 检查层：`dsl-searchable-unknown-field`（字段必须存在）、`dsl-searchable-field-type`
    （仅 String/Text/Email——FULLTEXT 只支持 CHAR/VARCHAR/TEXT 家族）；
  - 迁移链：`add_fulltext`（Low）/`drop_fulltext`（Medium）独立风险分级；重放模拟器
    支持 `ADD FULLTEXT INDEX ... WITH PARSER ngram`；基线/迁移/回滚全链可重放验证；
  - 应用层 q 查询保持 LIKE 子串语义（中文友好），FULLTEXT 索引为优化/外部工具支撑——
    零行为变化；无 searchable 声明零 FULLTEXT 产物；
  - **顺带修复两个既有 bug**：① `TablePlan.fulltext` 新字段缺 `#[serde(default)]` 导致旧
    ir-snapshot 反序列化失败 → 静默回退迁移链重放 → 产生 users 唯一约束 DROP+ADD 同名伪迁移
    （school 每次构建累积一个 000N 垃圾迁移）；② 新克隆场景（快照缺失）从迁移链重建快照后
    未落盘 → 每次构建重复重放且 `save_snapshot` 分支永不执行——现已持久化；
  - 验证：迁移链重放验证（`migration_verify`）通过；回归 `tests/searchable.rs`
    （4 项：基线 FULLTEXT / 升级链增量+重放 / 检查诊断 / 零产物）。

- **消息通知（@notify）能力（M10/P1 能力模块）**：
  - DSL：字段属性 `@notify(channel, target, event)`——email/sms/webhook/log 四渠道；
    **属性参数必须括号形式**（裸参数不被属性解析捕获——踩过并已入记忆）；
  - 检查层：`dsl-notify-unknown-channel`（渠道白名单）、`dsl-notify-missing-target`
    （email/sms/webhook 必须有 target）、`dsl-notify-unknown-event`
    （event 必须引用已声明的 command 名）；
  - Rust 后端：`src/notify.rs`——`NotifyRule` 表由 DSL 静态展开（运行时零 DSL 依赖），
    `fire(entity, event, id)` 按 entity+event 匹配规则派发；command handler 成功后自动触发；
  - v1 驱动：`log` 渠道真实输出（tracing 结构化字段）；email/sms/webhook 为占位输出
    （集成点已预留——零新增依赖宪法）；
  - 无 @notify 声明 → 零 notify.rs 产物、handler 零 fire 调用（产物零变化）；
  - 验证：生成后端 `cargo check` 零错误；回归 `tests/notifications.rs`
    （2 项：模块生成/handler 钩子/零产物 + 三类检查诊断）。

- **定时任务（scheduled jobs）能力（M10/P1 能力模块）**：
  - DSL：`job <name> { cron "0 2 * * *" label { zh ".." en ".." } op ItemApprove }`——
    调度器到点调用目标 command 的效果（服务端权威语义）；
  - **检查层**（零降级）：`dsl-job-cron-format`（cron 必须 5 字段，值/范围/列表校验）、
    `dsl-job-unknown-op`（op 必须引用已声明的 api 或 command）、
    `dsl-job-input-effect`（目标 command 的 effect 不得引用 input.*——调度器无输入来源）、
    `dsl-job-pre-unsupported`（pre 必须是单字段比较——复杂 pre 无法安全转 WHERE，
    拒绝而非静默全表 UPDATE）；
  - **MySQL/迁移**：`forma_jobs` 内部表（id/name/cron/label/op/status/last_run_at/next_run_at，
    name 唯一 + status 索引）；由 `plan_tables_with_internal` 自动并入基线与迁移链；
  - **Seed**：确定性 job 行（SHA-256 派生主键，INSERT IGNORE 幂等）；
  - **Rust 后端**：`src/jobs.rs`——进程内 tokio 调度器（30s tick，cron 求值零新增依赖）、
    `CommandTarget` 表由 DSL 静态展开（effect/pre → UPDATE...WHERE，运行时零 DSL 依赖）、
    管理端点 `GET /api/jobs` + `POST /api/jobs/:id/trigger|enable|disable`（挂受保护路由组，
    有 auth 时需登录）；`main.rs` 启动时 spawn；
  - **IR bridge**：`NodeKind::Job` 节点（`forma://project/{p}/job/{name}`）+
    job→command `references` 边（API 语义变更 → job 受影响）；
  - 验证：生成后端 `cargo check` 零错误零警告；回归 `tests/jobs.rs`（5 项：解析/建表/seed/
    调度器接线/检查诊断/IR 桥接/零 job 零产物）。

- **Next.js（App Router / SSR 变体）target（M10 前端扩栈）**：
  - `targets.frontend.framework: nextjs`（或 `next`）路由到 Next.js 14 App Router 工程
    （`frontend-nextjs` 构建步骤；默认 react 产物零变化）；
  - **复用 web_react 共享 TS 层**：`types_ts` / `validators_ts` / `api_ts` / `format_ts` 直接复用
    （validators 的 `FieldError` 从客户端 `src/api.ts` 导入——`/api` 经 next rewrites 代理）；
  - **SSR 形态**：列表页 = Server Component（`src/api-server.ts` 绝对地址 + `API_BASE` env +
    `cache: "no-store"` + `force-dynamic`；list 返回 `{Entity}ListRow[]`，关系列取 `{rel}_name`）；
  - **变更 = Server Actions**：`app/actions.ts`（`"use server"` + `revalidatePath`）——每实体
    get/create/update/delete + 空 input 命令（`{cmd_snake}(id)`）；列表页行内命令/删除按钮
    用 inline server action form；
  - 表单页 = client 组件，校验复用 `validateCreate{E}` / `validateUpdate{E}`（与 Rust 后端同源）；
  - **按 operations 裁剪**：VoteRecord 只有 [list get create] → 只生成列表页 + new 页，无 [id] 编辑页；
  - `next.config.mjs` rewrites `/api/:path*` → `API_BASE`；自带 GitHub Actions CI
    （tsc + next build；通用 web-engineering 的 vitest 模板对 Next 不适用，跳过）；
  - **v1 边界**：图表（viz runtime 需 echarts 依赖 + client 化，暂跳过 viz-web）、auth UI、
    带 input 命令对话框、关系 select 组件——均为后续增强；
  - 验证：`tsc --noEmit` + `next build` 双门通过；school 旗舰示例端到端（7 实体 21 路由全绿）。
  - 回归 `tests/nextjs.rs`（2 项：App Router 结构 + 默认 react 不变）。

- **GraphQL target（M10 API 风格扩栈）**：
  - `targets.api.framework: graphql` 路由到 schema + resolver 网关（`api-graphql` 构建步骤；
    无 `targets.api` 零产物）；
  - **schema.graphql**：DSL → SDL 契约——枚举、实体类型、正向对象关系（`owner: User` +
    `ownerId: ID`）、Create/Update Input、Query（list/get，按各 api operations 裁剪）、
    Mutation（create/update/delete）；Decimal→String（与后端序列化契约同形）；JSON 自定义标量；
  - **src/schema.ts**：typeDefs 与 schema.graphql 同源（运行时内嵌，免 .graphql loader）；
  - **src/rest.ts**：服务端 REST 客户端（`REST_BASE` 环境变量指向任意 Forma 后端 target——
    Rust/axum、NestJS、Hono、FastAPI 皆可前置；`GRAPHQL_TOKEN` 透传 bearer）；
  - **src/resolvers.ts**：Query/Mutation 委托 REST；对象关系按 `{rel}_id` 拉取（GraphQL 惯用形态）；
  - **src/server.ts**：graphql-yoga 可运行网关（默认 :4000/graphql）；安全语义（auth/access/tenant）
    由 REST 后端权威执行，网关只透传；
  - **余项（v2）**：逆向关系（`User.items`）需过滤器依赖与 N+1 处理（DataLoader）；
  - 验证：`graphql.parse()`/`buildSchema()` 接受 SDL、生成项目 `tsc --noEmit` 通过、
    school 旗舰示例端到端（25 定义 + 全部 Query/Mutation 字段 + tsc OK + server 启动冒烟）。
  - 回归 `tests/graphql.rs`（2 项：schema/resolver 结构 + 无 api target 零产物）。

- **Expo（React Native）target（M10 移动端扩栈）**：
  - `targets.mobile.framework: expo` 路由到 Expo + React Native 0.74 + TypeScript + @tanstack/react-query
    移动端（默认无 framework 仍生成 Flutter，产物零变化）；
  - **复用 web_react 的同一套 TypeScript 共享层**：`types_ts` / `validators_ts` / `api_ts` /
    `format_ts` / `access_ts` 直接复用——react-query 与 fetch 在 React Native 可用，
    保证与 Web 端类型/校验/API 客户端同源（`api_ts` 与 `display_field_of` 改 `pub`）；
  - 生成 `package.json` / `app.json` / `tsconfig.json`（extends expo/tsconfig.base）/ `main.tsx`
    （registerRootComponent）/ `App.tsx`（QueryClientProvider + 轻量手写栈导航 +
    `ROUTES`/`TITLES` 路由表）/ `src/pages/*.tsx` / README.md；
  - React Native 侧用 `View`/`Text`/`TextInput`/`Pressable`/`FlatList` 渲染（替代 Web DOM 原语），
    列表页内嵌编辑视图（`editing` 状态切换，避免跨页传参）；auth 存在时生成 `Login.tsx` +
    `setAuthToken` 接线；
  - 验证：生成项目 `npx tsc --noEmit` 通过（Expo SDK 50 + RN 0.74 + @types/react）。
  - 回归 `tests/expo.rs`（2 项：RN 工程结构 + 默认 Flutter 不变）。

- **M9 工程化：生成项目 Makefile + GitHub Actions CI**：
  - 后端 `Makefile`：build / check / test / run / migrate / seed / docker 全生命周期目标；
    `DATABASE_URL` / `PORT` 环境变量可覆盖。
  - 后端 `.github/workflows/ci.yml`：cargo check + test + build（dtolnay/rust-toolchain + rust-cache）。
  - Web `.github/workflows/ci.yml`：npm install + tsc --noEmit + vitest run + npm run build。
  - 回归 `tests/engineering.rs`（4 项：Makefile 生命周期目标 / 后端 CI / Web CI / 落盘验证）。

- **SQLite target（M10 横向扩栈）**：
  - `targets.db.engine: sqlite` / `sqlite3` 选择独立 SQLite 方言 emitter；
  - 新增 `db-sqlite` + `sqlite-migrate` 构建步骤，生成 SQLite 3 DDL（`PRAGMA foreign_keys = ON` +
    `CREATE TABLE IF NOT EXISTS` + 列级 PRIMARY KEY + UNIQUE + FOREIGN KEY + `CREATE INDEX IF NOT EXISTS`）；
  - 类型映射：`Uuid → TEXT`、`String/Email/Text → TEXT`、`Int → INTEGER`、`Float → REAL`、
    `Decimal → NUMERIC`、`Bool → INTEGER`、`Timestamp/Date → TEXT`、`Json → TEXT`——不使用 MySQL/PostgreSQL 方言；
  - 与 MySQL/PostgreSQL 共享 DSL / 表拓扑 / 约束语义（同一 `plan_tables`），但不复用方言字符串；
    默认 engine 仍为 mysql，旧工程产物零变化。
  - 回归 `tests/sqlite.rs`（3 项：类型映射、独立方言 DDL、MySQL 默认路由零变化）。

- **双语 OG 图**：英文 `README.md` 引用 `docs/og-image.png`，中文 `README.zh-CN.md` 引用
  `docs/og-image.zh-CN.png`。两张图均为 1600×900，使用同构的 HTML/CSS 版式；源文件分别在
  `docs/assets/og-image.en.html` 与 `docs/assets/og-image.zh-CN.html`，可用 Chrome headless 截图重建。

- **PostgreSQL target（M10 横向扩栈第一项）**：
  - `targets.db.engine: postgres` / `postgresql` 选择独立 PostgreSQL 方言 emitter；
  - 新增 `db-postgres` + `postgres-migrate` 构建步骤，生成 PostgreSQL 15+ DDL / upgrade migration；
  - 类型映射：`Uuid → UUID`、`Json → JSONB`、`Bool → BOOLEAN`、`Timestamp → TIMESTAMPTZ`、
    `Decimal → NUMERIC(p,s)`、`Float → DOUBLE PRECISION`；约束使用 PostgreSQL 原生语法；
  - 与 MySQL 共享 DSL / 表拓扑 / 约束语义，但不复用 MySQL 方言字符串；默认 engine 仍为 mysql，
    旧工程产物零变化。
  - 回归 `tests/postgres.rs`（4 项：类型映射、方言 DDL、build steps、默认 MySQL 零变化）。

- **Docker / docker-compose 产物（M9 / P1 完成）**：
  - 后端 target 配置时自动生成 `generated/backend/Dockerfile`（Rust 多阶段 build →
    debian:bookworm-slim 运行时）、`generated/backend/entrypoint.sh`（等待 MySQL → 建库 →
    按序执行 init/upgrade 迁移 → 可选 seed → 启动 server）、`.dockerignore`。
  - 工程根生成 `docker-compose.yml`：MySQL 8 + healthcheck、backend 服务、数据卷、
    有 Web 页面时附 Web dev server；Docker build context 为项目根，路径契约已锁定。
  - `DATABASE_URL` / `PORT` / `DB_HOST` / `DB_USER` / `DB_PASS` / `DB_NAME` 均可通过环境变量覆盖；
    与既有默认 8080 / 10.0.2.2 Flutter API_BASE 契约对齐。
  - 回归 `tests/docker.rs`（2 项：产物结构/路径契约 + 空工程零 Docker 产物）。

- **Generated-project benchmark（F-008 完成，`forma bench` 子命令）**：
  - **10×10 确定性任务矩阵**：10 个标准业务变更场景（加字段 / 必填字段 / Decimal 字段 /
    枚举字段 / api filter / api search / 索引 / 页面 / 规则 / chart 数据源）× 10 个确定性
    变体 = **100 cases**。
  - 每个 case：DSL 检查（`check_program`）→ 全量产物生成（后端 / DDL / 迁移 / 种子）→
    逐 case 诊断收集；每场景 variant=0 额外执行生成 backend 的 `cargo check`
    （10 个真实依赖编译代表样本，避免 benchmark 本身变成 100 次依赖安装）。
  - 输出 JSON 报告（schema `forma-benchmark/v1`，camelCase）：`checksPassed` / `buildsPassed` /
    `backendCompilesPassed` / `reliability` / `estimatedInputTokens`（**明确标注 4 chars/token
    proxy**，非 LLM tokenizer 实测）/ 逐 case 耗时与诊断。
  - CLI：`forma bench [dir] [--json]`；报告输出供 CI / 回归对照消费。
  - 基准（本机实测）：**100/100 checks、100/100 builds、10/10 backend cargo check、
    可靠性 100%、estimated tokens ≈ 7,060**。
  - 回归 `tests/cursor_pagination.rs` 新增 2 项（矩阵结构 / 代表 case 构建）。

- **DSL 补丁操作集扩展（RFC-0006 §16 / §17）**：新增三种事务式操作：
  - `add-entity`：新增实体声明，字段属性白名单（`@id` / `@default(uuid|now)` / `@required` /
    `@readonly`），越界属性显式拒绝；
  - `add-field-rule`：新增 `min` / `max` / `required` 字段约束，消息使用 DSL message block
   （`{ zh "…" }`），与 check/generator 同源；
  - `add-form-field`：向页面 form layout 追加控件（`Input` / email / money / datetime / Textarea），
    控件白名单校验，按块内现有缩进插入。
  - 三者复用同一事务、审批门、`patchId` 幂等、`expected` 冲突检测与回滚路径；
    回归 `tests/dsl_patch.rs`（20 项）。

- **字段级策略的前端显隐（体验层，P0-2 余项收口）**：
  - **Web**：生成 `FIELD_RULES`（实体 → 字段 → 允许角色）+ `canSeeField(entity, field, role)`；
    列表页按角色隐藏列（列头用展开表达式保持 `head` 为字符串数组）、表单页隐藏输入框。
  - **Flutter**：生成同构 `fieldRules` + `canSeeField(entity, field, role)`；列表页按角色隐藏行文本、
    表单页隐藏输入框、详情页隐藏行。
  - **角色未知（未登录/加载中）时不隐藏**——宁可让服务端裁剪，也不误伤（ADR-0004）。
  - **零变化原则**：无字段策略的工程不生成规则表、不引入引用（access.ts / access.dart 都不生成）。
  - 单一事实来源：服务端裁剪与前端显隐共用 `dsl::field_policy_roles`（消除三端口径漂移）。
  - 回归 `tests/field_policies.rs` 新增 2 项（两端显隐结构 / 无策略零变化）；
    端到端 Web `tsc` 与 Flutter `analyze` 均通过。

- **`requestId` 请求级追踪（Runtime Contract §4.4 / spec:protocols §7 缺口 8 关闭）**：
  - 生成 `src/request_id.rs` 中间件：优先透传调用方的 `x-request-id`，否则生成 UUID v4；
    经 `tokio::task_local` 暴露给错误构造（`IntoResponse` 拿不到请求扩展），
    并回写 `x-request-id` 响应头（日志/支持可对账）。挂载在最外层——覆盖 auth 层与全部错误响应。
  - 错误体 `requestId` 不再是占位 `None`：从 task-local 作用域取值。
  - Web（`err.requestId`）与 Flutter（`ApiException.requestId`，命名参数向后兼容，
    回退读响应头）同形读取；OpenAPI 的 `requestId` 描述去掉「预留」。
  - 回归 `tests/error_contract.rs` 新增 3 项（中间件结构 / 客户端同形 / Flutter 断言）；
    生成后端 `cargo check` 通过（school 参考应用零 warning）。

- **Flutter 端游标分页（spec:protocols §3.4.1 缺口 3 关闭）**：
  - 生成 `CursorPage<T>` 信封（`{items, nextCursor}`）与 `ListParams.cursor`；
    API 类的 `list` 返回信封（新增 `Api.getRaw` 取原始 JSON）。
  - 游标模式的列表页改为「首屏 + 加载更多」：按 `nextCursor` 续取并**追加**，
    游标模式 **MUST NOT** 生成 offset 偏移参数。
  - 移除构建期「Flutter + 游标 = 拒绝」的硬失败（能力已具备）。
  - **余项**：可视化运行时仍要求裸列表响应——chart 消费 cursor api 仍在构建期显式拒绝。
  - 回归：`tests/cursor_pagination.rs` 的 `cursor_api_refuses_flutter_target_before_build_writes`
    替换为 `cursor_api_supports_flutter_target`（信封/参数/页面结构断言）；
    端到端 `flutter analyze` + `flutter build web` 通过。

- **字段级策略（P0-2 / [ADR-0004](docs/architecture/adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md) 四层收口）**：
  - DSL 语法：`access Entity { fields { field → role in [...] } }`——仅所列角色可见。
  - **服务端权威执行（输出裁剪）**：生成的后端在 list/get 响应序列化处按当前角色移除不可见字段
    （`trim_<entity>_fields`：字段 → 允许角色表；同字段多条策略取角色并集）；
    list/get 强制注入 `Extension<CurrentUser>`（无操作级规则也需要角色）。
  - Check 层 5 条诊断：`dsl-field-policy-unknown-field` / `-relation-unsupported`（v1 仅标量字段）/
    `-no-provider` / `-unknown-role` / `-expression-unsupported`（限 `any` 或角色表达式）。
  - 生成的 `error.rs` 新增 `From<serde_json::Error>` 与 `AppError::Internal(String)`
    （裁剪路径的序列化失败 → 内部错误，细节只写日志，E5）。
  - **余项**：字段级的前端体验层显隐（列表列/表单字段）；多租户（RFC-0008 C4）。
  - 回归 `tests/field_policies.rs`（5 项：合法通过 / 5 类诊断 / 生成物结构 / 零策略零变化 / MCP check）。

- **规则一致性向量从 DSL 派生（F-004 完成）**：
  - 新增 `validation_vectors.rs`——从规则约束（min / max / required）**派生**跨端行为用例
    （低于下界 → 拒绝、下界 → 通过、超过上界 → 拒绝、上界 → 通过、必填缺省 → 拒绝），
    数值按类型渲染（Int 整数字面量、Float 数值、Decimal 按 scale 的字符串——与 TS/Dart
    输入侧的字符串语义对齐）。
  - 生成的 Web 工程新增 `validators.test.ts`（**vitest 测试运行器补齐**——package.json 增
    vitest devDep + `test` 脚本）与生成的 Flutter 工程新增 `rule_vector_test.dart`
    （替代手写版，同路径同覆盖）；两个文件由**同一派生函数**产出（同源保证）。
  - CI 新增 `npx vitest run`（school web）；`flutter test` 持续执行 Dart 校验器。
  - 语义：单一事实来源是 **DSL 本身**——规则改了，向量即改，三端测试同步变红。
  - 回归：`tests/rule_vectors.rs` 新增 `generated_behavior_tests_are_same_source`
    （两端用例数一致 + 边界字面量同时出现）。

- **产物清单与人工修改检测（F-002 / [ADR-0003](docs/architecture/adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md) 实现落地）**：
  - 新增 `.forma/artifacts.json` 产物清单：逐文件记录 sha256、归属（Generated / Adopted）、生成器。
  - **人工修改不被静默覆盖**：构建前快照受保护产物（清单哈希 ≠ 磁盘内容 = 人工改过；Adopted 条目
    无条件保护），构建后恢复人工版本并产出 `artifact-modified` 警告（默认 warning，保留人工版本）。
  - 裁决旗标（`forma build` / MCP）：`--adopt`（登记 Adopted——人工版本胜出且**永不覆盖**）、
    `--overwrite`（生成版本胜出 + `.forma/audit.jsonl` 审计）、`--strict`（冲突提升为 error，阻断 CI）。
  - `write_text` 升级为**原子写**（临时文件 + rename，ADR-0003 §2 原子写要求）。
  - 两条构建管线（DSL `run_build` 与联邦 `build_project`）共用 `artifacts.rs` 一套实现。
  - 回归 `tests/artifact_ownership.rs`（6 项：清单登记 / 人工修改保留 / strict / adopt 永不覆盖 /
    overwrite 审计 / 规格演进正常覆盖）。

- **风险审批门 + 审计落库（[RFC-0006](docs/architecture/rfc/RFC-0006-agent-engineering.zh-CN.md) P5 落地）**：
  - **审批门**：`forma_patch` 在写入之前先在**临时副本**上试运行并评估风险——High/Extreme 迁移
    （`modify_column` / `drop_column` / `drop_table`）或**权限规则变更**（`add-access-rule`，§8.7）
    在补丁文档未携带 `approval: { by, reason }` 时被**拒绝**（`patch-approval-required` 诊断，
    零写入、零产物触碰，审计记 rejected）。风险评估共享权威实现
    （`dsl::patch::assess_risk`）——`forma_plan` 的 verdict 与审批门走同一条路径，不允许漂移。
  - **审计落库**：每次补丁尝试（applied / rejected / failed）追加一行 JSON 到
    `.forma/audit.jsonl`——操作集、风险、审批来源（by/reason）、改动文件、重建规模；
    `forma_report` 附最近 5 条审计尾迹。RFC3339 时间戳零依赖手写（审计是运行时状态，
    不参与产物确定性约束，spec:ir §11 R3）。
  - 修复：CLI `forma patch` 的失败判定纳入 `rejected`（此前 rejected 会退出 0）。
  - **P5 追加：幂等 + 冲突检测**：补丁文档支持 `patchId`（同一 ID 重复提交返回
    `idempotent: true` / `already-applied`，`.forma/patches.json` 台账，零二次变更）与
    `expected: [{ entity, sha256 }]`（声明块哈希乐观并发检测，过期返回 `patch-conflict`，零写入）；
    `forma_plan` 回传当前实体声明块哈希，Agent 可原样带回补丁。
  - 回归：`tests/dsl_patch.rs`（18 项：P2 + P5 门 + 幂等 + 冲突）与
    `tests/agent_tools.rs`（11 项：plan verdict / expected 回传 / 幂等+冲突组合）。
  - **baseRevision 绑定**：补丁文档支持 `baseRevision: "<project>@<IR_VERSION>"`（也接受裸 IR 版本）；
    应用前与当前 `Program.ir_version` 比对，不匹配返回 `patch-base-revision`、零写入；
    `forma_plan` 回传当前 revision。回归新增 stale revision 拒绝 + 正常绑定应用。
  - **`set-field-rule`**：修改已有 min / max 值（message block 原样保留）；
    无既有约束时显式拒绝（`use add-field-rule first`）；`tests/dsl_patch.rs` 22 项。

- **Agent 会话闭环（[RFC-0006](docs/architecture/rfc/RFC-0006-agent-engineering.zh-CN.md) P4 落地，F-007 完成）**：
  新增 `crates/forma/src/agent.rs` 与四个 MCP 工具——一次 Agent 会话可完成 §8.5 全流程，**无需读取目标源码**：
  - **`forma_search`**：语义模型检索（实体 / 字段 / 枚举 / API 操作 / 页面 / 图表 / 表列），返回 URI、
    类型与来源位置；空结果附可检索类型提示。发现阶段的入口。
  - **`forma_get_context`**：目标上下文收敛——声明块摘录（`decl_block_line_span`，Agent 不读源码也能
    看到实体定义）、定义产物（字段 / 表列）、依赖与引用者（`references` 双向）、影响闭包。
  - **`forma_plan`**：变更计划（**试运行，零副作用**）——在临时副本上执行补丁事务（不重建产物），
    返回编辑清单、迁移操作与风险分级（`pending_migration_ops` + `MigrationRiskReport`）、
    新增节点与受影响实体闭包，并给出 **verdict**：`agent-safe`（全 Low）/ `requires-approval`
    （High/Extreme）/ `blocked`（操作失败）。应用前必调；High+ 必须**停**下等人工审批（§8.5 第 7 步）。
  - **`forma_report`**：变更与状态汇报——模型摘要、校验证据（check / lint / 迁移链重放）、
    **待执行迁移及其风险**（捕获「改了 .fm 但还没 build」的状态）、各 target 产物面。
  - MCP 工具描述同步修正：`forma_impact` / `forma_patch` 的描述不再声称「仅限联邦工程」。
  - 回归测试 `tests/agent_tools.rs`（8 项，含 **§8.5 全流程**：发现 → 收敛 → 计划 → 应用 → 汇报）。

- **DSL 主线补丁事务（[RFC-0006](docs/architecture/rfc/RFC-0006-agent-engineering.zh-CN.md) P2 落地）**：
  Agent 与人现在可以用**一个补丁文档**修改 DSL 规格——`dsl::patch` 提供四个主线操作：
  `add-entity-field`（加字段，`required` 映射 `@required` / 可空 `?`）、
  `set-entity-field-required`（切换必填）、`add-access-rule`（加规则——优先并入既有
  `access {Entity}` 块，否则追加 rules 块）、`add-table-page`（加页面——绑定某 api 的 list）。
  - **事务形状与联邦侧一致**：快照被改文件 → 应用编辑 → `check_program` 重校验 → 重建产物；
    **新引入的语义错误即整体回滚**（源码字节级复原，失败的补丁不触碰产物）；
    计划阶段失败（未知实体 / 非法操作名）按操作报告 failed 并继续。
  - 定位基于 M1 的声明级 `file`/`line`（RFC-0006 的关键路径：没有稳定定位就没有可寻址补丁）。
  - `forma patch`（CLI）与 `forma_patch`（MCP）按工程类型分发补丁文档——DSL 工程用主线操作集，
    联邦工程用规格导入操作集；报告形状同形（operations / edits / diagnostics / build）。
  - 同一事务内多个操作基于**当前磁盘状态**组合应用（踩坑修复：曾按原始快照重复应用导致互相覆盖）。
  - 回归测试 `tests/dsl_patch.rs`（11 项，含 P2 验收「单文档加字段 + 加规则 + 加页面」）。
- **桥接图并入表/列节点（RFC-0006 P3 收尾）**：`dsl::lower` 复用 gen-mysql 的权威表计划
  （`plan_tables`），为每个表/列产出 `Table` / `TableColumn` 节点与 `defines` / `belongs-to` 边——
  影响闭包现在覆盖「改实体 ⇒ 表列受影响」。表列 URI 走 `identity::local_uri`（联邦 SQL 侧同构）。
- **影响闭包含齐五类目标（RFC-0006 P3 完成）**：`NodeKind` 新增 `chart` / `migration`
  （spec:ir §3.1 可视化域 `ChartNode` + 数据域 `MigrationNode`）：
  - **图表目标**：`chart` 节点（`forma://project/{p}/chart/{Name}`）+ 三类边——chart → api 操作
    （数据契约变更 → 图表受影响）、chart → 编码通道字段（字段变更 → 图表受影响；聚合输出键
    如 `value` 不伪造实体字段边）、页面 → chart（chart 页引用）。
  - **迁移目标**：`migration-chain` 节点 + 表 → 迁移链 `generates` 边（改表 ⇒ 需要新的
    `000N_upgrade.sql`；`generates` 向前传播——用 `references` 不会进闭包，踩过）。
  - `IR_VERSION` → `1.2.0`（节点结构变更 ⇒ minor 递增，spec:ir §14 记录 1.2.0）。
  - 回归：`chart_and_migration_nodes_are_in_the_graph` +
    `impact_closure_covers_all_five_target_kinds`（改实体 ⇒ 表列/模型/表单/图表/迁移全部入闭包）。

- **IR 契约桥接 M1–M3 落地（[spec:ir §15](docs/architecture/specifications/ir/ir-contract.zh-CN.md)，gap-backlog 跨切面段全部关闭）**：
  - **M1 诊断携带精确 file/line**：声明级来源位置（`EntityDecl` / `EnumDecl` / `RulesDecl` /
    `ApiDecl` / `PageDecl` / `CommandDecl` 的 `line` + `file`；`line` 解析期确定、`file` 合并期回填）；
    `check_program` 经 `Origin` 统一注入位置——**全部** `dsl-*` 诊断可定位到 `file:line`
    （此前大量诊断只有 `file: None`）。
  - **M2 IR 版本与类型单一权威**：`IR_VERSION = "1.0.0"` + `Program.ir_version`（CLI `validate` /
    `graph` 与 MCP 输出可见）；`BUILTIN_TYPES` 作为唯一权威清单，新增**逐类型**四目标映射一致性
    回归（MySQL 列类型逐项断言，杜绝「检查层支持、生成层不支持」的空承诺）。
  - **M3 `Program → 联邦语义图` 单向桥接**：新增 `crates/forma/src/dsl/lower.rs`——
    DSL 实体 / 字段 / 枚举 / API 操作 / 命令 / 页面 / 表单控件投影为 `UnifiedSemanticGraph` 节点，
    边覆盖 `defines` / `references` / `belongs-to`；节点 URI 用联邦侧**同一套** URI 构造函数
    （`forma://project/{p}/entity/{E}` 等）→ **同构**可验证；**不改动** `Program` 既有字段语义，
    因此**不改动任何 emitter 契约**。
  - **效果**：DSL 工程的 `forma graph` 输出稳定标识 + 依赖边 + 来源位置并支持 `--export`；
    MCP `forma_impact` 对 DSL 工程**不再被拒**（与联邦工程共用同一套图 API，影响闭包覆盖
    实体 → 字段 → API 操作 → 页面）。
  - 回归测试 `tests/dsl_ir_bridge.rs`（8 项）；spec:ir 关闭 G-1 / G-2 / G-3 / G-4 / G-7。
  - **余项**：`forma patch` 对 DSL 工程仍限联邦工程（需声明级可寻址定位 + 快照/回滚，列入后续）。

- **P2 缺口批次关闭（[gap-backlog](docs/architecture/roadmap/gap-backlog.zh-CN.md) P2 段全部关闭）**：
  - **P2-6 移动端角色感知（与 Web 同构）**：gen-flutter 生成 `lib/access.dart`
    （`accessRules` + `can(entity, op)`），登录/注册写入 `Api.currentRole`；列表页
    「新建/编辑/删除」与编辑页「保存」按角色显隐，**角色未知（未登录/加载中）不隐藏**——
    服务端仍是判定方（ADR-0004）；api 级与实体级规则取角色交集，与 Web `access.ts` 语义一致；
    无授权规则的工程**不生成** `access.dart`、产物零变化。回归测试 `tests/flutter_access.rs`（4 项）；
    端到端验证 `flutter analyze` 0 issue（school 参考应用）。
  - **P2-4 生成后端导入全按需**：routes / repo / validators 的导入按实际操作生成
    （提取器逐符号判定：list→Query、get/update/delete/命令→Path、handler→State、授权→Extension）；
    只有实体、无 api/命令的工程 **0 warning**。回归测试 `backend_contract.rs` 新增 3 项
    （entity-only / list-only / auth-module-only）。
  - **P2-1 / P2-2 / P2-3 决议**：`pre` 保持「目标实体当前行」的可判定表达式子集
    （跨实体查询依赖 spec:ir §15 的 DSL→图 lowering，M3 稳定标识，编译期显式报错不静默）；
    弃用→移除窗口 = **≥ 2 个次版本（minor）且 ≥ 6 个月**（取更晚者），移除须 `breaking_change: true`
    + ADR + 迁移计划；`apiVersion` v1 内仅向后兼容演进、新版本分命名空间并存（≥ 1 个编译器次版本）、
    权限粒度 v1 保持 **capability 级**（操作级由 DSL `api { auth }` / `rules { access }` 承担，
    manifest 不重复建模）。决议分别写入 spec:protocols §4.5/§7、spec:compatibility §10/§11、
    spec:manifests §5、spec:types §7。
- **架构治理体系完整化：基础 RFC 全部补齐（RFC-0002…RFC-0009）+ 4 条 ADR + `spec:ir`** — 按
  [RFC-0001](docs/architecture/rfc/RFC-0001-architecture-overview.zh-CN.md) 与
  [ADR-0001](docs/architecture/adr/accepted/ADR-0001-adopt-rfc-governance.zh-CN.md) 建立的
  五层治理体系（RFC / Specification / ADR / Capability / Conformance）落地，每份文档均按
  `rfc/TEMPLATE.md` 的 20 节模板撰写：
  - **[RFC-0002 规格系统](docs/architecture/rfc/RFC-0002-specification-system.zh-CN.md)** — 规格领域划分
    （数据/规则/API/UI/可视化/规划中的查询·指标·工作流·集成）、8 条语法元规则、Specification/Project/
    IR/Migration 四类版本关系、标准互通的**保真度声明**（Lossless/Lossy/Partial/Unsupported）与往返测试契约。
    补齐设计宪法 G6「外部标准导入必须记录语义损失」这一**当前完全缺失**的条款。
  - **[RFC-0003 语义内核](docs/architecture/rfc/RFC-0003-semantic-core.zh-CN.md)** — 统一语义 IR 契约：
    Stable Identity、类型系统、来源追踪、依赖边、IR 版本；给出**四步渐进迁移路径**，不破坏既有 emitter 契约。
  - **[RFC-0005 应用运行时](docs/architecture/rfc/RFC-0005-application-runtime.zh-CN.md)** — Runtime Module 契约、
    配置四层（构建期/环境/租户/用户）、Secret Provider、统一错误契约、生命周期约定，以及
    「产物脱离 Forma 独立运行」的**可验证判据**。
  - **[RFC-0006 Agent 工程协议](docs/architecture/rfc/RFC-0006-agent-engineering.zh-CN.md)** — 任务型工具面
    （取代命令镜像式暴露）、补丁事务协议（预览→校验→影响→风险→审批→原子应用→验证→报告）、
    风险分级与**人工审批门**。
  - **[RFC-0007 构建与产物系统](docs/architecture/rfc/RFC-0007-build-and-artifact.zh-CN.md)** — 产物三类归属
    （Generated/Extension/Adopted）、产物清单、**冲突检测（人工修改不被静默覆盖）**、语义级增量、可复现构建。
  - **[RFC-0008 安全与治理](docs/architecture/rfc/RFC-0008-security-and-governance.zh-CN.md)** — 权限四层模型
    （认证/策略/资源级/行级/字段级）、密钥与审计契约、**验证门槛映射**（变更类型 → 最低验证等级）。
  - **[RFC-0009 开发者体验与 Studio](docs/architecture/rfc/RFC-0009-developer-experience-and-studio.zh-CN.md)** —
    CLI 契约与命令面组织原则、诊断统一结构、Studio 三阶段与能力边界、`forma docs` 治理校验。
  - **[`spec:ir` 统一语义 IR 契约](docs/architecture/specifications/ir/ir-contract.zh-CN.md)** — Layer 2 的机器可读
    语义契约（节点结构/标识规则/依赖边/来源/版本/扩展/类型系统/检查契约/序列化），并记录 **7 项现状差距**。
  - **ADR-0002** DSL 主线 `Program` 为规范语义表示，联邦语义图降级为导入视图；
    **ADR-0003** 产物三分类归属与「人工修改不被静默覆盖」；
    **ADR-0004** 权限服务端权威执行、前端显隐不构成安全边界。
  - 全部 RFC 均以**源码核对与实跑复现**为依据，记录了当前实现的真实缺口而非推测。

- **可视化子系统（RFC-0001：Visualization Specification & Rendering System，V0.1）** — DSL 新增顶层
  `chart` 块与页面挂载 `page { chart <Name> }`，可视化成为与 Data/API/Business/UI 并列的一等能力：
  - **Chart IR**（`crates/forma/src/visualization/`）：与语法、目标图表库双重解耦的统一语义——
    图表类型（line/bar/area/pie/scatter/funnel/gauge/radar/heatmap/treemap）、形状家族
    （cartesian/xy/part-to-whole/radial/gauge/heatmap）、编码通道（x/y/series/color）、
    变换计划（filter/bucket/aggregate/sort/limit）、轴/系列/提示/图例/交互/主题/响应式/无障碍。
    同一 IR 两次投影 byte 级一致（确定性铁律）。
  - **语义校验硬失败**：`viz-unknown-type/api/operation/field/format`、`viz-type-mismatch`、
    `viz-shape-mismatch`、`viz-aggregate-conflict`、`viz-duplicate`、`viz-unknown-transform`。
    `forma build` 现在在 Emit 前执行语义检查（编译管线第 3 阶段补齐）——可视化错误在编译期阻断，
    不留给运行时。
  - **ECharts Adapter**（纯函数 `chart_option(spec)`）：Chart IR → option 骨架（数据位置留空），
    temporal x → time 轴、zoom → dataZoom、integer 格式 → minInterval、heatmap → visualMap。
    运行时按工程实际用到的图表类型生成 `echarts.use([...])` 按需注册（实测 bundle 1.34 MB → 842 KB）。
  - **React Runtime**（viz-web emitter）：`src/viz/spec.ts`/`transform.ts`（适配器无关的类型与变换
    执行器）、`echarts.ts`（option 绑定/格式化/统一事件模型翻译）、`runtime.tsx`（`<FormaChart>`：
    取数→变换→整形→渲染→ResizeObserver→dispose）、`src/viz/charts/<Name>.ts`（Spec/Option/Source）。
    无 chart 的工程零产物、零 echarts 依赖。
  - **MCP 工具面**：`forma_viz_catalog`（能力词表）/ `forma_viz_describe`（单类型规格）/ 
    `forma_viz_validate`（校验 + 图表语义摘要）；`forma_graph` 摘要含图表清单。AI 生成图表规格
    不再读图表库文档。
  - **验收测试** `crates/forma/tests/visualization.rs`（23 条）：10 种图表类型投影、viz-* 诊断、
    关系字段 `{rel}_name` 维度、聚合输出通道、确定性、产物结构、增量更新（改 A 不动 B）、
    MCP 工具面。school 示例新增两个图表页（bar + 分桶计数多系列 line），浏览器端到端渲染验证通过
    （tsc && vite build 无错误、无 console error、实例数据绑定断言通过）。
  - 设计全文：[docs/architecture/rfc/RFC-0010-visualization.zh-CN.md](docs/architecture/rfc/RFC-0010-visualization.zh-CN.md)；
    DSL 语法：[docs/dsl-spec.zh-CN.md §11a](docs/dsl-spec.zh-CN.md)。
- **`forma migrate risk` — 迁移风险分级（L5 变更安全）**：每条迁移操作按破坏性分级
  （`Low` / `Medium` / `High` / `Extreme`，未知操作 fail-safe 归 `High`）。删列、删表、改类型等
  高风险变更在执行 `up --apply` 前需人工审批。`forma migrate risk` 在落库前评估「自上次构建
  以来」的待执行操作（旧快照 → 当前 DSL schema diff）。分级依据、回滚语义与 Expand-Contract
  模式见 [docs/migration-safety.zh-CN.md](docs/migration-safety.zh-CN.md)；分层验证模型见
  [docs/verification.zh-CN.md](docs/verification.zh-CN.md)。
- **gen-seed（确定性案例数据，非 AI 数据）** — `forma seed [dir]` / 自动随 `forma build` 产出：
  - 全部取值由 DSL 纯函数推导（类型/约束/枚举/关系），不经过任何模型；同一 DSL 生成结果
    byte 级一致，可 hash 校验、可跨机器重放。
  - 版本化 `db/seed/seed_{version}.sql`（INSERT IGNORE + SHA-256 派生 v5-shape 主键，幂等可重放；
    表按拓扑序先父后子，FK 引用父表同行号主键）+ `db/seed/fixtures/*.json`（实体/列表两种形态，
    键名与 Rust 后端 serde 序列化一致）。种子文件与迁移链版本一一对应：migrate up --to X
    后应用 seed_X 即得到该版本完整数据集；值满足同一套 DSL rules（min/max/枚举/必填/check）。
  - school 案例现带三轮数据链：`seed_0_1_0 → seed_0_2_0 → seed_0_3_0`（与迁移链对齐）。
- **gen-flutter（移动端适配层，Android / iOS 同源）** — `forma.yaml` 增加 `mobile: { framework: flutter }`：
  - `lib/models/models.dart`：实体/枚举/ListRow/Input 类，fromJson/toJson 键名与 Rust 后端
    serde 契约严格一致（实体/ListRow 用 `{rel}_id`、Create/Update 输入用裸关系名 `{rel}`；
    Dart 字段名 lowerCamelCase、JSON 键保持 snake_case；Decimal 走 string 保精度）。
  - `lib/api/api.dart`：REST 客户端（`/api/{plural}`，page/page_size/q/filters+_contains），
    API_BASE 经 `--dart-define` 注入（Android 模拟器默认 10.0.2.2）。
  - `lib/validation/validators.dart`：同一套 DSL rules 三端同源编译
    （Rust 权威 + Web zod + Dart），check 规则 DateTime 比较编译为 compareTo。
  - `lib/pages/*.dart` + `lib/main.dart`：页面原语 → Material 列表/编辑页，DSL 路由
    （含 `{id}` 参数路由与嵌套路由）→ 命名路由 + onGenerateRoute 模式匹配。
  - 产物是普通 Flutter 工程：`flutter pub get && flutter run` 即可运行（Flutter ≥ 3.35）。
  - 生成代码通过 `flutter analyze`（0 issues）与 `flutter build web` 生产编译。
- **案例协议文档** `docs/case-protocol.zh-CN.md`：多轮次迭代剧本（R1 基线 → R2 增量 →
  R3 演进）+ 12 项能力完整性验收矩阵（V1–V12），每轮固定跑同一组可脚本化命令。
- school 示例：v1/v2 快照目录可独立 seed；`app_mobile/` 产物入库。

### Fixed
- **`row [...]` 行动作列表解析丢动作（正确性）**：解析器不跳过逗号 → 首项之后的所有行动作
  （含 `delete`）被静默丢弃；叠加解析期把 `confirm` 拼进 target（`"Api.delete confirm"`）导致
  `ends_with(".delete")` 匹配失败——**此前 DSL 声明的删除按钮从未渲染**。两处修复后，
  school 等参考应用的列表页恢复删除按钮（未受授权约束的实体由服务端 403 兜底，ADR-0004）。
- **文档链接失效（架构目录重构后遗留）** — `docs/rfc/` 迁入 `docs/architecture/rfc/` 后，
  RFC-0001/0004/0010 与 ADR-0001 内的相对链接层级错误（`../README.md` 应为 `../../README.md` 等），
  `docs/architecture.zh-CN.md` / `roadmap.zh-CN.md` / `dsl-spec.zh-CN.md` 中的仓库根路径链接亦失效。
  全部修正后全仓 Markdown 相对链接零失效（history 归档除外）。
- **forma_patch：多行 required 数组漏改**——字段改名后 `required` 里遗留旧名（旧 fixture 为
  单行数组未暴露）。现在单行/多行两种排版都能正确改名。
- **gen-rust 产物的运行时契约（真实 MySQL 8 端到端抓出）**：
  - SELECT 列表尾逗号/缺换行 → MySQL 1064；命名占位符 `?name` → sqlx 运行时不支持 → 改位置参数；
  - 搜索条件三值逻辑：多字段应为 OR、NULL 列需 COALESCE（此前搜索恒空）；
  - 关系列名：DDL 是 `{rel}_id`，INSERT/UPDATE 曾用裸名 → 1054 Unknown column；
  - JOIN 表名曾用原始实体名（`Schools`）→ 与 gen-mysql 的 snake_case 复数不一致 → 1064；
  - UUID 物理表示：列是 CHAR(36)，sqlx 的 `Uuid` 仅解 16 字节二进制 → Rust 侧统一用文本 UUID；
  - 仓储函数/校验器按 api op 裁剪、导入按需生成 → 生成后端 66 warning → 0 warning。
- **跨端端口一致性**：后端默认 8080（可用 `PORT` 覆盖）、Web vite proxy 8080、Flutter 默认
  `API_BASE=http://10.0.2.2:8080/api`（此前 Flutter 默认 3000，连不上后端）。

### Added
- **auth module（M5，全栈认证）** —— `auth module { provider jwt; roles [...]; tokenHours N }`：
  - DSL 解析（`AuthDecl`：provider / roles / tokenHours）；users 表随 schema 管线自动附加
    （id/username/password_hash/role/created_at，密码仅存 argon2 哈希）；
  - gen-rust：auth.rs（注册/登录/me + JWT 签发校验 + Bearer 中间件 + CurrentUser 注入）、
    公开端点 `/api/auth/register|login` 与受保护业务路由（protected 子 router 先 merge 后 layer）、
    `AppError::Auth` → 401；
  - gen-web：token 存储与 Authorization 头、401 事件、登录/注册页（`/login` 路由）；
  - gen-flutter：Api.authToken + 401 回调、AuthApi 客户端、登录/注册页；
  - 审计 actor 自动取当前登录用户（命令 handler 注入 CurrentUser）。
- **迁移链自愈（架构修复）**：增量迁移与 `migrate verify` 改为以**入库的迁移文件**为权威
    （此前依赖本机 `.forma/` 快照与台账 → 新克隆仓库无法重建增量链、无法校验）。
    IR 快照随链入库（`db/migrations/ir-snapshot.json`），快照缺失时从链上 SQL 重放重建；
    台账退化为纯 applied 状态（缺失时从链上重建），哈希校验只针对已应用迁移。
- **`forma seed --verify`**：确定性校验——磁盘种子与当前 DSL 的纯函数推导 byte 级一致
  （SHA-256 报告；手工改动 / DSL 演进后未重建 → 非 0 退出）。「案例数据不是 AI 数据」
  从设计承诺变成 CI 可执行的命令。
- **Flutter Detail 原语**（此前只有占位注释，与 web 不对称）：section → Card、行 → 标签/值，
  与列表页同一套类型驱动渲染；`{id}` 参数路由接入。真实工具链 analyze 0 issues。
- **MCP 认识 v0.3 DSL 工程**（此前全部工具走联邦引擎，对 DSL 工程返回空）：
  - 新工具 `forma_check`（DSL 诊断 + lint + 结构摘要）、`forma_build`（与 CLI `forma build`
    同一条管线的产物清单）、`forma_migrate`（迁移链状态）；
  - `forma_graph` 双模式：DSL 工程 → 实体/枚举/API/页面/规则摘要；联邦工程 → 原语义图；
  - `forma_impact` / `forma_patch` 在 DSL 工程上给出带替代工具指引的明确报错；
  - AI 工作流闭环：读模型（graph）→ 改 .fm → 校验（check）→ 重建（build）→ 看迁移链（migrate）。
- **`forma init` 对齐 v0.3**：脚手架从联邦原型改为 DSL 工程（forma/app.fm 起步示例 +
  forma.yaml 含 mobile target + README + .gitignore）——init 后 validate/build/migrate verify
  开箱全绿（集成测试锁定）。
- **多字段排序**：`sort [createdAt desc, amount]` 逐项生成 ORDER BY（此前静默截断到第一个）。
- **cursor 分页显式报错**：按设计宪法 6.2，未实现的构造不再静默降级（写盘前失败并说明支持范围）。
- **CI 补齐案例协议 V13–V16**：flutter（analyze + 契约测试）、web（tsc + vite）、
  e2e（MySQL 8 service + 迁移链 + seed + API 冒烟：list/search/create/校验 400/守卫 409）。
- **测试体系适配**：build/patch 套件的联邦 fixture 从旧脚手架迁到 `examples/order-management`
  （`forma init` 现在生成 DSL 工程）；新增 `tests/scaffold_mcp.rs`（脚手架开箱全绿 + MCP 工具面）
  与 backend_contract 的多字段排序/cursor 报错断言；README badge 更新至 92 tests。
- **gen-flutter 契约测试**（`app_mobile/test/live_api_test.dart`）：离线 fromJson/toJson 往返 +
  在线「创建→读取→更新→删除」全链路（后端未启动时自动跳过，不阻塞 CI）。
- **gen-rust 运行时契约回归测试**（`crates/forma/tests/backend_contract.rs`）：把上述 8 类
  bug 固化为源码结构断言，防止回归。

### Changed
- CLI：`forma seed` 子命令；`forma build` 新增 seed/mobile-flutter 两个发射步骤。
- school 示例 `forma.yaml` 增加 `mobile` target（产物指向非 gitignored 目录，与迁移 SQL 同一可见性原则）。

### Planned
- DMN adapter (decision tables → rule execution)
- AsyncAPI adapter (event contracts → subscriptions)
- `forma patch --dry-run` (edit-plan preview without writing)
- Forma Studio (per-standard visual editors)

## [0.3.0] - 2026-11-03

First public release of the Forma DSL compiler. This release establishes the end-to-end
workflow: write one declarative DSL, compile every target deterministically, evolve the schema
across versions, and verify the migration chain before deployment — with the full SQL trail
visible in the repository.

### Added
- **Versioned migration engine（多版本迁移）** — schema evolution like a real project:
  - Structured diff → paired `{index:04}_upgrade.sql` + `{index:04}_rollback.sql` per change;
    rollback is the inverse sequence (add column ↔ drop column, create table ↔ drop table,
    add index/fk ↔ drop index/fk, modify ↔ modify-back).
  - MySQL-safe op ordering per table (drop constraints → add/modify columns → add constraints
    → drop columns) and content-derived index names (`idx_{table}_{cols}`) — long chains
    replay without duplicate-key or missing-column errors.
  - Immutable baseline: `0001_init.sql` is written once by gen-mysql and never rewritten;
    every later DSL change becomes an incremental migration.
  - Version ledger `.forma/migrations.json` (per-migration version, changes, up/down hashes,
    applied flag) + generated `MIGRATIONS.md` making the whole SQL trail visible.
  - CLI `forma migrate {status|history|verify|up|down} [--sql] [--apply] [--to v] [--steps N]`:
    step to a target version, roll back N steps, and `verify` the chain (numbering, pairing,
    hash tamper-detection, forward-replay == current DSL schema; CI-ready exit codes).
  - SQL replay simulator (`emit/replay.rs`): executes the generated SQL subset with real
    MySQL semantics — no database needed — proving forward-replay to latest and
    backward-rollback to baseline.
  - School example now ships a committed **three-version chain**:
    `examples/school/db/migrations/` — v0.1.0 (CRUD baseline) → v0.2.0 (election feature)
    → v0.3.0 (phone/remark/title-widen/voter-index). Versioned DSL snapshots in `v1/`, `v2/`.
  - manifest: `targets.db` / `targets.web` accepted as aliases of `database` / `frontend`.
- English-first public README: positioning, current status, quick start, DSL overview,
  generated targets, versioned-migration guide, CLI reference, and contribution guide.
- Release metadata: package `repository`/`keywords`/`categories`, `v0.3.0` release tag.

### Fixed
- `ADD COLUMN` on a `@unique` field dropped the unique constraint (new columns skipped the
  column-level unique diff) — now emits a named `uq_{table}_{col}` key, consistent between
  CREATE and ALTER paths.
- Positional index names (`idx_{table}_{i}`) collided across versions (baseline
  `idx_enrollments_0 (student_id, course_id)` vs upgrade `idx_enrollments_0 (status)` →
  real MySQL "Duplicate key name") — replaced with content-derived names.
- `DROP COLUMN` was emitted before its dependent `DROP INDEX` — MySQL auto-drops the index,
  making the explicit drop fail; constraints now drop first.

### Fixed (infra)
- Flaky parallel-test failure: build-test tmp projects were named with pid + a truncated
  `SystemTime` debug string; two threads could mint the same directory in one window
  ("forma.yaml already exists"). Directories now use an in-process atomic sequence plus true
  nanoseconds.
- Debug leftovers removed (`examples/dbg10.rs`, debug `eprintln!` in a migration test).

### Changed (design)
- **Architecture v0.3 — DSL middle layer.** Forma is repositioned: one DSL is the single
  source of truth for data, rules, API and pages; adapter layers compile it to MySQL,
  Rust backend (axum + sqlx) and React web (TanStack + shadcn/ui). The v0.2
  standards-federation prototype is kept as a working reference and its standard
  adapters become generated export views (M6).
- DSL spec v0.1 published (`docs/dsl-spec.zh-CN.md`): app/enum/entity/rules/api/page,
  closed rule-expression language compiled to both backend (Rust) and frontend (zod).

## [0.2.0] - 2026-11-02

First public implementation. Full rewrite in Rust (the earlier TypeScript prototype was superseded).

Specification Federation: Forma organizes existing standards instead of inventing a DSL.

### Added
- `forma.yaml` manifest: spec inventory, standards/versions, build targets, identity bindings.
- Entity identity protocol: stable `forma://` URIs; binding priority `x-forma-entity` → manifest `bindings` → name conventions (`orders ≡ Order`).
- Adapters: SQL DDL (SQLite subset, hand-written parser), JSON Schema 2020-12, OpenAPI 3.1 (with `$ref` resolution), JSON Forms UI Schema, BPMN 2.0 subset (hand-written XML parser).
- Unified Semantic Graph with `affectedBy()` impact closure (forward `defines/binds/generates`, reverse `references/belongs-to`).
- Cross-standard doctor rules: `field-type-mismatch`, `field-required-mismatch`, `field-schema-missing`, `field-column-missing`, `field-name-style`, `form-binding-missing`, `workflow-reference-soft`, `table-entity-unbound`.
- Generators (deterministic, `@generated`-stamped): TypeScript interfaces + zero-dependency validators, Hono routes with request validation, SQLite baseline migration, React forms from JSON Forms layout.
- File-level incremental builds (`.forma/build.json`); manual edits to generated files are detected.
- Semantic Patch API (transactional): `add-entity-field`, `rename-entity-field`, `set-entity-field-required` — each propagated across SQL DDL, JSON Schema, OpenAPI and JSON Forms, then re-validated and incrementally rebuilt.
- CLI: `init` / `validate` / `doctor` / `graph` / `build` / `patch` / `mcp`.
- MCP stdio server (self-implemented JSON-RPC): `forma_validate`, `forma_graph`, `forma_impact`, `forma_patch`.
- End-to-end example project `examples/order-management` (5 standards + patch demo).
- 17 integration tests (`cargo test`).

### Changed
- Project language: TypeScript prototype → **Rust** (single binary serves CLI and MCP).

[Unreleased]: https://github.com/huifer/forma/commits/main
[0.3.0]: https://github.com/huifer/forma/commit/77899ea91d79853a63cb50171c57597445cc5dbf
[0.2.0]: https://github.com/huifer/forma/commit/5477039
