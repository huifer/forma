---
id: ADR-0003
title: 产物分三类归属，人工修改不被静默覆盖
status: Accepted
date: 2026-11-04
deciders: [forma-maintainers]
domain: build
supersedes: []
superseded_by: []
related_rfcs: [RFC-0001, RFC-0007]
related_adrs: [ADR-0001, ADR-0002]
---

# ADR-0003：产物分三类归属，人工修改不被静默覆盖

## 1. Context（背景）

`docs/architecture.zh-CN.md` §1 承诺：「目标代码是生成物：带 `@generated` 头，增量重编译，**手改会被缓存检出**；自定义逻辑走 `extension` 扩展点」。

源码核对显示承诺与实现存在明确缺口：

- `@generated` 头**未覆盖全部产物**：`emit/openapi.rs` 与 `emit/diagram.rs` 的产物不带标记；`emit/mysql.rs`、`emit/seed.rs`、`emit/flutter.rs` 三个 emitter 虽带标记，但每次构建无条件重写（无内容哈希跳过），「连续两次 build 零变更」在文件时间戳层面不成立。
- 「手改会被缓存检出」**未实现**：仓库无产物清单（`.forma/` 下只有 `migrations.json`、`ir-snapshot.json`、`build.json`），无所有权记录，自然无从检出。
- `extension` 扩展点**未实现**：全仓搜索 `extension` 仅命中 Dart 的 `extension` 关键字与路径后缀判断。

后果：人工修改过的生成文件会在下次 `forma build` 时被**静默覆盖**，人工工作无声丢失。这是生成式工作流中最不可接受的一类失败。

## 2. Decision（决策）

**产物按归属分三类，且构建器默认不覆盖人工修改过的生成物。**

1. **Generated**：完全由规格生成。重新构建时覆盖——但若检测到磁盘内容与清单记录不符（说明人工改过），则**不覆盖**，产出冲突诊断并保留人工版本。
2. **Extension**：人工实现，Forma 只保证锚点存在，不覆盖内容。
3. **Adopted**：既有代码纳入管理，Forma 检查差异但不覆盖。

配套决策：

- 引入产物清单（`.forma/artifacts.json`），记录每个产物的路径、归属、生成器 id/version、来源规格、哈希、依赖节点。
- 全部产物写入为**原子写**（临时文件 + rename）。
- 全部 emitter 统一使用 `@generated` 头与内容哈希跳过写入。
- 冲突裁决路径：`forma build --adopt`（登记为 Extension/Adopted）、`--overwrite`（显式覆盖，须审计）、或人工改回与清单一致。
- 冲突时构建**成功但带 warning**（可用 `--strict` 提升为失败）。

## 3. Consequences（后果）

- **正面**：人工修改不再丢失；产物可溯源（「这个文件由哪条规格、哪个 emitter 生成」）；`@generated` 头覆盖一致；连续两次构建真正零文件写入。
- **负面**：引入清单读写与 O(产物数) 次哈希比较的开销；「冲突时构建成功但带 warning」降低了 CI 严格性（由 `--strict` 缓解）。
- **中性**：emitter 从「自己写盘」改为「返回产物」是一次内部重构，产物内容零变化；**行为变更点**是「人工修改不再被静默覆盖」——依赖旧行为的流水线会开始看到冲突诊断，属期望内的行为修正，须在 CHANGELOG 显著标注。

## 4. Alternatives Considered（被否决的方案）

- **保持现状（每次构建覆盖生成物）**：拒绝——这是本 ADR 要修正的核心问题；静默丢失人工工作是生成式工作流中最不可接受的失败模式。
- **只补 `@generated` 头，不做冲突检测**：拒绝——头只是标记，不解决「检测」问题；且当前头覆盖不全说明「靠标记维持边界」不可靠。
- **把产物清单纳入 git 跟踪**：拒绝——清单含 `generatedAt` 等本机状态，应与 `.forma/` 一致作为本机状态，否则每次构建都产生 git 噪音。
- **冲突时构建直接失败**：不拒绝但作为次选——默认失败会阻断「只想重建一次」的场景；采用「成功 + warning + `--strict` 可提升」更实用，同时提供逃生门。
- **构建失败时回滚全部已写产物**：拒绝——产物是派生物，重跑即可；全局回滚的复杂度与收益不成比例。
- **用文件时间戳代替哈希**：拒绝——mtime 不表达内容，复制或检出后不可靠。

## 5. References（引用）

- 构建与产物系统：[RFC-0007-build-and-artifact.zh-CN.md](../../rfc/RFC-0007-build-and-artifact.zh-CN.md)
- 架构总览（生成物边界）：[RFC-0001-architecture-overview.zh-CN.md](../../rfc/RFC-0001-architecture-overview.zh-CN.md)
- 治理总入口：[README.md](../../README.md)
- 实现：`crates/forma/src/emit/`、`crates/forma/src/generators.rs`、`crates/forma/src/util.rs`

## 6. Change Log（变更记录）

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版采纳：裁决产物三分类归属、引入产物清单与冲突检测、人工修改不被静默覆盖 |
| 1.1 | 2026-11-05 | **实现落地（F-002）**：`.forma/artifacts.json` 清单（路径/哈希/归属/生成器）；冲突检测采用「构建前快照、构建后恢复」（emitter 在内部写盘，无法前置拦截）；Adopted 条目**无条件保护**（adopt 后的干净状态不再触发快照会导致覆盖——实现期修正）；`--adopt` / `--overwrite`（审计留痕）/ `--strict` 三个裁决旗标（CLI build 与 MCP forma_build 同管线）；`artifact-modified` / `artifact-adopted` / `artifact-overwritten` 诊断。余项：Extension 锚点机制（§2.2）仍未实现；清单的「来源规格 / 依赖节点」字段待补 |
