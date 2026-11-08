//! 迁移链导出回归（roadmap M9，轨道 A-8）：Flyway / golang-migrate / Liquibase。

use forma::emit::migration_export::{export_files, export_to, MigrationFormat};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-migexp-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("migrations")).unwrap();
    dir
}

fn fixture(dir: &std::path::Path) {
    let m = dir.join("migrations");
    fs::write(
        m.join("0001_init.sql"),
        "-- init\nCREATE TABLE `users` (\n  id CHAR(36) NOT NULL\n);\nCREATE TABLE `items` (\n  id CHAR(36) NOT NULL\n);\n",
    )
    .unwrap();
    fs::write(
        m.join("0002_upgrade.sql"),
        "-- upgrade\nALTER TABLE users ADD COLUMN name VARCHAR(50);\n",
    )
    .unwrap();
    fs::write(
        m.join("0002_rollback.sql"),
        "-- rollback\nALTER TABLE users DROP COLUMN name;\n",
    )
    .unwrap();
}

fn paths(files: &[forma::emit::migration_export::ExportFile]) -> Vec<&str> {
    files.iter().map(|f| f.path.as_str()).collect()
}

#[test]
fn flyway_export_uses_versioned_filenames_and_copies_sql_verbatim() {
    let dir = tmp("flyway");
    fixture(&dir);
    let files = export_files(&dir.join("migrations"), MigrationFormat::Flyway).unwrap();
    let names = paths(&files);
    assert!(names.contains(&"V0001__init.sql"), "{names:?}");
    assert!(names.contains(&"V0002__upgrade.sql"), "{names:?}");
    let init = files.iter().find(|f| f.path == "V0001__init.sql").unwrap();
    assert!(
        init.content.contains("CREATE TABLE `users`"),
        "{}",
        init.content
    );
    assert_eq!(files.len(), 3, "SQL + README");
}

#[test]
fn golang_migrate_export_pairs_up_and_down_files() {
    let dir = tmp("golang");
    fixture(&dir);
    let files = export_files(&dir.join("migrations"), MigrationFormat::GolangMigrate).unwrap();
    let names = paths(&files);
    assert!(names.contains(&"000001_init.up.sql"), "{names:?}");
    assert!(names.contains(&"000001_init.down.sql"), "{names:?}");
    assert!(names.contains(&"000002_upgrade.up.sql"), "{names:?}");
    assert!(names.contains(&"000002_upgrade.down.sql"), "{names:?}");
    let down = files
        .iter()
        .find(|f| f.path == "000002_upgrade.down.sql")
        .unwrap();
    assert!(
        down.content.contains("DROP COLUMN name"),
        "{}",
        down.content
    );
    // 基线 down 由 CREATE TABLE 反推，逆序 DROP（items → users）
    let base_down = files
        .iter()
        .find(|f| f.path == "000001_init.down.sql")
        .unwrap();
    assert!(
        base_down.content.contains("DROP TABLE IF EXISTS `items`"),
        "{}",
        base_down.content
    );
    assert!(
        base_down.content.contains("DROP TABLE IF EXISTS `users`"),
        "{}",
        base_down.content
    );
    assert!(
        base_down.content.find("`items`") < base_down.content.find("`users`"),
        "{}",
        base_down.content
    );
}

#[test]
fn liquibase_export_generates_changelog_with_sqlfile_refs() {
    let dir = tmp("liquibase");
    fixture(&dir);
    let files = export_files(&dir.join("migrations"), MigrationFormat::Liquibase).unwrap();
    let xml = files.iter().find(|f| f.path == "db.changelog.xml").unwrap();
    assert!(
        xml.content.contains("changesets/0001_init.sql"),
        "{}",
        xml.content
    );
    assert!(
        xml.content.contains("changesets/0002_upgrade.sql"),
        "{}",
        xml.content
    );
    assert!(
        xml.content.contains("relativeToChangelogFile=\"true\""),
        "{}",
        xml.content
    );
    let names = paths(&files);
    assert!(names.contains(&"changesets/0001_init.sql"), "{names:?}");
    // 写盘（含子目录）
    let out = dir.join("out");
    let written = export_to(&dir.join("migrations"), &out, MigrationFormat::Liquibase).unwrap();
    assert_eq!(written.len(), files.len());
    assert!(out.join("changesets/0001_init.sql").is_file());
    assert!(out.join("db.changelog.xml").is_file());
}

#[test]
fn export_rejects_unknown_format_and_missing_chain() {
    assert!(MigrationFormat::parse("dbt").is_err());
    let dir = tmp("empty");
    assert!(export_files(&dir.join("migrations"), MigrationFormat::Flyway).is_err());
}
