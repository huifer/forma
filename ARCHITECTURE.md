# Forma 架构蓝图 v0.2 — Specification Federation

> Forma = Universal Specification Compiler
> The integration and compilation layer for AI-native software development.
>
> v0.1 蓝图（自研 8 类 DSL，见 `docs/history/04-forma-dsl-blueprint-v0.1.md`）已被本蓝图取代。
> 本文是当前代码库实现的规范。凡代码与本文冲突，以本文为准并修代码。

---

## 0. 一句话定位

**Forma 不发明新的软件规格语言。** Forma 把现有成熟标准（SQL DDL、OpenAPI、JSON Schema、BPMN……）组织成一套 AI 可操作、可视化、可编译的软件规格系统，并解决它们之间的**语义关联、统一引用、增量变更、跨标准编译**四个问题。

## 1. 设计原则

1. **Polyglot Specification**：每个软件领域用该领域最成熟的标准，Forma 不重复定义。
2. **Forma 只拥有四样东西**：
   - **Manifest**（`forma.yaml`）：项目有哪些规格、什么标准/版本、编译目标。不含任何业务语义。
   - **实体身份协议**（`forma://` URI）：跨标准的统一业务实体身份。
   - **Unified Semantic Graph**：解析后的统一语义模型（内部表示，非用户语言）。
   - **Semantic Patch API**：AI 与人修改软件的唯一语义入口。
3. **AI 不直接写文件**：AI 通过 Semantic Patch 表达业务变化，Forma 负责跨标准传播、校验、编译。
4. **确定性编译**：同一输入必产生同一输出；生成文件带 `@generated` 头。
5. **标准优先**：解析尽量复用标准自身机制（`$ref`、`$id`、BPMN XML），不为 Forma 添加非标准语法；跨标准关联用 `x-forma-*` 扩展字段或 Manifest 映射，不污染标准本体。
6. **增量优先**：一切编译以「变更影响分析」为入口，永远只重算受影响的子图。

## 2. 标准选型

### 2.1 MVP（Phase 0，本仓库已实现）

| 领域 | 标准 | 版本 | Forma 角色 |
|---|---|---|---|
| 数据库 | SQL DDL（SQLite 方言） | — | 解析表/列/索引/约束 → 生成迁移 |
| 数据类型 | JSON Schema | 2020-12 | 跨模块数据契约、类型与校验规则的唯一事实来源 |
| API | OpenAPI | 3.1 | REST 契约 → 生成服务端路由 + 客户端类型 |
| UI 表单 | JSON Forms（JSON Schema + UI Schema） | 5.x | 表单布局/控件/绑定 → 生成 React 表单 |
| 业务流程 | BPMN 2.0（子集） | 2.0 | 流程节点/网关/事件 → 流程图数据与执行骨架 |

### 2.2 后续阶段

| 阶段 | 标准 | 用途 |
|---|---|---|
| Phase 1 | DMN | 决策表 → 规则执行 |
| Phase 1 | AsyncAPI | 事件契约 → 消息类型与订阅骨架 |
| Phase 1 | SQL 迁移差异（DDL Diff → ALTER） | 数据库演进 |
| Phase 2 | DBML | 可选的图形化建模输入（编译为 SQL DDL） |
| Phase 2 | Node 级增量编译（Dependency DAG + 节点 Hash） | 替代文件级增量 |
| Phase 2 | C4 / Structurizr DSL | Architecture Studio、依赖分析 |
| Phase 3 | Protocol Buffers / GraphQL SDL / gRPC | 服务与数据契约 |
| Phase 3 | Kubernetes / Terraform | 部署与运行环境 |

### 2.3 技术边界（重要）

- **BPMN 不是「XML 转 TypeScript 函数」**：涉及持久化任务、重试、定时器、补偿、长事务时需要工作流运行时。MVP 只支持有限子集（start/end 事件、task、userTask、serviceTask、exclusiveGateway、sequenceFlow），生成流程定义数据 + 执行骨架，完整运行时是 Phase 1 交付。
- **SQL DDL 解析是方言化子集解析器**，不是通用 SQL 引擎。MVP 覆盖 SQLite 建表常用语法；完整方言矩阵靠适配器接口逐步替换实现（每方言一个 adapter，接口不变）。
- **JSON Schema 校验是子集求值器**（type/required/enum/format/min/max/pattern），不是完整 2020-12 规范实现；生成端同样只生成子集校验函数。完整规范校验引入 ajv 属 Phase 1 可选依赖。

## 3. 系统架构

```
                ┌──────────────────────────────┐
                │      AI Specification Agent   │
                │  (Codex / Claude Code / Pi …) │
                └──────────────┬───────────────┘
                               │ Semantic Patch / MCP tools
                               ▼
┌───────────┐   ┌──────────────────────────────────────────────┐
│ specs/    │   │                Forma Engine                   │
│           │   │  ┌────────────┐  ┌────────────────────────┐  │
│ schema.sql│──▶│  │  Adapters  │─▶│  Unified Semantic Graph │  │
│ *.json    │   │  │ sql/js/oas │  │  (identity · reference │  │
│ openapi   │   │  │ forms/bpmn │  │   · dependency · diff) │  │
│ *.bpmn    │   │  └────────────┘  └───────────┬────────────┘  │
│ *.ui.json │   │  ┌────────────┐  ┌───────────▼────────────┐  │
└───────────┘   │  │  Validate  │  │   Deterministic Build  │  │
                │  │ (doctor)   │  │  (incremental · cache) │  │
                │  └────────────┘  └───────────┬────────────┘  │
                │  ┌────────────┐  ┌───────────▼────────────┐  │
forma.yaml ────▶│  │   Patch    │  │      Generators        │  │
                │  │ (semantic) │  │  ts · hono · sql · ui  │  │
                │  └────────────┘  └────────────────────────┘  │
                └──────────────────────────────────────────────┘
                               ▼
                 Standard Software Project (generated/)
```

### 3.1 目录结构（本仓库）

```
Cargo.toml             cargo workspace
crates/forma/          引擎 + 适配器 + 编译器 + CLI + MCP（单一 crate，模块化内部结构）
  src/manifest.rs      forma.yaml 加载与校验
  src/identity.rs      forma:// 实体身份协议
  src/graph.rs         Unified Semantic Graph：节点/边/查询/影响分析
  src/adapters/        sql | jsonschema | openapi | jsonforms | bpmn
  src/validate.rs      跨标准一致性规则（doctor）
  src/generators.rs    ts-types | hono-routes | sqlite-migration | react-forms
  src/engine.rs        load → validate → build 编排；增量缓存
  src/patch.rs         语义操作定义与跨标准编辑计划（事务式）
  src/scaffold.rs      forma init 脚手架
  src/mcp.rs           stdio MCP server（自实现 JSON-RPC）
  src/main.rs          forma 命令行
  tests/               集成测试（17 个：SQL 解析 / 图 / 构建 / 补丁）
apps/studio/           Forma Studio（Vite + React，最小可用：语义图浏览 + 诊断）
examples/order-management/  端到端示例（五种标准 + 一个语义补丁演示）
docs/history/          v0.1 蓝图与全部历史决策（ChatGPT 对话归档）
```

## 4. Manifest（forma.yaml）

只做五件事：声明规格清单、标准与版本、编译目标、身份映射（仅当标准文件内无法表达时）、编译配置。**不含业务语义。**

```yaml
project:
  name: order-management
  version: 0.1.0

specifications:
  database:
    - path: specs/database/schema.sql
      dialect: sqlite
  schemas:
    - path: specs/schemas/order.schema.json
      format: json-schema
      dialect: "2020-12"
  api:
    - path: specs/api/openapi.yaml
      version: "3.1"
  ui:
    - path: specs/ui/order-form.ui.json
      format: json-forms
  workflows:
    - path: specs/workflows/order-approval.bpmn
      format: bpmn

bindings:            # 可选：当文件内没有 x-forma-entity 时的身份映射
  - uri: forma://project/order-management/entity/Order
    table: orders

targets:
  typescript: src/shared
  backend: { framework: hono, out: generated/backend }
  database: { dialect: sqlite, out: generated/database }
  frontend: { framework: react, out: generated/frontend }
```

校验规则：path 必须存在；format/dialect 已知；bindings 引用的表必须存在于某个 SQL 规格。

## 5. 实体身份协议

统一 URI 形态：

```
forma://project/{project}/entity/{Entity}
forma://project/{project}/entity/{Entity}/field/{name}
forma://project/{project}/type/{SchemaId}
forma://project/{project}/api/{operationId}
forma://project/{project}/workflow/{ProcessId}
forma://project/{project}/ui/{FormId}
```

绑定优先级（高 → 低）：

1. 文件内 `x-forma-entity` 扩展（JSON Schema / OpenAPI schema / BPMN 扩展元素）
2. Manifest `bindings`（SQL 表 ↔ 实体；SQL 不塞非标准注释）
3. 约定匹配：表/类型名归一化后一致（`orders` ≡ `Order`）

冲突检测：同一 URI 被两个不同定义认领 → error 级诊断。

## 6. Unified Semantic Graph

```ts
type NodeKind =
  | 'entity' | 'entity-field'          // 业务实体（身份协议拥有）
  | 'type'                             // JSON Schema 类型
  | 'table' | 'table-column' | 'index' // SQL DDL
  | 'api-operation'                    // OpenAPI operation
  | 'ui-form' | 'ui-control'           // JSON Forms
  | 'workflow' | 'workflow-step';      // BPMN

interface SemanticNode {
  uri: string;                 // forma:// 或内部节点 URI
  kind: NodeKind;
  name: string;
  standard: 'sql' | 'json-schema' | 'openapi' | 'json-forms' | 'bpmn' | 'core';
  source: { file: string; line?: number };
  data: unknown;               // 各适配器的领域数据
}

type EdgeKind = 'defines' | 'belongs-to' | 'binds' | 'references' | 'generates';
```

核心查询：

- `node(uri)` / `nodesByKind(kind)` / `entities()`
- `edgesFrom / edgesTo`
- **`affectedBy(uri)`**：反向依赖闭包 —— 跨标准语义变更传播的引擎。「改 Order.status」返回 SQL 列、JSON Schema 属性、OpenAPI operation、UI 控件、BPMN 步骤等全部受影响节点。

## 7. 跨标准校验（`forma doctor`）

| 规则 ID | 严重级 | 内容 |
|---|---|---|
| `identity-duplicate` | error | 同一实体 URI 被冲突定义认领 |
| `field-name-mismatch` | warning | SQL 列名与 JSON Schema 属性名归一化后不一致 |
| `field-required-mismatch` | warning | NOT NULL ↔ required 不对齐 |
| `field-type-mismatch` | error | 类型映射表不可互认（TEXT↔string、INTEGER↔integer、REAL↔number、TIMESTAMP↔string+date-time、BOOLEAN↔boolean） |
| `api-schema-unresolved` | error | OpenAPI 内部 `$ref` 无法解析 |
| `api-schema-orphan` | warning | OpenAPI schema 组件未关联任何实体或 JSON Schema |
| `form-binding-missing` | error | JSON Forms 引用的 JSON Schema `$id` 不存在 |
| `workflow-reference-soft` | info | BPMN 变量命名疑似引用实体字段（提示建立绑定） |

诊断结构统一：`{ severity, code, message, file, line?, nodes?: uri[] }`。`forma validate` 只报 error 阻断；`forma doctor` 输出全量诊断表。

## 8. Semantic Patch API

AI（或人）表达业务变化，Forma 计算跨标准编辑计划并执行：

```jsonc
// patch.json
{
  "operations": [
    {
      "op": "add-entity-field",
      "entity": "Order",
      "field": {
        "name": "paymentStatus",
        "type": "string",
        "enum": ["unpaid", "paid", "refunded"],
        "required": false,
        "sql": { "column": "payment_status", "type": "TEXT" }
      }
    }
  ]
}
```

执行流程（每次 patch 固定九步，来自战略提案）：

```
1 解析 patch → 2 定位实体身份 → 3 计算影响闭包（affectedBy）
→ 4 生成跨标准编辑计划 → 5 逐标准应用编辑（原位修改规格文件）
→ 6 重载语义图 → 7 doctor 校验 → 8 增量编译受影响目标 → 9 输出变更报告
```

MVP 实现的操作：

| 操作 | 覆盖标准 |
|---|---|
| `add-entity-field` | SQL DDL（插列）、JSON Schema（加属性）、OpenAPI（同步组件 schema）、JSON Forms（补默认控件） |
| `rename-entity-field` | 同上四处 |
| `set-entity-field-required` | 同上四处 |

其余操作在 `src/patch/operations.ts` 中以 `not_implemented` 计划项报告，不静默跳过。

失败语义：任一步编辑失败 → 回滚全部文件编辑（编辑前快照 + 还原），保证规格目录永远自洽（事务式 patch）。

## 9. 编译（`forma build`）

| 生成器 | 输入 | 输出 |
|---|---|---|
| `ts-types` | JSON Schema 实体 | `types.ts`（interface + enum）+ `validators.ts`（零依赖校验函数） |
| `hono-routes` | OpenAPI operations | `routes.ts`（Hono 路由骨架 + 类型化 handler + 请求校验挂钩） |
| `sqlite-migration` | SQL DDL | `migrations/0001_init.sql`（规范化重排） |
| `react-forms` | JSON Forms + JSON Schema | `OrderForm.tsx`（零运行时依赖的受控表单） |

增量机制（MVP 文件级，Phase 2 升级节点级）：

- `.forma/build.json` 记录 `{ inputHash, generatorVersion, outputHash }`
- 输入 Hash 未变且生成器版本未变 → 跳过
- 任何 patch 之后强制重载图并只重建受影响实体关联的产物

## 10. AI 接口

### 10.1 CLI（`forma`）

```
forma init [dir]      生成骨架项目（forma.yaml + specs/ 示例）
forma validate        校验 manifest + 规格（error 才失败）
forma doctor          全量跨标准诊断报告
forma graph [--json]  打印/导出语义图（--export-graph forma-graph.json）
forma build [--force] 增量编译全部目标
forma patch <file>    应用语义补丁（事务式）
forma mcp             启动 MCP stdio server（供 Codex/Claude Code/Pi 接入）
```

### 10.2 MCP 工具（`forma mcp`）

| 工具 | 作用 |
|---|---|
| `forma_validate` | 返回当前项目诊断列表 |
| `forma_graph` | 返回语义图摘要（实体 × 关联标准 × 诊断） |
| `forma_impact` | 给定实体/字段 URI，返回受影响节点闭包 |
| `forma_patch` | 应用 Semantic Patch 并返回变更报告 |

AI 的工作流被约束为：`forma_graph → forma_impact → forma_patch → forma_validate`。AI 永远不手改规格文件、更不直接写生成代码。

## 11. 与 v0.1 蓝图的差异对照

| v0.1（自研 DSL 版） | v0.2（本蓝图） |
|---|---|
| 8 类自研 DSL 是唯一事实来源 | 现有标准文件是事实来源；Forma 无业务 DSL |
| Canonical IR 从自研 DSL 归一化 | Unified Semantic Graph 从各标准适配归一化 |
| Studio 编辑自研 DSL | Studio 浏览/编辑各标准（MVP 只读浏览 + 诊断） |
| Semantic Patch 修改 DSL 文件 | Semantic Patch 跨标准原位修改规格文件 |
| 编译 = DSL → 代码 | 编译 = 标准联邦 → 代码（语义关联层是新增核心资产） |
| 保留 | 节点 Hash + Dependency DAG 增量编译（Phase 2 落地节点级）、Verification System 思想（doctor + 事务式 patch + 生成后校验）、Token 优化（AI 只发语义补丁，不发全文） |

## 12. 语言与运行时决策（2026-11 定案：整套 Rust）

- **引擎 / 适配器 / 编译器 / CLI / MCP 全部 Rust**（`crates/forma`，单一 crate 模块化内部结构）。用户定案：整套实现为 Rust，性能与单二进制分发（`forma` 一个可执行文件覆盖 CLI 与 MCP）是第一优先级。
- **依赖策略**：`serde` / `serde_json` / `serde_yaml` / `sha2` / `anyhow`，无重依赖；CLI 参数手写解析；MCP 为自实现 stdio JSON-RPC（协议子集：`initialize` / `tools/list` / `tools/call`）；测试用内置 `cargo test`。
- **生成代码目标不变**：产物是标准 TypeScript（Hono / React）与 SQL——Forma 用 Rust 写，生成的是用户的语言。
- **Studio**：Vite + React + TypeScript（用户原架构表定位），仅消费 `forma build --export-graph` 产物，不直接依赖引擎（数据契约解耦）。
