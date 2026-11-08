---
id: roadmap:p2-plan
title: P2 全面任务规划（余项 Closure + 目标扩栈 + 治理收尾）
status: Accepted
version: 1.1.0
authors: [forma-maintainers]
created: 2026-11-08
updated: 2026-11-08
domain: roadmap
depends_on: [RFC-0001, RFC-0004]
related_adrs: [ADR-0002, ADR-0003]
related_specs: [spec:ir, spec:types, spec:protocols, spec:compatibility, spec:manifests]
breaking_change: false
---

# P2 全面任务规划

> **执行结果（2026-11-08）**：**轨道 A 八项已全部完成**（含 2 项「其实已实现、文档滞后」的核实）；
> **轨道 B 目标扩栈已按用户决策撤销，后续不再考虑**（§2 仅作历史记录保留）。
> 全量 `cargo test` 396 项 / 63 套件通过；新增 6 个回归测试文件；生成工程验证门：
> Rust backend `cargo check` 零错误零警告（含 school 旗舰示例）、GraphQL / Web `npx tsc --noEmit`、
> SDL `graphql.parse/buildSchema`、三条新 CLI 命令端到端冒烟。
>
> 基线：官方 P2-1~P2-6 已在 [gap-backlog](gap-backlog.zh-CN.md) 全部关闭；P0/P1 backlog 10/10 完成；
> F-001~F-008 全部完成。本文档规划 P2 阶段的**剩余全部工作**，分三条轨道。
>
> 规则（沿用 gap-backlog）：缺口只在被实现并通过 Conformance 后才算关闭；
> **「静默忽略」不是关闭方式**——未支持的语义必须产生显式诊断。

## 0. 排期原则与完成定义

| 轨道 | 性质 | 完成定义（Definition of Done） |
|---|---|---|
| **A. 余项 Closure** | 已落地功能的遗留尾巴，小而具体 | 功能实现 + 回归测试 + 对应 spec「已知缺口」章节移除 + gap-backlog 移入「已关闭」 |
| **B. 目标扩栈** | 新 target / 新能力，每项需先立契约 | **契约文档（先行）** → emitter 实现 → 验证门（编译/行为）→ 回归测试 → capability registry 登记 |
| **C. 治理收尾** | 叙事与合规 | 文档状态变更 + `forma docs validate/lint/check-*` 全绿 + CI 无回归 |

**轨道间依赖**：A 无外部依赖；B 的扩栈项**不依赖** A；C 的 RFC 升格依赖 A/B 的实际完成状态（先说已完成的事）。
**实际执行**：A 全部完成；B 撤销；C 中与 A 相关的 spec 同步、gap-backlog 双向关闭已完成，
RFC-0001~0010 状态升格**随 B 撤销一并取消**。

---

## 1. 轨道 A：余项 Closure（8 项）

### ✅ A-1 字段级策略的前端显隐（Web + Flutter）
- **来源**：P0-2 关闭记录的「余项：字段级前端显隐（体验层）」；backend 裁剪已落地（`trim_<entity>_fields`）。
- **内容**：Web 复用 `src/access.ts` 的 `canSeeField(entity, field, role)` 形态；Flutter 在 `access.dart` 增加 `canSeeField`。列表页列、表单输入框、详情行按角色隐藏；**角色未知时不隐藏**（服务端裁剪是边界，与 P0-3/P2-6 同一原则）。
- **验收**：`tests/web_access.rs` / `tests/flutter_access.rs` 扩项；学校示例（字段级策略工程）浏览器端到端验证。
- **估时**：数天级。

### ✅ A-2 行级访问条件的多层关系遍历
- **来源**：P0-1a 余项——两层以上 `relation.field` 遍历目前是编译期显式诊断。
- **内容**：支持任意深度的关系链遍历（生成器自动建 `t{N}` 别名）；Optional 关系的兜底语义、类型系统约束、**越界用例保持编译期显式诊断**（fail-closed 不放松）。
- **验收**：`tests/api_auth.rs` 扩项；`expr_to_rust_mode(input="row")` 递归展开的单元测试。
- **估时**：周级；是轨道 A 里语义设计最重的一项。

### ✅ A-3 `requestId` 请求级中间件（spec:protocols §7.8）
- **来源**：P1-7 关闭记录的「requestId 预留」。
- **内容**：生成的 axum 应用加请求级中间件注入 `request_id`（现成 `request_id.rs` 模块已生成，接线到 `ErrorBody.requestId`）；错误响应携带；日志结构化字段关联。
- **验收**：`tests/error_contract.rs` 扩项（错误响应 `requestId` 非空且全链路一致）；`--json` 日志含 request_id。
- **估时**：天级。

### ✅ A-4 GraphQL 逆向关系（v2）
- **来源**：graphql target 契约的「余项（v2）」。
- **内容**：`User.items: [Item!]!` 形态的反向关系字段；关系过滤器依赖；N+1 处理（DataLoader 或等价批处理）。
- **验收**：`tests/graphql.rs` 扩项；resolver 层真实调用 REST 后端的集成冒烟（非 mock）。
- **估时**：周级。

### ✅ A-5 多租户管理 UI
- **来源**：tenant 收尾契约——后端吊销/迁移端点已落地，无 UI。
- **内容**：租户列表、吊销、迁移（`{to}`）三个操作的管理页面（Web；Flutter 视优先级）；操作确认与结果回显；沿用 `TENANT_ADMIN_TOKEN` 授权语义。
- **验收**：生成工程端到端：吊销 → 用户 login 被拒；迁移 → 数据归属变更；未设 token → 403。
- **估时**：周级。

### ✅ A-6 artifacts manifest 补全（Extension 锚点 + 来源规格）
- **来源**：F-002 关闭记录的「余项：Extension 锚点机制、清单的来源规格/依赖节点字段」。
- **内容**：`.forma/artifacts.json` 增加来源规格（生成器版本/IR 版本/输入指纹）与依赖节点字段；`extension` 扩展点的锚点登记（ADR-0003 的「扩展点文件不动」承诺需要清单级证据）。
- **验收**：`tests/artifacts.rs`（或 artifacts 相关回归）扩项；`--strict` 模式下 anchor 缺失显式报错。
- **估时**：周级。

### ✅ A-7 patch 操作集扩展（图表/迁移节点细粒度）
- **来源**：spec:ir §15.4 余项（P5 审批门之后未覆盖的部分）。
- **内容**：`forma_patch` / `dsl::patch` 增加图表控件级操作（改 chart 编码、加 dashboard 槽位）与迁移链节点级操作；安全语义等同既有 op 集（快照 → 编辑 → 重校验 → 错误整体回滚）。
- **验收**：`tests/patch.rs`扩项；图表工程补丁端到端（Studio 走 `POST /api/patch` 同一事务）。
- **估时**：周级。

### ✅ A-8 逆向导入与 SQL 迁移双向对接（M7/M9/M10 矩阵项验收）
- **来源**：roadmap §4 表内已标 M7/M9/M10 但未验收的行：SQL DDL → 实体（M7）、Prisma schema → 实体（M10）、Flyway/Liquibase/golang-migrate 目录约定双向（M9）。
- **内容**：`forma import sql <path.sql>` 与 `forma import prisma <schema.prisma>` 落地或对既有适配器显式标注状态；迁移对接目标（Flyway 等）产出「迁移 SQL 落进既有体系的包装产物」（或显式诊断不支持范围）。
- **验收**：roadmap §4 的验收条：既有 Swagger + MySQL 项目导入为 `.fm` → 导出 OpenAPI 与 DDL，语义等价；M10 引入 Prisma 示例工程走通。
- **估时**：SQL 逆向（M7）周级；Prisma 数天；Flyway 双向周级。

**轨道 A 完成后的判据**：gap-backlog 与全部 spec「已知缺口」章节无遗留行；`forma docs validate` 全绿。

> **A-8 边界决议**：「双向对接」落地的是**导出方向**（Forma 版本化链 → Flyway / Liquibase /
> golang-migrate 目录约定）。反向导入外部工具的 changelog 不是 Forma DSL 源，v1 显式不做
> 并给出说明（不静默降级）；如未来需要，应单独立项并先定义外部格式到 `.fm` 的语义映射。

---

## 2. 轨道 B：目标扩栈（**已撤销，仅作历史记录**）

> 用户决策（2026-11-08）：**后续不再考虑轨道 B**。以下 B-1~B-7 内容不作为计划执行，
> 保留仅为记录当时评估过的 target 契约模式与边界，不得据此排期。

**统一契约模式**（已由 NestJS / Expo / GraphQL / Next.js / Vue / PostgreSQL / SQLite 验证，禁止偏离）：

```
① target 契约文档（DSL 语法面 / 能力边界 / 显式拒绝清单 / 产物契约 / 验证门）
   → ② emitter 实现（build.rs 路由 + manifest 字段 + 构建步骤名 + 零变化默认路径）
   → ③ 验证门（生成端真实编译/行为，非 cargo test 代跑）
   → ④ 回归测试（结构断言 + 默认 target 零变化）
   → ⑤ capability registry 登记（若属能力模块）
```

铁律（各 target 契约共识）：
- **零变化原则**：未启用该 target 的工程产物 byte 级不变（默认路径回归锁定）。
- **显式拒绝**：不支持的 DSL 构造构建期显式 bail，不静默降级（安全语义尤其如此）。
- **纯 `cargo test` 覆盖不到的目标层**（TS/Dart/SDL/真实库）必须有独立验证门。

### B-1 数据库扩栈
| target | 契约要点（起草时须固化） | 验收 |
|---|---|---|
| **SQL Server** | T-SQL 方言（IDENTITY / NVARCHAR / DATETIME2）；迁移链语义差异（NONCLUSTERED 索引、无 `ON UPDATE`）；entrypoint 容器换 mssql | 生成 DDL 在 SQL Server 容器真实执行；迁移链重放往返 |
| **Oracle** | PL/SQL 方言；序列 + 触发器主键（无 IDENTITY 语义）；VARCHAR2 / NUMBER / CLOB；索引命名 ≤ 30 字符的历史包袱处理 | 同上 |
| **MongoDB**（文档模型专项） | ⚠ 不是 SQL 方言翻译——文档建模需要 RFC 级设计（嵌套 vs 引用、多态集合、事务语义）：先出 **RFC-0011 document-model** 再动代码 | RFC 通过后再排期；验收走真实 mongod 容器 |

### B-2 后端扩栈
| target | 契约要点 | 验收 |
|---|---|---|
| **Spring Boot (Java) + JPA** | `backend.language: java` 路由；entity/dto/service/controller/application.yml；安全语义（auth/access/tenant）显式拒绝或等价实现（参照 NestJS 的边界：完整安全语义是 Rust target 独有，除非明确声明 Java 侧 conformance） | `mvn package` 真实编译；REST 冒烟 |
| **Go: Gin + GORM** | `backend.language: go`；模块结构 / 路由 / sqlx 等价物（GORM 迁徙）；错误响应体对齐 `FORMA_*` 错误码契约（E1–E5 同形） | `go build ./...` + `go vet`；HTTP 冒烟 |
| **PHP: Laravel / .NET** | 同类模式；.NET 用 EF Core / ASP.NET Core | 编译 + 冒烟 |

### B-3 API 风格扩栈
| target | 契约要点 | 验收 |
|---|---|---|
| **gRPC（Protobuf 导出）** | `proto/` 产物 + 按 api operations 裁剪服务定义；枚举/关系映射；`forma export protobuf` 离线导出 | protoc 编译通过；可选：生成 TS/Dart 客户端 stub |
| **tRPC** | 端到端 TS 类型安全；与 GraphQL target 的能力边界差异（无 schema 导出）需在契约中写明 | `tsc --noEmit` + tRPC 类型链断裂测试 |

### B-4 前端扩栈
| target | 契约要点 | 验收 |
|---|---|---|
| **SvelteKit** | 共享 TS 层复用模式（同 Vue：校验/类型同源，访问层用 Svelte Query 原生重写） | `svelte-check` + `vite build` |
| **Angular** | 独立组件 + signals；不引 RxJS 之外的额外方案 | `ng build` |
| **桌面 Tauri / Electron** | 打包壳工程形态：复用既有 rust backend 或 web 产物；窗口/菜单最小配置 | 构建产物可启动 |

### B-5 移动端扩栈（在 Flutter/Expo 之后）
| target | 契约要点 | 验收 |
|---|---|---|
| **SwiftUI / Jetpack Compose** | 原生性能敏感的列表/表单原语映射；共享 TS 层**不可复用**——需要 DSL → Swift/Kotlin 类型层的独立映射契约（这是与 Expo 最大的分叉） | Xcode build / Android assemble（CI 可行性需先验证） |
| **微信小程序 / uni-app** | 国内分发通道；网络层 API 差异（wx.request vs fetch）；表单组件映射 | miniprogram-ci 上传验证或 uni-app build |

### B-6 能力模块扩栈
| 能力 | 契约要点 | 验收 |
|---|---|---|
| **工作流运行时** | RFC-0006 的 command 是单步语义；工作流（多步 + 审批节点 + 状态驻留）需要新的 DSL 构造与运行时——**先出 RFC-0012 workflow-runtime** | RFC 先行；验收走 state machine 端到端 |
| **支付** | 对接外部支付渠道的集成点（零新增依赖宪法下为占位 + 显式集成点）；金额类型必须 Decimal 全链路 | 沙箱冒烟（或模拟 provider） |
| **Playwright e2e 生成** | 从 DSL 页面 + 命令派生 e2e 用例（与 F-004 行为向量同源思路：用例从规格派生，不手写） | `npx playwright test` 全绿 |
| **Token Benchmark 增强** | F-008 目前是 4 chars/token proxy；升级为可选接真实 tokenizer（用户自带 key 时） | 双口径报告，proxy 口径不变为默认 |

### B-7 试点选择建议
**第一个试点建议 gRPC（Protobuf 导出）**：纯增量导出（不碰 DSL 语义、不碰生成工程运行时）、验证门明确（protoc）、能快速验证「契约→实现→验证」模式。第二个试点建议 SvelteKit（共享层复用模式已由 Vue 验证，风险低）。
MongoDB / 工作流运行时**必须 RFC 先行**，不可直接动代码。

---

## 3. 轨道 C：治理收尾

| 项 | 内容 | 判据 |
|---|---|---|
| C-1 RFC 状态升格 | RFC-0001~0010 逐一对照实现状态，Proposed → Accepted/Superseded | 每份 RFC 的 §验收 逐条有实现证据 + `docs validate` 通过 |
| C-2 Conformance 扩面 | 轨道 B 新 target 纳入 conformance suite（semantic / runtime / adapter） | `conformance/` 有对应 profile 且 CI 执行 |
| C-3 spec 同步 | A/B 轨道每关闭一项，对应 spec「已知缺口」移除并标注关闭日期 | gap-backlog 双向一致（登记/关闭规则见 gap-backlog §使用方式） |
| C-4 IR 版本策略回顾 | A-2 / A-6 若引入 IR 语义扩展，走 IR minor bump + 兼容矩阵更新（spec:ir / spec:compatibility） | `IR_VERSION` 演进有 ADR |

---

## 4. 里程碑建议

```text
迭代 1（已完成，2026-11-08）：A-1 / A-3（核实已实现 + 文档同步）、A-6、A-8、A-7、A-4、A-5、A-2
          → 轨道 A 八项全部关闭；连带修复生成后端 4 类存量编译错误
轨道 C（随 A 同步完成）：spec:protocols / spec:ir / spec:serialization / gap-backlog 双向关闭
轨道 B：已撤销，不排期
```

## 5. 边界与不做（本阶段沿用 roadmap §6.4）

- 不做运行时低代码引擎 / 像素级 UI 表达 / 通用编程语言 / 运行时解释执行 DSL。
- 不做破坏「零变化原则」的改动：任何新 target 不得改变默认（Rust backend + MySQL + React web）产物。
- 不做无验证门的扩栈：纯 `cargo test` 覆盖不到的目标层（TS/Dart/SDL/真实 DB/真实构建链）缺验证门的 target 不得进入「已完成」。

## 6. 安全考量

- 新 target 的安全语义（auth/access/tenant 服务端权威执行）是**功能开关级**决策：继承 Rust target 的完整语义，或显式声明边界并给出诊断——**二选一，禁止半实现**（ADR-0004）。
- 逆向导入（A-8）与扩栈 emitters 的构造必须全部经过现有 `check_program` 语义检查，不信任外部输入。

## 7. 验收（本规划的整体 Definition of Done）

1. ✅ 轨道 A 八项全部：实现/核实 + 回归测试 + spec/gap-backlog 双向关闭。
2. ✅ 轨道 C（A 相关部分）：文档状态与实现证据一致；`forma docs validate/lint` 全绿；全量 `cargo test` 无回归。
3. ✅ 生成工程验证门：Rust backend `cargo check`（含 school）零错误零警告；GraphQL / Web `tsc --noEmit`；SDL parse/buildSchema。
4. ⛔ 轨道 B（目标扩栈）：用户决策撤销，不再是完成条件。

## 8. 开放问题（已随轨道 B 撤销关闭）

> 原 §8 的四个问题均针对轨道 B 的投入取舍；B 撤销后不再需要决策，保留原文仅作记录。

1. **Java/Go/PHP 后端的安全语义 conformance**：完整实现（大工程）还是显式拒绝（与 NestJS 一致）？建议：Go/PHP 先显式拒绝，Java 视 Spring Security 集成评估。
2. **原生移动端（SwiftUI/Compose）的投入优先级**：工程量大且与 Flutter/Expo 重叠，建议放 B-5 末位，先做小程序/uni-app（国内分发）。
3. **工作流运行时是否值得 RFC-0012 级投入**：command 已覆盖单步审批流，多步工作流的真实需求需用户确认。
4. **Token Benchmark 真实 tokenizer 口径**：默认保持 proxy 不变，真实口径仅 opt-in。
