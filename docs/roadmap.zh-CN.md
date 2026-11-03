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

Wave 4 — 企业级与长尾
M11 Spring/Go/.NET、原生移动、小程序、工作流运行时、支付
```

## 2. 适配层完整矩阵

每个目标 = 一个 emitter crate（`forma-gen-*`），实现统一接口契约（见 §6.1）。

### 2.1 数据库

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **MySQL 8** | M2 | InnoDB/utf8mb4；DDL + 基线迁移 → IR diff 增量迁移 |
| 🔨 P1 | **PostgreSQL** | M10 | jsonb/数组/丰富约束，与 MySQL 共享映射逻辑 |
| 🔨 P1 | **SQLite** | M10 | 嵌入式/本地场景 |
| 🔨 P1 | **Redis**（键设计派生） | M9 | 会话/缓存/限流 |
| 👁 P2 | SQL Server / Oracle / MongoDB | M11 | 企业遗留 / 文档模型专项设计 |

### 2.2 后端

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **Rust：axum + sqlx** | M3 | models / validators / routes / repositories |
| 🔨 P1 | **Node：NestJS** | M10 | TypeORM 或 Prisma |
| 🔨 P1 | **Node：Hono** | M10 | 轻量、边缘友好 |
| 🔨 P1 | **Python：FastAPI** | M10 | SQLAlchemy 2.0 + Pydantic（与规则校验同构） |
| 👁 P2 | Java：Spring Boot | M11 | JPA + Spring Security |
| 👁 P2 | Go：Gin + GORM / PHP：Laravel / .NET | M11 | |

### 2.3 API 风格

| 优先级 | 风格 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | REST | M3 | `api` 块 → REST 路由 |
| ✅ 首批 | **OpenAPI 3.1 导出 + Swagger UI** | M7 | 生成视图 + 本地文档站 |
| 🔨 P1 | **OpenAPI 导入** | M7 | 既有 Swagger 项目 → DSL（逆向工程） |
| 🔨 P1 | GraphQL（schema + resolver） | M10 | |
| 👁 P2 | gRPC（Protobuf 导出）/ tRPC | M11 | |

### 2.4 前端 Web

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **React + TanStack Router/Query/Table + shadcn/ui + Tailwind** | M4 | 校验 zod 与后端同源 |
| ✅ 首批 | **i18n 资源生成**（react-i18next） | M4 | DSL 内 message → 语言包 |
| 🔨 P1 | React Hook Form 适配 | M9 | 复杂表单 |
| 🔨 P1 | 状态管理生成（Zustand / Pinia） | M9 | 从页面数据依赖派生 |
| 🔨 P1 | **Vue 3 + Vue Router + Pinia + Element Plus/Naive UI** | M10 | |
| 🔨 P1 | Next.js（SSR 变体） | M10 | |
| 👁 P2 | SvelteKit / Angular | M11 | |
| 👁 P2 | 桌面：Tauri / Electron | M11 | |

### 2.4a 可视化（RFC-0001）

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| ✅ 首批 | **Chart IR + ECharts Adapter + React Runtime** | V0.1（已落地） | `chart` 块 → 确定性 option + 按需注册运行时；十种图表类型词表 |
| 🔨 P1 | Data Binding 完整化（分页/服务端聚合）+ Metric/Table widget | V0.2 | 大数据量走服务端聚合 |
| 🔨 P1 | **Dashboard IR**（Grid/Flex 布局 + 筛选器 + 联动事件模型） | V0.3 | `page { dashboard ... }` |
| 🔨 P1 | Theme / Preview / 浏览器渲染验证门 | V0.5 | 与 Studio v1 同一渲染源 |
| 👁 P2 | AntV 适配层（图分析/关系可视化）、GIS、金融/工业领域图表 | V0.6 | 同一 Chart IR 换 Adapter |
| 👁 P2 | 移动端图表投影 | V0.6+ | Flutter 自绘/内嵌图 |

设计全文：[docs/architecture/rfc/RFC-0010-visualization.zh-CN.md](docs/architecture/rfc/RFC-0010-visualization.zh-CN.md)。

### 2.5 移动端

| 优先级 | 技术 | 里程碑 | 说明 |
|---|---|---|---|
| 🔨 P1 | **Expo（React Native）** | M10 | 与 React emitter 复用度最高 |
| ✅ 已交付（提前于 M11） | Flutter | gen-flutter：models/api/validators/pages 四件套，与 Rust 后端 serde 契约严格对齐；school 案例 app_mobile/ 已入库 |
| 👁 P2 | 原生 SwiftUI / Jetpack Compose | M11 | |
| 👁 P2 | 微信小程序 / uni-app | M11 | 国内场景 |

### 2.6 能力模块（全部双端生成）

| 优先级 | 能力 | 里程碑 | DSL 形态（草案） |
|---|---|---|---|
| ✅ 首批 | **Auth 模块**（用户/角色/会话 + 登录注册页 + JWT/session 中间件） | M5 | `auth module { provider jwt }` |
| ✅ 首批 | 审计日志 | M7 | `audit { action "…" }` |
| 🔨 P1 | 文件上传 | M9 | `field Attachment` + `storage s3\|oss\|local` |
| 🔨 P1 | 导入导出 | M9 | `export Order as csv\|xlsx` |
| 🔨 P1 | 消息通知 | M10 | `notify email\|sms\|webhook` |
| 🔨 P1 | 全文搜索 | M10 | `searchable [...]` → Meilisearch/ES |
| 🔨 P1 | 定时任务 | M10 | `job { cron "…" { … } }` |
| 🔨 P1 | 实时推送 | M10 | `live OrderList` → WebSocket/SSE |
| 🔨 P1 | 多租户 | M10 | `@tenant` 行级隔离 |
| 👁 P2 | 工作流运行时 / 支付 | M11 | |

### 2.7 工程化

| 优先级 | 项 | 里程碑 |
|---|---|---|
| ✅ 首批 | 生成代码可编译验证（cargo check / tsc） | M3 / M4 |
| ✅ 首批 | 各目标工程自带 CI 模板（lint/test/build） | M4 |
| 🔨 P1 | Docker / docker-compose 产物 | M9 |
| 🔨 P1 | K8s / Helm / Terraform | M11 |
| 🔨 P1 | OpenTelemetry + 结构化日志 | M10 |
| 👁 P2 | Playwright e2e 生成 / Token Benchmark | M11 |

## 3. 可视化路线（Forma Studio）

可视化与 DSL 同级：**写 `.fm` 必须能被看见**。Studio 是审阅面的图形投影，分三阶段：

**M6 — Studio v1（实时只读预览）**
- `forma studio` 本地 Web 服务；DSL 变更刷新即见
- 实体关系图（ER）：实体/字段/关系/索引，节点点击定位 DSL 源行
- 页面原型：`table/form/detail` 用真实 shadcn 组件渲染（与 M4 emitter 同源组件，非手绘 mockup）
- API 文档视图：Swagger UI 风格浏览（操作/请求/响应/权限）
- 规则矩阵：字段校验/守卫/权限按实体×操作一览
- 验收：仅通过 Studio 即可完整理解一个陌生项目的现状

**M9 — Studio v2（双向编辑）**
- ER 图上增删字段/关系 → 写回 `.fm`（等价于一次语义补丁，走同一校验管线）
- 页面原型上调整列/控件 → 写回 `page` 块
- 诊断点击跳转源码

**M11 — Studio v3（协作）**
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

**M9 工程化 + Studio v2**：Docker、上传/导出/审计、RHF/Zustand、迁移对接、Studio 双向编辑。

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

## 7. 遗留代码处置

`crates/forma`（v0.2 联邦引擎，17 测试绿）：`graph / patch / engine` 模块被 DSL 编译器复用；SQL/OpenAPI/JSON Schema 适配器 → M7 改造为导入器与导出视图；Hono 生成器经验并入 Node emitter。`docs/history/` 只读归档。
