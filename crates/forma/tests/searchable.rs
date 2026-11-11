//! 全文搜索（searchable）能力回归测试（M10/P1）。
//! 覆盖：FULLTEXT 索引生成（ngram）、迁移链增量、重放验证、检查诊断。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forma-search-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: search\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

#[test]
fn searchable_generates_fulltext_index_in_baseline() {
    let dir = tmp_dir("baseline");
    write_project(
        &dir,
        "app search { version = \"0.1.0\" }\nentity Article {\n id Uuid @id @default(uuid)\n title String(200) @required\n body Text?\n searchable [title, body]\n}\n",
    );
    forma::build::run_build(&dir).unwrap();
    let ddl = fs::read_to_string(dir.join("generated/db/migrations/0001_init.sql")).unwrap();
    assert!(
        ddl.contains("FULLTEXT KEY ft_articles_title_body (title, body) WITH PARSER ngram"),
        "{ddl}"
    );
}

#[test]
fn searchable_upgrade_produces_fulltext_migration_and_replays() {
    let dir = tmp_dir("upgrade");
    // v1：无 searchable
    write_project(
        &dir,
        "app search { version = \"0.1.0\" }\nentity Article {\n id Uuid @id @default(uuid)\n title String(200) @required\n body Text?\n}\n",
    );
    forma::build::run_build(&dir).unwrap();
    // v1.1：加 searchable
    write_project(
        &dir,
        "app search { version = \"0.1.1\" }\nentity Article {\n id Uuid @id @default(uuid)\n title String(200) @required\n body Text?\n searchable [title, body]\n}\n",
    );
    forma::build::run_build(&dir).unwrap();
    let up = fs::read_to_string(dir.join("generated/db/migrations/0002_upgrade.sql")).unwrap();
    assert!(
        up.contains("ADD FULLTEXT INDEX ft_articles_title_body (title, body) WITH PARSER ngram"),
        "{up}"
    );
    // 风险分级 Low（可逆增量）
    let md = fs::read_to_string(dir.join("generated/db/migrations/MIGRATIONS.md")).unwrap();
    assert!(
        md.contains("全文索引") || md.contains("add_fulltext") || md.contains("Low"),
        "{md}"
    );

    // 重放验证（迁移链前向 == 最新 schema；最硬回归）
    let program = forma::dsl::load_program(&dir, &["forma/**/*.fm".to_string()])
        .unwrap()
        .0;
    let problems = forma::emit::migration_verify(&dir, &program, "generated/db").unwrap();
    assert!(problems.is_empty(), "replay verify failed: {problems:?}");
}

#[test]
fn searchable_check_rejects_unknown_field_and_type() {
    let dir = tmp_dir("check");
    write_project(
        &dir,
        "app search { version = \"0.1.0\" }\nentity Article {\n id Uuid @id @default(uuid)\n title String(200) @required\n count Int\n searchable [missing, count]\n}\n",
    );
    let err = forma::build::run_build(&dir).expect_err("diagnostics must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("dsl-searchable-unknown-field"), "{msg}");
    assert!(msg.contains("dsl-searchable-field-type"), "{msg}");
}

#[test]
fn no_searchable_zero_fulltext_change() {
    let dir = tmp_dir("zero");
    write_project(
        &dir,
        "app search { version = \"0.1.0\" }\nentity Article {\n id Uuid @id @default(uuid)\n title String(200) @required\n}\n",
    );
    forma::build::run_build(&dir).unwrap();
    let ddl = fs::read_to_string(dir.join("generated/db/migrations/0001_init.sql")).unwrap();
    assert!(!ddl.contains("FULLTEXT"), "{ddl}");
}

/// 回归（生产项目实跑抓出）：含 `searchable` 的工程**首次**构建后不得再产生
/// 增量迁移（0002）。此前重放模拟器不认表内 `FULLTEXT KEY`，把它当成名为 FULLTEXT
/// 的幽灵列 → 基线与当前 schema 不一致 → 生成
/// 「DROP COLUMN FULLTEXT + 重复 ADD FULLTEXT ft_*」的废迁移，真库执行必炸
/// （ERROR 1061 Duplicate key name）。
#[test]
fn searchable_does_not_produce_phantom_incremental_migration() {
    let dir = tmp_dir("phantom");
    write_project(
        &dir,
        "app search { version = \"0.1.0\" }\nentity Article {\n id Uuid @id @default(uuid)\n title String(200) @required\n body Text?\n searchable [title, body]\n}\n",
    );
    forma::build::run_build(&dir).unwrap();
    // 首次构建：只有基线
    assert!(
        !dir.join("generated/db/migrations/0002_upgrade.sql")
            .exists(),
        "phantom incremental migration was generated"
    );
    // 二次构建：零新增（幂等）
    forma::build::run_build(&dir).unwrap();
    assert!(
        !dir.join("generated/db/migrations/0002_upgrade.sql")
            .exists(),
        "second build must not create a migration"
    );
    // 基线里 FULLTEXT 只出现一次，且不含幽灵列
    let init = fs::read_to_string(dir.join("generated/db/migrations/0001_init.sql")).unwrap();
    assert_eq!(
        init.matches("FULLTEXT KEY").count(),
        1,
        "duplicate fulltext key: {init}"
    );
    // 幽灵列形态：名为 FULLTEXT 的**列定义行**（不是合法的 FULLTEXT KEY 约束）
    let ghost = init.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with("FULLTEXT") && !t.starts_with("FULLTEXT KEY")
    });
    assert!(!ghost, "ghost column FULLTEXT: {init}");
}
