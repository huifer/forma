---
id: RFC-0007
title: Build and Artifact System
status: Proposed
version: 1.0
authors: [forma-maintainers]
created: 2026-11-04
updated: 2026-11-04
domain: build
supersedes: []
superseded_by: []
depends_on: [RFC-0001, RFC-0003]
related_adrs: [ADR-0001, ADR-0003]
related_specs: [spec:manifests, spec:ir, spec:compatibility]
breaking_change: false
---

# RFC-0007：构建与产物系统（Build and Artifact System）

## 1. Metadata

见 frontmatter。本 RFC 规定**构建管线**与**产物治理**：编译阶段契约、产物三类归属（Generated / Extension / Adopted）、产物清单与来源追踪、增量构建、可复现构建。

## 2. Abstract

Forma 当前的构建是「全量重新生成 + 按内容哈希跳过写入」。这在功能上正确（同一规格两次构建产物一致，已有回归断言），但**产物治理缺位**：

1. **没有产物清单**——无法回答「这个文件由哪条规格、哪个 emitter、哪个版本生成」。
2. **没有归属判定**——人工修改过的生成文件会在下次构建被**静默覆盖**（除 4 个 emitter 外，见 §3.1）。
3. **增量粒度粗**——跳过写入按内容哈希，而非按语义影响面（改了无关实体也会触发全量重算）。
4. **`@generated` 头覆盖不全**——`openapi.rs` / `diagram.rs` 两个 emitter 的产物不带 `@generated` 标记（见 §3.1）。

本 RFC 定义产物清单格式、三类归属语义与**冲突检测**（人工修改不被静默覆盖）、按影响面的增量构建，以及可复现构建的完整依赖锁定。

## 3. Motivation

### 3.1 Current Problem

**构建管线现状**（`crates/forma/src/build.rs::run_build`）：

```text
load_dsl_program → check_program（语义检查，硬错误阻断）
    ↓
db-mysql → backend-rust → web-react → viz-web → openapi → diagrams → migrate → seed → mobile-flutter
```

共 9 个 emitter 步骤，产物写入各自 target 的 `out` 目录。

**逐 emitter 的写入策略不一致**（源码核对结果）：

| emitter | 内容哈希跳过写入 | `@generated` 头 |
|---|---|---|
| `web_react` | ✅ | ✅ |
| `rust_backend` | ✅ | ✅ |
| `visualization` | ✅ | ✅ |
| `migrate` | ✅ | ✅ |
| `openapi` | ✅ | ❌ 无（`openapi.json` / `swagger-ui.html`） |
| `diagram` | ✅ | ❌ 无（HTML 图页） |
| **`mysql`** | ❌ 直接 `write_text` | ✅（SQL 注释形式） |
| **`seed`** | ❌ 直接 `write_text` | ✅（SQL 注释 / JSON 注释形式） |
| **`flutter`** | ❌ 直接 `write_text` | ✅ |

两个 emitter 每次构建都重写文件（内容可能相同），导致「连续两次 `forma build` 零变更」这一承诺在**文件时间戳**层面不成立（内容层面仍一致）；另有 `openapi` / `diagram` 的产物缺 `@generated` 标记，人工编辑与生成内容的边界不可辨。

> 注：`mysql` 的基线 `0001_init.sql` 采用**不可变基线**策略——文件已存在则不再重写（见 `crates/forma/src/emit/migrate.rs` 的基线不变量），因此其写入行为与内容哈希跳过是两种不同的幂等机制。

**其他缺口**：

- **无产物清单**：`.forma/` 下只有 `migrations.json`（迁移台账）、`ir-snapshot.json`（迁移快照）、`build.json`（联邦增量缓存），**没有**记录「文件 → 来源规格 → emitter → 版本」的产物清单。
- **无冲突检测**：人工修改过的生成文件，下次构建直接覆盖。
- **无版本锁定**：产物不可复现性取决于编译器版本 + 依赖版本 + 配置，二者均未记录进产物。

### 3.2 Why Existing Architecture Is Insufficient

`docs/architecture.zh-CN.md` §1 承诺「目标代码是生成物：带 `@generated` 头，增量重编译，手改会被缓存检出；自定义逻辑走 `extension` 扩展点」。但：

- `@generated` 头**未覆盖全部产物**；
- 「手改会被缓存检出」**未实现**（无所有权记录，自然无从检出）；
- `extension` 扩展点在代码中**尚不存在**（全仓搜索 `extension` 仅命中 Dart 的 `extension` 关键字与路径后缀判断）。

承诺与实现之间存在明确缺口。

### 3.3 Expected Outcomes

- 每个生成产物都可追溯到「哪条规格 + 哪个 emitter + 哪个版本」。
- 人工修改生成文件不会被静默覆盖，而是**显式冲突**。
- 增量构建按语义影响面计算，无关模块不重算。
- 相同输入产生可复现产物，且复现所需的全部依赖被记录。

## 4. Goals

- 定义编译阶段契约与 emitter 接口。
- 定义产物三类归属语义与冲突处理策略。
- 定义产物清单（Artifact Manifest）格式与来源追踪。
- 定义增量构建的粒度与失效条件。
- 定义可复现构建的依赖锁定要求。

## 5. Non-goals

- 不定义 IR 语义（见 RFC-0003）。
- 不定义 Capability 扩展机制（见 RFC-0004）。
- 不定义验证等级（见 [verification.zh-CN.md](../../verification.zh-CN.md)）。
- 不定义部署与环境配置（属 RFC-0008 的部署部分）。
- 不实现产物签名或供应链安全（见 §10 风险登记）。

## 6. Terminology

- **Generated**：完全由规格生成，可被随时重新生成覆盖。
- **Extension**：人工实现、Forma 只保证其锚点存在，不覆盖其内容。
- **Adopted**：既有代码被纳入管理，Forma 检查差异但不主动覆盖。
- **Artifact Manifest**：记录每个产物的来源、归属、哈希与版本的清单。
- **Ownership**：产物归属判定结果（Generated / Extension / Adopted）。
- **Reproducible Build**：相同输入与锁定依赖产生 byte 级相同产物。

## 7. Architectural Context

### 7.1 System Context

```text
   规格 (.fm) ──Lowering──▶ Semantic Core ──▶ Build Plan
                                                  │
                            ┌─────────────────────┼─────────────────────┐
                            ▼                     ▼                     ▼
                        Adapter A            Adapter B            Adapter C
                            │                     │                     │
                            └─────────────────────┴─────────────────────┘
                                          ▼
                                 Artifact Manager
                        （清单 · 哈希 · 归属 · 冲突检测 · 原子写入）
                                          ▼
                                    生成产物
```

### 7.2 Dependencies

- 依赖 RFC-0003：影响面（增量粒度）建立在语义依赖图之上。
- 依赖 RFC-0001：生成物与人工逻辑的边界是核心契约的一部分。

### 7.3 Boundaries

- Adapter **只写**其声明负责的产物路径；不得写其他 target 的目录。
- 构建器**不得**覆盖 Extension / Adopted 归属的产物内容。
- 产物清单是**构建器的**职责，不得由 adapter 自行维护。
- 构建器**不解释**规格语义（语义属 Semantic Core）。

## 8. Design

### 8.1 Conceptual Model

构建是三段：**计划 → 生成 → 落盘**。当前实现把「生成」与「落盘」混在 emitter 内（每个 emitter 自己写文件），这是产物治理缺位的根因。本 RFC 把落盘统一收归 **Artifact Manager**。

```text
Plan（哪些节点受影响 → 哪些产物需重算）
  ↓
Generate（Adapter 产出内存中的 GeneratedFile，不落盘）
  ↓
Manage（归属判定 → 冲突检测 → 哈希比较 → 原子写入 → 更新清单）
```

### 8.2 Architecture

**Emitter 接口契约**（统一现有 9 个自由函数形态）：

```rust
pub trait Emitter {
    fn id(&self) -> &'static str;                    // "gen-mysql"
    fn version(&self) -> &'static str;               // 产物失效键
    fn emit(&self, ir: &SemanticGraph, cfg: &TargetConfig)
        -> Result<Vec<GeneratedFile>>;                // 只产出，不落盘
}
```

约束：

- **纯投影**：输入只有 IR 与配置，输出 byte 级确定（无随机、无时间依赖、无环境读取）。
- **不落盘**：产物由 Artifact Manager 统一写入。
- **声明归属**：每个产物路径声明其归属类别与来源规格。

> 这是 [roadmap.zh-CN.md §6.1](../../roadmap.zh-CN.md) 已声明的接口契约的**落地**——当前 emitter 是自由函数（`emit_mysql` / `emit_rust_backend` / …），契约存在于文档但未实现。

**产物清单格式**：

```yaml
apiVersion: forma.dev/v1
kind: ArtifactManifest
metadata:
  project: school
  specification: "0.3.0"
  irVersion: "1.0.0"
  compilerVersion: "0.3.0"
  generatedAt: "2026-11-04T10:00:00Z"   # 唯一允许的非确定性字段
spec:
  artifacts:
    - path: generated/backend/src/models.rs
      kind: generated
      generator: { id: gen-rust-axum, version: "1.0.0" }
      sources:
        - { file: forma/app.fm, line: 12 }
      hash: "sha256:..."
      dependsOn: ["school/entity/Student", "school/entity/Enrollment"]
    - path: generated/web/src/components/OrderTable.tsx
      kind: extension
      generator: { id: gen-web-react, version: "1.0.0" }
      sources: [{ file: forma/app.fm, line: 40 }]
      hash: "sha256:..."
      note: "人工实现，Forma 只保证锚点存在"
```

**三类归属语义**：

| 归属 | 谁写内容 | 重新构建时 | 人工修改后 |
|---|---|---|---|
| **Generated** | 编译器 | 覆盖 | **冲突**（报告并保留人工版本） |
| **Extension** | 人 | 不动（仅校验锚点存在） | 保留 |
| **Adopted** | 人（原先） | 差异报告，不覆盖 | 保留 |

**冲突检测**（当前完全缺失，本 RFC 核心新增）：

```text
构建器计算目标内容 hash
  ↓
与清单中记录的 hash 比较
  ├─ 相同 → 跳过写入（无变更）
  ├─ 不同，且磁盘内容 == 清单 hash（纯生成漂移）→ 覆盖（正常重生成）
  └─ 不同，且磁盘内容 != 清单 hash（人工改过）→ **冲突**
       → 不覆盖；产出 conflict 诊断；要求人工裁决
```

裁决方式：
- `forma build --adopt`：把人工版本登记为 Adopted/Extension，后续不再覆盖；
- `forma build --overwrite`：显式覆盖人工修改（须记录审计）；
- 人工手动把文件改回与清单一致。

> **这是行为变更**：当前生成文件被静默覆盖；变更后默认**不覆盖**并显式报冲突。为避免破坏既有工作流，冲突时构建**成功但带 warning**，并提供 `--overwrite` 逃生门。

### 8.3 Data Model

**原子写入**：所有产物写入必须是「写临时文件 → fsync → rename」，避免构建中断导致半截文件。当前 `util::write_text` 是直接 `File::create` + `write_all`（无原子性），需替换为原子写。

**增量失效条件**（当前按内容哈希逐文件比较，粒度是「文件」；本 RFC 改为「语义节点」）：

| 变更 | 失效范围 |
|---|---|
| 改实体字段类型 | 该实体的表列 + 模型 + 表单 + 引用该字段的图表 + 迁移 |
| 改枚举变体 | 引用该枚举的实体 + 模型 + 表单 + 校验 |
| 加实体 | 该实体相关全部产物（不含其他实体） |
| 改页面 | 该页面 + 其依赖的实体查询产物 |
| 改图表 | 该图表 + 其宿主页面 |

判定依据是 RFC-0003 M3 的依赖边。**在依赖图就绪前**（M3 之前），保持当前的文件级内容哈希行为——增量语义化是 M3 之后的增量工作，不阻塞本 RFC 其余部分落地。

**可复现构建的锁定项**：

| 锁定对象 | 记录位置 |
|---|---|
| 规格内容哈希 | 清单 `specification` + 各产物 `sources` |
| IR 版本 | 清单 `metadata.irVersion` |
| 编译器版本 | 清单 `metadata.compilerVersion` |
| Adapter 版本 | 各产物 `generator.version` |
| 目标配置 | 清单新增 `targetConfig` 段（框架、UI 库、路由、数据层等） |
| 外部依赖 | `forma.yaml` 的 lock 段（前端依赖、模板版本） |

**唯一允许的非确定性**：清单中的 `generatedAt` 时间戳。**产物内容本身必须无时间戳**——否则不可复现。

### 8.4 Interfaces and Contracts

- **Adapter → Artifact Manager**：只产出 `GeneratedFile { path, content, generator }`（现有结构已具备 `generator` 字段，见 `crates/forma/src/generators.rs`）。
- **Artifact Manager → 磁盘**：唯一写盘入口；负责原子写、哈希比较、冲突检测、清单更新。
- **清单 → 工具面**：`forma_adopt`（RFC-0006）、`forma_report`、CI 校验均消费清单。

### 8.5 Execution Flow

```text
1. load_dsl_program → check_program（error 阻断）
2. 计算影响面（RFC-0003 依赖图就绪后）或全量（当前）
3. 逐 emitter 产出内存产物（不落盘）
4. Artifact Manager：
   a. 读取既有清单
   b. 判定归属（Generated / Extension / Adopted）
   c. 冲突检测（磁盘 vs 清单 hash）
   d. 原子写入变更文件
   e. 更新清单（含新 hash、来源、依赖）
5. 输出构建报告（新增/变更/跳过/冲突）
```

### 8.6 Extension Points

- **新 emitter**：实现 `Emitter` trait，声明 id / version / 归属策略即可接入统一落盘。
- **新归属类别**：扩展 `kind` 枚举（如 `generated-template`）。
- **新冲突裁决策略**：扩展 `--adopt` / `--overwrite` 之外的策略（如 `--merge`）。
- **产物分组**：清单可按 target 分组，便于分目录审查。

## 9. Compatibility

- **Backward**：`GeneratedFile` 结构不变；emitter 从「自己写盘」改为「返回产物」是**内部重构**，对产物内容零影响。
- **Forward**：清单中未知字段必须保留（供未来版本使用），不得擅解释。
- **Version Compatibility**：
  - 清单格式版本独立于 IR 版本与编译器版本；
  - 清单 `apiVersion` 变更按 minor/major 管理（新增字段 = minor，字段语义变更 = major）；
  - 旧清单读取失败时按「无清单」处理（等价于首次构建），**不得**因此构建失败。
- **Migration Compatibility**：schema 变更产物（迁移 SQL）遵循既有配对契约（upgrade + rollback + 台账），本 RFC 不改变该契约，只把它纳入清单。
- **Deprecation Policy**：`@generated` 头格式变更须经一个 minor 版本的兼容读取期。

## 10. Security Considerations

- **构建期执行**：生成的 Rust/TS/Dart 代码会在下游被编译与执行。构建器**不执行**生成代码，但必须保证：
  - 语义检查在生成之前（`error` 阻断），规则表达式不引入任意代码；
  - emitter 不把规格内容注入到代码字符串的**结构位置**（如 Rust 标识符位置），避免代码注入。既有实现已对标识符做 `to_snake_case` 归一（`identity.rs`），须保持。
- **产物清单的信任**：清单是本地状态文件，可能被篡改。清单用于**冲突检测**而非授权，因此篡改清单最多导致错误覆盖人工文件——因此清单须与产物一同纳入版本管理或明确标注为本机状态（与 `.forma/migrations.json` 的处理一致）。
- **路径穿越**：产物路径来自 emitter 生成的相对路径，写盘前必须校验不逃逸出 target 目录（`..`、绝对路径、符号链接）。
- **密钥**：产物与清单中不得包含密钥；Secret 只能通过 Secret Provider 在运行时注入。
- **供应链**：产物清单记录 emitter 与其版本，是审计的起点；但**不构成**签名或来源证明（见 §18 Open Questions）。

## 11. Performance and Reliability

- **增量收益**：影响面就绪后，改单字段只重算相关实体产物。当前为全量重算（跳过写入只省磁盘 IO，不省 CPU）。
- **写入原子性**：单个文件写入失败不产生半截文件；构建整体失败时，已写文件保持已写状态并在报告中列出（不做全局回滚——产物是派生物，重建即可）。
- **确定性**：同一规格两次构建产物 byte 级一致（既有测试 `crates/forma/tests/build.rs` 与 `crates/forma/tests/visualization.rs` 已覆盖）。
- **性能目标**：清单读写为 O(产物数)；冲突检测为 O(产物数) 次哈希比较。

## 12. Alternatives Considered

- **保持「全量重算 + 内容哈希跳过」不做清单**：拒绝——无法回答「这个文件从哪来」，人工修改被静默覆盖，且 `@generated` 头覆盖不全使归属边界不可辨。
- **把清单写进 git 跟踪的目录**：拒绝——清单含 `generatedAt` 等本机状态，应与 `.forma/` 一致地作为本机状态（否则每次构建都产生 git 噪音）。
- **构建失败时回滚全部已写产物**：拒绝——产物是派生物，重跑即可；全局回滚的复杂度与收益不成比例。
- **用文件时间戳代替哈希**：拒绝——mtime 不表达内容，且复制/检出后不可靠。
- **每次构建都覆盖生成文件（现状）**：拒绝——这是当前要修正的核心问题（静默丢失人工修改）。

## 13. Trade-offs

- 引入清单与冲突检测增加构建期开销（O(产物数) 次哈希）与磁盘占用，换取产物可溯源与人工修改不被丢失。
- 「冲突时构建成功但带 warning」降低了 CI 的严格性；缓解方式是提供 `--strict` 开关（冲突即失败）。
- 统一落盘要求 emitter 改造（从自己写盘改为返回产物），是一次内部重构，换取产物治理的集中化。

## 14. Migration Plan

| 步 | 内容 | 完成判据 |
|---|---|---|
| **B1** | 补齐 `@generated` 头（`openapi` / `diagram`）与内容哈希跳过写入（`mysql` / `seed` / `flutter`） | 9 个 emitter 行为一致；连续两次 build 零文件写入 |
| **B2** | 引入原子写（替换 `util::write_text` 的直接截断写） | 构建中断不留半截文件 |
| **B3** | 引入产物清单（`.forma/artifacts.json`）——先只记不判 | 清单含全部产物的路径/生成器/来源/hash |
| **B4** | 引入冲突检测：默认不覆盖人工修改，报冲突 | 人工改过的生成文件在 rebuild 后内容不变，且有诊断 |
| **B5** | 引入 `forma_adopt` / `--overwrite` 裁决路径 + `--strict` | 三类归属语义完整可操作 |
| **B6** | 影响面就绪后，把增量粒度从文件级改为语义节点级 | 改无关实体不触发相关产物重算 |

> B1/B2 是低风险高收益（纯一致性修复），可独立于 RFC-0003 立即落地。B3–B5 依赖清单格式定稿。B6 依赖 RFC-0003 M3。

## 15. Implementation Plan

| 阶段 | 交付物 | 依赖 |
|---|---|---|
| 阶段 1 | B1 + B2（写入一致性 + 原子写） | 无 |
| 阶段 2 | B3 + B4 + B5（清单 + 冲突检测 + 裁决） | 阶段 1 |
| 阶段 3 | B6（语义级增量） | RFC-0003 M3 |
| 阶段 4 | 清单消费方（`forma_adopt` / `forma_report` / CI 校验） | RFC-0006 P4 |

## 16. Conformance Requirements

- [ ] 全部 9 个 emitter 的产物均带 `@generated` 头。
- [ ] 全部 9 个 emitter 均使用内容哈希跳过写入；连续两次构建零文件写入。
- [ ] 全部产物写入为原子写（临时文件 + rename）。
- [ ] 产物清单覆盖全部生成产物，含 generator id/version 与来源规格。
- [ ] 人工修改过的生成文件不被覆盖，且产生冲突诊断。
- [ ] 产物内容不含时间戳等非确定性内容（清单的 `generatedAt` 除外）。
- [ ] 相同规格 + 相同锁定的两次构建，产物 byte 级一致。

## 17. Acceptance Criteria

1. `forma build` 两次连续执行，第二次报告「零文件写入」，且 `git status` 无变化。
2. 人工修改 `generated/backend/src/models.rs` 后再次 `forma build`：该文件内容**保持人工版本**，构建输出含冲突诊断，且退出码按 `--strict` 开关区分。
3. `forma_adopt` 后再次构建：文件不再报冲突，且后续重新生成不覆盖它。
4. 产物清单能回答：「`generated/backend/src/models.rs` 由哪个 emitter 的哪个版本、从 `forma/app.fm` 的哪一行生成」。

## 18. Open Questions

- 清单是否需要纳入 git？倾向不纳入（属本机状态），但 CI 需要它来校验产物归属——CI 场景下应由 CI 自行生成而非依赖开发者本机文件。
- 产物是否需要**签名**（如 minisign / sigstore）以证明来源？当前范围外，但对「AI 生成代码进入生产」是重要议题。
- 冲突的裁决是否应支持三方合并（生成版 vs 人工版 vs 基线）？当前只支持保留 / 覆盖二选一。
- `Adopted` 归属的产物如何进入编译管线？是否需要「纳入管理」的编译期接入约定？

## 19. References

- 治理总入口：[README.md](../README.md)
- 架构总览与边界：[RFC-0001-architecture-overview.zh-CN.md](RFC-0001-architecture-overview.zh-CN.md)
- 语义内核（影响面依据）：[RFC-0003-semantic-core.zh-CN.md](RFC-0003-semantic-core.zh-CN.md)
- Agent 协议（清单消费方）：[RFC-0006-agent-engineering.zh-CN.md](RFC-0006-agent-engineering.zh-CN.md)
- 适配层接口契约与迁移安全：[roadmap.zh-CN.md §6.1](../../roadmap.zh-CN.md)、[migration-safety.zh-CN.md](../../migration-safety.zh-CN.md)
- 验证体系 L1–L5：[verification.zh-CN.md](../../verification.zh-CN.md)
- 实现位置：`crates/forma/src/build.rs`、`crates/forma/src/generators.rs`、`crates/forma/src/util.rs`、`crates/forma/src/emit/*`

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | 初版提案：定义产物三类归属、产物清单、冲突检测（人工修改不静默覆盖）、语义级增量与可复现构建 |
