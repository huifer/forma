# Forma 架构蓝图 v0.3 — DSL 中间层与多目标编译

> Forma = One DSL. Every platform.
> 一套 DSL 作为软件的中间层与唯一事实来源；通过适配层（编译器）生成不同语言与平台的目标代码。
> 本蓝图取代 v0.2「标准联邦」方案（历史决策见 [history/](history/README.md)）。

---

## 0. 定位

```
                    ┌─────────────────────────┐
   自然语言需求 ──▶  │   AI Specification Agent │ ──▶ Forma DSL（.fm）
                    └─────────────────────────┘         ▲ 唯一事实来源
                                                        │
   ┌──────────────────── Forma Compiler (Rust) ─────────┴──────────────┐
   │  Lexer → Parser → AST → Lower → IR(Semantic Graph) → Check        │
   │                                      │                            │
   │             ┌────────────────────────┼────────────────────────┐  │
   │             ▼                        ▼                        ▼  │
   │      ┌─────────────┐         ┌─────────────┐         ┌──────────┐ │
   │      │  db-mysql   │         │ backend-rust│         │ web-react│ │
   │      │ DDL+迁移    │         │ axum + sqlx │         │ TanStack │ │
   │      └──────┬──────┘         └──────┬──────┘         │ + shadcn │ │
   │             │                       │                └────┬─────┘ │
   └─────────────┼───────────────────────┼─────────────────────┼───────┘
                 ▼                       ▼                     ▼
            MySQL Schema          Rust Backend          React Web App
                                                        （iOS/Android → 后续适配层）
```

**本质**：一套 DSL 描述前端 + 后端 + 数据 + 规则；更新 DSL → 执行编译器 → 得到新代码。适配层是「解释器还是编译器」并不重要，重要的是：**DSL 是唯一事实来源，目标代码全部是生成物**。

## 1. 分层模型

| 层 | 职责 | 形态 |
|---|---|---|
| **DSL 层（中间层）** | 实体、枚举、关系、规则、API、页面、权限——纯语义 | `.fm` 文件（`forma/*.fm`），人写 + AI 写 |
| **IR 层** | 语义图（节点/边/影响闭包），与语法解耦 | `UnifiedSemanticGraph`（已有实现复用） |
| **适配层（Emitter）** | IR → 具体语言/平台的目标代码 | 每个 target 一个 emitter crate |
| **产物层** | 标准工程，可独立运行/测试/部署 | MySQL DDL、Rust crate、React 应用 |

关键约束：

1. **DSL 不泄漏目标概念**：`.fm` 里没有 axum、没有 React——页面是 `table/form/detail` 抽象原语，组件偏好（shadcn）是 target 配置。
2. **规则一处声明，两端执行**：校验规则编译为后端权威校验（Rust）+ 前端体验校验（zod / TanStack Form）。
3. **目标代码是生成物**：带 `@generated` 头，增量重编译，禁止手改（手改会被缓存检出）。
4. **AI 只面对 DSL**：语义摘要（graph）、影响闭包（impact）、语义补丁（patch）——上下文里没有目标语言源码。

## 2. DSL 设计总纲

完整语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)。核心构造七个：

```fm
// forma/app.fm —— 一个文件看懂全部层
app crm { version = "0.1.0" }

enum OrderStatus { pending approved rejected }

entity Order {
  id            Uuid          @id @default(uuid)
  customerEmail Email         @required
  amount        Decimal(12,2) @required @min(0.01)
  status        OrderStatus   @default(pending)
  createdAt     Timestamp     @default(now) @readonly
  index         [status, createdAt]
}

// ── 规则校验：写一次，后端权威执行 + 前端即时反馈 ──
rules OrderRules {
  field Order.amount {
    min 0.01  "金额必须大于 0"
  }
  check Order "已审批订单金额不可再修改" {
    guard on update
    deny old.status == "approved"
  }
}

// ── API：声明式 CRUD + 命令 ──
api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [status] search [customerEmail] sort [createdAt desc] }
  auth { update delete → role in ["admin", "ops"] }
}

// ── 页面：抽象原语（table/form/detail），适配层映射为 shadcn/ui ──
page OrderList {
  route "/orders"
  title "订单列表"
  table {
    query Orders.list
    columns [createdAt: datetime, customerEmail, amount: money, status: badge]
    filter status select
    actions [create → OrderEdit, delete confirm]
  }
}
```

### 2.1 为什么 DSL 而不是继续标准联邦（v0.2 → v0.3 决策变更）

- **中间层价值**：一次语义变更 → 所有目标同步。标准联邦只解决「规格互认」，不解决「一套语义生成多端」。
- **Token 密度**：DSL 为 AI 而设计——一行一个字段、内联约束、无 import/样板，比 SQL+OpenAPI+JSON Schema 的组合更稠密。
- **LLM 亲和**：语法刻意贴近 Prisma/GraphQL/HCL 的公共模式（模型熟），语法面积小、可整文件进上下文、diff 即补丁。
- **规则一等公民**：跨端校验在标准体系里没有归属，在 DSL 里是核心构造。

### 2.2 与标准的关系（不废弃，降级为视图）

标准产物（`openapi.yaml`、JSON Schema、SQL DDL）变成**生成视图**：由 DSL 编译产生，供生态工具消费（Swagger UI、数据库工具、第三方客户端）。联邦时期的适配器代码改造为 importer/exporter，不删除。

## 3. 适配层目标（Targets）

> 首批三个目标详述如下。**完整技术接入矩阵**（PostgreSQL / Node / Python / Vue / Expo / Spring Boot / GraphQL / Auth / 能力模块 / 工程化等全部波次）见 [roadmap.zh-CN.md §2](roadmap.zh-CN.md)。

### 3.1 db-mysql（当前）

| 项 | 决策 |
|---|---|
| 引擎 | MySQL 8.x，InnoDB，utf8mb4 |
| 类型映射 | `String(n)`→VARCHAR(n)、`Text`→TEXT、`Int`→BIGINT、`Decimal(p,s)`→DECIMAL(p,s)、`Bool`→TINYINT(1)、`Uuid`→CHAR(36)、`Timestamp`→DATETIME(3)、`Date`→DATE、`Json`→JSON、枚举→VARCHAR(64)+应用层校验（可移植） |
| 命名 | 实体→snake_case 复数表名，字段→snake_case 列名 |
| 迁移 | M1：基线 DDL；M2：IR diff → `ALTER TABLE` 增量迁移 |
| 产物 | `generated/db/migrations/0001_init.sql` |

### 3.2 backend-rust（当前）

| 项 | 决策 |
|---|---|
| Web 框架 | axum（框架名进 target 配置，emitter 可替换） |
| 数据访问 | sqlx（MySQL，编译期检查查询） |
| 类型 | 每实体：struct + serde；枚举：Rust enum + serde |
| 校验 | 规则表达式 → Rust 校验函数（权威执行层） |
| API 形态 | REST：CRUD 由 `api` 块生成 handler + repository；命令类生成专用 endpoint |
| 产物 | `generated/backend/`：`routes.rs`、`models.rs`、`validators.rs`、`repo.rs` |

### 3.3 web-react（当前）

| 项 | 决策 |
|---|---|
| 路由 | TanStack Router（`page.route` → 类型安全路由树） |
| 数据 | TanStack Query（每个 api 操作 → typed hooks：`useOrdersList` / `useOrderCreate`…） |
| UI | shadcn/ui 组件映射：`table`→Table、`form`→Form+Input/Select、`badge`、`Dialog` 确认；Tailwind |
| 校验 | 规则表达式 → zod schema / TanStack Form validator（与后端同源） |
| 类型 | DSL 实体 → TS types（decimal 以 string 序列化，避免浮点误差） |
| 产物 | `generated/web/`：`routes/`、`hooks/`、`components/`、`schema/zod.ts` |

### 3.4 mobile-ios-android（Phase 2+）

页面抽象原语（list/form/detail/dashboard）与目标解耦，移动适配层可映射为 Swift/SwiftUI、Kotlin/Compose，或 Expo。规则与 API 客户端同一 DSL 生成。

## 4. 编译器结构（crate 划分）

```
crates/
  forma-syntax    Lexer → Parser → AST（span 保留，供诊断与增量）
  forma-ir        AST → Semantic Graph + 类型检查 + 规则表达式检查
  forma-gen-mysql DDL / 迁移 emitter
  forma-gen-rust  axum 后端 emitter
  forma-gen-web   React emitter
  forma-engine    build 编排 + 增量缓存（复用现有 .forma/build.json 思路）
  forma-patch     语义补丁（AST 级编辑 → 重编译）
  forma-cli       CLI + MCP（graph/impact/patch 语义不变，数据源换为 DSL）
```

编译阶段（每次 `forma build`）：

```
1. 解析所有 .fm → AST        （语法错误即停）
2. Lower → IR 语义图          （实体/字段/规则/API/页面全部成节点）
3. Check：类型、引用、规则表达式可编译性、路由冲突、权限完整性
4. 影响分析：变更 IR 节点 Hash → 受影响目标文件
5. Emit：仅重生成受影响文件
6. 产物校验：生成 SQL 可执行、Rust 可编译（可选 CI 钩子）
```

## 5. 规则校验设计（一等公民）

规则三分类，全部声明在 DSL、编译到两端：

| 类别 | DSL | 后端产物（权威） | 前端产物（体验） |
|---|---|---|---|
| 字段校验 | `field Order.amount { min 0.01 "msg" }` | validator 函数 | zod schema / Form validator |
| 跨字段检查 | `check Order "msg" { expr }` | validator 函数 | 表单级校验 |
| 守卫/权限 | `guard Order.delete { deny expr }` | middleware/handler 前置检查 | UI 隐藏按钮 + 禁用态 |
| 状态迁移 | `transition Order.status pending→approved require role` | 状态机执行器 | 可见操作集 |

表达式语言是**极小子集**（每个运算符有 Rust/TS 双向映射表，见 dsl-spec §6）：比较、逻辑、算术、`in`、`matches`、聚合（`sum/count/exists`，Phase 2）。表达式的可编译性在 Check 阶段静态保证——**不存在「规则在 Rust 能跑在 TS 跑不了」的情况**。

## 6. AI 工作流（不变，数据源更换）

```
forma_graph → forma_impact → forma_patch → forma_validate
```

MCP 四工具语义不变；patch 从「跨标准文件编辑」升级为「AST 级语义编辑 → 重编译」。Token 经济学不变：AI 上下文 = 语义摘要；输出 = DSL 补丁；语法展开 = 编译器免费完成。

## 7. 与 v0.2 的差异对照

| v0.2 标准联邦 | v0.3 DSL 中间层 |
|---|---|
| 现有标准文件是事实来源 | **DSL 是唯一事实来源**；标准降级为生成视图 |
| 适配器 = 解析标准 | 适配层 = 从 IR **生成**目标代码（方向反转） |
| 一个 Web 目标（Hono/React 骨架） | 多目标矩阵：MySQL / Rust / React(+)，移动端预留 |
| 规则无归属 | 规则一等公民，双端同源编译 |
| patch = 原位改标准文件 | patch = AST 语义编辑 |
| 保留 | 语义图、doctor 思想、增量编译、事务式 patch、MCP 协议、Token 经济学 |

## 8. 里程碑

| 里程碑 | 内容 | 验收 |
|---|---|---|
| **M1 DSL 内核** | syntax（lexer/parser/AST）+ IR + Check + `forma check` | 示例 .fm 全量通过语义检查，错误带行号 |
| **M2 db-mysql** | DDL emitter + 基线迁移 | DSL → MySQL 8 可执行 DDL |
| **M3 backend-rust** | models + validators + axum routes + sqlx repo | `cargo build` 可编译、`cargo test` 跑通 CRUD |
| **M4 web-react** | TanStack Router/Query + shadcn 页面 + zod | `pnpm dev` 可跑通列表/表单/详情 |
| **M5 语义补丁** | patch 操作覆盖实体/字段/规则 → 全目标增量重编 | 「加字段」一条补丁四处生效 |
| **M6 标准视图** | openapi/json-schema 导出 | 与联邦产物互通 |
| **M7+** | 迁移 diff、规则聚合表达式、移动适配层、Studio | — |
