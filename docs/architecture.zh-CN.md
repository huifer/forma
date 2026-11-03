# Forma 架构蓝图 v0.3 — DSL 中间层与多目标编译

> Forma = One DSL. Every platform.
> 一套 DSL 作为软件的中间层与唯一事实来源，适配层（编译器）生成不同语言与平台的目标代码。
> 核心目标与理念见 [philosophy.zh-CN.md](philosophy.zh-CN.md)；完整语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)；路线图见 [roadmap.zh-CN.md](roadmap.zh-CN.md)。历史决策见 [history/](history/README.md)。
>
> **治理契约**：本文件是「详细蓝图伴侣」，定义 crate 级实现细节；系统边界、依赖红线、稳定性分层与 RFC 生命周期由 [架构治理体系](architecture/README.md)（RFC-0001 / RFC-0004 / ADR-0001）统一约束。

---

## 0. 定位与核心目标

Forma 的核心目标：**AI 通过持续生产 DSL，完成一整个软件系统的开发——稳定、优质、持续，而非每次从头对话。**

```text
                    ┌─────────────────────────┐
   自然语言需求 ──▶  │   AI Specification Agent │ ──▶ Forma DSL（.fm，持久状态）
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

   Forma Studio（可视化）与 生态互通（OpenAPI/SQL/Postman…）横向贯穿全部层
```

**本质**：一套 DSL 描述前端 + 后端 + 数据 + 规则；更新 DSL → 执行编译器 → 得到新代码。适配层叫解释器还是编译器并不重要，重要的是：**DSL 是唯一事实来源，目标代码全部是生成物，DSL 本身是随 git 演进的持久状态**。

## 1. 分层模型

| 层 | 职责 | 形态 |
|---|---|---|
| **DSL 层（中间层）** | 实体、枚举、关系、规则、API、页面、权限——纯语义 | `.fm` 文件（`forma/*.fm`），人写 + AI 写，git 版本化 |
| **IR 层** | 语义图（节点/边/影响闭包），与语法解耦 | `UnifiedSemanticGraph` |
| **适配层（Emitter）** | IR → 具体语言/平台的目标代码 | 每个 target 一个 emitter crate |
| **产物层** | 标准工程，可独立运行/测试/部署 | MySQL DDL、Rust crate、React 应用 |
| **可视化层** | DSL 的图形投影（ER/页面原型/API 文档/规则矩阵） | Forma Studio（与编译器同源渲染） |
| **互通层** | 与既有生态双向转换 | OpenAPI / SQL / Postman / Mermaid / JSON Schema 导入导出 |

关键约束：

1. **DSL 不泄漏目标概念**：`.fm` 里没有 axum、没有 React——页面是 `table/form/detail` 抽象原语，组件偏好（shadcn）是 target 配置。
2. **规则一处声明，两端执行**：校验规则编译为后端权威校验（Rust）+ 前端体验校验（zod / TanStack Form）。
3. **目标代码是生成物**：带 `@generated` 头，增量重编译，手改会被缓存检出；自定义逻辑走 `extension` 扩展点。
4. **AI 只面对 DSL**：语义摘要（graph）、影响闭包（impact）、语义补丁（patch）——上下文里没有目标语言源码。

## 2. DSL 设计总纲

完整语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)。核心构造七个：

```fm
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

rules OrderRules {
  field Order.amount {
    min 0.01  { zh "金额必须大于 0" en "Amount must be positive" }
  }
  guard Order.delete {
    deny status == "approved"
  }
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [status] search [customerEmail] sort [createdAt desc] }
  auth { update delete → role in ["admin", "ops"] }
}

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

### 2.1 与 v0.2 标准联邦方案的对比

v0.2 采用「现有标准联邦」（SQL + OpenAPI + JSON Schema）。v0.3 修正为 DSL 中间层，依据：

- **多端投影**：标准组合只解决规格互认；一套语义生成 MySQL/Rust/React/移动端，需要一个统一语义归属地，规则校验尤其如此。
- **Token 密度**：DSL 为 AI 设计——一行一个字段、内联约束、无 import/样板，单一文件可整体进入上下文。
- **LLM 亲和**：语法贴近 Prisma/GraphQL/HCL 公共模式，语法面积小，规则表达式封闭（每个运算符有双端映射）。
- **标准降级为视图**：OpenAPI/JSON Schema/DDL 由 DSL 编译产生；联邦期适配器改造为导入器与导出视图，先期投入不沉没。

## 3. 适配层目标（Targets）

首批三个目标详述如下。完整技术接入矩阵（PostgreSQL / Node / Python / Vue / Expo / Spring Boot / GraphQL / Auth / 能力模块 / 工程化等全部波次）见 [roadmap.zh-CN.md §2](roadmap.zh-CN.md)。

### 3.1 db-mysql（当前）

| 项 | 决策 |
|---|---|
| 引擎 | MySQL 8.x，InnoDB，utf8mb4 |
| 类型映射 | `String(n)`→VARCHAR(n)、`Text`→TEXT、`Int`→BIGINT、`Decimal(p,s)`→DECIMAL(p,s)、`Bool`→TINYINT(1)、`Uuid`→CHAR(36)、`Timestamp`→DATETIME(3)、`Date`→DATE、`Json`→JSON、枚举→VARCHAR(64)+应用层校验（可移植） |
| 命名 | 实体→snake_case 复数表名，字段→snake_case 列名 |
| 迁移 | M2：基线 DDL；M2 后续：IR diff → `ALTER TABLE` 增量迁移 |
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
| 数据 | TanStack Query（每个 api 操作 → typed hooks） |
| UI | shadcn/ui 组件映射：`table`→Table、`form`→Form+Input/Select、`badge`、Dialog 确认；Tailwind |
| 校验 | 规则表达式 → zod schema / TanStack Form validator（与后端同源） |
| 类型 | DSL 实体 → TS types（decimal 以 string 序列化） |
| i18n | DSL 内 message → react-i18next 资源 |
| 产物 | `generated/web/`：`routes/`、`hooks/`、`components/`、`schema/zod.ts` |

### 3.4 mobile-ios-android（后续波次）

页面抽象原语与目标解耦，移动适配层映射为 SwiftUI、Jetpack Compose 或 Expo。规则与 API 客户端同一 DSL 生成。

## 4. 编译器结构（crate 划分）

```
crates/
  forma-syntax    Lexer → Parser → AST（span 保留，供诊断与增量）
  forma-ir        AST → Semantic Graph + 类型检查 + 规则表达式检查
  forma-gen-mysql DDL / 迁移 emitter
  forma-gen-rust  axum 后端 emitter
  forma-gen-web   React emitter
  forma-engine    build 编排 + 增量缓存
  forma-patch     语义补丁（AST 级编辑 → 重编译）
  forma-studio    可视化（ER/页面原型/API 文档/规则矩阵）
  forma-cli       CLI + MCP
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

规则三分类 + 权限，全部声明在 DSL、编译到两端：

| 类别 | DSL | 后端产物（权威） | 前端产物（体验） |
|---|---|---|---|
| 字段校验 | `field Order.amount { min 0.01 "msg" }` | validator 函数 | zod schema / Form validator |
| 跨字段检查 | `check Order "msg" { expr }` | validator 函数 | 表单级校验 |
| 守卫/权限 | `guard Order.delete { deny expr }` | middleware 前置检查 | UI 隐藏按钮 + 禁用态 |
| 状态迁移 | `transition Order.status pending→approved require role` | 状态机执行器 | 可见操作集 |

表达式语言是**极小子集**（每个运算符有 Rust/TS 双向映射表，见 dsl-spec §9），可编译性在 Check 阶段静态保证。

## 6. AI 持续开发工作流

AI 对 Forma 系统的每次会话遵循同一协议，**不从零开始**：

```text
读取现状      forma_graph      （实体级语义摘要，秒级理解系统）
评估影响      forma_impact     （确定性变更闭包）
表达变更      forma_patch      （AST 级语义补丁，事务式）
验证结果      forma_validate   （跨目标诊断）
```

- DSL 即记忆：系统知识全部持久在 `.fm`，git 提交即开发里程碑；会话之间零上下文依赖。
- AI 输出永远是语义补丁（数十 Token），目标代码由编译器生成——AI 不接触目标语言源码。
- MCP 工具（validate / graph / impact / patch）语义不变，数据源切换为 DSL。

## 7. Studio 可视化层

可视化与编译器同源：Studio 不维护第二套模型，直接消费 IR 渲染。

| 视图 | 内容 | 数据来源 |
|---|---|---|
| 实体关系图 | 实体/字段/关系/索引，节点联动 DSL 源行 | IR entity/relation 节点 |
| 页面原型 | `table/form/detail` 用真实 shadcn 组件渲染（预览即最终形态） | IR page 节点 + web emitter 组件 |
| API 文档 | Swagger UI 风格（操作/请求/响应/权限） | IR api 节点 + OpenAPI 导出视图 |
| 规则矩阵 | 字段校验/守卫/权限按实体 × 操作一览 | IR rules 节点 |
| 影响分析图 | 语义补丁的可视化评审 | IR diff |

阶段：v1 只读实时预览（M6）→ v2 双向编辑写回 `.fm`（M9）→ v3 协作（M11）。详见 roadmap §3。

### 7a. 可视化规格与渲染系统（RFC-0001，V0.1 已落地）

Studio 是「用 IR 渲染给人看」；本节是「把图表/看板作为软件能力编译出来」——两者同源但方向不同。

```text
DSL chart/dashboard 块 → Chart IR / Dashboard IR → Adapter（ECharts…）→ Runtime（React）
```

| 层 | 位置 | 职责 |
|---|---|---|
| Specification | `dsl/parser.rs`（`chart` 块） | 语义图表类型 + 编码 + 变换，无任何图表库词汇 |
| Chart IR | `visualization/` | 形状家族推导、变换计划、形状级校验 |
| Adapter | `visualization/echarts.rs` | 纯函数 IR → option 骨架（确定性投影） |
| Runtime | `emit/visualization.rs` + 模板 | 取数 → 变换 → 整形 → 渲染 → 事件 → 响应式 |
| Check | `dsl/mod.rs` + `visualization/validate.rs` | `viz-*` 诊断码，`forma build` 硬失败 |

设计原则：DSL 不泄漏 ECharts 概念；变换语义属于 IR（AntV/D3 适配器复用同一执行器）；
`nativeExtension` 提供受控逃逸口。全文见 [docs/architecture/rfc/RFC-0010-visualization.zh-CN.md](docs/architecture/rfc/RFC-0010-visualization.zh-CN.md)。

## 8. 生态互通层

与历史优质项目双向结合（完整表见 roadmap §4）：

| 方向 | 项目 |
|---|---|
| 导出 | OpenAPI 3.1 + Swagger UI · Postman Collection · Mermaid（ER/流程图）· JSON Schema · DBML |
| 导入 | OpenAPI → 实体+API 骨架 · SQL DDL → 实体（逆向） · Prisma schema · JSON Schema |
| 对接 | SQL 迁移目录约定（Flyway / Liquibase / golang-migrate） · VS Code 扩展 |

互通产物与目标产物同级：同样是 IR 的确定性投影，带版本与 `@generated` 语义。

## 9. 与 v0.2 的差异对照

| v0.2 标准联邦 | v0.3 DSL 中间层 |
|---|---|
| 现有标准文件是事实来源 | **DSL 是唯一事实来源**；标准是生成视图与导入源 |
| 适配器 = 解析标准 | 适配层 = 从 IR **生成**目标代码（方向反转） |
| 单一 Web 骨架目标 | 多目标矩阵：MySQL / Rust / React(+)，移动端预留 |
| 规则无归属 | 规则一等公民，双端同源编译 |
| patch = 原位改标准文件 | patch = AST 语义编辑 |
| 无可视化、无互通设计 | **Studio 一等公民 + 生态互通层** |
| 保留 | 语义图、doctor 思想、增量编译、事务式 patch、MCP 协议、Token 经济学 |

## 10. 里程碑

M1 DSL 内核 → M2 db-mysql → M3 backend-rust → M4 web-react → M5 Auth → M6 Studio v1 → M7 生态互通 → M8 AI 补丁闭环 → M9 工程化+Studio v2 → M10/M11 扩栈与企业级。完整验收标准见 [roadmap.zh-CN.md](roadmap.zh-CN.md)。
