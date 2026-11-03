# Agent 原生工作流：Agent 是一等用户

> 验证体系见 [verification.zh-CN.md](verification.zh-CN.md) · 架构见 [architecture.zh-CN.md](architecture.zh-CN.md)
> · MCP 实现见 `crates/forma/src/mcp.rs`

Forma 的核心用户不是手动写 DSL 的程序员，而是 **AI Agent**（Codex、Claude Code、Pi 等）。因此 Agent 集成不是「把 CLI 包装成命令入口」，而是一套可靠的工作流：Agent 理解需求、设计系统、提出规格补丁，Forma 解析 / 校验 / 分析影响 / 确定性编译 / 验证 / 输出证据，人只审核业务意图与变更证据。

## 1. 核心理念：按需上下文，而非全量源码

Agent 不应该直接持有全部源码和全部 IR。应该**按需获取上下文**——只读当前任务所需的语义切片，而不是把整份生成工程塞回上下文。

```text
AI 理解需求
   │
   ▼
forma_graph      秒级理解系统现状（实体级语义摘要）
   │
   ▼
forma_impact     计算确定性变更闭包（影响哪些字段 / API / 规则 / 页面）
   │
   ▼
forma_patch      提交最小化的语义补丁（数十 Token，非全量重写）
   │
   ▼
forma_validate  跨目标结构化诊断
   │
   ▼
forma_build     确定性编译 + L3/L4 验证
   │
   ▼
forma_report    输出完整变更报告（影响 + 风险 + 证据）
```

## 2. 推荐的工作流

```text
用户需求
   │
   ▼
Agent 理解意图，检索现有规格（forma_graph / forma_search）
   │
   ▼
设计必要的实体 / API / 规则变更
   │
   ▼
forma_patch     提交语义补丁（AST 级，事务式）
   │
   ▼
forma_validate 语法 + 语义检查（失败 → 结构化诊断 → 局部修复）
   │
   ▼
forma_impact    变更影响分析（高 / 低 / 破坏性）
   │
   ▼
forma_build     生成 Rust / React / MySQL / OpenAPI / Flutter
   │
   ▼
验证（L3 产物可编译 · L4 行为 · L5 迁移风险）
   │
   ▼
forma_report    审查报告
   │
   ▼
人审业务意图 → 批准 / 驳回
```

整个过程**不需要 Agent 读取并重新生成所有业务源码**——AI 输出永远是语义补丁，目标代码由编译器生成。

## 3. 结构化诊断（创新点）

不要只返回 `Error: Invalid field`。返回机器可读、可局部修复的结构：

```json
{
  "code": "E2031",
  "severity": "error",
  "message": "引用的字段不存在",
  "location": { "file": "forma/orders.fm", "line": 28, "column": 14 },
  "subject": "Order.customer",
  "suggestions": [{ "type": "similar_field", "value": "customerId" }],
  "related": [{ "file": "forma/customer.fm", "entity": "Customer" }]
}
```

Agent 可以直接据此做局部修复，不必反复重读整份文档。Forma 的诊断体系见 `crates/forma/src/validate.rs`（`Diagnostic { severity, code, ... }`），MCP 工具直接返回稳定 `code`。

## 4. 规格补丁应有事务机制

不要让 Agent 直接写 `.fm` 文件再执行 build。推荐：

```text
当前规格
   │
   ▼
Proposed Patch（AST 级语义补丁）
   │
   ▼
Patch Validation（字段 / 类型 / 业务规则）
   │
   ▼
Semantic Diff（确定性变更闭包）
   │
   ▼
Impact Analysis（影响范围 + 风险等级）
   │
   ▼
Apply / Reject（原子应用，失败全回滚）
   │
   ▼
Build + Verify → Report
```

补丁必须携带：Patch ID、Base Revision、目标文件及语义节点、变更内容、影响范围、验证结果、应用结果，并支持并发修改冲突检测。这样 Forma 才能成为 Agent 的可靠执行基础设施，而非辅助生成工具。

## 5. 建议的 Agent Tool API（按任务组织，而非按 CLI 命令暴露）

| Tool | 功能 | 优先级 | 当前状态 |
|---|---|---|---|
| `forma_inspect` | 项目概览、规格摘要 | P0 | ✅ `forma_graph` / `mcp` |
| `forma_search` | 搜索实体 / 字段 / API / 规则 | P0 | 🔨 规划 |
| `forma_graph` | 获取语义依赖图 | P0 | ✅ `forma_graph` |
| `forma_get_context` | 获取指定任务所需上下文 | P0 | 🔨 规划 |
| `forma_validate` | 语法 + 语义检查 | P0 | ✅ `forma_check` / `forma_validate` |
| `forma_propose_patch` | 提交规格变更提案 | P0 | ✅ `forma_patch` |
| `forma_diff` | 规格差异分析 | P0 | ✅ `forma_diff` |
| `forma_impact` | 变更影响分析 | P0 | ✅ `forma_impact`（graph 闭包） |
| `forma_plan` | 生成执行计划 | P0 | 🔨 规划 |
| `forma_build` | 编译产物 | P0 | ✅ `forma_build` |
| `forma_verify` | 验证编译与行为 | P0 | ✅ `forma_migrate verify` / 产物编译 |
| `forma_report` | 输出完整审查报告 | P0 | 🔨 规划 |
| `forma_explain` | 解释规格与产物的关系 | P1 | 🔨 规划 |
| `forma_adopt` | 接管人工修改的产物 | P1 | 🔨 规划（`extension` 机制） |
| `forma_migrate` | 迁移计划与风险分析 | P1 | ✅ `forma_migrate {status|history|verify|up|down|risk}` |

## 6. 起点验收示例

给 Agent 一个真实需求，例如：

> 给订单增加优惠券功能，优惠券金额不能超过订单金额，且需要在后台展示优惠券使用记录。

Agent 应当能够：读取当前项目规格 → 找到订单相关实体 → 设计数据结构 → 设计 API 与业务规则 → 生成规格补丁 → 检查字段 / 类型 / 业务规则 → 分析数据库迁移风险 → 生成 Rust 与 React 产物 → 执行编译与测试 → 输出完整变更报告。

## 7. 参考

- 验证体系：[verification.zh-CN.md](verification.zh-CN.md)
- 迁移安全：[migration-safety.zh-CN.md](migration-safety.zh-CN.md)
- 架构：[architecture.zh-CN.md](architecture.zh-CN.md)
- 近期里程碑：[next-milestone.zh-CN.md](next-milestone.zh-CN.md)
