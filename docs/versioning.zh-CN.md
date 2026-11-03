# Forma 版本管理与升级设计

## 核心场景

```
v0.1  学生选课系统（已有）
  ↓ DSL 修改：加竞选会长功能
v0.2  + 竞选会长（新实体/新页面/新API/新字段）
  ↓ forma build
自动产出：
  → 0002_upgrade.sql（ALTER/CREATE TABLE 增量迁移）
  → 全量重新生成后端（Rust）+ 前端（React）
  → 版本差异可视化 HTML 报告
```

## 版本管理机制

### 每次构建的流程

```
1. 读取 forma/*.fm → Program
2. 读取 .forma/versions/{version}.json（上一版快照）
3. diff 上一版 vs 当前版 → 结构化变更集
4. 变更集 → 增量迁移 SQL（000N_upgrade.sql）
5. 变更集 → 差异可视化 HTML（diff-report.html）
6. Program → 全量重新生成所有目标代码
7. 保存当前版本快照
```

### 版本快照内容

每个版本快照记录完整的 DSL 结构化状态：
- 实体列表（含字段/类型/属性/关系）
- 枚举列表（含变体）
- API 列表（含操作）
- 页面列表

快照存放在 `.forma/versions/{version}.json`，其中 version 取自 `forma.yaml` 的 `project.version`。

### 版本对比

`forma diff --versions 0.1.0 0.2.0` 或 `forma diff examples/school/v1 examples/school`。

## 竞选会长升级用例（v0.1 → v0.2）

### 新增实体

```
entity Election {
  id          Uuid   @id @default(uuid)
  title       String(200) @required
  status      ElectionStatus @default(nominating)
  startsAt    Timestamp @required
  endsAt      Timestamp @required
}

entity Candidate {
  id          Uuid   @id @default(uuid)
  election    Election @required
  student     Student @required
  slogan      String(500) @required
  votes       Int @default(0) @readonly
}

entity VoteRecord {
  id          Uuid   @id @default(uuid)
  election    Election @required
  candidate   Candidate @required
  voter       Student @required
  createdAt   Timestamp @default(now) @readonly
  unique [election, voter]
}
```

### 新增枚举

```
enum ElectionStatus { nominating voting closed }
```

### 修改已有实体

Student 加 `isCandidate Bool @default(false)` 字段。

### 新增页面

- ElectionList / ElectionEdit / ElectionDetail
- CandidateList / CandidateEdit

### 新增 API

- Elections / Candidates / VoteRecords
