---
id: RFC-0002
title: Specification System
status: Implemented
version: 1.1
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-08
domain: architecture
supersedes: []
superseded_by: []
depends_on: [RFC-0001]
related_adrs: [ADR-0001]
related_specs: [spec:ir, spec:types, spec:manifests, spec:serialization]
breaking_change: false
---

# RFC-0002：规格系统（Specification System）

## 1. Metadata

见 frontmatter。本 RFC 规定**规格体系**：DSL 作为唯一事实来源的地位、规格的领域划分与组织方式、语法设计约束、规格版本治理，以及外部标准（SQL / OpenAPI / JSON Schema 等）的导入导出互操作契约。

## 2. Abstract

本 RFC 定义 Forma「什么是规格、规格如何组织、规格如何演进、规格如何与外部标准互通」的体系性规则。它是 [RFC-0001](RFC-0001-architecture-overview.zh-CN.md) 之下、语义内核（RFC-0003）之上的**表达层契约**。

核心立场：**`.fm` 是业务事实的唯一来源**；其他一切格式（OpenAPI / DDL / JSON Schema / Mermaid / DBML / Postman）都是**派生视图**或**导入源**，不是并列的事实来源。

## 3. Motivation

### 3.1 Current Problem

当前规格体系存在四个未成文的问题：

1. **领域划分未成文**。DSL 现有 7 类顶层构造（`app` / `enum` / `entity` / `rules` / `api` / `page` / `chart`，另含 `api` 块内的命令类操作），但哪些属于「数据层」、哪些属于「规则层」、哪些属于「表现层」，从未被显式定义。这导致新能力该加在哪里、该写进哪个 Specification，全凭惯例。
2. **语法约束未成文**。[dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md) 定义了语法，但**语法设计的元规则**（如「不为每个组件库加专属语法」「未支持的语义必须显式报错」）散落在各文档中，未成为契约。
3. **规格版本与项目版本的关系未成文**。DSL 有 `app.version`，项目有 `project.version`，迁移链有版本号，IR 将有 IR 版本——四者关系未定义。
4. **标准互通的保真度未成文**。5 个 importer（sql / openapi / jsonschema / jsonforms / bpmn）与 2 个 exporter（openapi / diagrams）已实现，但**每个方向的语义损失没有声明**。这直接违反设计宪法「不支持的外部语义不得静默丢弃」（[roadmap.zh-CN.md §6.2](../../roadmap.zh-CN.md)）。

### 3.2 Why Existing Architecture Is Insufficient

RFC-0001 规定了**系统边界**，但不规定**表达体系**。「规格如何组织、语法遵循什么原则、版本如何治理、标准如何互通」是另一个独立的契约层次，缺了它，新增能力会各自发明组织方式，最终回到「一个巨型 DSL」。

### 3.3 Expected Outcomes

- 任何新能力都能明确回答「它属于哪个规格领域、放在哪个 Specification」。
- 外部标准导入导出有**声明的保真度**，损失可见。
- 规格版本、项目版本、IR 版本、迁移链版本的关系明确。

## 4. Goals

- 定义规格的领域划分与顶层构造归属。
- 定义语法设计约束（元规则）。
- 定义规格组织（文件划分）约定。
- 定义版本治理（规格版本 / 项目版本 / IR 版本 / 迁移链版本）。
- 定义标准互通的保真度声明机制与往返测试要求。

## 5. Non-goals

- 不定义 IR 语义（见 RFC-0003）。
- 不定义具体语法细节（见 [dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md)）。
- 不定义 Capability 扩展机制（见 RFC-0004）。
- 不定义多语言 i18n 的具体实现。

## 6. Terminology

- **Specification Domain**：规格的语义分区（数据 / 规则 / API / UI / 可视化 …）。
- **Fact Source**：业务事实的权威来源。Forma 中唯一是 `.fm`。
- **Derived View**：由规格派生出的输出（OpenAPI / DDL / Mermaid / 图表产物）。
- **Import Source**：外部标准文件，经 importer 降级为规格语义。
- **Fidelity Level**：某个互通方向的语义保真等级。

## 7. Architectural Context

### 7.1 System Context

```text
                ┌──────────────────────────────┐
                │  .fm 规格（唯一事实来源）      │
                │  app / entity / enum / rules  │
                │  api / page / chart          │
                └───────────┬──────────────────┘
                            │ 派生视图（可重新生成）
        ┌───────────┬───────┼────────┬────────────┐
        ▼           ▼       ▼        ▼            ▼
    OpenAPI      MySQL   Mermaid   React 产物    Rust 产物
                                  (含图表)

   导入源（外部标准 → 规格语义）
   SQL DDL · OpenAPI · JSON Schema · JSON Forms · BPMN
```

### 7.2 Dependencies

- 依赖 RFC-0001（边界与五层文档体系）。

### 7.3 Boundaries

- **`.fm` 是唯一事实来源**。其他格式不得被手工维护为「第二份规格」。
- 派生视图**可重新生成**，不作为 diff 依据。
- 导入源是**单向降级**入口：导入后的语义由 `.fm` 承载，源文件不再被跟踪。
- 规格层**不依赖**任何目标技术栈（DSL 不泄漏 axum / React / ECharts 词汇）。

## 8. Design

### 8.1 Conceptual Model

**规格领域划分**（顶层构造 → 领域 → 归属 Specification）：

| 领域 | 顶层构造 | 语义内容 | 优先级 | 归属 |
|---|---|---|---|---|
| App | `app` | 项目标识、版本、语言 | P0 | `spec:types` |
| Data | `entity` / `enum` | 数据结构、关系、索引、唯一约束 | P0 | `spec:ir` |
| Rules | `rules`（field/check/guard/access） | 校验、跨字段、守卫、权限 | P0 | `spec:ir` + `spec:types` |
| API | `api`（含命令类操作） | 操作、请求响应、过滤排序分页、鉴权 | P0 | `spec:protocols` |
| UI | `page`（table/form/detail/chart） | 页面语义原语、路由、控件提示 | P0 | `spec:protocols` |
| Visualization | `chart` | 图表语义、编码、变换 | P1 | `spec:ir`（见 RFC-0010） |
| Query / Metric | （规划） | 查询、聚合、指标口径 | P1 | 规划中 |
| Workflow / Event | （规划；命令类操作是其子集） | 事件、流程、作业 | P1 | 规划中 |
| Integration / Media | （规划） | 外部服务、文件与媒体 | P2 | 规划中 |
| Deployment | （规划） | 环境、服务、容器 | P2 | 规划中 |

**领域划分的作用**：它是「新增能力该写进哪个 Specification」的判定表，避免 Specification 互相吞并。

### 8.2 Architecture

**语法设计元规则**（约束性，非建议）：

| 编号 | 规则 | 现状核对 |
|---|---|---|
| G1 | 不为每个组件库/第三方库增加专属语法 | ✅ 图表用语义类型 + `nativeExtension` 逃逸口（RFC-0010 §7.6） |
| G2 | 业务规格优先声明式；复杂算法走 Extension | ✅ 表达式语言是封闭子集（dsl-spec §9） |
| G3 | 目标框架实现细节不得泄漏进通用语义 | ✅ DSL 无 axum/React/ECharts 词汇 |
| G4 | 每个规格领域必须有 Schema 与 Validator | ⚠️ 部分实现：`check_program` 覆盖数据/规则/API/UI，`viz-*` 覆盖可视化 |
| G5 | **未支持的语义必须显式报错，不允许静默忽略** | ✅ 已落地（`emit/rust_backend.rs` 对 `paginate cursor` 显式 `bail!`） |
| G6 | 外部标准导入必须记录语义损失 | ❌ **缺失**（见 §8.4） |
| G7 | 规格文件须保留稳定 Source ID，支持定位、Diff、Patch | 🔨 部分（仅 `FieldDecl.line` / `ChartDecl.line`；见 RFC-0003 §14 M1） |

> G5 与 G6 是设计宪法 6.2 的两条硬约束（[roadmap.zh-CN.md §6.2](../../roadmap.zh-CN.md)）。G6 目前**未落地**，是本 RFC 要补齐的缺口。

**规格组织约定**：

- 规格源文件由 `forma.yaml` 的 `source` glob 声明（当前实现：`glob_files` 支持 `**` 递归，`crates/forma/src/dsl/mod.rs`）。
- 单文件与多文件等价——`merge_program` 按文件名排序后合并，重复定义报 `dsl-duplicate`。因此**目录划分是纯组织约定**，不改变语义。
- 建议按领域分目录（`data/` `rules/` `api/` `ui/` `visualization/`），但**不强制**。

### 8.3 Data Model

**四类版本的关系**（互不混用）：

| 版本 | 载体 | 变更触发 | 兼容性判定 |
|---|---|---|---|
| **Specification Version** | DSL 语法版本 | 语法变更（新增构造、语义变更） | 弃用周期：标记 → 一个里程碑 warning → 移除 |
| **Project Version** | `app.version` / `project.version` | 业务演进 | 用于迁移链分组与差异对比 |
| **IR Version** | IR 契约版本（规划中） | IR 结构变更 | 见 RFC-0003 §9 |
| **Migration Chain Version** | `.forma/migrations.json` + `000N_*.sql` 序号 | schema 变更 | 迁移链不可变；向前重放 == 当前 schema |

关系约束：

- Specification Version 与 Project Version **独立**：DSL 2.0 可以用于项目 0.5.0。
- Project Version 驱动迁移链：版本推进时生成配对的 upgrade/rollback（既有迁移引擎契约）。
- Migration 链序号**不可复用**，不可重写历史迁移（不可变基线原则）。

### 8.4 Interfaces and Contracts

**标准互通的保真度声明**（补齐 G6）：

每个互通方向必须声明保真等级：

| 等级 | 含义 | 要求 |
|---|---|---|
| **Lossless** | 语义完全等价 | 往返测试必须通过 |
| **Lossy** | 有已知信息损失 | 必须产出**损失报告**（丢失了哪些语义） |
| **Partial** | 仅覆盖子集 | 必须声明覆盖范围 |
| **Unsupported** | 不支持 | 必须显式报错，不得静默丢弃 |

**当前各方向的实际状态**（源码核对）：

| 方向 | 实现 | 保真度 | 说明 |
|---|---|---|---|
| SQL DDL → 实体 | `adapters/sql.rs` | Lossy | 解析 DDL 得表/列/约束；注释、外键语义等信息有损（当前**无损失报告**） |
| OpenAPI → 实体+API | `adapters/openapi.rs` | Lossy | 同上 |
| JSON Schema → 实体 | `adapters/jsonschema.rs` | Lossy | 同上 |
| JSON Forms → 表单 | `adapters/jsonforms.rs` | Partial | 仅表单结构 |
| BPMN → 工作流 | `adapters/bpmn.rs` | Partial | 仅流程步骤 |
| 实体 → OpenAPI | `emit/openapi.rs` | Lossless（单向） | 由 API 块生成 3.1 |
| 实体 → DDL | `emit/mysql.rs` | Lossless（单向） | MySQL 8 |
| 实体 → Mermaid | `emit/diagram.rs` | Lossless（单向） | ER 图 |

> **待补**：所有 Lossy / Partial 方向需产出损失报告（当前无）。

**往返测试契约**（Lossless 方向的强制要求）：

```text
外部标准 → Import → 规格语义 → Export → 外部标准 → 语义比较
```

比较的是**语义等价**（表结构、字段、类型、约束、操作、请求响应），不是文件文本一致。

### 8.5 Execution Flow

```text
1. 读取 forma.yaml → source glob
2. 收集 .fm → 按路径排序 → 逐文件解析 → 合并为 Program（重复定义报 dsl-duplicate）
3. check_program → 语义检查（error 阻断）
4. Lowering → Semantic Core（RFC-0003）
5. 派生视图生成（emitter）
6. 导入路径（如有）：外部标准 → importer → Program → 合并进同一语义空间
```

### 8.6 Extension Points

- **新顶层构造**：须先在 §8.1 领域表中登记归属，再实现 parser + check + 至少一个 emitter。
- **新 Specification**：在 `docs/architecture/specifications/` 下按主题建目录，并被相关 RFC 以 `related_specs` 引用。
- **新互通方向**：必须声明保真度（§8.4）并补往返测试。
- **新领域**：扩展 §8.1 领域表（属 RFC 变更，需评审）。

## 9. Compatibility

- **Backward**：DSL 语法新增构造为 minor；既有构造语义变更为 major。
- **Forward**：未知属性须保留并忽略，不得报错（保证新工具写、旧工具读）；但**语义未支持**的构造必须报错（G5）。
- **Version Compatibility**：见 §8.3 四类版本的关系；Specification Version 的弃用周期为「标记 → 一个里程碑 warning → 移除」。
- **Migration Compatibility**：DSL 语法变更需提供 AST 级重写工具（既有 `forma migrate` 的定位），不得要求用户手改。
- **Deprecation Policy**：被弃用的语法构造在下一个里程碑版本移除，移除前须有迁移路径文档。

## 10. Security Considerations

- **信任边界**：`.fm` 是用户内容，其所有构造都可能被恶意编写。因此：
  - 规格**永不**被解释执行——只被编译；
  - 表达式语言封闭，禁止任意代码；
  - 类型名、标识符必须白名单校验（防止注入额外声明）。
- **导入源的信任更低**：外部 SQL/OpenAPI 文件可能来自不可信来源。Importer 必须：
  - 限制解析深度与文件大小；
  - 不执行导入内容中的任何引用（如 SQL DDL 中的外部路径）；
  - 导入结果必须经 `check_program` 校验后才可使用。
- **信息泄露**：规格与派生视图可能含敏感字段名。派生视图（尤其 OpenAPI 导出）默认含全部实体——对外暴露前需过滤，这属于部署策略（RFC-0008）。
- **审计**：规格变更须可追溯到来源（Patch / Commit），并与 `audit { action }` 机制联动。

## 11. Performance and Reliability

- **解析性能**：多文件工程的 `load_program` 需按文件排序保证确定性；解析开销与源码规模线性相关。
- **确定性**：合并顺序固定（按路径排序）→ 相同规格集合必然得到相同 `Program`。
- **校验开销**：`check_program` 为单趟遍历，诊断聚合后一次性返回，不早退（保证一次看到全部问题）。
- **导入健壮性**：Importer 对畸形输入必须给出诊断而非 panic。

## 12. Alternatives Considered

- **多格式并列作为事实来源**（v0.2 联邦思路）：拒绝——规则与权限会失去唯一归属，多份事实必然漂移。v0.3 已论证（[architecture.zh-CN.md §2.1](../../architecture.zh-CN.md)）。
- **一个巨型 DSL 涵盖所有领域**：拒绝——语法面积膨胀，AI 生成与校验成本上升，领域边界不可维护。
- **强制单文件规格**：拒绝——多文件是事实（glob 已支持），强制单文件无收益。
- **导入时不声明损失**：拒绝——直接违反设计宪法 G6，导致「导入后语义悄悄变了」这类最难排查的问题。

## 13. Trade-offs

- 显式的保真度声明与损失报告增加了 importer 的工作量，换取「导入前后语义差异可见」——这是可信赖生成的前提。
- 领域表与元规则需要随新能力持续维护（新增构造须登记），换来 Specification 边界稳定。
- 四类版本分离带来概念负担，换取「升级规格语法不必改项目版本号」的实际灵活性。

## 14. Migration Plan

| 步 | 内容 | 完成判据 |
|---|---|---|
| **S1** | 建立领域表与元规则（§8.1 / §8.2），纳入评审清单 | 新增顶层构造须先登记领域 |
| **S2** | 为 Lossy / Partial 导入方向补**损失报告** | 导入非无损时产出显式损失清单 |
| **S3** | 为 Lossless 方向补**往返测试** | SQL/OpenAPI 往返语义等价有测试覆盖 |
| **S4** | 补齐 G4（每个领域有 Validator） | 各领域诊断码齐备且被测试覆盖 |
| **S5** | 明确四类版本关系并写入工具 | 版本信息在构建报告中可查 |

> S2 与 S3 优先级最高——它们是「可信赖地互通」的核心，也是当时唯一**完全缺失**的宪法条款（G6）。

**S1–S5 实现状态（2026-11-08，P3/C-2 核实）**：

| 步 | 状态 | 证据 / 边界 |
|---|---|---|
| S1 领域表与元规则 | ✅ | §8.1 领域表与语法元规则已纳入 [REVIEW-CHECKLIST](../REVIEW-CHECKLIST.zh-CN.md)；新增顶层构造先登记 |
| S2 损失报告 | ✅ | `forma import sql\|prisma\|openapi --loss-report <path>` 输出机读 `LossReport`（`format` / `source` / `entities` / `items[]`），同时写入 `.fm` 头部注释；`tests/adapter_roundtrip.rs` |
| S3 往返测试 | ✅ | `tests/adapter_roundtrip.rs`：OpenAPI → `.fm` → OpenAPI、SQL DDL → `.fm` → `plan_tables`，断言可表达部分等价 |
| S4 每领域独立 Validator | ⚠️ 部分 | 诊断由 `check_program` 集中承担（不是按领域拆分的独立 Validator）；诊断码前缀 `dsl-*` 已覆盖各构造，见 §18 开放问题 |
| S5 四类版本关系可查 | ✅ | `--version` 输出编译器版本 + IR 版本；`BuildReport.ir_version`（CLI 打印 / MCP `irVersion`）；`tests/cli_version.rs` / `tests/adapter_roundtrip.rs::build_report_exposes_ir_version` |

## 15. Implementation Plan

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | S1 + S5（领域表 + 版本关系） | 无 |
| 阶段 2 | S2 + S3（损失报告 + 往返测试） | 无 |
| 阶段 3 | S4（领域 Validator 补齐） | RFC-0003 M1/M2 |

## 16. Conformance Requirements

> 核实日期 2026-11-08（P3/C-2）：有回归证据才勾选；未覆盖项**保留未勾选并写明缺口**。

- [x] 每个顶层构造在领域表中有归属，且有对应诊断码覆盖（2026-11-08，G-9 收口）。
  证据：`tests/domain_validators.rs` 覆盖 **17 个规格领域**（entity / field / type /
  constraint / enum / relation / access / api / page / cursor / job / live / notify /
  searchable / storage / tenant / visualization），每个领域构造最小坏 DSL 并断言专属诊断码出现；
  领域清单即「领域 → 诊断码」机器可读映射，新增领域必须登记。
  边界：仍是单一 `check_program` 入口（非物理拆分模块），§18 开放问题保留。
- [x] 所有 Lossy / Partial 导入方向产出损失报告。
  证据：`adapters::{LossReport, LossItem}` + 三个 `*_with_report`；CLI `--loss-report`；
  `tests/adapter_roundtrip.rs::lossy_imports_emit_machine_readable_loss_reports` /
  `lossless_import_has_empty_report` / `cli_loss_report_flag_writes_json_file`。
- [x] 所有 Lossless 方向有往返语义等价测试。
  证据：`tests/adapter_roundtrip.rs::openapi_roundtrip_preserves_expressible_fields_and_operations` /
  `sql_ddl_roundtrip_preserves_tables_columns_and_relations`。边界：只断言**可表达部分**
  等价（Decimal→string 是序列化契约；array / 反向列表关系由损失报告显式记录）。
- [x] 未支持的构造/操作显式报错（无静默忽略）。
  证据：设计宪法 6.2；emitter `reject_unsupported` / `check_program` 编译期诊断；
  `tests/adapter_roundtrip.rs::malformed_inputs_return_errors_not_panics`。
- [x] 相同规格集合（含文件顺序变化）产生相同 `Program`。
  证据：`dsl::load_program` 对 glob 结果 sort + dedup；
  `tests/adapter_roundtrip.rs::file_order_does_not_change_program` /
  `repeated_check_program_is_deterministic`。
- [x] 导入畸形输入产出诊断而非 panic。
  证据：`tests/adapter_roundtrip.rs::malformed_inputs_return_errors_not_panics`
  （三种格式的错误输入均返回 `Err`）。

## 17. Acceptance Criteria

1. 导入一个含注释与复杂外键的 MySQL DDL，工具报告明确的语义损失清单，且不静默丢弃。
   ✅ `sql_import::sql_ddl_to_dsl_with_report`：外键→关系字段、不可映射列（如 BLOB）逐条进 `LossReport` 与 `.fm` 注释。
2. OpenAPI → 实体 → OpenAPI 往返后，操作路径、请求体字段、响应结构语义等价。
   ✅ **可表达部分**：`tests/adapter_roundtrip.rs` 断言字段集合、类型映射与 `/api/pets` 的 get/post；
   数组 / 枚举降级由损失报告记录，Forma 统一 `/api` 前缀属显式约定。
3. `app.version` 与 DSL 语法版本相互独立：升级语法版本不改变项目版本号。
   ✅ `IR_VERSION` 独立于 `app.version`；`tests/cli_version.rs` 与 `build_report_exposes_ir_version`。
4. 重复 `check_program` 对同一规格产出完全相同的诊断序列。
   ✅ `tests/adapter_roundtrip.rs::repeated_check_program_is_deterministic`。

## 18. Open Questions

- Query / Metric / Workflow / Integration / Media 五个规划领域的顶层构造形态尚未确定；是否需要独立的 `metric` 顶层构造，还是扩展 `rules`？
- 损失报告的机器可读格式（JSON？）与消费方（CI / Studio / Agent）如何对接？
- 往返测试的「语义等价」是否需要形式化定义，还是以 fixture 快照比对为充分？
- `forma.yaml` 是否应承担规格领域到目录的显式映射，还是继续依赖 glob 约定？

## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 语义内核：[RFC-0003-semantic-core.zh-CN.md](RFC-0003-semantic-core.zh-CN.md)
- DSL 语言规范：[dsl-spec.zh-CN.md](../../dsl-spec.zh-CN.md)
- 架构蓝图（v0.2/v0.3 对照）：[architecture.zh-CN.md](../../architecture.zh-CN.md)
- 路线图与治理规则：[roadmap.zh-CN.md](../../roadmap.zh-CN.md)
- 实现位置：`crates/forma/src/dsl/`、`crates/forma/src/adapters/`、`crates/forma/src/emit/`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义规格领域划分、语法元规则、四类版本关系、标准互通保真度与往返测试契约 |
| 1.1 | 2026-11-08 | P3/C-2 互通验收补齐：S2 机读损失报告（三种导入 + `--loss-report`）、S3 往返测试、S5 构建报告 IR 版本；修复 OpenAPI `number` 导入生成非法 `Decimal` 的缺陷；§16 逐条附证据（S4 保留未勾选）；状态升为 Implemented |
