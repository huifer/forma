# Forma RFC 架构审查清单（Architecture Review Checklist）

> 每份 RFC 在进入 `In Review` 前必须通过本清单。AI 可生成 RFC 与分析变更，但**不得自动批准**
> 任何破坏核心兼容性的架构决策——破坏 Tier 1 契约的变更必须经人工审批。

---

## A. Scope（范围）

- [ ] 问题与目标明确
- [ ] Non-goals 明确
- [ ] 归属领域明确（Core / Capability / Adapter / Runtime）
- [ ] 不重复已有能力（已检索 `docs/architecture/rfc/`、`capability/registry.md`）

## B. Architecture（架构）

- [ ] Core / Capability / Adapter / Runtime 边界明确
- [ ] 依赖方向正确（Tier 1 ← Tier 2 ← Tier 3，禁止反向）
- [ ] 不引入不必要的核心依赖
- [ ] 扩展点明确（未来如何扩展）
- [ ] 失败处理明确（错误传播、降级、回滚）

## C. Specification（规格）

- [ ] 数据结构明确
- [ ] 语义明确（可判定、无歧义）
- [ ] 版本明确
- [ ] 序列化规则明确
- [ ] 未知字段处理明确（保留 / 忽略 / 拒绝）

## D. Compatibility（兼容性）

- [ ] 向后兼容策略明确
- [ ] Breaking Change 已识别并声明（`breaking_change: true`）
- [ ] 迁移策略明确（见 §14 Migration Plan）
- [ ] 依赖兼容矩阵明确

## E. Security（安全）

- [ ] 信任边界明确
- [ ] 权限需求明确
- [ ] 外部输入经过验证
- [ ] 秘密信息不会泄露
- [ ] 插件 / Adapter 执行风险已评估

## F. Quality（质量）

- [ ] Conformance Tests 已定义（见 §16）
- [ ] 单元测试已规划
- [ ] 集成测试已规划
- [ ] 性能指标明确
- [ ] 验收标准可量化（见 §17）

## G. Evolution（演进）

- [ ] 能力可独立扩展（不修改 Tier 1）
- [ ] 能力可独立替换
- [ ] 有废弃机制（Deprecation Policy）
- [ ] 有迁移机制
- [ ] 文档依赖关系已更新（`depends_on` / `related_specs` / `related_adrs`）

---

## 审查结论

| 项 | 结论 |
|---|---|
| 范围 / 架构 / 规格 | ☐ 通过 ☐ 需修改 |
| 兼容性 / 安全 | ☐ 通过 ☐ 需修改 |
| 质量 / 演进 | ☐ 通过 ☐ 需修改 |
| **是否触碰 Tier 1 契约** | ☐ 否 ☐ 是（需人工审批） |

审查人：__________  日期：__________  决策：☐ Accepted ☐ In Review ☐ Rejected

---

## 附录：`forma docs` 命令 ↔ 清单条目映射（RFC-0009 §18，2026-11-08）

| docs 命令 | 检查内容 | 对应清单条目 |
|---|---|---|
| `docs validate` | frontmatter / ID 唯一 / 状态合法 / 依赖存在 / 无循环 | 「元数据完整」「依赖闭合」 |
| `docs lint` | 模板章节完整性（核心章节缺失 = error） | 「结构完整」 |
| `docs graph` | 依赖图（节点 + 边，`--json` 可机读） | 「依赖可视化」 |
| `docs check-compatibility` | 破坏性变更检测（spec 删除 / major bump / MUST 弱化） | 「兼容性」 |
| `docs check-dependencies` | 引用存在 / 自依赖 / 循环 | 「依赖闭合」 |
| `docs check-links` | 内部相对链接有效 | 「链接有效」 |
| `capability check` | 注册表 / 依赖闭包 / 版本兼容（需工程目录） | 「能力契约」 |

CI `docs` job 强制执行 `validate` + `lint` + `capability check` + `check-compatibility`（PR 基线）。

