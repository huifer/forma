---
id: spec:dependencies
title: 依赖契约（表达式级依赖与影响闭包）
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: specification
depends_on: [RFC-0003]
related_adrs: [ADR-0002]
related_specs: [spec:ir]
breaking_change: false
---

# 依赖契约（表达式级依赖与影响闭包）

> RFC-0003 §18-4 收口：表达式级依赖已落地，本文件是其契约文档。
> 单一事实来源：DSL `Program`；唯一实现：`crates/forma/src/dsl/lower.rs`（桥接）。

## 1. 节点与边

| 节点 | URI 形态 | 来源 |
|---|---|---|
| 实体 / 字段 | `forma://project/{p}/entity/{E}[/field/{f}]` | `entity_uri` / `entity_field_uri` |
| 枚举 | `forma://project/{p}/type/{T}` | `type_uri` |
| API 操作 | `forma://project/{p}/api/{Api}.{op}` | `api_operation_uri` |
| 命令 | `forma://project/{p}/api/{Cmd}`（裸名） | `api_operation_uri` |
| 规则 | `forma://project/{p}/rule/{E}/{op}`（guard 加 `guard:` 前缀） | `rule_uri` |
| 图表 / 任务 / 迁移 / 页面 / 表单 / 控件 | 各自 `*_uri` | `identity.rs` |

| 边 | 方向 | 语义 |
|---|---|---|
| `defines` | 实体 → 字段；表 → 列 | 向前（语义变更 → 产物受影响） |
| `references` | 操作/命令/规则/图表/页面 → 被引用者（实体/字段/类型/API） | 向后（被引用者变更 → 引用者受影响） |
| `belongs-to` | 表列 → 表 | 归属 |
| `generates` | 表 → 迁移链 | 向前 |

## 2. 表达式级依赖（2026-11-08 落地）

**规则 → 引用字段**：`access` 条件、`guard` 的 `deny`、`check`、`field rules`、字段级策略
（`fields { f → ... }`）对字段表达式的引用，生成 `rule → field` 的 `references` 边。
绑定变量（`role` / `any`）不是实体字段，不建边。

**命令 → 引用字段**：`pre` 条件表达式 + `effect` 目标字段（`set E.f = ...`）生成
`command → field` 的 `references` 边。

**影响闭包**：`graph.affected_by(uri)` 沿 `references` 反向 + `defines`/`generates` 正向
传递——改一个字段，能同时看到受影响的**规则**与**命令**（以及经规则/命令继续传递的
操作/页面，若存在边）。

## 3. 边界

- 表达式级依赖只覆盖**可静态解析的字段引用**（`expr_refs`）；动态字符串拼接的引用
  不在范围内（DSL 无此能力）。
- 规则/命令**不**直接连页面（页面的行按钮调用命令是 UI 层关系，由页面 → API/命令的
  既有边覆盖）。

## 4. 验证

- `tests/dsl_ir_bridge.rs`：规则节点、command → field 边、`affected_by` 闭包、
  MCP `forma_impact` 一致性（15 项）。
- 桥接确定性：`lowering_output_is_byte_identical_across_runs`。
