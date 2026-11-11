# 迁移安全：不是所有 SQL 变更都能安全回滚

> 验证体系见 [verification.zh-CN.md](verification.zh-CN.md) · 迁移引擎实现见 `crates/forma/src/emit/migrate.rs`
> · CLI 见 `crates/forma/src/main.rs` 的 `forma migrate`

Forma 的迁移引擎已经有基线、升级、回滚、哈希、迁移链验证与 SQL 重放模拟器（见
[architecture.zh-CN.md § 迁移](architecture.zh-CN.md)）。但数据库迁移有一个被低估的事实：

> **不是所有 SQL 变更都可以安全地自动回滚。**

例如：

```sql
ALTER TABLE orders DROP COLUMN customer_email;
```

即使生成了反向 SQL：

```sql
ALTER TABLE orders ADD COLUMN customer_email VARCHAR(255);
```

原始数据也不会回来。回滚语句能重建列，但不能重建数据。因此 Forma 在「能否回滚」之外，额外引入**迁移风险分级**，把「安全 / 需审批 / 禁止自动」显式化。

## 1. 风险分级（L5 变更安全的核心）

每条迁移操作按破坏性分为四级。分级是 `forma migrate risk` 与 `up --execute` 前置门禁的依据。

| 变更 | 风险 | 回滚语义 | 建议 |
|---|---|---|---|
| 新增可空字段（`add_column`，以 NULL 落地便于存量回填） | **Low** | 安全 | 自动 |
| 新增表 / 索引 / 唯一约束 / 外键（纯增量、可逆） | **Low** | 安全 | 自动 |
| 删除索引 / 唯一约束 / 外键 | **Medium** | 可逆，但影响查询计划 | 检查影响范围 |
| 修改字段类型（`modify_column`，精度 / 可空变化） | **High** | 数据兼容风险 | 检查数据兼容 |
| 删除字段（`drop_column`） | **High** | **回滚丢数据** | 人工审批 |
| 删除表（`drop_table`） | **Extreme** | 数据销毁 | 强制审批 |
| 修改主键 / 数据转换 | **Extreme** | 专项迁移 | 强制审批 |

分级的机器可读代码：`RISK_LOW` / `RISK_MEDIUM` / `RISK_HIGH` / `RISK_EXTREME`。
`High` 与 `Extreme` 需要人工审批（`requires_approval() == true`）。

> **Fail-safe 原则**：未识别的操作一律按 `High` 处理，避免静默放过破坏性变更。

### 实现

分级逻辑集中在 `crates/forma/src/emit/migrate.rs`：

- `enum MigrationRisk { Low, Medium, High, Extreme }`（`PartialOrd`，升序）
- `fn classify_op(kind: &str) -> MigrationRisk`
- `struct MigrationRiskReport { overall, counts, approval_required }`
- `fn migration_risk_lines(ops) -> Vec<String>`、`fn pending_migration_ops(project_dir, program) -> Vec<MigrationOp>`

`ops` 由 `diff_ops(old_snapshot, new_plans)` 产生，每条 `MigrationOp.kind` 直接对应上表的变更类型。

## 2. `forma migrate risk` — 应用前先评估破坏性

在 `up --execute` 之前，先运行：

```bash
cargo run -- migrate examples/school risk
```

输出示例：

```text
迁移风险概览：整体 HIGH（RISK_HIGH） — 需人工审批
  EXTREME  0 项
  HIGH     1 项
  MEDIUM   0 项
  LOW      2 项
逐项：
  [LOW] school: 新增列 phone VARCHAR(20)（可空，便于存量回填）
  [LOW] school: 新增索引 idx_school_voter (voter)
  [HIGH] school: 删除列 remark VARCHAR(255) ⚠ 需审批
```

`risk` 计算「自上次构建以来」的待执行操作（旧快照 → 当前 DSL schema），让开发者在落库前就看清破坏性。

## 3. Expand-Contract：从「SQL Generator」进化为「Schema Evolution Engine」

单纯生成反向 SQL 不足以应对 `drop_column` / `modify_column` 这类不可逆变更。Forma 规划采用
**Expand-Contract（扩展—收缩）** 模式，把破坏性变更拆成可逆的多个阶段：

```text
阶段 1：新增兼容字段 / 双写        （Expand：加列、加冗余、双写或回填）
阶段 2：数据回填与一致性校验
阶段 3：切换读取到新字段
阶段 4：停止旧字段写入
阶段 5：确认无旧版本依赖
阶段 6：删除旧字段               （Contract：此时删列才安全）
```

这要求迁移引擎区分「纯 DDL 变更」与「数据转换 / 收缩」两类操作，并对后者强制审批或独立迁移脚本。当前 `forma migrate` 已保证链完整与重放一致；Expand-Contract 的阶段化执行与数据转换迁移脚本是下一步（见 [next-milestone.zh-CN.md](next-milestone.zh-CN.md) 的 F-002 扩展）。

## 4. 与验证体系的衔接

迁移风险属于 [verification.zh-CN.md](verification.zh-CN.md) 的 **L5 变更安全**：

```text
L3 产物可编译  ──▶  L4 行为验证  ──▶  L5 变更安全
                                  │
                                  ├─ forma migrate risk     （破坏性分级）
                                  ├─ forma migrate verify    （链完整 + 重放一致）
                                  ├─ forma graph / impact    （语义影响闭包）
                                  └─ @generated / extension  （产物所有权边界）
```

## 5. 参考

- 验证体系：[verification.zh-CN.md](verification.zh-CN.md)
- 迁移引擎实现：`crates/forma/src/emit/migrate.rs`
- 近期里程碑：[next-milestone.zh-CN.md](next-milestone.zh-CN.md)
