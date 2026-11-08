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
- **移动端（Flutter）**：同样 **MUST** 走信封——生成 `CursorPage<T>`（`items` + `nextCursor`）、
  `ListParams.cursor`，列表页首屏加载后按「加载更多」用 `nextCursor` 续取并**追加**；
  游标模式的输入体 **MUST NOT** 生成 offset 偏移参数（`page`）。

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

### 4.3 字段级策略（P0-2 / ADR-0004 四层收口）

DSL 语法（在既有 `access Entity` 块内）：

```fm
access Student {
  update → role in ["admin", "teacher"]
  fields {
    grade → role in ["hr", "manager"]   # 仅这些角色可见
  }
}
```

语义与实现约束：

- **输出裁剪**：当前角色不在允许集 → 该字段从 list/get 响应 JSON 中**移除**
  （响应序列化处执行，RFC-0008「字段级 → 服务端（输出裁剪）」）；不是 403、不是脱敏标记。
- **执行位置在服务端**：生成的后端在 list/get 返回前裁剪（`trim_<entity>_fields`，
  字段 → 允许角色表）；前端不构成安全边界（ADR-0004）。
- **同字段多条策略 → 角色并集**（任何一条允许即可见）；`any` → 全员可见（无裁剪）。
- **关联字段不支持**（v1：仅标量字段；关联走实体级访问规则）——编译期显式诊断。
- **fail-closed**：字段策略引用角色 ⇒ 必须声明 `auth module`；未认证请求走 401（既有保护层）。
- **诊断码**：`dsl-field-policy-unknown-field` / `-relation-unsupported` / `-no-provider` /
  `-unknown-role` / `-expression-unsupported`（表达式限 `any` 或角色表达式）。
- **前端体验层**（列表列 / 表单字段 / 详情行显隐）已实现：Web 生成 `FIELD_RULES` +
  `canSeeField(entity, field, role)`，Flutter 生成同构的 `fieldRules` + `canSeeField`；
  列表页按角色隐藏列、表单页隐藏输入框、详情页隐藏行。**角色未知时不隐藏**；
  无字段策略的工程**零变化**（不生成规则表、不引入引用）。

### 4.3.1 行级条件的实现约定

- 行级条件**MUST** 在取到实体行之后判定（`get` / `update` / `delete`）。
- Timestamp / Date **仅支持同类型字段对字段比较**；时间字符串字面量、Timestamp↔Date 混比编译期报 `dsl-access-temporal-type-mismatch`。
- Option 字段**MUST** 以「克隆后取兜底值」参与比较——直接 `unwrap_or` 会移动出实体行。
- 数值字面量**MUST** 按字段类型渲染（`Decimal` 需显式类型，否则整数字面量类型不明确）。
- 行级条件**MUST NOT** 出现在无实体行的操作上（`create` / `list`）。

### 4.3.2 统一错误响应体契约（Runtime Contract · RFC-0005 E1–E5）

**所有后端错误同形**（E4）：

```json
{
  "error": "Validation failed",
  "code": "FORMA_VALIDATION_FAILED",
  "fields": [{ "field": "email", "message": "邮箱必填" }],
  "requestId": "…（由 request_id.rs 中间件注入；见 §7.8）"
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

**`requestId`（请求追踪 id）**：由生成的 `request_id` 中间件产出——优先透传调用方的
`x-request-id`，否则生成 UUID v4；经 `tokio::task_local` 暴露给错误构造（`IntoResponse`
拿不到请求扩展），并回写 `x-request-id` 响应头。前端（Web `err.requestId` /
Flutter `ApiException.requestId`）同形读取，便于与服务端日志对账。

**实现落点**：

| 目标 | 位置 |
|---|---|
| 后端（权威） | `error.rs::ErrorBody` + `error_code` 模块 + `status_for` + `request_id.rs`（追踪 id 中间件） |
| Web 客户端 | `api.ts` 的 `req()` → `err.code` |
| Flutter 客户端 | `ApiException.code` |
| OpenAPI | `components.schemas.Error`（含错误码枚举）+ 每个操作的统一错误响应 |

### 4.4 前端角色感知（体验层）

生成的 Web 端：

- 由 DSL 的授权规则编译出 `src/access.ts`：`ACCESS_RULES`（实体 → 受约束操作 + 允许角色）
  与 `useCurrentRole()` / `useCan(entity, op)` 两个 hook。
- 列表页的「新建 / 编辑 / 删除」按钮在对应操作受约束时**按角色显隐**。
- 角色未知（未登录 / 请求中）时**不隐藏**——服务端才是判定方。
- 无授权规则的工程**不生成** `access.ts`，产物与之前**完全一致**（零行为变更）。

生成的 Flutter 端（P2-6，与 Web 同构）：

- 生成 `lib/access.dart`：`accessRules`（实体 → 受约束操作 + 允许角色）与
  `currentRole()` / `can(entity, op)`；登录 / 注册响应将角色写入 `Api.currentRole`。
- 列表页「新建 / 编辑 / 删除」与编辑页「保存」按钮在对应操作受约束时**按角色显隐**。
- 角色未知时**不隐藏**；无授权规则的工程**不生成** `access.dart`，产物零变化。
- 两端语义一致：api 级与实体级规则对同一 (实体, 操作) 取**角色交集**；
  行级条件不在前端判定，仍由服务端返回 403。

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
| ~~1~~ | ~~**行级条件不支持跨关系遍历**~~ **已关闭（2026-11-08，轨道 A-2）**：任意深度 `relation.relation.field` 遍历已实现——`dsl::resolve_access_field` 为检查层/生成层单一来源；生成层逐跳 `repo::find_{E}` 取数（关系行缺失 → 403 fail-closed），即使目标实体无 get/update/delete API 也生成 `find_X`；非关系遍历 / 可选关系 / 未知字段仍编译期显式拒绝（回归 `tests/access_relation_depth.rs`，生成后端 `cargo check` 零错误零警告） |
| ~~2~~ | ~~**行级条件不支持 Json 比较**~~ **已关闭**：Timestamp/Date 同类型比较已实现（混比报 `dsl-access-temporal-type-mismatch`）；Json 字段已入 `access_comparable`（`==`/`!=`） |
| ~~3~~ | ~~**Flutter target 不支持 cursor pagination**~~ **已关闭（2026-11-05）**：Flutter 端支持游标分页——`CursorPage<T>` 信封（`{items, nextCursor}`）、`ListParams.cursor`、列表页「加载更多」按 `nextCursor` 续取（首屏 + 增量追加）；`Api.getRaw` 取原始信封。**余项**：可视化运行时仍要求裸列表响应（chart 消费 cursor api 仍在构建期显式拒绝） |
| 4 | ~~统一错误响应体契约未落地~~（已实现，见 §4.4） | `error/code/fields/requestId` 跨后端 / Web / Flutter / OpenAPI 同形；`requestId` 由 `request_id.rs` 中间件真实注入（见缺口 #8，轨道 A-3 已核实） |
| 5 | ~~`pre` 的表达式子集受限~~ **已决议（2026-11-05）**：`pre` 限定为**目标实体当前行**的可判定表达式子集（字段比较 / 枚举字面量 / 逻辑组合）；跨实体查询依赖 spec:ir §15 的 DSL→图 lowering（M3 稳定标识），列入后续；未知字段与越界路径编译期显式报错 | 已决议——不再是缺口，是显式的语言边界 |
| ~~6~~ | ~~资源级 / 字段级策略未实现~~ **已关闭（2026-11-05，P0-2）**：四层模型全部落地——认证（JWT）+ 策略（RBAC）+ 资源/实体级（`access` 操作规则 + 行级条件）+ **字段级**（`fields { }` 输出裁剪）。前端体验层（列/输入框/详情行按角色隐藏）亦已补齐（Web `FIELD_RULES` / Flutter `fieldRules`）。**余项**：多租户（RFC-0008 C4） |
| ~~7~~ | ~~**移动端（Flutter）不做角色感知**~~ **已关闭（2026-11-05）**：生成 `lib/access.dart` + `can(entity, op)`，登录写入 `Api.currentRole`；按钮按角色显隐；角色未知不隐藏；无规则的工程产物零变化（回归测试 `tests/flutter_access.rs`） | ✅ 已完成 |
| ~~8~~ | ~~**`requestId` 未生成**~~ **已关闭（2026-11-05；2026-11-08 轨道 A-3 复核）**：生成 `request_id.rs` 中间件——透传/生成 id、`tokio::task_local` 作用域、`x-request-id` 响应头回写；错误体从作用域填充；Web/Flutter 客户端同形读取。回归 `tests/error_contract.rs` 有专项断言 |

### A-5 追加：多租户管理 UI（2026-11-08）

`@tenant` 工程生成 `src/pages/TenantAdmin.tsx`（吊销 / 迁移，管理令牌 `x-admin-token` 头），
与后端 `/api/admin/tenants/:tenant/{revoke,transfer}` 同一判定条件；无租户工程产物零变化
（回归 `tests/tenant_admin_ui.rs`，生成 Web `tsc --noEmit` 通过）。

## 8. 开放问题

> 收口（2026-11-08）：1 项关闭、1 项决议、1 项本阶段落地（命令幂等键约定）。

- [x] ~~错误响应体是否统一到 RFC-0005 的统一错误契约。~~
  **已落地**：E1–E5 即 RFC-0005 契约（`ErrorBody { error, code, fields?, requestId }` +
  `FORMA_*` 稳定错误码 + `status_for` 单点映射）；Web / Flutter / OpenAPI 同形解析。
- [x] ~~是否需要批量操作协议（`bulk`）。~~
  **已决议**：v0.3 不做。批量属客户端循环（`Promise.all` / 并行请求）或未来协议扩展；
  服务端批量会引入事务边界与错误语义的复杂度。
- [x] 命令的幂等键约定。
  **已落地（2026-11-08）**：约定 `Idempotency-Key` 请求头（客户端生成 UUID）；生成的命令
  handler 读取该头并写入审计（`audit_logs.idempotency_key`），重复提交可被对账识别。
  详见 §10.5。

## 10.5 命令幂等键（Idempotency-Key）

命令类操作（§5）支持**幂等键**约定（2026-11-08 落地）：

| 层 | 行为 |
|---|---|
| 客户端（Web / Flutter） | 每次命令执行生成唯一键（Web `crypto.randomUUID()`；Flutter `newIdempotencyKey()`），随 `Idempotency-Key` 头发送 |
| 生成后端 | 命令 handler 读取该头，写入审计：`audit_logs.idempotency_key`（CHAR(36)，可空） |
| 对账语义 | 同一键的重复提交可在审计中识别（v1 不做服务端去重——客户端保证键唯一；服务端记录以供对账） |

**边界**：幂等键**不构成**服务端去重保证；它解决「重复提交的可识别性」，
不解决「重复执行的业务副作用」。需要强去重时由业务侧用状态机（command 的 `pre`）保证。

回归：`tests/seed_flutter.rs`（客户端生成）、`tests/backend_contract.rs`（审计列与 handler）。

## 9. 参考

- RFC-0002 规格体系：[../../rfc/RFC-0002-specification-system.zh-CN.md](../../rfc/RFC-0002-specification-system.zh-CN.md)
- RFC-0005 应用运行时：[../../rfc/RFC-0005-application-runtime.zh-CN.md](../../rfc/RFC-0005-application-runtime.zh-CN.md)
- RFC-0010 可视化：[../../rfc/RFC-0010-visualization.zh-CN.md](../../rfc/RFC-0010-visualization.zh-CN.md)
- ADR-0004 权限服务端权威：[../../adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md](../../adr/accepted/ADR-0004-server-authoritative-authorization.zh-CN.md)
- 游标分页参考工程（Rust / Web / OpenAPI）：[examples/cursor-pagination/](../../../../examples/cursor-pagination/)（CI 强制 generated backend `cargo check` + Web `tsc` / Vite build）。
- DSL 语言规范：[../../../dsl-spec.zh-CN.md](../../../dsl-spec.zh-CN.md)
