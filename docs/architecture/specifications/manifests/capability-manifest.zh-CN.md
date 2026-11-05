---
id: spec:manifests
title: Capability Manifest Schema
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-05
domain: capability
governing_rfc: RFC-0004
depends_on: [RFC-0004]
related_specs: [spec:ir, spec:compatibility]
breaking_change: false
---

# spec:manifests — Capability Manifest 模式

定义 Capability 的统一 Manifest 结构（机器可读）。
**实现**：`crates/forma/src/capability.rs`（`CapabilityManifest` / `CapabilitySpec` / `Compatibility` / `Conformance`）。

---

## 1. 顶层结构

```yaml
apiVersion: forma.dev/v1
kind: Capability
metadata:
  id: <category>.<name>        # 全局唯一，如 visualization.echarts
  version: <semver>            # Capability Version
  description: <可选说明>
spec:
  category: <data|visualization|ui|workflow|integrations>
  provides: [<capability-token>, ...]   # 如 chart.bar
  consumes: [<IR 或 数据契约>, ...]      # 如 ChartIR, DataBinding
  adapters: [<adapter-id>, ...]
  runtime:  [<runtime-id>, ...]
  dependencies: [<capability-id>, ...]
  permissions: [<permission>, ...]       # 如 data.read
  compatibility:
    ir: "<range>"              # 兼容的 IR 版本范围
    runtime: "<range>"
  conformance:
    suite: <suite-id>          # 对应 conformance/ 下的测试套件
```

## 2. 字段规则

- **`apiVersion`**：**MUST** 等于 `forma.dev/v1`；未知版本 → 拒绝注册。
- **`kind`**：**MUST** 等于 `Capability`。
- **`id`**：**MUST** 形如 `<category>.<name>`；**MUST** 全局唯一（重复 → 拒绝注册）。
- **`version`**：**MUST** 是 semver；破坏性变更（字段删除 / 语义变更）须 `breaking_change: true` 并在 ADR 记录迁移。
- **`provides`**：本能力对外暴露的能力令牌，供 DSL / 其他 Capability 引用。
- **`consumes`**：本能力消费的 IR 模块或数据契约；**MUST** 是 Tier 1 / Tier 2 已定义契约，**MUST NOT** 引入新通用语义（见 RFC-0001 §7.3 红线）。
- **`adapters`**：**MUST** 是编译器已实现的 adapter；未实现的 adapter → **警告**（声明被接受，但不产生额外产物）。
- **`dependencies`**：**MUST** 指向已注册能力；未知依赖 → **错误**；依赖成环 → **错误**。
- **`permissions`**：权限声明，构建期汇总为权限并集。
- **`compatibility.ir`**：兼容的 IR 版本范围；**MUST** 匹配编译器实际提供的 IR 版本（范围语法见 [spec:compatibility §3](../compatibility/compatibility-rules.zh-CN.md)）。
- **`conformance.suite`**：实现必须通过的测试套件标识（见 `conformance/`）。

## 3. 与 RFC-0004 的关系

本 spec 是 RFC-0004 §8.2 示例字段的**正式定义来源**，也是 Rust 类型的直接对应物。
Adapter 的实现细节不在本 spec 范围。

## 4. 实现映射

| Manifest 字段 | Rust 类型 | 校验位置 |
|---|---|---|
| `apiVersion` | `CapabilityManifest::api_version` | `register()` |
| `kind` | `CapabilityManifest::kind` | `register()` |
| `metadata.id` | `CapabilityMetadata::id` | `register()`（格式 + 唯一性） |
| `metadata.version` | `CapabilityMetadata::version` | `register()`（semver） |
| `spec.category` | `CapabilitySpec::category` | 自由字段（分类展示） |
| `spec.provides` | `CapabilitySpec::provides` | 能力令牌 |
| `spec.consumes` | `CapabilitySpec::consumes` | 消费的 IR / 数据契约 |
| `spec.adapters` | `CapabilitySpec::adapters` | `resolve()`（未实现 → 警告） |
| `spec.runtime` | `CapabilitySpec::runtime` | 运行环境 |
| `spec.dependencies` | `CapabilitySpec::dependencies` | `resolve()`（未知 / 成环 → 错误） |
| `spec.permissions` | `CapabilitySpec::permissions` | `resolve()`（汇总） |
| `spec.compatibility.ir` | `Compatibility::ir` | `resolve()`（版本范围匹配） |
| `spec.compatibility.runtime` | `Compatibility::runtime` | `resolve()`（版本范围匹配） |
| `spec.conformance.suite` | `Conformance::suite` | 测试套件标识 |

### 4.1 发现与注册

- **内置能力**：`CapabilityRegistry::with_builtins()`（权威副本见 `builtin_manifests()`）。
- **项目能力**：扫描 `<project>/capabilities/*.yaml`（或 `.yml`），逐个 `register()`。
- 注册失败 → `capability-manifest-invalid`（错误），构建阻断。

### 4.2 启用与解析

工程在 `forma.yaml` 的 `capabilities:` 段声明启用的能力：

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

注册表解析**依赖闭包**并输出：启用集（区分显式 / 依赖带入）、权限并集、诊断。
**能力错误阻断构建**（未知能力 / 依赖缺失 / 版本不兼容 / pin 不匹配）。

## 5. 开放问题

- `apiVersion` 的演进与 `forma.dev/v1` 之外的版本如何声明？
- 多 runtime 下 `permissions` 的作用域粒度（capability 级 vs 操作级）？
- Manifest 是否应携带 `provides` 的能力级语义版本（如 `chart.bar@1`）？
- 项目锁文件是否应从 `forma.yaml` 迁出到 `.forma/lock.json`。

## 6. 参考

- RFC-0004：[../../rfc/RFC-0004-capability-system.zh-CN.md](../../rfc/RFC-0004-capability-system.zh-CN.md)
- 实现：`crates/forma/src/capability.rs`
- 注册表：[../../capability/registry.md](../../capability/registry.md)
- 兼容规则：[../compatibility/compatibility-rules.zh-CN.md](../compatibility/compatibility-rules.zh-CN.md)
