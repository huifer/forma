# Capability 注册表（Capability Registry）

> 本文件是 Forma 所有受管能力的**索引与实现状态**。
> 每个能力须有 Manifest（模式见 [spec:manifests](../specifications/manifests/capability-manifest.zh-CN.md)），
> 并满足 Discovery / Registration / Versioning / Dependency / Permissions / Validation / Adapter / Runtime / Conformance / Deprecation 十项契约。
>
> **实现**：`crates/forma/src/capability.rs` · **CLI**：`forma capability list | check | show <id>`
> **新增能力不必修改 Semantic Core**——放一个 YAML 到项目的 `capabilities/` 目录即可接入。

---

## 内置能力（编译器自带）

| ID | 类别 | 版本 | 提供 | 适配器 | 依赖 |
|---|---|---|---|---|---|
| `data.binding` | data | 1.0.0 | `data.binding` | `binding-adapter` | — |
| `data.mysql` | data | 1.0.0 | `db.ddl` `db.migration` `db.seed` | `mysql-adapter` | — |
| `data.postgresql` | data | 1.0.0 | `db.ddl` `db.migration` `db.seed` | `postgres-adapter` | — |
| `data.sqlite` | data | 1.0.0 | `db.ddl` `db.migration` `db.seed` | `sqlite-adapter` | — |
| `data.redis` | data | 1.0.0 | `db.key-design` | `redis-key-adapter` | — |
| `visualization.echarts` | visualization | 1.0.0 | `chart.bar` … `chart.treemap` | `echarts-adapter` | `data.binding` |
| `visualization.antv` | visualization | 1.0.0 | `chart.bar` … `chart.treemap`（G2Plot 支持子集） | `antv-adapter` | `data.binding` |
| `ui.form` | ui | 1.0.0 | `ui.form` | `web-react-adapter` | — |
| `ui.table` | ui | 1.0.0 | `ui.table` | `web-react-adapter` | — |
| `ui.dialog` | ui | 1.0.0 | `ui.dialog` | `web-react-adapter` | — |
| `ui.navigation` | ui | 1.0.0 | `ui.navigation` | `web-react-adapter` | — |
| `integration.openapi` | integrations | 1.0.0 | `integration.openapi` | `openapi-adapter` | — |
| `integration.jsonschema` | integrations | 1.0.0 | `integration.jsonschema` | `jsonschema-adapter` | — |
| `integration.sql` | integrations | 1.0.0 | `integration.sql` | `sql-adapter` | — |

> Manifest 权威副本在 `crates/forma/src/capability.rs::builtin_manifests()`；本表是它的人类可读镜像。

## 项目自备能力

放在工程目录的 `capabilities/*.yaml`，由 `forma capability list` / `forma build` 自动发现。

| ID | 状态 | Manifest |
|---|---|---|
| `visualization.echarts` | ✅ 内置 | [capability/visualization/echarts.zh-CN.md](visualization/echarts.zh-CN.md) |

> 说明：PostgreSQL / SQLite / Redis / AntV 等是 `targets.*` 路由的 emitter 能力，不要求每个工程在 `capabilities:` 中声明；上表只列需要显式注册的能力。

## 规划中（尚未实现）

| ID | 类别 | 说明 |
|---|---|---|
| `visualization.vega-lite` | 可视化 | Vega-Lite 渲染器（已有 Manifest 范例语义） |
| `visualization.maplibre` | 可视化 | 地图 |
| `visualization.react-flow` | 可视化 | 流程图 |
| `workflow.approval` | 工作流 | 审批流执行器 |
| `workflow.event` | 工作流 | 事件驱动 |
| `workflow.scheduler` | 工作流 | 定时调度 |

---

## 在工程中启用能力

`forma.yaml`：

```yaml
capabilities:
  data.mysql: ">=1.0.0 <2.0.0"        # 映射形式：id → 版本约束（pin）
  visualization.echarts: ">=1.0.0 <2.0.0"
```

或列表形式（不带 pin）：

```yaml
capabilities:
  - data.mysql
  - visualization.echarts
```

启用后：

- 注册表解析**依赖闭包**（`visualization.echarts` → `data.binding`）；
- 校验版本 pin 与 IR / Runtime 兼容范围；
- 汇总权限声明（`data.read` / `data.write`）；
- **能力错误阻断构建**（未知能力、依赖缺失、版本不兼容、pin 不匹配）。

## 登记规则

1. 在对应类别表新增一行。
2. 在 `capability/<category>/` 下写能力文档（含 Manifest）。
3. 在 `conformance/` 下提供或规划对应测试套件。
4. 引用相关 RFC / ADR / spec。
5. **不得修改 Semantic Core**，除非发现新的通用语义概念、现有 IR 无法合理表达（见 RFC-0004 §7.3 红线）。
