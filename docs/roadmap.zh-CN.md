# Forma 路线图（v0.3 — DSL 中间层）

> 架构见 [architecture.zh-CN.md](architecture.zh-CN.md) · DSL 语法见 [dsl-spec.zh-CN.md](dsl-spec.zh-CN.md)

## 已定案（设计完成，2026-11）

- [x] 定位定案：**一层 DSL 中间层**——一套 DSL 生产前端（Web/移动）+ 后端 + 数据库；更新 DSL → 执行编译器 → 得到新代码
- [x] 目标矩阵定案：适配层首先做 **Rust 后端（axum + sqlx）+ Web（React + TanStack Router/Query + shadcn/ui）+ MySQL**；移动端（iOS/Android）Phase 2 预留
- [x] 规则校验定案：DSL 一等公民——字段校验/跨字段检查/守卫/权限/状态迁移，双端同源编译（后端权威 + 前端体验）
- [x] DSL 语法规范 v0.1（[dsl-spec](dsl-spec.zh-CN.md)）：app/enum/entity/rules/api/page 七构造 + 封闭表达式语言
- [x] 编译器结构定案：forma-syntax / forma-ir / forma-gen-{mysql,rust,web} / forma-engine / forma-patch / forma-cli
- [x] 标准联邦（v0.2）降级为生成视图（openapi/JSON Schema 导出），适配器代码转 importer/exporter

## M1 — DSL 内核（进行中）

- [ ] `forma-syntax`：Lexer + Parser + AST（span 保留）
- [ ] `forma-ir`：AST → 语义图（复用 UnifiedSemanticGraph）+ 类型检查 + 规则表达式检查
- [ ] `forma check`：全量语义诊断（错误带行号 + did-you-mean）
- [ ] 示例 `examples/order-management/forma/*.fm` 全量通过

## M2 — db-mysql 适配层

- [ ] MySQL 8 DDL emitter（InnoDB/utf8mb4，类型映射见架构 §3.1）
- [ ] 基线迁移 `0001_init.sql`
- [ ] IR diff → `ALTER TABLE` 增量迁移

## M3 — backend-rust 适配层

- [ ] 实体 → struct + serde；枚举 → Rust enum
- [ ] 规则 → Rust 校验函数（权威执行层）
- [ ] `api` → axum Router + handler + sqlx repository
- [ ] `forma build` 产物 `cargo build && cargo test` 可跑

## M4 — web-react 适配层

- [ ] `page` → TanStack Router 路由树
- [ ] `api` 操作 → TanStack Query typed hooks + fetch client
- [ ] `table/form/detail` 原语 → shadcn/ui 组件组合
- [ ] 规则 → zod schema（与后端同源）
- [ ] `pnpm dev` 跑通列表/表单/详情

## M5 — 语义补丁（AI 闭环）

- [ ] patch 操作：add-entity-field / rename / set-required / add-rule …（AST 级编辑）
- [ ] 影响闭包 → 全目标增量重编
- [ ] MCP 四工具切换数据源为 DSL

## M6 — 标准视图与生态

- [ ] openapi.yaml / JSON Schema 导出（联邦产物互通）
- [ ] VS Code 扩展（DSL 语法 + 诊断）

## M7+ — 扩展

- [ ] 移动适配层（iOS/Android 原生或 Expo）
- [ ] 聚合表达式（sum/count/exists）全端编译
- [ ] Studio 可视化编辑
- [ ] Token 经济学 Benchmark（DSL 补丁 vs 传统全量重写）

## 遗留代码处置

- `crates/forma`（联邦引擎：SQL/JSON Schema/OpenAPI/JSON Forms/BPMN 适配器 + Hono 生成器）：M1–M3 期间保留可编译；`graph/patch/engine` 模块被 DSL 内核复用；标准适配器改造为 exporter（M6）。
- `docs/history/`：全部历史决策归档，只读。
