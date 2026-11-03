# 案例协议：多轮次迭代 + 能力完整性验收

> 案例数据**不是 AI 数据**——全部由编译器从 DSL 纯函数推导（类型/约束/枚举/关系），
> 不经过任何模型；同一 DSL 两次生成 byte 级一致，可 hash 校验、可跨机器重放。
> 「AI 表达意图，Forma 承担重复」在数据这一环同样成立。

本协议把 `examples/school` 的三轮迭代模板化：**每一轮固定跑同一组验收命令**，
结果全部可脚本化、可进 CI。它同时是：

1. Forma 编译器能力完整性的验收矩阵（六个 emitter + 迁移引擎 + diff/lint/diagrams）；
2. 新适配层（如 gen-flutter）的实战考卷——新 emitter 必须在每一轮都吃到产物。

## 案例布局

```
examples/school/
├── forma/app.fm               # v0.3.0 当前 DSL（14 实体/枚举、rules、api、pages）
├── v1/forma/app.fm            # R1 基线快照（0.1.0：4 实体 CRUD）
├── v2/forma/app.fm            # R2 增量快照（0.2.0：+竞选域，FK/唯一约束）
├── db/
│   ├── migrations/            # 迁移链：0001_init + 0002/0003 upgrade/rollback + MIGRATIONS.md
│   └── seed/                  # 案例数据链：seed_0_1_0 → seed_0_2_0 → seed_0_3_0 + fixtures/
├── generated/                 # backend(web 前端产物（gitignored）
└── app_mobile/                # gen-flutter 产物（入库，Android/iOS 同源）
```

种子文件与迁移版本一一对应：**migrate up --to X 之后应用 seed_X，即得到该版本的完整数据集**。
每一轮 seed 都是该版本 schema 的全量快照（不是增量），重放任意历史版本开箱即用。

## 验收矩阵（每轮必跑）

| # | 能力 | 命令 | 通过标准 |
|---|---|---|---|
| V1 | DSL 语义检查 | `forma check examples/school` | 0 error |
| V2 | Lint | `forma lint examples/school` | 0 error |
| V3 | 全目标构建 | `forma build examples/school` | db/backend/web/openapi/diagrams/migrate/seed/mobile-flutter 全部产出 |
| V4 | 基线不可变 | 二次 build | `0001_init.sql` 不被改写（gen-mysql 幂等） |
| V5 | 迁移链完整性 | `forma migrate examples/school verify` | 编号连续 · 升级/回滚配对 · 哈希一致 · 前向重放 == 当前 DSL schema |
| V6 | 数据确定性 | 两次 `forma seed` + diff | byte 级一致 |
| V7 | 数据-规则一致性 | 检查 fixtures | 值满足 min/max/枚举/必填；check 规则（enrolledAt ≥ createdAt）成立 |
| V8 | 数据-schema 一致性 | seed 列集 == DDL 列集 | 每列可插入，FK 引用父表同行号主键 |
| V9 | 移动端契约 | `flutter analyze`（app_mobile/） | No issues found（Flutter ≥3.35） |
| V10 | 移动端可编译 | `flutter build web`（或真机 build） | 编译通过 |
| V11 | 三端同源校验 | 抽查 validators.rs / validators.ts / validators.dart | 同一条 DSL 规则（如 age min 6、enrolledAt ≥ createdAt）在三端同语义出现 |
| V12 | 图 | `forma diagrams` | ER/数据流/生命周期图随轮次更新；状态图按枚举分块 |
| V13 | 后端编译 | `cd generated/backend && cargo check` | 0 error **0 warning**（导入/死代码按需生成） |
| V14 | 后端运行时契约 | 真 MySQL 8 + curl（见下方 E2E） | list/filter/search/create/update/delete/guard/分页全部正确 |
| V15 | 移动端契约 | `cd app_mobile && flutter test` | 离线往返 + 在线全链路全绿（后端未启动时在线用例自动跳过） |
| V16 | Web 编译 | `cd generated/web && npm i && npm run build` | tsc 0 error + vite build 成功 |
| V17 | 数据确定性 | `forma seed --verify` | 磁盘种子与 DSL 推导 byte 级一致（SHA-256 发布前/CI 必跑） |
| V18 | 可视化 | `forma build` 后检查 `generated/web/src/viz/` + 浏览器打开图表页 | chart 三件套（Spec/Option/Source）齐全；V16 的 tsc 覆盖 viz 运行时；浏览器渲染无 console error，ECharts 实例数据绑定正确（RFC-0001 V0.1） |

## 三轮迭代剧本

### R1 · 基线（v0.1.0 — school/student/course/enrollment CRUD）

```bash
cargo run -- seed examples/school/v1          # → v1/db/seed/seed_0_1_0.sql
# 验收：V1–V8。此轮验证最小闭环：DSL → DDL → 迁移基线 → 种子数据。
```

数据特征：4 实体 × 5 行；`enrollment` 的 `unique [student, course]` 由
「子行 i 引用父行 i」的确定性引用策略天然满足。

### R2 · 增量（v0.2.0 — +election/candidate/vote_record）

```bash
cargo run -- seed examples/school/v2          # → v2/db/seed/seed_0_2_0.sql
cp v2/db/seed/seed_0_2_0.sql db/seed/         # 提升进主案例（与迁移链同住）
forma migrate examples/school verify
# 验收：V1–V8。此轮验证增量迁移：新表/FK/唯一约束 + 数据追加。
```

### R3 · 演进（v0.3.0 — 加列 phone/remark、列宽扩展、voter 索引、+Flutter 适配层）

```bash
forma build examples/school                   # 全目标 + seed_0_3_0 + app_mobile/
forma migrate examples/school verify
cd app_mobile && flutter analyze && flutter build web
# 验收：V1–V18 全量。此轮验证列级演进迁移 + 新适配层接入。
```

## 运行完整案例（端到端）

```bash
# 1) 数据库 + 种子
mysql -uroot -p -e "CREATE DATABASE IF NOT EXISTS school"
mysql -uroot -p school < examples/school/db/seed/seed_0_3_0.sql   # INSERT IGNORE，可重复执行

# 2) 后端（Rust/axum，端口 3000，CORS 全放行）
cd examples/school/generated/backend
DATABASE_URL="mysql://root:你的密码@127.0.0.1:3306/school" cargo run

# 3a) Web（React/TanStack）
cd ../web && npm i && npm run dev

# 3b) Flutter（Android/iOS 同源）
cd ../../app_mobile
flutter pub get
flutter run                                                    # Android 模拟器（API_BASE=10.0.2.2:3000/api）
flutter run --dart-define=API_BASE=http://127.0.0.1:3000/api   # iOS 模拟器
```

三端操作同一份数据：Web 改一条记录 → Flutter 列表下拉刷新可见；
Flutter 提交违反 rules 的数据（如 capacity=0）→ 三端同源的校验规则给出同一消息。

## 端到端验收（真实 MySQL 8）

cargo test 覆盖不到 SQL 语义与运行时行为——**只有真库能抓到那类 bug**。
本节是 R3 的强制流程（也是本仓库历史 bug 的来源清单）：

```bash
# 1) 起一个干净 MySQL 8
docker run -d --name forma-mysql -e MYSQL_ROOT_PASSWORD=forma123 -e MYSQL_DATABASE=school \
  -p 3307:3306 mysql:8.0
until docker exec forma-mysql mysqladmin ping -uroot -pforma123; do sleep 1; done

# 2) 应用迁移链（基线 + 两轮升级）→ 得到当前版本 schema
for f in db/migrations/0001_init.sql db/migrations/0002_upgrade.sql db/migrations/0003_upgrade.sql; do
  docker exec -i forma-mysql mysql -uroot -pforma123 school < "$f"
done

# 3) 应用案例数据（seed 与迁移版本一一对应）
docker exec -i forma-mysql mysql -uroot -pforma123 school < db/seed/seed_0_3_0.sql

# 4) 起后端（端口可用 PORT 覆盖，默认 8080——与 Web proxy / Flutter 默认 API_BASE 对齐）
cd generated/backend && cargo build
DATABASE_URL="mysql://root:forma123@127.0.0.1:3307/school" ./target/debug/school-backend &

# 5) 打真实 API（CRUD + 规则 + 守卫 + 分页）
B=http://127.0.0.1:8080/api
curl -s "$B/students?page_size=2"                       # 列表（含 JOIN 显示列 school_name）
curl -s "$B/students?q=student0"                       # 搜索（LIKE + COALESCE）
curl -s "$B/students?school=<uuid>"                     # 过滤
curl -s -X POST "$B/students" -H 'Content-Type: application/json' \
  -d '{"school":"<uuid>","name":"x","email":"x@example.test","age":20}'   # 创建
curl -s -X POST "$B/students" -H 'Content-Type: application/json' \
  -d '{"school":"<uuid>","name":"x","email":"x@example.test","age":2}'    # 校验失败 → 400 + fields
curl -s -X DELETE "$B/enrollments/<completed-id>"       # 守卫 → 409 + 中文原因

# 6) 移动端契约测试（真实 HTTP 打真实后端）
cd ../../app_mobile && flutter test
```

### Detail 原语（三端对称性）

`detail` 页面原语三端均已实现：web → Card+表格，flutter → Card+`_kv` 行（同一套
类型驱动渲染：datetime/money/badge/ref）。回归测试：`tests/seed_flutter.rs::flutter_detail_page_renders_sections`。

### 这条流程真实抓到的 bug（已全部修掉并固化为回归测试）

| bug | 症状 | 根因 | 回归测试 |
|---|---|---|---|
| SELECT 尾逗号 | MySQL 1064 | `s.created_at,\nFROM` | `backend_contract.rs::select_list_has_no_trailing_comma_and_keeps_newline` |
| 命名占位符 | MySQL 1064 | sqlx 运行时不支持 `?school` | `::where_clause_uses_positional_placeholders_only` |
| 搜索三值逻辑 | 搜索永远空 | `FALSE OR NULL` = NULL → 整行被过滤；多字段应为 OR | `::search_is_or_semantics_with_coalesce` |
| 关系列名 | INSERT 1054 Unknown column | DDL 是 `school_id`，SQL 用了 `school` | `::relation_columns_use_id_suffix_in_sql_but_bare_name_in_input` |
| JOIN 表名 | MySQL 1064 | `LEFT JOIN Schools` vs `schools` | `::join_table_names_match_mysql_ddl` |
| UUID 物理表示 | 读写永远匹配不到 | 列是 CHAR(36)，sqlx 的 Uuid 只解 16 字节二进制 | `::uuid_columns_are_text_end_to_end` |
| op 裁剪 | 生成后端 66 个 warning | 未声明的 op 也生成仓储函数 | `::repo_functions_follow_declared_ops` |
| 导入冗余 | unused import 警告 | post / validators / chrono 按需导入 | `::routes_imports_and_guard_binding_are_conditional` |
| 端口不一致 | 客户端连不上 | 后端 8080 / Flutter 默认 3000 | 端到端流程（本节） |

## MCP（AI 编码环境集成）

在 Claude Code / Codex / Pi 里挂上 MCP 后，AI 可以走完整的 DSL 工作流
（不需要读写生成物——生成物永远由 build 产出）：

```bash
forma mcp examples/school
```

| 工具 | 适用工程 | 作用 |
|---|---|---|
| `forma_graph` | DSL / 联邦 | DSL：实体（字段/关系）、枚举、API、页面、规则摘要；联邦：跨标准语义图 |
| `forma_check` | DSL | .fm 诊断（error/warning）+ lint + 结构摘要 |
| `forma_build` | DSL | 与 CLI 同一条管线重建全部产物，返回每 emitter 的文件清单 |
| `forma_migrate` | DSL | 迁移链版本/applied 状态 |
| `forma_validate` | 联邦 | 跨标准校验 |
| `forma_impact` / `forma_patch` | 联邦 | 影响闭包 / 语义补丁（DSL 工程会得到带替代指引的报错） |

## 新适配层的接入清单（以 gen-flutter 为例）

1. `manifest.rs`：`Targets` 增加目标槽位 + 键名别名（`mobile`/`flutter`）；
2. `emit/flutter.rs`：实现 emitter（确定性输出、`@generated` 头、产物内不回读业务语义）；
3. `main.rs`：build steps 注册（seed 跟随 db out，flutter 跟随 mobile out）；
4. 测试：`tests/seed_flutter.rs`（独立 tmp 目录——并行 cargo test 不共享路径）；
5. 案例验收：`forma.yaml` 加 target → R3 全量跑 V1–V18。

## 确定性数据生成规则（gen-seed 内核）

| 维度 | 策略 |
|---|---|
| 主键 | SHA-256(namespace‖0x00‖name) 前 16 字节，RFC4122 v5-shape 位（合法 UUID 格式 + 确定性，仓库零新增依赖） |
| 引用顺序 | 复用 `plan_tables` 拓扑序，父表先插；子行 i 引用父行 i |
| 可空字段 | 前 2 行填满、后 3 行 NULL（覆盖空值渲染路径） |
| 数值 | @min/@max（含 rules 约束）区间内的固定序列 |
| 枚举 | 变体轮转；文案取 @label（zh→en→原名） |
| 时间 | 固定锚点 2025-01-06T08:00Z + 行号步长；start/end/enroll 语义偏移保证 check 规则成立 |
| 文本 | (实体, 字段) 命中业务文案池（贴近真实案例），未命中走确定性兜底公式 |
| 幂等 | INSERT IGNORE + 确定性主键，重放安全 |
