---
id: spec:protocols
title: API and UI Protocol Specification
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-05
domain: specification
governing_rfc: RFC-0002
depends_on: [RFC-0001]
related_adrs: [ADR-0004]
related_specs: [spec:ir, spec:types, spec:serialization]
breaking_change: false
---

# spec:protocols — API 与 UI 协议规范

> **规范性**：本规范定义 `api` / `page` / 命令类操作的**协议语义**。
> 治理依据：[RFC-0002 规格体系](../../rfc/RFC-0002-specification-system.zh-CN.md) · [RFC-0005 应用运行时](../../rfc/RFC-0005-application-runtime.zh-CN.md) · [ADR-0004 权限服务端权威](../../adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md)。

---

## 1. 范围

| 构造 | 协议面 |
|---|---|
| `api` 的 `operations` | 操作集合与端点形状 |
| `list` 的 filters / search / sort / paginate | 查询协议与参数形状 |
| `api { auth }` | 权限表达与执行位置 |
| 命令类操作 | pre → effect → audit 执行协议 |
| `page` 的 table / form / detail / chart | UI 原语协议与路由 |

## 2. 操作协议

### 2.1 操作集合

- `api` 块 **MUST** 声明其 `entity` 与 `operations`。
- 操作集合：`list` `get` `create` `update` `delete`，以及**命令类操作**（见 §5）。
- 生成器 **MUST** 按 `operations` **裁剪**产出（路由 / 仓储 / 校验器 / 前端 hooks）。
  未声明的操作 **MUST NOT** 出现在产物中——否则产生 dead code 与虚假的能力面。

### 2.2 端点形状

| 操作 | 方法 | 路径 | 输入 | 输出 |
|---|---|---|---|---|
| list | GET | `/{entities}` | 查询参数（filter/search/sort/page） | 列表 + `ListRow` |
| get | GET | `/{entities}/{id}` | 路径参数 | 实体 |
| create | POST | `/{entities}` | Create 输入体 | 新实体 |
| update | PUT/PATCH | `/{entities}/{id}` | Update 输入体 | 更新后实体 |
| delete | DELETE | `/{entities}/{id}` | 路径参数 | 空 / ok |

- 路径参数为**文本 UUID**（`Path(id): Path<String>`，见 [spec:serialization §4](../serialization/serialization.zh-CN.md)）。

## 3. 查询协议

### 3.1 过滤

- `list { filters [status] }` → 查询参数 `?status=<value>`；多值以逗号分隔（**IN** 语义）。
- 过滤列对应**关系列** `{rel}_id`（如 `?school=<id>`）。

### 3.2 搜索

- `list { search [customerEmail, name] }` → 查询参数 `?q=<term>`。
- 多搜索字段为 **OR** 语义。
- **MUST** 使用 `COALESCE(col LIKE CONCAT('%', ?, '%'), 0) = 1` 形式（NULL 列**不得**吞掉整行）——见 [spec:serialization §6](../serialization/serialization.zh-CN.md)。

### 3.3 排序

- `list { sort [createdAt desc] }` → 多字段**逐项**生成 `ORDER BY`（不是只取首个）。
- 方向：`asc` / `desc`。

### 3.4 分页

- `paginate` 的**页码**形式已支持（`?page=…&page_size=…`）。
#### 3.4.1 游标分页契约（已实现）

`paginate cursor` 的**契约**（实现时必须遵守）：

- **响应信封**：游标分页的 list **MUST** 返回 `{ "items": [...], "nextCursor": string | null }`
  ——游标语义需要 next 指针，裸数组无法承载。该形状变化**仅影响**声明了
  `paginate cursor` 的 api，其余 api 保持裸数组（**不得**全局改形状）。
- **keyset 判定**：游标 **MUST** 编码「最后一行的全部排序键值 + id」，wire 形式为不透明 JSON 字符串
  `{"values":[...],"id":"..."}`。SQL **MUST** 使用 lexicographic seek：
  每个排序键生成「前序键相等 AND 当前键按方向比较」分支，最后追加 id tie-breaker；
  `ORDER BY` **MUST** 与 DSL 声明顺序/方向一致，`id` 自动按最后一个排序键的方向追加。
- **编译期约束**（不满足即报错，**MUST NOT** 静默降级为 offset）：
  - `list.sort` **MUST** 至少声明一列；支持多列（逐列 keyset），重复列**MUST**报错。
  - 所有排序列 **MUST** 非空且可比较；每列类型都必须在 cursor 编解码支持集合内。
  - 跨关系排序列**不支持**。
- **前端**：声明游标分页的 api，页面 **MUST** 用 `useInfiniteQuery` + 「加载更多」；
  选项查询（`XApi.list({ pageSize: 200 })`）**MUST** 改读信封的 `items`。

#### 3.4.2 offset 分页（现状）

- **pageSize** 默认 20，范围 clamp 为 1–200；后端每次取 `pageSize + 1` 行判断 `nextCursor`。无更多结果时 `nextCursor: null`。

## 4. 鉴权协议

### 4.1 两层声明

| 形式 | 粒度（按什么声明） | 语法 |
|---|---|---|
| `api { auth { ... } }` | **操作级**（按 api） | `update delete → role in ["admin", "ops"]` |
| `rules { access { ... } }` | **实体级**（按实体） | `read → any` / `create → role in [...]` / `delete → role == "admin"` |

- 两层**可叠加**：同一操作同时受两者约束时，**两者都需满足**。
- `read → any` 同时覆盖 `list` 与 `get`。
- 条件表达式支持**角色级 + 行级**：
  - 角色级：`any` / `role in [...]` / `role == "..."`；
  - 行级：`&& <字段比较>`（需实体行 → 仅 `get` / `update` / `delete` 可用）。

### 4.2 执行语义（服务端权威）

- 权限 **MUST** 由**服务端权威执行**；前端按同一权限集隐藏/禁用操作**不构成安全边界**（[ADR-0004](../../adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md)）。
- 前端**MAY** 做角色感知（体验层）：生成的 Web 在**已知角色不足**时隐藏对应操作；
  角色未知（未登录 / 加载中）时**不隐藏**——宁可让服务端判定，也不误伤。
  行级条件无法在前端判定，仍由服务端返回 403。
- 未登录 → **401**（缺失/无效 token）。
- 已登录但角色不足 → **403**。
- 判定发生在**参数校验之前**（不向未授权调用方泄露校验细节）；
  行级条件**必须**先取到实体行再判定（取行 → 授权 → 校验 → 变更）。
- 字段级可见性与行级可见性遵循同一原则：过滤**MUST** 在数据访问层完成。

### 4.3 行级条件的实现约定

- 行级条件**MUST** 在取到实体行之后判定（`get` / `update` / `delete`）。
- Timestamp / Date **仅支持同类型字段对字段比较**；时间字符串字面量、Timestamp↔Date 混比编译期报 `dsl-access-temporal-type-mismatch`。
- Option 字段**MUST** 以「克隆后取兜底值」参与比较——直接 `unwrap_or` 会移动出实体行。
- 数值字面量**MUST** 按字段类型渲染（`Decimal` 需显式类型，否则整数字面量类型不明确）。
- 行级条件**MUST NOT** 出现在无实体行的操作上（`create` / `list`）。

### 4.4 统一错误响应体契约（Runtime Contract · RFC-0005 E1–E5）

**所有后端错误同形**（E4）：

```json
{
  "error": "Validation failed",
  "code": "FORMA_VALIDATION_FAILED",
  "fields": [{ "field": "email", "message": "邮箱必填" }],
  "requestId": "…（预留，当前不生成）"
}
```

| 错误码 | HTTP | 说明 |
|---|---|---|
| `FORMA_VALIDATION_FAILED` | 400 | 参数 / 规则校验失败（带 `fields`） |
| `FORMA_UNAUTHORIZED` | 401 | 缺失 / 无效凭据 |
| `FORMA_FORBIDDEN` | 403 | 角色不足 |
| `FORMA_NOT_FOUND` | 404 | 资源不存在 |
| `FORMA_CONFLICT` | 409 | 状态冲突 / 守卫拒绝 / 唯一键冲突 |
| `FORMA_INTERNAL` | 500 | 内部错误（**不泄露** SQL / 内部路径，细节只写日志） |

规则：`code` 是**公开 API**（E1，改动视为破坏性变更）；`error` / `fields[].message` 为面向用户的文案（E2）；
错误码 → 状态码的映射**只在** `error.rs::status_for` 声明（E3）。

**实现落点**：

| 目标 | 位置 |
|---|---|
| 后端（权威） | `error.rs::ErrorBody` + `error_code` 模块 + `status_for` |
| Web 客户端 | `api.ts` 的 `req()` → `err.code` |
| Flutter 客户端 | `ApiException.code` |
| OpenAPI | `components.schemas.Error`（含错误码枚举）+ 每个操作的统一错误响应 |

### 4.5 前端角色感知（体验层）

生成的 Web 端：

- 由 DSL 的授权规则编译出 `src/access.ts`：`ACCESS_RULES`（实体 → 受约束操作 + 允许角色）
  与 `useCurrentRole()` / `useCan(entity, op)` 两个 hook。
- 列表页的「新建 / 编辑 / 删除」按钮在对应操作受约束时**按角色显隐**。
- 角色未知（未登录 / 请求中）时**不隐藏**——服务端才是判定方。
- 无授权规则的工程**不生成** `access.ts`，产物与之前**完全一致**（零行为变更）。

### 4.5 编译期约束

不满足以下任一条件 **MUST** 编译失败（**MUST NOT** 静默忽略）：

| 诊断码 | 含义 |
|---|---|
| `dsl-auth-no-provider` | 声明了授权规则但无 `auth module`（无认证提供方 → 无法执行） |
| `dsl-auth-unknown-operation` | 授权规则引用了未在该 api `operations` 中声明的操作 |
| `dsl-auth-unknown-role` | 引用了 `auth module` 未声明的角色 |
| `dsl-access-unknown-operation` | 实体级规则的操作名非法 |
| `dsl-access-unknown-role` | 实体级规则引用了未声明的角色 |
| `dsl-access-no-provider` | 实体级规则无 `auth module` |
| `dsl-access-row-unsupported` | 行级条件出现在**无实体行**的操作上（create / list） |
| `dsl-access-relation-unsupported` | 行级条件跨关系遍历（`school.name`） |
| `dsl-access-field-type-unsupported` | 行级条件比较了**不可比较**的类型（Json / 关系） |
| `dsl-access-temporal-type-mismatch` | Timestamp/Date 字段混比，或与字符串/数字字面量比较（仅同类型字段对字段受支持） |

## 5. 命令类操作协议

命令类操作承载真实业务流（状态机审批等）：

```text
POST /{entities}/{id}/act
```

```fm
api Orders {
  POST "/orders/{id}/approve" "审批通过" {
    input { approver String @required }
    pre  { status == "pending" }
    effect { set Order.status = "approved" }
    audit { action "order.approved" }
  }
}
```

### 5.1 执行顺序（规范性）

```text
解析输入 → 校验输入 → 执行 pre（前置条件）→ 执行 effect（状态变更）→ 写审计
```

| 阶段 | 失败语义 |
|---|---|
| 校验输入 | 400（字段级错误） |
| `pre` 不满足 | 409（守卫拒绝）——与 `guard` 的删除拒绝一致 |
| `effect` 越界 | 编译期失败（枚举值不在词表内等） |

### 5.2 各段约束

- **`input`**：字段 **MUST** 通过 required 校验；生成 `{Cmd}Input` 模型。
- **`pre`**：条件表达式 **MUST** 可编译为后端守卫表达式。编译时**MUST** 保留保护括号 `!({expr})`——剥除最外层括号会把 `!` 错误地作用到左操作数上。
- **`effect`**：赋值目标 **MUST** 是实体字段；值 **MUST** 是枚举变体字面量、`now`，或 `input.<field>` 引用。越界 **MUST** 在**写盘前**失败。
- **`audit`**：**仅声明 `audit` 的命令**才生成审计钩子（`tracing`）。

### 5.3 端点独立性

命令路由 **MUST** 挂在**独立子 router** 上并 `with_state`——汇总 router 已是 `Router<State<Db>>` 之下，直接注册 `State<Db>` handler 会类型不匹配。

## 6. UI 原语协议

| 原语 | 协议 |
|---|---|
| `table` | 列定义（字段 + 格式）、过滤、排序、分页；数据源为某 api 的 list 操作 |
| `form` | Create/Update 输入；校验由 `rules` 同源编译（后端权威 + 前端体验） |
| `detail` | 单条实体的只读呈现 + 行内命令按钮 |
| `chart` | 引用顶层 `chart` 块（见 RFC-0010） |

- 路由由 `page.route` 生成类型安全路由树（TanStack Router）。
- 行按钮的可见性：命令的 `pre` 为简单 `field == "lit"` 时，按行状态显隐。
- UI 原语 **MUST NOT** 引入目标框架概念（无 axum / React / ECharts 词汇）。

## 7. 已知缺口（未实现，不受本规范约束）

| # | 缺口 | 影响 |
|---|---|---|
| ~~1~~ | ~~**行级条件不支持跨关系遍历**~~ **已关闭**：一层 `relation.field` 遍历已实现；两层以上 / Optional 关系 / 非关系遍历仍有编译期显式诊断 |
| ~~2~~ | ~~**行级条件不支持 Json 比较**~~ **已关闭**：Timestamp/Date 同类型比较已实现（混比报 `dsl-access-temporal-type-mismatch`）；Json 字段已入 `access_comparable`（`==`/`!=`） |
| 3 | **Flutter target 不支持 cursor pagination** | Build 阶段在任何 emitter 写盘前显式拒绝；同一工程的 Web cursor API 已实现 |
| 4 | ~~统一错误响应体契约未落地~~（已实现，见 §4.4） | `error/code/fields/requestId` 跨后端 / Web / Flutter / OpenAPI 同形；`requestId` 尚为预留 |
| 5 | `pre` 的表达式子集受限 | 仅编译期可判定的表达式；不支持跨实体查询 |
| 6 | 资源级 / 字段级策略未实现 | ADR-0004 描述的权限四层模型当前只落地了认证层 + 操作/实体级授权 |
| 7 | **移动端（Flutter）不做角色感知** | Web 已按角色隐藏操作；Flutter 端仍由服务端 403 兜底 |
| 8 | **`requestId` 未生成** | 错误体已预留字段；需要请求级追踪 id（axum 中间件）才能填充 |

## 8. 开放问题

- 错误响应体是否统一到 [RFC-0005](../../rfc/RFC-0005-application-runtime.zh-CN.md) 的统一错误契约。
- 是否需要批量操作协议（`bulk`）。
- 命令的幂等键约定。

## 9. 参考

- RFC-0002 规格体系：[../../rfc/RFC-0002-specification-system.zh-CN.md](../../rfc/RFC-0002-specification-system.zh-CN.md)
- RFC-0005 应用运行时：[../../rfc/RFC-0005-application-runtime.zh-CN.md](../../rfc/RFC-0005-application-runtime.zh-CN.md)
- RFC-0010 可视化：[../../rfc/RFC-0010-visualization.zh-CN.md](../../rfc/RFC-0010-visualization.zh-CN.md)
- ADR-0004 权限服务端权威：[../../adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md](../../adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md)
- 游标分页参考工程（Rust / Web / OpenAPI）：[examples/cursor-pagination/](../../../../examples/cursor-pagination/)（CI 强制 generated backend `cargo check` + Web `tsc` / Vite build）。
- DSL 语言规范：[../../../dsl-spec.zh-CN.md](../../../dsl-spec.zh-CN.md)
