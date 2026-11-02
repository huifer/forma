# Forma 路线图

## Phase 0 — Specification Federation MVP（当前，本仓库）

目标：证明「现有标准联邦 + 统一语义图 + 跨标准补丁」这条路线端到端成立。

- [x] `forma.yaml` Manifest 加载与校验
- [x] `forma://` 实体身份协议（x-forma-entity → bindings → 约定匹配）
- [x] 适配器：SQL DDL（SQLite 子集）、JSON Schema 2020-12、OpenAPI 3.1、JSON Forms、BPMN 2.0 子集
- [x] Unified Semantic Graph + `affectedBy` 影响闭包
- [x] 跨标准校验（doctor 8 条规则）
- [x] 生成器：TS 类型/校验器、Hono 路由、SQLite 迁移、React 表单
- [x] 文件级增量编译（`.forma/build.json`）
- [x] Semantic Patch：`add-entity-field` / `rename-entity-field` / `set-entity-field-required`（事务式）
- [x] CLI：init / validate / doctor / graph / build / patch
- [x] MCP server：validate / graph / impact / patch
- [x] Studio 最小可用：语义图浏览 + 实体跨标准详情 + 诊断面板
- [x] 端到端示例：order-management（含「付款状态」补丁演示）

## Phase 1 — 语义深化

- [ ] DMN 适配器（决策表 → 规则执行骨架）
- [ ] AsyncAPI 适配器（事件契约 → 消息类型 + 订阅骨架）
- [ ] SQL 迁移差异：DDL Diff → `ALTER TABLE` 迁移文件
- [ ] BPMN 工作流运行时（持久化任务、重试、补偿；MVP 子集之外）
- [ ] 完整 JSON Schema 2020-12 校验（可选引入 ajv）
- [ ] `set-entity-field-type` / `add-entity` / `remove-entity-field` 等 patch 操作补全
- [ ] patch 计划预览模式（`--dry-run` 输出编辑计划，不落盘）

## Phase 2 — Studio 与增量升级

- [ ] Studio 编辑能力：Data Studio（ER 图改 SQL）、API Studio（改 OpenAPI）、UI Studio（改 JSON Forms）、Workflow Studio（BPMN 图形编辑）
- [ ] 各 Studio 共享：项目上下文、实体身份、版本、依赖、修改记录、验证结果、编译状态
- [ ] 节点级增量编译（Dependency DAG + 节点 Hash + 编译缓存，v0.1 蓝图第 9 章落地）
- [ ] DBML 输入适配器（编译为 SQL DDL）
- [ ] API Client 生成（fetch/axios 双目标）

## Phase 3 — 生态扩展

- [ ] C4 / Structurizr：Architecture Studio、依赖分析、`forma doctor` 架构规则
- [ ] Protocol Buffers / GraphQL SDL 适配器
- [ ] Kubernetes / Terraform 目标
- [ ] VS Code 扩展（诊断 + 图视图内嵌）
- [ ] Benchmark：同一业务变更，Forma 补丁 vs 传统 AI 全文重写的 Token 对比（v0.1 蓝图第 15 章思想）

## 成功标准（Phase 0 验收线）

1. `forma init && forma build` 零配置跑通，产物为标准 TS/Hono 工程，可直接 `npm run dev`。
2. 「Order 增加付款状态」一条 patch：SQL / JSON Schema / OpenAPI / JSON Forms 四处同步 + 校验 + 受影响目标增量重编，全程 < 3 秒。
3. AI 通过 MCP 四个工具完成上述操作，全程不接触文件路径与具体语法。
4. 生成代码确定性：连续两次 `forma build` 产物 byte 级一致。
