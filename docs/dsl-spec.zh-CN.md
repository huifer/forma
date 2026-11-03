# Forma DSL 语言规范 v0.1

> Forma DSL（`.fm` 文件）是软件的中间层：一套 DSL 描述数据、规则、API 与页面，由适配层编译为 Rust 后端、MySQL、React Web（后续 iOS/Android）。
> 本文是语法的权威定义。设计动机见 [architecture.zh-CN.md](architecture.zh-CN.md) §2。

---

## 1. 设计原则

1. **纯语义**：不含任何目标语言概念（无 axum/React/SQL 词汇）。
2. **稠密**：一行表达一个完整事实（字段 + 约束 + 文案），无 import、无样板。
3. **LLM 亲和**：语法刻意复用 Prisma / GraphQL / HCL 的公共模式；单一文件可整体进上下文；行式结构 diff 即补丁。
4. **可校验**：所有引用（实体/枚举/API/页面）在 Check 阶段静态解析；规则表达式有封闭的运算符集，保证双端可编译。
5. **中文友好**：字符串与 message 原生支持 UTF-8；校验消息即 i18n 载体。

## 2. 词法

- 标识符：`[A-Za-z_][A-Za-z0-9_]*`（区分大小写）
- 字符串：`"..."`，支持 `\n \" \\` 与 UTF-8 原文
- 数字：整数、小数；`Decimal` 字面量禁止科学计数法歧义
- 注释：`//` 行注释、`/* */` 块注释
- 文件：UTF-8，扩展名 `.fm`，目录 `forma/`，可多文件（编译器按 `forma.yaml` 的 `source` glob 收集）

## 3. 顶层构造

```ebnf
file        = { app | enum | entity | rules | api | page | chart | config } ;
```

八个构造一节一个；全部可跨文件引用（按标识符全局解析）。

## 4. app

```fm
app crm {
  version = "0.1.0"
  locale  = ["zh", "en"]        // 可选：i18n 主语言列表
}
```

## 5. 类型系统

| 类别 | 类型 | 说明 |
|---|---|---|
| 文本 | `String` | 短文本；`String(n)` 限长 |
| 文本 | `Text` | 长文本 |
| 文本 | `Email` `Url` `Phone` | 语义别名：String + 内置 pattern 校验 |
| 数值 | `Int` `Float` `Decimal(p,s)` | 金额一律 `Decimal`（跨端以字符串序列化，杜绝浮点误差） |
| 布尔 | `Bool` | |
| 时间 | `Timestamp` `Date` | 带时区的时间戳 / 纯日期 |
| 标识 | `Uuid` | |
| 结构 | `Json` | 自由结构（Phase 2 支持 schema 化） |
| 枚举 | 用户 `enum` | |
| 引用 | 实体名 | 关系字段（见 §7） |

可选性：类型后缀 `?`（如 `Text?`）。

## 6. enum

```fm
enum OrderStatus { pending approved rejected }

enum Priority {
  low    @label { zh "低" en "Low" }
  medium @label { zh "中" en "Medium" }
  high   @label { zh "高" en "High" }
}
```

- 产物映射：MySQL `VARCHAR(64)` + 应用层校验；Rust `enum` + serde；TS 字符串联合类型。

## 7. entity（数据层）

```fm
entity Order {
  id            Uuid          @id @default(uuid)
  customerEmail Email         @required
  amount        Decimal(12,2) @required @min(0.01)
  discount      Decimal(5,2)?                 // `?` = 可空
  status        OrderStatus   @default(pending)
  note          Text?
  createdAt     Timestamp     @default(now) @readonly
  updatedAt     Timestamp     @default(now) @onUpdate(now)

  index [status, createdAt]                    // 复合索引
}

entity OrderItem {
  id      Uuid  @id @default(uuid)
  order   Order @required                      // 关系字段 → FK
  product Product @required
  qty     Int   @required @min(1)
  price   Decimal(12,2) @required
}
```

### 字段属性（attributes）

| 属性 | 含义 | MySQL 侧 | Rust 侧 | TS 侧 |
|---|---|---|---|---|
| `@id` | 主键 | PRIMARY KEY | 主键字段 | — |
| `@required` | 必填 | NOT NULL | 非.Option | required |
| `@unique` | 唯一 | UNIQUE | 查重校验 | — |
| `@default(v)` | 默认值（`uuid` / `now` / 字面量） | DEFAULT | 初始化 | defaultValue |
| `@min(v)` `@max(v)` | 数值/日期界 | （应用层） | validator | zod |
| `@length(min,max)` | 文本长度 | VARCHAR 尺寸 | validator | zod |
| `@pattern(re)` | 正则 | — | validator | zod |
| `@indexed` | 单列索引 | INDEX | — | — |
| `@readonly` | 创建后不可改 | — | update 白名单排除 | 表单只读 |
| `@hidden` | 不出现在 API/表单 | — | 序列化跳过 | 不渲染 |
| `@onUpdate(now)` | 更新时间戳 | ON UPDATE 语义（应用层） | 更新钩子 | — |

### 关系

- 关系字段：字段类型 = 另一实体 → 编译器派生 FK（`order` → `order_id` → `orders.id`）。
- 反向集合：`items List<OrderItem>`（声明在「一」侧；不产生列，产生查询）。
- Check 阶段校验：FK 目标必须 `@id`；循环关系允许但需显式 `@onDelete(cascade|restrict)`。

## 8. rules（规则校验，一等公民）

四类规则，全部编译到**后端（权威）+ 前端（体验）**：

```fm
rules OrderRules {

  // 1) 字段校验：绑定到字段，进表单与 API 校验
  field Order.amount {
    min 0.01  { zh "金额必须大于 0" en "Amount must be positive" }
  }
  field Order.customerEmail {
    required  { zh "邮箱必填" en "Email is required" }
  }

  // 2) 跨字段检查：表达式规则
  check Order "折扣不能超过金额" {
    discount == null || discount <= amount
  }
  check Order "明细合计必须等于订单金额" {
    sum(items[].price * items[].qty) == amount      // 聚合：Phase 2
  }

  // 3) 守卫：命令前置条件（删除/更新保护、权限）
  guard Order.delete {
    deny status == "approved"  reason { zh "已审批订单不可删除" en "Approved orders cannot be deleted" }
  }
  guard Order.update {
    deny old.status == "approved" && old.status != status
  }

  // 4) 权限：角色规则（RBAC）
  access Order {
    read   → any
    create → role in ["admin", "ops"]
    update → role in ["admin", "ops"] && status != "approved"
    delete → role == "admin"
  }
}
```

- `old.` 前缀引用更新前的值（后端直取；前端仅提示）。
- `reason` 是面向用户的 i18n 消息（API 400 响应体 + 表单错误文案）。

## 9. 表达式语言（封闭子集）

每个运算符有 Rust 与 TS 的 1:1 映射；Check 阶段做类型检查，**不可能生成单端无法编译的规则**。

| 类别 | 语法 | Rust | TS/zod |
|---|---|---|---|
| 字面量 | `42 3.14 "text" true null` | 同 | 同 |
| 字段引用 | `amount`、`order.customer.id` | `self.amount` / 关联查询 | `values.amount` |
| 旧值 | `old.status` | `before.status` | —（提示用） |
| 比较 | `== != < <= > >=` | 同 | 同 |
| 逻辑 | `&& \|\| !` | 同 | 同 |
| 算术 | `+ - * /` | 同（Decimal 用 rust_decimal） | decimal.js |
| 成员 | `x in ["a","b"]` | `matches!` / `.contains()` | `.includes()` |
| 字符串 | `contains` `startsWith` `matches(re)` | 同名方法 | 同名方法 |
| 聚合（P2） | `sum(expr) count(xs) exists(xs where …)` | iterator | array reduce |

## 10. api（API 层）

```fm
api Orders {
  entity Order
  operations [list get create update delete]      // 声明式 CRUD
  list {
    filters [status]                              // 精确过滤
    search  [customerEmail]                       // contains 搜索
    sort    [createdAt desc, amount]
    paginate cursor                                // 或 offset
  }
  auth {
    create update delete → role in ["admin", "ops"]
  }
}

api OrderApprove {
  POST "/orders/{id}/approve"                      // 命令类操作
  input {
    comment Text?
  }
  pre  { status == "pending" }                     // 前置条件（复用表达式）
  effect {
    set Order.status = approved
  }
  audit { action "order.approve" }                 // 审计日志钩子
}
```

产物映射（backend-rust）：`api` → axum `Router` + handler + repository 方法 + 请求校验；`POST` 命令 → 专用 endpoint。产物映射（web-react）：每个操作 → TanStack Query hook + 类型化 fetch client。

产物映射（backend-rust）：`api` → axum `Router` + handler + repository 方法 + 请求校验；`POST` 命令 → 专用 endpoint。产物映射（web-react）：每个操作 → TanStack Query hook + 类型化 fetch client。

## 11a. chart（可视化层，RFC-0001）

顶层 `chart` 块描述一个可视化组件的**语义规格**——DSL 里没有 ECharts、没有 xAxis。规格编译为 Chart IR，再由适配层投影为目标图表库（当前 ECharts）：同一 IR 两次编译产物 byte 级一致。

```fm
chart EnrollmentTrend {
  type  line                                     // line|bar|area|pie|scatter|funnel|gauge|radar|heatmap|treemap
  title { zh "每日报名趋势" en "Daily enrollments" }

  source Enrollments.list                        // 数据绑定：已声明的 api 块.操作

  encoding {
    x      { field enrolledAt type temporal format datetime }   // temporal|quantitative|nominal|ordinal
    y      { field value       type quantitative format integer }
    series { field status     type nominal }       // 可选：多系列分组
  }

  transform {
    filter    { field status eq "active" }        // eq|ne|gt|gte|lt|lte|in
    bucket    { field enrolledAt by day }         // hour|day|week|month|year
    aggregate { op count groupBy [enrolledAt, status] }   // sum|avg|count|min|max
    sort      { by enrolledAt desc }
    limit     30
  }

  axis {
    x { label "报名时间" type datetime }
    y { label "报名数"   format integer }
  }

  tooltip     { enabled true }
  legend      { enabled true }
  interaction { zoom true }
  appearance  { smooth true showSymbol false }
}

page EnrollmentDashboard {
  route "/enrollments/dashboard"
  title { zh "报名分析" en "Enrollment analytics" }
  chart EnrollmentTrend                          // 页面挂载图表
}
```

要点：

- **聚合输出字段**：`transform.aggregate` 产生名为 `value` 的数值字段，编码通道可直接引用（`y { field value ... }`）；`op count` 不带 `field`，其余聚合必须带。
- **数据键**：编码字段在运行时的键是 snake_case 列名；关系字段取显示名列 `{rel}_name`（与后端 ListRow 契约一致）。
- **语义校验**：`viz-unknown-type/api/operation/field`、`viz-type-mismatch`、`viz-shape-mismatch`、`viz-aggregate-conflict`、`viz-duplicate`——全部在 Check 阶段硬失败（`forma build` 直接报错）。
- **产物**（web-react）：`src/viz/spec.ts` + `transform.ts`（适配器无关的类型与变换执行器）、`echarts.ts`（按需注册本工程用到的图表/组件）、`runtime.tsx`（`<FormaChart>`）、`src/viz/charts/<Name>.ts`（Spec/Option/Source 三件套）。无 `chart` 块的工程不产出任何 viz 文件、不引入 echarts 依赖。
- **MCP**：`forma_viz_catalog`（能力词表）、`forma_viz_describe`（单类型规格）、`forma_viz_validate`（校验 + 图表语义摘要）。

设计全文与 Dashboard IR（V0.3）见 [RFC-0001：可视化规格与渲染系统](architecture/rfc/RFC-0010-visualization.zh-CN.md)。

## 11b. page（UI 层，目标无关原语）

四种**抽象原语**：`table`（列表）、`form`（编辑/创建）、`detail`（详情）、`dashboard`（P2）。适配层把原语映射为 shadcn/ui 组合；移动适配层把同一原语映射为原生列表/表单。

```fm
page OrderList {
  route "/orders"
  title { zh "订单列表" en "Orders" }

  table {
    query  Orders.list
    columns [createdAt: datetime, customerEmail, amount: money, status: badge]
    filter  status as select
    search  customerEmail
    actions {
      create → OrderEdit
      row    [edit → OrderEdit, delete → Orders.delete confirm]
    }
  }
}

page OrderEdit {
  route "/orders/{id}"
  title { zh "编辑订单" en "Edit Order" }

  form Order {
    layout grid 2 {
      customerEmail Input email
      amount        Input money
      status        Select from OrderStatus
      note          Textarea
    }
    validation [OrderRules]              // 前端校验与后端同源生成
    submit → Orders.update redirect "/orders"
    submit create → Orders.create redirect "/orders"
  }
}

page OrderDetail {
  route "/orders/{id}"
  detail Order {
    sections {
      基本 [customerEmail, amount, status, createdAt: datetime]
    }
    children [items → OrderItemList]
  }
}
```

- `component` 提示：`Input email`、`badge`、`money`、`datetime`、`select` —— 语义级控件类型，适配层映射为具体组件（web → shadcn Input/Badge/…）。
- 权限联动：`guard/access` 规则自动控制按钮显隐与路由守卫。

## 12. forma.yaml（项目清单）

```yaml
project: { name: crm, version: "0.1.0" }

source: ["forma/**/*.fm"]          # DSL 收集 glob

targets:
  db:
    engine: mysql                  # mysql 8.x
    out: generated/db
  backend:
    language: rust
    framework: axum                # emitter 可替换
    database: sqlx
    out: generated/backend
  web:
    framework: react
    router: tanstack
    data: tanstack-query
    ui: shadcn
    out: generated/web
  # mobile: { later }
```

只做三件事：收集 DSL、声明目标、给出输出路径。**不含业务语义**（业务语义只存在于 .fm）。

## 13. 错误与诊断

- 语法错误：`error[E1001] forma/order.fm:12:5 expected '}'`
- 语义错误：`error[E2003] forma/order.fm:30 unknown entity "Prodct"`（含 did-you-mean）
- 规则错误：`error[E3002] rule expression uses aggregate 'sum' (Phase 2, not yet compilable)`
- 诊断结构统一：`{ code, file, line, col, message, hint }`——同一结构喂给 CLI（人类）与 MCP（AI）。

## 14. 示例全集

见 `examples/order-management/forma/*.fm`（M1 起随编译器演进）。
