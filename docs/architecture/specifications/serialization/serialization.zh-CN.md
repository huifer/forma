---
id: spec:serialization
title: Serialization and Stable Encoding Specification
status: Accepted
version: 1.0.0
authors: [forma-maintainers]
created: 2026-11-05
updated: 2026-11-05
domain: semantic-core
governing_rfc: RFC-0003
depends_on: [RFC-0001]
related_adrs: [ADR-0002, ADR-0003]
related_specs: [spec:ir, spec:types]
breaking_change: false
---

# spec:serialization — 序列化与稳定编码规范

> **规范性**：本规范定义 Forma 产物的**可复现编码规则**。
> 核心铁律：**同一份语义输入 MUST 产出字节级一致的产物（byte 级）**。
> 治理依据：[RFC-0003](../../rfc/RFC-0003-semantic-core.zh-CN.md) · [RFC-0007 构建与产物](../../rfc/RFC-0007-build-and-artifact.zh-CN.md) · [ADR-0003 产物归属](../../adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md)。

---

## 1. 范围

覆盖四类编码对象：IR 快照、产物清单、迁移 SQL、目标代码/数据。规定其**字段顺序、字节表示、哈希口径与换行处理**。

## 2. 通用规则

| 项 | 规则 |
|---|---|
| 字符集 | UTF-8（**无 BOM**） |
| 换行 | LF（`\n`） |
| 缩进 | Rust 产物用 `rustfmt`；SQL 用 2 空格；TS/TSX 用 2 空格 |
| JSON 键序 | **稳定**——`serde_json` 启用 `preserve_order`（保持插入顺序，不做字母排序） |
| 确定性 | **MUST NOT** 引入时间戳、随机数、无序集合迭代等非确定因素 |

## 3. 两套键名体系（关系字段）

关系字段在**不同层使用不同键名**，这是规范的一部分，不是实现瑕疵：

| 层 | 键名 | 例 |
|---|---|---|
| DDL / INSERT / UPDATE / 实体模型 / ListRow | `{rel}_id`（另附 `{rel}_name` 显示名） | `school_id` |
| Create / Update **输入体** | 裸 `{rel}` | `school` |

- 输入体使用**裸关系名**是为了让 API 载荷自解释（`{"school": "..."}` 而非 `{"school_id": "..."}`）。
- 转换发生在**边界**：输入体裸名 → 持久层 `{rel}_id`。
- **MUST NOT** 统一成一套——那会破坏既有 API 契约。

## 4. UUID 的物理表示

- 数据库：`CHAR(36)`（**文本**，非 16 字节二进制）。
- Rust 侧模型 / 路径参数 / 仓储签名统一用 `String`（文本 UUID）；主键由 `Uuid::new_v4().to_string()` 生成。
- `Path(id): Path<String>`，仓储形参 `id: &str`（`String` 非 `Copy`，不可按值传两次）。

## 5. 命名规则

| 对象 | 规则 | 例 |
|---|---|---|
| 表名 | `to_snake_case(pluralize(entity))` | `entity School` → `schools` |
| 列名 | `to_snake_case(field)` | `createdAt` → `created_at` |
| 关系列 | `{rel}_id` | `school` → `school_id` |
| 索引名 | **内容派生**：`idx_{table}_{cols…}` | `idx_schools_city` |
| 唯一键名 | `uq_{table}_{col}` | `uq_students_email` |

- 索引名**MUST** 内容派生：位置序号命名（`idx_{table}_{i}`）在多版本迁移链上会重名（真实 MySQL 报 `Duplicate key name`）。

## 6. SQL 编码

| 规则 | 说明 |
|---|---|
| 占位符 | **仅位置占位符 `?`**。运行时 `sqlx::query()` 不支持具名参数（`?name` 仅 `query!` 宏支持） |
| SELECT 列表 | 末尾**MUST NOT** 留逗号；生成端 `trim_end().trim_end_matches(',')` 后补 `\n` |
| 搜索三值逻辑 | 多搜索字段为 OR；**MUST** 用 `COALESCE(col LIKE CONCAT('%', ?, '%'), 0) = 1`——`(? IS NULL OR col LIKE ?)` 在列值为 NULL 时得 `NULL`，整行被过滤（搜索恒空） |
| JOIN 表名 | **MUST** 使用 snake_case 复数表名；MySQL on Linux 表名大小写敏感 |
| 迁移文件 | `000N_upgrade.sql` + `000N_rollback.sql` **配对**；`0001_init.sql` 基线**不可变** |

## 7. 数值类型序列化

| 类型 | JSON / TS | 说明 |
|---|---|---|
| `Decimal` | **字符串** | 前端 `Decimal` → `string`（`emit/web_react.rs`）；避免 JS 浮点精度丢失 |
| `Int` / `Float` | 数值 | — |
| `Bool` | 布尔 | MySQL 侧为 `TINYINT(1)` |

## 8. 移动端（Dart）编码

- Dart 字段名 **lowerCamelCase**；JSON 键**保持 snake_case**。`fromJson`/`toJson` 中显式写键名映射（`j['school_id'] ↔ schoolId`）。
  （`flutter analyze` 的 `non_constant_identifier_names` 会把 `school_id` 类字段报 info 并导致 analyze 失败。）
- `DateTime` **没有** `>=` / `<=` 运算符——DSL 的时间比较**MUST** 编译为 `(a).compareTo(b) >= 0`。

## 9. IR 快照与产物清单

| 文件 | 内容 | 用途 |
|---|---|---|
| `.forma/ir-snapshot.json` | IR 语义结构（稳定键序） | 迁移 diff / 影响分析的基线 |
| `.forma/build.json` | 产物清单（路径 + 归属 + 哈希） | 增量跳过与冲突检测 |
| `.forma/migrations.json` | 迁移台账（`applied` 标志） | **本机状态**——提交前应重置为全 pending |

## 10. 哈希口径

| 对象 | 口径 |
|---|---|
| 产物内容 | 整文件字节（SHA-256），**哈希前归一化换行**（CRLF/CR → LF，P1-4） |
| 迁移 SQL | **去头尾空行后的正文**（`extract_section` 口径）——避免仅空白差异触发重生成 |
| 确定性种子数据 | SQL 正文 hash |

## 11. 产物归属

- 生成物 **MUST** 带 `@generated` 头。
- 产物分三类：**Generated**（可无条件覆盖）/ **Extension**（人工扩展点，内容保留）/ **Adopted**（人工改写，检测到冲突时**不得静默覆盖**——见 [ADR-0003](../../adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md)）。

## 12. 已知缺口

| # | 缺口 |
|---|---|
| ~~1~~ | ~~跨平台换行（CRLF）未在哈希前归一化~~ **已关闭（2026-11-05）**：`util::sha256_hex` 统一执行 `CRLF/CR → LF` 归一化后再哈希（Windows 检出的工作树不再产生假差异），有单元测试锁定 |
| ~~2~~ | ~~二进制/大文件产物的哈希口径未与文本产物统一约定~~ **已决议（2026-11-05）**：Forma 的产物**全部是文本**（生成器只产文本文件），故**不设**二进制哈希口径；若未来引入二进制产物，**必须**新增 `sha256_bytes` 并在本文件与 `util.rs` 声明，不得复用文本口径 |

## 13. 开放问题

- 是否引入规范化编码层（如 canonical JSON）作为跨语言交换格式。
- 迁移 SQL 的哈希是否应覆盖**文件级**依赖（如所引用的表定义）。

## 14. 参考

- RFC-0003 语义内核：[../../rfc/RFC-0003-semantic-core.zh-CN.md](../../rfc/RFC-0003-semantic-core.zh-CN.md)
- RFC-0007 构建与产物系统：[../../rfc/RFC-0007-build-and-artifact.zh-CN.md](../../rfc/RFC-0007-build-and-artifact.zh-CN.md)
- ADR-0003 产物归属：[../../adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md](../../adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md)
- 案例数据协议：[../../../case-protocol.zh-CN.md](../../../case-protocol.zh-CN.md)
