//! CLI 导入/迁移导出端到端回归（轨道 A-8 收尾）。
//!
//! 覆盖 `forma import sql|prisma --out/--loss-report` 与
//! `forma migrate export --format flyway|golang-migrate|liquibase` 的命令行路径、
//! 退出码与落盘产物；库函数级断言见 import_reverse.rs / migration_export.rs。

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-cliio-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn forma() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forma"))
}

const SQL: &str = "CREATE TABLE authors (\n  id CHAR(36) NOT NULL PRIMARY KEY,\n  name VARCHAR(100) NOT NULL\n);\nCREATE TABLE books (\n  id CHAR(36) NOT NULL PRIMARY KEY,\n  author_id CHAR(36) NOT NULL,\n  title VARCHAR(200) NOT NULL,\n  price DECIMAL(10,2),\n  blob BLOB,\n  FOREIGN KEY (author_id) REFERENCES authors(id)\n);\n";

const PRISMA: &str = "model Author {\n  id    String  @id @default(uuid())\n  name  String\n  books Book[]\n}\nmodel Book {\n  id       String @id @default(uuid())\n  authorId String\n  author   Author @relation(fields: [authorId], references: [id])\n  title    String\n  blob     Bytes?\n}\n";

#[test]
fn cli_import_sql_writes_dsl_and_loss_report() {
    let dir = tmp("sql");
    let src = dir.join("schema.sql");
    fs::write(&src, SQL).unwrap();
    let out = dir.join("app.fm");
    let report = dir.join("loss.json");

    let status = forma()
        .args(["import", "sql"])
        .arg(&src)
        .args(["--out"])
        .arg(&out)
        .args(["--loss-report"])
        .arg(&report)
        .status()
        .unwrap();
    assert!(status.success(), "import sql failed");

    let dsl = fs::read_to_string(&out).unwrap();
    assert!(dsl.contains("entity Author"), "{dsl}");
    assert!(dsl.contains("entity Book"), "{dsl}");
    let loss = fs::read_to_string(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&loss).unwrap();
    assert_eq!(v["format"], "sql", "{v}");
    assert!(!v["items"].as_array().unwrap().is_empty(), "{v}");
}

#[test]
fn cli_import_sql_without_out_prints_to_stdout() {
    let dir = tmp("sql-stdout");
    let src = dir.join("schema.sql");
    fs::write(&src, SQL).unwrap();
    let output = forma().args(["import", "sql"]).arg(&src).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("entity Author"), "{stdout}");
}

#[test]
fn cli_import_prisma_writes_dsl_and_loss_report() {
    let dir = tmp("prisma");
    let src = dir.join("schema.prisma");
    fs::write(&src, PRISMA).unwrap();
    let out = dir.join("app.fm");
    let report = dir.join("loss.json");
    let status = forma()
        .args(["import", "prisma"])
        .arg(&src)
        .args(["--out"])
        .arg(&out)
        .args(["--loss-report"])
        .arg(&report)
        .status()
        .unwrap();
    assert!(status.success(), "import prisma failed");
    let dsl = fs::read_to_string(&out).unwrap();
    assert!(dsl.contains("entity Author"), "{dsl}");
    assert!(dsl.contains("entity Book"), "{dsl}");
    let loss = fs::read_to_string(&report).unwrap();
    assert!(
        loss.contains("\"format\": \"prisma\"") || loss.contains("\"format\":\"prisma\""),
        "{loss}"
    );
}

#[test]
fn cli_import_unknown_format_fails() {
    let dir = tmp("bad");
    let src = dir.join("x.txt");
    fs::write(&src, "x").unwrap();
    let output = forma().args(["import", "dbt"]).arg(&src).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown import target"), "{stderr}");
}

#[test]
fn cli_migrate_export_all_three_formats() {
    let dir = tmp("mig");
    let mig = dir.join("generated/db/migrations");
    fs::create_dir_all(&mig).unwrap();
    fs::write(
        mig.join("0001_init.sql"),
        "-- init\nCREATE TABLE `users` (id CHAR(36) NOT NULL);\n",
    )
    .unwrap();
    fs::write(
        mig.join("0002_upgrade.sql"),
        "-- up\nALTER TABLE users ADD COLUMN name VARCHAR(50);\n",
    )
    .unwrap();
    fs::write(
        mig.join("0002_rollback.sql"),
        "-- down\nALTER TABLE users DROP COLUMN name;\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: cliiomig\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  database:\n    engine: mysql\n    out: generated/db\n",
    )
    .unwrap();

    for (fmt, expect) in [
        ("flyway", "V0001__init.sql"),
        ("golang-migrate", "000001_init.up.sql"),
        ("liquibase", "db.changelog.xml"),
    ] {
        let out_dir = dir.join(format!("out-{fmt}"));
        let status = forma()
            .args(["migrate", "export"])
            .arg(&dir)
            .args(["--format", fmt, "--out"])
            .arg(&out_dir)
            .status()
            .unwrap();
        assert!(status.success(), "migrate export {fmt} failed");
        assert!(out_dir.join(expect).is_file(), "missing {expect}");
    }
}

#[test]
fn cli_migrate_export_rejects_bad_format() {
    let dir = tmp("badfmt");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: clibad\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    let output = forma()
        .args(["migrate", "export"])
        .arg(&dir)
        .args(["--format", "dbt"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown migration format"), "{stderr}");
}
