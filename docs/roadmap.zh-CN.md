# Forma 完整路线图（v0.3 — DSL 中间层）

> 架构见 [architecture.zh-CN.md](architecture.zh-CN.md) · DSL 语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)

## 0. 接入决策原则

目标技术按三个维度排期：**真实业务覆盖面**（CRUD → 中后台 → 多端 → 企业级）、**LLM 生态成熟度**（模型训练数据越多，生成越可靠）、**适配层成本**（IR 到该目标的映射复杂度与复用率）。

首批目标：Rust（axum + sqlx）、Web（React + TanStack + shadcn/ui）、MySQL。

## 1. 阶段总览

```text
Wave 1 — 端到端闭环
M1 DSL 内核 → M2 db-mysql → M3 backend-rust → M4 web-react

Wave 2 — 可视化 + 生态互通 + AI 闭环
M5 Auth 模块 → M6 Forma Studio 可视化 → M7 生态互通(Swagger/SQL/Postman)
→ M8 AI 语义补丁闭环（diff 可视化 ✅ 已随 M1 交付：`forma diff` HTML/JSON 报告）

Wave 3 — 工程化 + 横向扩栈
M9 工程化产物 + 能力模块 + Studio 双向编辑
M10 数据库/后端/前端多栈（PostgreSQL、Node、Python、Vue、Expo、GraphQL）

Wave 4 — 部署与企业级（范围已收窄）
M11 部署产物（K8s / Helm / Terraform）✅
企业级扩栈（Spring/Go/.NET、原生移动、小程序、工作流、支付）→ 已撤销，不再排期

P2 余项 Closure（轨道 A 8/8 完成，轨道 B 撤销）→ P3 治理收尾与发布就绪
→ 发布前一致性收口（同步 Studio V3 / 可视化 V0.4–V0.6 / CLI E2E 到治理叙事）
```

## 2. 适配层完整矩阵

> **范围决议（2026-11-08）**：矩阵中标 `👁 P2` 的目标扩栈行（SQL Server / Oracle / MongoDB /
> Spring Boot / Gin / Laravel / .NET / gRPC / tRPC / SvelteKit / Angular / Tauri / Electron /
> SwiftUI / Compose / 小程序 / uni-app / 工作流运行时 / 支付 / Playwright e2e / Token Benchmark）
> 已按用户决策**撤销，不再排期**。本表保留为历史评估记录。P2 阶段实际完成的是
> [P2 轨道 A 余项 Closure](architecture/roadmap/p2-plan.zh-CN.md)（8/8，详见 gap-backlog「轨道 A 收尾」）。

每个目标 = 一个 emitter crate（`forma-gen-*`），实现统一接口契约（见 §6.1）。

### 2.1 数据库

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **MySQL 8** | M2 | InnoDB/utf8mb4；DDL + 基线迁移 → IR diff 增量迁移 |
| ✅ 已完成 | **PostgreSQL** | M10 | PostgreSQL 15+ DDL target：UUID / JSONB / BOOLEAN / TIMESTAMPTZ / NUMERIC；独立 `db-postgres` + `postgres-migrate` 构建步骤；与 MySQL 共享 DSL/拓扑语义但独立方言渲染，MySQL 产物零变化 |
| ✅ 已完成 | **SQLite** | M10 | SQLite 3 DDL target：TEXT/NUMERIC/INTEGER 亲和性映射 + PRAGMA foreign_keys；独立 `db-sqlite` + `sqlite-migrate` 构建步骤；与 MySQL/PostgreSQL 共享表拓扑但独立方言渲染 |
| ✅ 已完成 | **Redis**（键设计派生） | M9 | `targets.db.engine: redis` → 从 DSL 实体/索引/唯一/auth/jobs/live 派生键设计（`redis/key-design.md` + `.json`）；默认/关系型工程零产物（P1-#2） |
| 👁 P2 | SQL Server / Oracle / MongoDB | M11 | 企业遗留 / 文档模型专项设计 |

### 2.2 后端

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **Rust：axum + sqlx** | M3 | models / validators / routes / repositories |
| ✅ 已完成 | **Node：NestJS** | M10 | NestJS 10 + TypeORM 0.3 + MySQL + class-validator；CRUD 子集（标量字段/枚举/Decimal/必填/min/max）；auth/access/tenant/commands 显式拒绝（不静默降级安全语义）；`backend.language: typescript` + `framework: nestjs` 路由到 `backend-nestjs` 构建步骤；npm run build + tsc --noEmit 通过 |
| ✅ 已完成 | **Node：Hono** | M10 | Hono 4 + @hono/node-server + mysql2 + zod；轻量 CRUD（list/get/create/update/delete、offset 分页、字段 min/max/required）；auth/access/tenant/commands/cross-field rules 显式拒绝；`backend.language: typescript` + `framework: hono` 路由到 `backend-hono` 构建步骤 |
| ✅ 已完成 | **Python：FastAPI** | M10 | FastAPI + SQLAlchemy 2.0 + Pydantic 2 + PyMySQL；`models.py`（Mapped/mapped_column）/`schemas.py`（Create/Read DTO + ge/le）/`routes.py`（CRUD router）；auth/access/tenant/commands/cross-field rules 显式拒绝；`backend.language: python` + `framework: fastapi` → `backend-fastapi` 构建步骤；`python -m compileall` 通过 |
| ✅ 已完成 | **Vue 3 前端** | M10 | Vue 3 + Vite + TypeScript + Vue Router + Pinia + Element Plus；`frontend.framework: vue`（或 `vue3`）路由到 `frontend-vue` 构建步骤；复用 web_react 的 `types_ts`/`validators_ts`/`format_ts`/`access` 契约；auth（`AuthApi`）+ 命令操作（`XxxApi.execute`）+ 角色感知（`access.ts` 用 `@tanstack/vue-query` 的 Vue 原生 `useCurrentRole`/`useCan`）；列表页：搜索/过滤器（关系/枚举 select + options 查询）/分页/导出/命令按钮/删除；表单页：create/update 提交 + validators 同源校验 + 关系选择（`{rel}_id` → 裸名映射）；详情页：分节 descriptions；Pinia store（filters/search/分页）；`vue-tsc --noEmit && vite build` 通过；默认（无 framework 或 `react`）仍生成 React，产物零变化 |
| 👁 P2 | Java：Spring Boot | M11 | JPA + Spring Security |
| 👁 P2 | Go：Gin + GORM / PHP：Laravel / .NET | M11 | |

### 2.3 API 风格

| 优先级 | 风格 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | REST | M3 | `api` 块 → REST 路由 |
| ✅ 首批 | **OpenAPI 3.1 导出 + Swagger UI** | M7 | 生成视图 + 本地文档站 |
| ✅ 已完成 | **OpenAPI 导入** | M7 | `forma import openapi <spec> [--out]` → 从 components.schemas/paths 逆向生成 `.fm`（实体/api/类型映射/@id 主键保证），生成物通过 check_program（P1-#1） |
| ✅ 已完成 | **GraphQL（schema + resolver 网关）** | M10 | `targets.api.framework: graphql` → `api-graphql` 构建步骤（无 `targets.api` 零产物）；DSL → `schema.graphql`（枚举/实体/正向对象关系/Create·Update Input/Query/Mutation，按各 api operations 裁剪；Decimal→String 同后端序列化契约）+ `src/schema.ts`（typeDefs 同源）+ `src/rest.ts`（服务端 REST 客户端，`REST_BASE` 可指向任意后端 target）+ `src/resolvers.ts`（委托 REST + 关系 resolver）+ `src/server.ts`（graphql-yoga 可运行）；安全语义由 REST 后端权威执行、网关透传 token；逆向关系留 v2（过滤器依赖与 N+1） |
| 👁 P2 | gRPC（Protobuf 导出）/ tRPC | M11 | |

### 2.4 前端 Web

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **React + TanStack Router/Query/Table + shadcn/ui + Tailwind** | M4 | 校验 zod 与后端同源 |
| ✅ 首批 | **i18n 资源生成**（react-i18next） | M4 | DSL 内 message → 语言包 |
| ✅ 已完成 | React Hook Form 适配 | M9 | `targets.frontend.usesRhf` opt-in；useForm/register/useController + 校验复用 validators.ts（P1-#5） |
| ✅ 已完成 | 状态管理生成（Zustand） | M9 | `usesZustand` opt-in；store 从列表页数据依赖派生（filters/search），setter 签名兼容（P1-#6）。Pinia（Vue）已跟进（2026-11-09）：Vue 列表页引入 filters/search/分页依赖面后生成 `src/stores/{page}.ts` Pinia store |
| ✅ 已完成 | **Vue 3 + Vue Router + Pinia + Element Plus** | M10 | `frontend.framework: vue` → 复用 React 契约；auth/命令/角色感知双端对齐 |
| ✅ 已完成 | **Next.js（App Router / SSR 变体）** | M10 | `frontend.framework: nextjs`（或 `next`）→ `frontend-nextjs` 构建步骤；复用 web_react 的 `types_ts`/`validators_ts`/`api_ts`/`format_ts`；列表页 = Server Component（`src/api-server.ts` + `API_BASE` env + `force-dynamic`）；变更/命令 = Server Actions（`app/actions.ts` + `revalidatePath`）；表单 = client 组件复用 validators 同源校验 + **关系选择器**（options 查询 + `{rel}_id` → 裸名初始值）；根布局 `app/providers.tsx` 提供 QueryClientProvider（`@tanstack/react-query` 依赖）；new/[id] 页按 api operations 裁剪；`next.config.mjs` rewrites `/api/*`；自带 CI（tsc + next build，无 vitest）；`tsc --noEmit` + `next build` 通过；默认（react）零变化 |
| 👁 P2 | SvelteKit / Angular | M11 | |
| 👁 P2 | 桌面：Tauri / Electron | M11 | |

### 2.4a 可视化（RFC-0001）

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **Chart IR + ECharts Adapter + React Runtime** | V0.1（已落地） | `chart` 块 → 确定性 option + 按需注册运行时；十种图表类型词表 |
| ✅ 已完成 | Data Binding 完整化 + Metric/Table widget | V0.2 | cursor 源解包信封；`type metric`（KPI 卡）/`type table`（真实数据回退）；metric/table-only 工程不装 echarts（P1-#7） |
| ✅ 已完成 | **Dashboard IR**（Grid 布局 + 筛选器 + 联动） | V0.3 | `page { dashboard { grid { <Chart> w N h N } filters [...] } }`；useMemo source 联动取数；Flutter/Vue/Expo 显式拒绝（P1-#8） |
| ✅ 已完成 | Theme / Preview / 浏览器渲染验证门 | V0.5 | `theme dark|data-screen` token + Provider；`viz-preview.html` mock 自检渲染门（P1-#9） |
| ✅ 已完成 | Dashboard 联动与下钻 | V0.4 | 图表点击 → dashboard 筛选器联动；`drilldown` → 列表页 URL query 初始筛选（`tests/visualization_v04.rs`） |
| ✅ 已完成 | AntV / G2Plot 适配层 | V0.6 | `targets.visualization.adapter: antv`；共享 ChartSpec / transform；显式拒绝不支持类型；`examples/viz-antv` + CI `viz-antv` |
| ✅ 已完成 | legend / tooltip / size / label 交互与通道 | V0.6 | 多系列 crosshair、数据驱动点大小、数据标签；`tests/visualization_v06.rs` |
| ✅ 已完成 | GIS / 工业监控 / K 线 | V0.6 | ECharts map + GeoJSON 契约；三端 threshold；AntV Candlestick（echarts 显式拒绝） |
| ✅ 已完成 | AntV 图分析/关系可视化 | V0.6+ | 关系图（`type graph` + `link { source target }`）：ECharts force 图 + AntV **G6** 渲染器（按需注入 `@antv/g6`）；节点由边值去重派生、标签取 `{rel}_name` |
| ✅ 已完成（全矩阵） | 移动端图表投影 | V0.6+ | Flutter fl_chart：bar/line/area/pie/scatter/gauge/radar + 变换计划（filter/bucket/aggregate/sort/limit，语义与 Web runtime 一致）；heatmap/treemap/funnel 为占位（fl_chart 无对应图型） |

设计全文：[architecture/rfc/RFC-0010-visualization.zh-CN.md](architecture/rfc/RFC-0010-visualization.zh-CN.md)。

### 2.5 移动端

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 已完成 | **Expo（React Native）** | M10 | `targets.mobile.framework: expo` → `mobile-expo` 构建步骤；复用 web_react 的 `types_ts`/`validators_ts`/`api_ts`/`format_ts`/`access_ts`（react-query 在 RN 可用）；React Native 侧 `View/Text/FlatList/TextInput` 渲染 + 轻量手写栈导航（无 react-navigation）；auth（`Login.tsx` + `setAuthToken`）+ 列表页内嵌编辑视图；`tsc --noEmit` 通过；默认（无 framework）仍生成 Flutter，产物零变化 |
| ✅ 已交付（提前于 M11） | Flutter | gen-flutter：models/api/validators/pages 四件套，与 Rust 后端 serde 契约严格对齐；school 案例 app_mobile/ 已入库 |
| 👁 P2 | 原生 SwiftUI / Jetpack Compose | M11 | |
| 👁 P2 | 微信小程序 / uni-app | M11 | 国内场景 |

### 2.6 能力模块（全部双端生成）

| 优先级 | 能力 | 里程碑 | DSL 形态（草案） |
|---|---|---|---|
| ✅ 首批 | **Auth 模块**（用户/角色/会话 + 登录注册页 + JWT/session 中间件） | M5 | `auth module { provider jwt }` |
| ✅ 首批 | 审计日志 | M7 | `audit { action "…" }` |
| ✅ 已完成 | 文件上传 | M9 | `Attachment` 内建类型 + `@storage(provider, bucket)` 属性；**local / s3 / oss 三种驱动全部真实实现**（local 落盘；s3 经 aws-sdk-s3 + `AWS_REGION`/`AWS_*` 凭据；oss 经 OSS-HMAC-SHA1 签名 + `OSS_*`；均为条件依赖）；Multipart 端点；Web 文件输入（P1-#3） |
| ✅ 已完成 | 导入导出 | M9 | `export <Api> as csv|xlsx` → 流式 CSV 端点 + **xlsx 真实驱动（rust_xlsxwriter，条件依赖）**、OpenAPI 契约（xlsx 200 + media type）、列表页导出按钮（P1-#4） |
| ✅ 已完成 | **消息通知** | M10 | 字段属性 `@notify(channel, target, event)`（email/sms/webhook/log 四渠道；属性参数必须括号形式）；检查层 `dsl-notify-unknown-channel` / `dsl-notify-missing-target`（email/sms/webhook 必填）/ `dsl-notify-unknown-event`（事件引用已声明 command）；Rust 后端 `src/notify.rs`：NotifyRule 静态展开（运行时零 DSL 依赖）+ `fire(entity, event, id)`；command handler 成功后自动触发；**四渠道全部真实驱动**：log（tracing）/ webhook（reqwest POST JSON，spawn）/ email（lettre SMTP，spawn_blocking）/ sms（通用 HTTP 网关 `SMS_GATEWAY_URL`）；均为条件依赖 |
| ✅ 已完成 | **全文搜索（基线）** | M10 | 实体级 `searchable [f1, f2]` → MySQL `FULLTEXT KEY ft_{t}_{cols} WITH PARSER ngram`（MySQL 8 内置 ngram 分词，中文 bigram 可用）；检查层 `dsl-searchable-unknown-field` / `dsl-searchable-field-type`（仅 String/Text/Email）；迁移链 `add_fulltext`/`drop_fulltext`（风险 Low/Medium）+ 重放模拟器支持；应用层 q 搜索默认保持 LIKE 子串语义（对中文友好，零行为变化）。**外部引擎已接入（2026-11-09）**：`src/search.rs` 适配 Meilisearch / Elasticsearch（REST + reqwest，运行时按 `SEARCH_ENGINE` 选择）；写路径 create/update/delete 同步索引，读路径 list q 经引擎取 id（未配置/失败自动回退 LIKE） |
| ✅ 已完成 | **定时任务** | M10 | DSL `job <name> { cron "0 2 * * *" label {...} op Command }`；检查层 `dsl-job-cron-format`（5 字段/范围/列表/范围区间）+ `dsl-job-unknown-op`（api/command 名）+ `dsl-job-input-effect`（InputRef effect 拒绝——调度器无输入来源）+ `dsl-job-pre-unsupported`（复杂 pre 拒绝——fail-safe 不做全表 UPDATE）；MySQL `forma_jobs` 表（迁移链自动增量）+ 确定性 seed；Rust 后端 `jobs.rs`：进程内 30s 轮询调度器（cron 求值零依赖）、TARGETS 由 DSL 静态展开（运行时零 DSL 依赖）、管理端点 GET /api/jobs + POST trigger/enable/disable（挂受保护路由组）；IR bridge `Job` 节点 + job→command references 边 |
| ✅ 已完成 | **实时推送（live → SSE）** | M10 | DSL `live <name> { api <Api>.list; events [create update delete] }`；检查层 `dsl-live-unknown-api` / `dsl-live-unknown-event`（仅 create/update/delete）；Rust 后端 `src/live.rs`：SSE 通道（`GET /api/live/{name}`，axum SSE + tokio broadcast）+ `publish(table, event)`；CRUD handler 成功后自动广播；**Web 订阅**（EventSource → react-query invalidate）+ **Flutter 订阅**（`LiveSubscriber` + dart:io HttpClient 流式 SSE）；无 live → 零产物 |
| ✅ 已完成 | **多租户（@tenant 行级隔离 + 管理）** | M10 | DSL `@tenant` 字段（检查层 dsl-tenant-multiple/optional/type/no-auth）；MySQL users 表带 tenant + disabled 列；Rust 后端：JWT claim → CurrentUser.tenant → **list/get/update/delete 查询按租户过滤**（`WHERE tenant = ?`，None → NULL 恒不命中→空集）+ **create 绑定当前租户**（覆盖输入体）；租户管理端点 `POST /api/admin/tenants/:tenant/revoke|transfer`（TENANT_ADMIN_TOKEN env 授权，未设置时 403 关闭）；吊销 = 停用租户全部用户（disabled=1 + login 拒绝）；迁移 = 业务行改绑新租户（可回滚）；无 @tenant → 全部产物零变化 |
| 👁 P2 | 工作流运行时 / 支付 | M11 | |

### 2.7 工程化

| 优先级 | 项 | 里程碑 |
|---|---|---|
| ✅ 首批 | 生成代码可编译验证（cargo check / tsc） | M3 / M4 |
| ✅ 首批 | 各目标工程自带 CI 模板（lint/test/build） | M4 |
| ✅ 已完成 | Docker / docker-compose 产物 | M9 | `generated/backend/Dockerfile`（多阶段 Rust build → debian-slim）、`docker-compose.yml`（MySQL 8 healthcheck + backend + 可选 Web）、`entrypoint.sh`（等待库就绪 → 迁移 → 启动）、`.dockerignore`；环境变量 `DATABASE_URL` / `PORT` 可覆盖 |

| ✅ 已完成 | K8s / Helm / Terraform | M11 | `deployment:` manifest 段 opt-in（provider: kubernetes\|helm\|terraform\|all）→ 同源生成 k8s manifests / Helm v3 Chart / Terraform kubernetes provider 模块；未知 provider / 非 Rust backend 显式拒绝（M11/P1） |
| ✅ 已完成 | Makefile + GitHub Actions CI 模板 | M9 | 后端 Makefile（build/check/test/run/migrate/seed/docker）、后端 CI（cargo check/test/build）、Web CI（npm install + tsc + vitest + build）|
| ✅ 已完成 | 结构化日志基础（RUST_LOG + LOG_FORMAT=json） | M10 | `tracing-subscriber`（env-filter + json features）统一初始化——默认 human-readable，`LOG_FORMAT=json` 输出 JSON Lines；错误/审计/请求 id 统一走 `tracing::` 宏；**OTLP exporter 已接入（2026-11-09，opt-in feature）**：`cargo run --features otlp` 启用 gRPC 导出（`OTEL_EXPORTER_OTLP_ENDPOINT` / `OTEL_SERVICE_NAME`），默认构建零依赖 |

| 👁 P2 | Playwright e2e 生成 / Token Benchmark | M11 |

## 3. 可视化路线（Forma Studio）

可视化与 DSL 同级：**写 `.fm` 必须能被看见**。Studio 是审阅面的图形投影，分三阶段：

**M6 — Studio v1（实时只读预览）** ✅ 已交付（`forma studio`：语义模型/语义图 API + 最小 Web UI，P1-#10）
- `forma studio` 本地 Web 服务；DSL 变更刷新即见
- 实体关系图（ER）：实体/字段/关系/索引，节点点击定位 DSL 源行
- 页面原型：`table/form/detail` 用真实 shadcn 组件渲染（与 M4 emitter 同源组件，非手绘 mockup）
- API 文档视图：Swagger UI 风格浏览（操作/请求/响应/权限）
- 规则矩阵：字段校验/守卫/权限按实体×操作一览
- 验收：仅通过 Studio 即可完整理解一个陌生项目的现状

**M9 — Studio v2（双向编辑）** ✅ 已交付（UI 加字段/设必填/加表格页 → `POST /api/patch` → 补丁事务写回 `.fm`，P1-#10）
- ER 图上增删字段/关系 → 写回 `.fm`（等价于一次语义补丁，走同一校验管线）
- 页面原型上调整列/控件 → 写回 `page` 块
- 诊断点击跳转源码

**M11 — Studio v3（协作）**

- ✅ **V3 本地收尾（2026-11-08）**：变更历史可视化与回放——`GET /api/patch-history`（读 `.forma/audit.jsonl`）+ UI 历史面板；回放 = 审计摘要 + patchId 幂等重提交。多人实时协作仍明确排除。
- 原规划：多人实时协作、变更评审视图（DSL diff 的图形化）
- 多人实时协作、变更评审视图（DSL diff 的图形化）

## 4. 生态互通路线（与历史优质项目结合）

Forma 是生态的枢纽，不孤立。互通分导出与导入两个方向，均为生成视图：

| 方向 | 项目 | 里程碑 | 内容 |
|---|---|---|---|
| 导出 | **OpenAPI 3.1 + Swagger UI** | M7 | `api` 块 → 规范文档 + 本地文档站 |
| 导出 | **Postman Collection** | M7 | API 调试即用 |
| 导出 | **Mermaid**（ER/流程图） | M7 | 文档即代码 |
| 导出 | JSON Schema / DBML | M7 / M10 | 契约与建模互通 |
| 导入 | **OpenAPI** → 实体 + API 骨架 | M7 | 既有 Swagger 项目接入 Forma |
| 导入 | **SQL DDL**（MySQL/PG）→ 实体 | M7 | 数据库逆向工程 |
| 导入 | **Prisma schema** → 实体 | M10 | |
| 导入 | JSON Schema → 实体 | M10 | |
| 双向 | SQL 迁移对接（Flyway/Liquibase/golang-migrate 目录约定） | M9 | 产物落进既有迁移体系 |

验收（M7）：把一个既有 Swagger + MySQL 项目导入为 `.fm`，再导出 OpenAPI 与 DDL，语义等价。

## 5. 里程碑详情与验收

**M1 DSL 内核**（✅ 已完成）：lexer/parser/AST（span 保留）、AST → 语义图、类型与规则表达式检查、`forma check` 诊断（行号 + did-you-mean）。验收：`examples/school/forma/app.fm`（学生-学校-报名-上课，4 实体/3 关系/8 页面）全量通过。

**M2 db-mysql**（✅ 已完成）：DDL emitter + 版本化迁移引擎（外键/索引/唯一约束/可空；
不可变基线 + upgrade/rollback 配对迁移 + MySQL 安全顺序 + 内容派生索引名 + 版本台账
MIGRATIONS.md + SQL 重放模拟器）。验收：DDL 在 MySQL 8 直接执行；DSL 逐版本演进产出成对
迁移（回滚为逆序反向操作）；`forma migrate {status|history|verify|up|down} [--to] [--steps]`
全生命周期可用；`verify` 保证链完整 + 前向重放 == 当前 schema（CI 可接）；三版本示例链
（school v0.1.0→v0.2.0→v0.3.0）重放往返零漂移；连续两次 `forma build` 零变更。

**M3 backend-rust**（✅ 已完成）：struct/serde、规则校验函数（含守卫）、axum 路由 + sqlx 仓库（关系 JOIN 列表）。验收：`cargo build` 通过（school-backend 二进制）；CRUD 端到端；规则生效（400 + i18n 消息）。

**M4 web-react**（✅ 已完成）：TanStack Router/Query、shadcn 风格页面、双端同源校验器、i18n。验收：`tsc` 零错误、`vite build` 通过；列表（过滤/搜索/分页）、表单（校验即时反馈）、详情。

**M5 Auth 模块**：DSL `auth module` → 双端生成。验收：登录 → 带 token CRUD → 无权限被拒（后端 403 + 前端按钮隐藏）。

**M6 Studio v1 / M7 生态互通 / M8 AI 语义补丁闭环（diff 可视化 ✅ 已随 M1 交付：`forma diff` HTML/JSON 报告）**：见 §3、§4；AI 闭环验收——「Order 加付款状态」一条补丁，MySQL + Rust + React 三处同步 + 测试绿，AI 全程不接触目标代码。

**M9 工程化 + Studio v2**：✅ 全部完成——Docker 产物 / 上传 / 导出 / RHF / Zustand / Studio 双向编辑（P1-#2~#6、#10，均有独立回归测试与生成端编译验证）。

**M10 横向扩栈 / M11 企业级**：见 §2 矩阵标注。

## 6. 治理

### 6.1 适配层接口契约

```rust
trait Emitter {
    fn id(&self) -> &'static str;                 // "gen-mysql" / "gen-rust-axum" / "gen-web-react"
    fn version(&self) -> &'static str;            // 产物缓存失效键
    fn emit(&self, ir: &SemanticGraph, cfg: &TargetConfig) -> Result<Vec<GeneratedFile>>;
}
```

约束：确定性输出、`@generated` 头、参与增量缓存、产物内不回读业务语义。

### 6.2 DSL 版本演进

`app.version` 与 DSL 语法版本双轨；语法变更加 `forma migrate`（AST 级重写）；弃用周期 = 标记 → 下一里程碑 warning → 再下一里程碑移除。emitter 遇到未识别构造显式报错，不静默跳过。

### 6.3 生成代码升级路径

生成物整体重生成；自定义逻辑通过 `extension` 扩展点承接（后端 handler 钩子、前端组件覆写），升级时扩展点文件不动。

### 6.4 明确不做

- 运行时低代码引擎（产物是普通代码，脱离 Forma 可运行）
- 像素级 UI 表达（页面原语之上由目标生态自治）
- 通用编程语言（算法/复杂逻辑走 extension）
- 运行时解释执行 DSL（例外：规则表达式在特殊场景的候选实现）

### 6.5 当前近程重点（Reliability & Verification）

下一阶段不急于扩展目标技术栈，而先把「可信赖地生成」做扎实：分层验证（L1–L5，见
[docs/verification.zh-CN.md](verification.zh-CN.md)）、迁移风险分级（[docs/migration-safety.zh-CN.md](migration-safety.zh-CN.md)）、
Agent 原生工作流（[docs/agent-workflow.zh-CN.md](agent-workflow.zh-CN.md)）。具体任务清单与状态见
[docs/next-milestone.zh-CN.md](next-milestone.zh-CN.md)（F-001…F-008）。

### 6.6 P2 轨道 A 收尾（2026-11-08）

P2 任务按「余项 Closure（轨道 A）/ 目标扩栈（轨道 B）」分轨，用户决策只完成轨道 A、
**轨道 B 撤销**。轨道 A 八项已全部关闭：多层关系遍历、GraphQL 逆向关系（DataLoader）、
多租户管理 UI、artifacts 来源规格与 Extension 锚点、补丁图表/看板操作、SQL/Prisma 逆向导入
与迁移链导出（Flyway/Liquibase/golang-migrate），以及两项「已实现、文档滞后」的核实
（字段级前端显隐 / requestId）。完整验收与回归见
[docs/architecture/roadmap/p2-plan.zh-CN.md](architecture/roadmap/p2-plan.zh-CN.md)。

## 7. 遗留代码处置

`crates/forma`（v0.2 联邦引擎，17 测试绿）：`graph / patch / engine` 模块被 DSL 编译器复用；SQL/OpenAPI/JSON Schema 适配器 → M7 改造为导入器与导出视图；Hono 生成器经验并入 Node emitter。`docs/history/` 只读归档。
