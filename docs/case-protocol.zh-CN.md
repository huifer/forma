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
# 验收：V1–V12 全量。此轮验证列级演进迁移 + 新适配层接入。
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

## 新适配层的接入清单（以 gen-flutter 为例）

1. `manifest.rs`：`Targets` 增加目标槽位 + 键名别名（`mobile`/`flutter`）；
2. `emit/flutter.rs`：实现 emitter（确定性输出、`@generated` 头、产物内不回读业务语义）；
3. `main.rs`：build steps 注册（seed 跟随 db out，flutter 跟随 mobile out）；
4. 测试：`tests/seed_flutter.rs`（独立 tmp 目录——并行 cargo test 不共享路径）；
5. 案例验收：`forma.yaml` 加 target → R3 全量跑 V1–V12。

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
