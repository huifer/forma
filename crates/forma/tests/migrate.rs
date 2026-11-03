use forma::dsl::lexer::lex;
use forma::dsl::parser::parse;
use forma::emit::migrate::diff_down_sql;
use forma::emit::migrate::diff_sql;
use forma::emit::mysql::plan_tables;
use forma::emit::{
    diff_programs, emit_diagrams, emit_migration, emit_mysql, migration_down, migration_status,
    migration_up,
};
use std::path::PathBuf;

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forma-mig-{}-{}-{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn program(src: &str) -> forma::dsl::Program {
    parse(&lex(src)).program
}

const V1: &str = r#"
app demo { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  tag String(20)?
  index [name]
}
"#;

const V2_ADD_FIELD: &str = r#"
app demo { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  phone String(20)?
  tag String(20)?
  index [name]
}
"#;

const V3_DROP_FIELD: &str = r#"
app demo { version = "0.1.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  index [name]
}
"#;

const V4_DROP_TABLE: &str = r#"
app demo { version = "0.1.0" }
"#;

const V5_VERSIONED: &str = r#"
app demo { version = "0.2.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  phone String(20)?
  tag String(20)?
  index [name]
}
"#;

const V6_VERSIONED: &str = r#"
app demo { version = "0.3.0" }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  phone String(20)?
  email Email? @unique
  tag String(20)?
  index [name]
}
"#;

#[test]
fn migration_add_column_on_schema_change() {
    let dir = tmp_dir("addcol");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    let first = emit_migration(&dir, &p1, "generated/db").unwrap();
    assert!(first.is_none(), "首次构建只保存基线快照");

    let p2 = program(V2_ADD_FIELD);
    let second = emit_migration(&dir, &p2, "generated/db").unwrap();
    let file = second.expect("should generate upgrade migration");
    assert!(file.path.ends_with("0002_upgrade.sql"), "{}", file.path);
    assert!(
        file.content
            .contains("ALTER TABLE students ADD COLUMN phone VARCHAR(20) NULL;"),
        "{}",
        file.content
    );
}

#[test]
fn migration_drop_column() {
    let p1 = program(V1);
    let p3 = program(V3_DROP_FIELD);
    let old = forma::emit::mysql::plan_tables(&p1);
    let new = forma::emit::mysql::plan_tables(&p3);
    let sql = forma::emit::migrate::diff_sql(&old, &new);
    assert!(!sql.is_empty(), "diff_sql returned empty");
}

#[test]
fn migration_drop_table() {
    let dir = tmp_dir("droptable");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();

    let p4 = program(V4_DROP_TABLE);
    let file = emit_migration(&dir, &p4, "generated/db")
        .unwrap()
        .expect("migration");
    assert!(
        file.content.contains("DROP TABLE IF EXISTS students;"),
        "{}",
        file.content
    );
}

#[test]
fn migration_modify_column_type() {
    let dir = tmp_dir("modcol");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();

    let v2 = V1.replace("String(50)", "String(100)");
    let p2 = program(&v2);
    let file = emit_migration(&dir, &p2, "generated/db")
        .unwrap()
        .expect("migration");
    assert!(
        file.content
            .contains("MODIFY COLUMN name VARCHAR(100) NOT NULL;"),
        "{}",
        file.content
    );
}

#[test]
fn no_migration_when_schema_unchanged() {
    let dir = tmp_dir("stable");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();
    let again = emit_migration(&dir, &p1, "generated/db").unwrap();
    assert!(again.is_none(), "schema unchanged → no migration");
}

// ── 多版本迁移链：upgrade/rollback 配对 + 台账 + MIGRATIONS.md ──

fn read_ledger(dir: &std::path::Path) -> forma::emit::MigrationLedger {
    let text = std::fs::read_to_string(dir.join(".forma/migrations.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn migration_chain_generates_paired_upgrade_and_rollback() {
    let dir = tmp_dir("chain");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap(); // 基线 0001_init.sql
    emit_migration(&dir, &p1, "generated/db").unwrap(); // 登记基线

    // v0.2.0：新增 phone 列 → #0002 upgrade + rollback 配对
    let up2 = emit_migration(&dir, &program(V5_VERSIONED), "generated/db")
        .unwrap()
        .expect("v0.2 upgrade");
    assert!(up2.path.ends_with("0002_upgrade.sql"), "{}", up2.path);
    assert!(up2.content.contains("-- UP"), "{}", up2.content);
    assert!(up2
        .content
        .contains("ALTER TABLE students ADD COLUMN phone VARCHAR(20) NULL;"));

    let rollback2 = std::fs::read_to_string(dir.join("generated/db/migrations/0002_rollback.sql"))
        .expect("rollback file");
    assert!(rollback2.contains("-- DOWN"), "{rollback2}");
    assert!(rollback2.contains("ALTER TABLE students DROP COLUMN phone;"));

    // v0.3.0：新增 email 列 → #0003
    let up3 = emit_migration(&dir, &program(V6_VERSIONED), "generated/db")
        .unwrap()
        .expect("v0.3 upgrade");
    assert!(up3.path.ends_with("0003_upgrade.sql"), "{}", up3.path);

    // 台账：3 条记录，版本递进，含哈希与 applied=false
    let ledger = read_ledger(&dir);
    assert_eq!(ledger.entries.len(), 3);
    let versions: Vec<&str> = ledger
        .entries
        .iter()
        .map(|e| e.version.as_str())
        .collect();
    assert_eq!(versions, vec!["0.1.0", "0.2.0", "0.3.0"], "{versions:?}");
    assert!(ledger.entries[1].rollback_file.is_some());
    assert!(ledger.entries[1].up_hash.len() == 64);
    assert!(!ledger.entries[2].applied);

    // MIGRATIONS.md 可见化：版本链 + SQL 更新全部在文档里
    let md = std::fs::read_to_string(dir.join("generated/db/migrations/MIGRATIONS.md")).unwrap();
    assert!(md.contains("# Migrations — 数据库版本台账"));
    assert!(md.contains("| 2 | 0.2.0 | 0002_upgrade.sql | 0002_rollback.sql |"));
    assert!(md.contains("## #2 — upgrade to v0.2.0"));
    assert!(md.contains("ALTER TABLE students ADD COLUMN phone"));
    assert!(md.contains("-- DOWN（回滚）"));
}

#[test]
fn rollback_sql_is_inverse_of_upgrade() {
    let v1 = program(V1);
    let v2 = program(V5_VERSIONED);
    let old = plan_tables(&v1);
    let new = plan_tables(&v2);
    let up = diff_sql(&old, &new);
    let down = diff_down_sql(&old, &new);
    assert!(up.contains("ADD COLUMN phone VARCHAR(20)"), "{up}");
    assert!(down.contains("DROP COLUMN phone"), "{down}");
    // 升级里新增的每一个动作，回滚里都能找到反向动作
    assert!(up.matches("ALTER TABLE").count() == down.matches("ALTER TABLE").count());
}

#[test]
fn migrate_up_and_down_track_applied_state() {
    let dir = tmp_dir("apply");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &program(V5_VERSIONED), "generated/db").unwrap();
    emit_migration(&dir, &program(V6_VERSIONED), "generated/db").unwrap();

    // 初始全部 pending
    let ledger = read_ledger(&dir);
    assert_eq!(ledger.entries.iter().filter(|e| e.applied).count(), 0);

    // up（dry run）不改变状态，输出包含基线 + 两个升级
    let plan = migration_up(&dir, "generated/db", false, None).unwrap();
    assert_eq!(plan.len(), 3, "{}", plan.len());
    assert!(plan[0].0.ends_with("0001_init.sql"));
    assert!(plan.iter().any(|(_, s)| s.contains("ADD COLUMN phone")));
    let ledger = read_ledger(&dir);
    assert_eq!(ledger.entries.iter().filter(|e| e.applied).count(), 0);

    // up --apply：全部标记 applied，MIGRATIONS.md 状态刷新
    migration_up(&dir, "generated/db", true, None).unwrap();
    let ledger = read_ledger(&dir);
    assert_eq!(ledger.entries.iter().filter(|e| e.applied).count(), 3);
    let md = std::fs::read_to_string(dir.join("generated/db/migrations/MIGRATIONS.md")).unwrap();
    assert!(md.contains("✅ applied"));

    // down --apply：回滚最近一次（#0003），台账回退
    let rolled = migration_down(&dir, "generated/db", true, 1, None)
        .unwrap();
    assert_eq!(rolled.len(), 1);
    assert!(rolled[0].0.ends_with("0003_rollback.sql"), "{}", rolled[0].0);
    assert!(rolled[0].1.contains("DROP COLUMN email"), "{}", rolled[0].1);
    let ledger = read_ledger(&dir);
    assert_eq!(ledger.entries.iter().filter(|e| e.applied).count(), 2);

    // status 渲染含 pending/applied 混合
    let status = migration_status(&dir).unwrap();
    assert_eq!(status.len(), 3);
    assert!(status[0].contains("[applied]"));
    assert!(status[2].contains("[pending]"));
}

#[test]
fn migrate_down_at_head_returns_none() {
    let dir = tmp_dir("head");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();
    let rolled = migration_down(&dir, "generated/db", false, 1, None).unwrap();
    assert!(rolled.is_empty(), "nothing applied → nothing to roll back");
}

#[test]
fn migrate_down_on_baseline_errors_with_clear_message() {
    let dir = tmp_dir("baseline-down");
    let p1 = program(V1);
    emit_mysql(&dir, &p1, "generated/db").unwrap();
    emit_migration(&dir, &p1, "generated/db").unwrap();
    migration_up(&dir, "generated/db", true, None).unwrap();
    let err = migration_down(&dir, "generated/db", true, 1, None).unwrap_err();
    assert!(err.to_string().contains("基线没有回滚脚本"), "{err}");
}

// ── 多版本重放保障：verify / up --to / down --steps ──────

use forma::emit::{migration_verify, replay_sql, schema_diff, schema_from_plans, SimSchema};

fn build_chain(tag: &str) -> PathBuf {
    let dir = tmp_dir(tag);
    emit_mysql(&dir, &program(V1), "generated/db").unwrap();
    emit_migration(&dir, &program(V1), "generated/db").unwrap();
    emit_migration(&dir, &program(V5_VERSIONED), "generated/db").unwrap();
    emit_migration(&dir, &program(V6_VERSIONED), "generated/db").unwrap();
    dir
}

/// 核心保证：把基线 + 全部升级按序重放 = 最新 DSL schema；
/// 再逆序回滚 = 回到基线。等价于在真实 MySQL 上跑一遍 up 全链 + down 全链。
#[test]
fn migration_chain_replays_forward_to_latest_and_backward_to_baseline() {
    let dir = build_chain("replay");
    let migrations = dir.join("generated/db/migrations");
    let read_up = |name: &str| -> String {
        let text = std::fs::read_to_string(migrations.join(name)).unwrap();
        match text.find("-- UP") {
            Some(pos) => text[pos + 5..].trim().to_string(),
            None => text, // 基线无标记段，整体即 SQL
        }
    };
    let read_down = |name: &str| -> String {
        let text = std::fs::read_to_string(migrations.join(name)).unwrap();
        text[text.find("-- DOWN").unwrap() + 7..].trim().to_string()
    };

    // 前向：空 schema → 0001 → 0002 → 0003
    let mut schema = SimSchema::default();
    replay_sql(&mut schema, &read_up("0001_init.sql")).unwrap();
    replay_sql(&mut schema, &read_up("0002_upgrade.sql")).unwrap();
    replay_sql(&mut schema, &read_up("0003_upgrade.sql")).unwrap();
    let expected_latest = schema_from_plans(&plan_tables(&program(V6_VERSIONED)));
    let diffs = schema_diff(&schema, &expected_latest);
    assert!(diffs.is_empty(), "前向重放漂移: {diffs:?}");

    // 反向：0003↓ → 0002↓ → 回到基线
    replay_sql(&mut schema, &read_down("0003_rollback.sql")).unwrap();
    replay_sql(&mut schema, &read_down("0002_rollback.sql")).unwrap();
    let expected_base = schema_from_plans(&plan_tables(&program(V1)));
    let diffs = schema_diff(&schema, &expected_base);
    assert!(diffs.is_empty(), "回滚后与基线漂移: {diffs:?}");
}

#[test]
fn verify_passes_on_intact_chain_and_flags_drift() {
    let dir = build_chain("verify-ok");
    let issues = migration_verify(&dir, &program(V6_VERSIONED), "generated/db").unwrap();
    assert!(issues.is_empty(), "完整链不应有 issue: {issues:?}");

    // 篡改一个迁移文件 → verify 必须报出来（CI 拦截点）
    let p = dir.join("generated/db/migrations/0002_upgrade.sql");
    let tampered = std::fs::read_to_string(&p).unwrap().replace(
        "ALTER TABLE students ADD COLUMN phone VARCHAR(20) NULL;",
        "ALTER TABLE students ADD COLUMN phone VARCHAR(30) NULL;",
    );
    std::fs::write(&p, tampered).unwrap();
    let issues = migration_verify(&dir, &program(V6_VERSIONED), "generated/db").unwrap();
    assert!(
        issues.iter().any(|i| i.contains("哈希与台账不符")),
        "篡改未被发现: {issues:?}"
    );
}

#[test]
fn verify_flags_replay_drift_when_chain_is_missing_a_migration() {
    // 只有基线 + 0002（缺 0003）→ 重放结果 != 最新 schema
    let dir = tmp_dir("verify-drift");
    emit_mysql(&dir, &program(V1), "generated/db").unwrap();
    emit_migration(&dir, &program(V1), "generated/db").unwrap();
    emit_migration(&dir, &program(V5_VERSIONED), "generated/db").unwrap();
    let issues = migration_verify(&dir, &program(V6_VERSIONED), "generated/db").unwrap();
    assert!(
        issues.iter().any(|i| i.contains("不一致")),
        "缺迁移未被发现: {issues:?}"
    );
}

#[test]
fn up_to_applies_target_version_only() {
    let dir = build_chain("up-to");
    // up --to 0.2.0：只应应用 #0001 与 #0002
    let plan = migration_up(&dir, "generated/db", false, Some("0.2.0")).unwrap();
    assert_eq!(plan.len(), 2, "{:?}", plan.iter().map(|f| &f.0).collect::<Vec<_>>());
    assert!(plan.iter().all(|(f, _)| !f.contains("0003")));

    // apply 后再 up（无 target）只补 0003
    migration_up(&dir, "generated/db", true, Some("0.2.0")).unwrap();
    let rest = migration_up(&dir, "generated/db", true, None).unwrap();
    assert_eq!(rest.len(), 1);
    assert!(rest[0].0.contains("0003"));
}

#[test]
fn down_steps_and_down_to_rollback_multiple_versions() {
    let dir = build_chain("down-multi");
    migration_up(&dir, "generated/db", true, None).unwrap();

    // down --steps 2：回滚 #0003、#0002
    let rolled = migration_down(&dir, "generated/db", true, 2, None).unwrap();
    assert_eq!(rolled.len(), 2);
    assert!(rolled[0].0.contains("0003"));
    assert!(rolled[1].0.contains("0002"));
    let ledger = read_ledger(&dir);
    assert_eq!(ledger.entries.iter().filter(|e| e.applied).count(), 1);

    // down --to 0.1.0：基线版本 ≤ 0.1.0，无可回滚
    let rolled = migration_down(&dir, "generated/db", true, 99, Some("0.1.0")).unwrap();
    assert!(rolled.is_empty(), "目标版本之下不应回滚基线");
}

#[test]
fn index_names_are_content_derived_and_collision_free() {
    // 回归：位置序号命名（idx_{t}_{i}）在多版本链里会重名——基线已建
    // idx_enrollments_0 (student_id, course_id)，升级再加 idx_enrollments_0 (status)
    // 时，真实 MySQL 直接报 Duplicate key name。内容派生命名（idx_{t}_{列…}）必须：
    // 1) 不同列集 → 不同名字；2) 升级里 ADD 的索引名不与基线已有名字冲突。
    let v1 = plan_tables(&program(V1));
    let v2 = plan_tables(&program(V5_VERSIONED));

    let baseline_names: std::collections::HashSet<String> = v1
        .iter()
        .flat_map(|t| {
            t.indexes
                .iter()
                .map(|cols| forma::emit::mysql::key_name("idx", &t.table, cols))
                .chain(
                    t.uniques
                        .iter()
                        .map(|cols| forma::emit::mysql::key_name("uq", &t.table, cols)),
                )
                .collect::<Vec<_>>()
        })
        .collect();

    let upgrade = diff_sql(&v1, &v2);
    for stmt in upgrade.lines().filter(|l| l.contains("ADD INDEX")) {
        // ALTER TABLE t ADD INDEX idx_{t}_{cols} (...);
        let name = stmt
            .split("ADD INDEX ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("");
        assert!(!name.is_empty(), "{stmt}");
        assert!(
            !name.chars().all(|c| c.is_ascii_digit() || c == '_'),
            "索引名仍是位置序号: {stmt}"
        );
        assert!(
            !baseline_names.contains(name),
            "升级索引与基线重名（重放会失败）: {stmt}"
        );
    }

    // CREATE TABLE 里生成的名字同样内容派生
    let create = forma::emit::mysql::render_create_table(&v2[0]);
    assert!(create.contains("KEY idx_"), "{create}");
    assert!(!create.contains("idx_0 ") && !create.contains("_0 ("), "{create}");
}

// ── 可视化图表 ─────────────────────────────────────────

#[test]
fn diagrams_emit_er_dataflow_status() {
    let dir = tmp_dir("diagrams");
    let p = program(V1);
    let files = emit_diagrams(&dir, &p, "generated/diagrams").unwrap();
    assert!(files.len() >= 4);

    let er = files.iter().find(|f| f.path.ends_with("er.mmd")).unwrap();
    assert!(er.content.contains("erDiagram"));
    assert!(er.content.contains("Student {"), "{}", er.content);

    let dataflow = files
        .iter()
        .find(|f| f.path.ends_with("dataflow.mmd"))
        .unwrap();
    assert!(dataflow.content.contains("flowchart LR"));
    assert!(dataflow.content.contains("MySQL students"));

    let status = files.iter().find(|f| f.path.ends_with("status.mmd"));
    // V1 无枚举 → 无状态图（school 用例有 EnrollmentStatus）
    let _ = status;

    let html = files
        .iter()
        .find(|f| f.path.ends_with("diagrams.html"))
        .unwrap();
    assert!(html.content.contains("mermaid"));
}

#[test]
fn status_flow_includes_guard_notes() {
    let src = r#"
app demo { version = "0.1.0" }
enum Status { pending active completed }
entity Order {
  id Uuid @id @default(uuid)
  status Status @default(pending)
}
rules R {
  guard Order.delete {
    deny status == "completed"
      reason { zh "不可删" }
  }
}
"#;
    let p = program(src);
    // 独立 tmp 目录：并行测试下不能共用 CWD 的 generated/ 输出（文件竞态）
    let files = emit_diagrams(&tmp_dir("status-guard"), &p, "generated/diagrams").unwrap();
    let status = files
        .iter()
        .find(|f| f.path.ends_with("status.mmd"))
        .unwrap();
    assert!(status.content.contains("stateDiagram-v2"));
    // 变体名加枚举命名空间前缀，避免撞 Mermaid 关键字（note/state/end…）
    assert!(status.content.contains("[*] --> Status_pending"));
    assert!(status.content.contains("Status_pending --> Status_active"));
    assert!(status.content.contains("Status_active --> Status_completed"));
    assert!(status
        .content
        .contains("note right of Status_completed : completed 时不可 删除"));
}

/// 回归：变体名撞 Mermaid 关键字（note/state/end/direction）时，
/// 旧版直接输出裸名导致 `Syntax error in t…`，必须加枚举命名空间前缀。
#[test]
fn status_flow_namespaces_variants_that_collide_with_mermaid_keywords() {
    let src = r#"
app demo { version = "0.1.0" }
enum Note { note state end direction }
entity Ticket {
  id Uuid @id @default(uuid)
  status Note @default(note)
}
"#;
    let p = program(src);
    let files = emit_diagrams(&tmp_dir("status-kw"), &p, "generated/diagrams").unwrap();
    let status = files
        .iter()
        .find(|f| f.path.ends_with("status.mmd"))
        .unwrap();
    for kw in ["note", "state", "end", "direction"] {
        // 不得出现裸关键字作为状态名（即行内直接跟关键字且无前缀）
        let bare = format!("    {} --> {}\n", kw, kw);
        assert!(
            !status.content.contains(&bare),
            "bare mermaid keyword leaked: {kw}"
        );
        assert!(
            status.content.contains(&format!("Note_{kw}")),
            "namespaced state missing for {kw}"
        );
    }
    assert!(status.content.contains("[*] --> Note_note"));
}

#[test]
fn diff_reports_entity_changes() {
    let old = r#"
app demo { version = "0.1.0" }
entity A { id Uuid @id @default(uuid) name String(50) @required }
"#;
    let new = r#"
app demo { version = "0.1.0" }
entity A { id Uuid @id @default(uuid) name String(100) @required email Email @required }
"#;
    let report = diff_programs(&parse(&lex(old)).program, &parse(&lex(new)).program);
    assert_eq!(report.summary.modified, 1);
    let a = report.entities.iter().find(|e| e.name == "A").unwrap();
    assert!(a.fields_added.iter().any(|f| f.name == "email"));
    assert!(a
        .fields_modified
        .iter()
        .any(|m| m.name == "name" && m.new.contains("100")));
}

#[test]
fn diff_sql_direct_add_and_drop() {
    let v1 = "app demo { version = \"0.1.0\" }\nentity Student {\n  id Uuid @id @default(uuid)\n  name String(50) @required\n  tag String(20)?\n}";
    let v3 = "app demo { version = \"0.1.0\" }\nentity Student {\n  id Uuid @id @default(uuid)\n  name String(100) @required\n}";
    let old = plan_tables(&parse(&lex(v1)).program);
    let new = plan_tables(&parse(&lex(v3)).program);
    let sql = diff_sql(&old, &new);
    assert!(sql.contains("DROP COLUMN tag"), "{sql}");
    assert!(sql.contains("MODIFY COLUMN name VARCHAR(100)"), "{sql}");
}
