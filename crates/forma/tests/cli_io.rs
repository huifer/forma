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

// ── CLI: patch --dry-run / artifacts verify / conflicts / capability check ──

fn write_project(dir: &std::path::Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: cliops\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

#[test]
fn cli_patch_dry_run_leaves_source_untouched() {
    let dir = tmp("patch-dry");
    let fm = "app cliops { version = \"0.1.0\" }\nentity Order {\n  id     Uuid @id @default(uuid)\n  status String(20) @required\n}\napi Orders { entity Order operations [list get create update delete] }\npage Orders {\n  route \"/orders\"\n  title { en \"Orders\" }\n  table { query Orders.list columns [status] actions { row [delete → Orders.delete confirm] } }\n}\n";
    write_project(&dir, fm);
    let fm_path = dir.join("forma/app.fm");
    let before = fs::read_to_string(&fm_path).unwrap();

    let patch = dir.join("patch.json");
    fs::write(
        &patch,
        r#"{"patchId":"dry-1","operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"note","type":"Text"},"required":false}]}"#,
    )
    .unwrap();
    let out = dir.join("draft.json");
    let status = forma()
        .args([
            "patch",
            dir.to_str().unwrap(),
            patch.to_str().unwrap(),
            "--dry-run",
            "--out",
        ])
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "patch --dry-run failed");
    // 源工程零写入
    assert_eq!(
        fs::read_to_string(&fm_path).unwrap(),
        before,
        "source must be untouched"
    );
    // 报告落盘且标记 applied（副本上）
    let draft = fs::read_to_string(&out).unwrap();
    assert!(draft.contains("dry-1"), "{draft}");
    assert!(
        draft.contains("\"status\": \"applied\"") || draft.contains("\"status\":\"applied\""),
        "{draft}"
    );
}

#[test]
fn cli_patch_without_dry_run_writes_back() {
    let dir = tmp("patch-apply");
    let fm = "app cliops { version = \"0.1.0\" }\nentity Order {\n  id     Uuid @id @default(uuid)\n  status String(20) @required\n}\napi Orders { entity Order operations [list get create update delete] }\npage Orders {\n  route \"/orders\"\n  title { en \"Orders\" }\n  table { query Orders.list columns [status] actions { row [delete → Orders.delete confirm] } }\n}\n";
    write_project(&dir, fm);
    let patch = dir.join("patch.json");
    fs::write(
        &patch,
        r#"{"operations":[{"op":"add-entity-field","entity":"Order","field":{"name":"note","type":"Text"},"required":false}]}"#,
    )
    .unwrap();
    let status = forma()
        .args(["patch", dir.to_str().unwrap(), patch.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "patch apply failed");
    let after = fs::read_to_string(dir.join("forma/app.fm")).unwrap();
    assert!(after.contains("note Text"), "{after}");
}

#[test]
fn cli_artifacts_verify_and_conflicts() {
    let dir = tmp("artifacts");
    let fm = "app cliops { version = \"0.1.0\" }\nentity Order {\n  id     Uuid @id @default(uuid)\n  status String(20) @required\n}\napi Orders { entity Order operations [list get create update delete] }\npage Orders {\n  route \"/orders\"\n  title { en \"Orders\" }\n  table { query Orders.list columns [status] actions { row [delete → Orders.delete confirm] } }\n}\n";
    write_project(&dir, fm);
    // 先 build 生成 manifest
    let status = forma().args(["build"]).arg(&dir).status().unwrap();
    assert!(status.success(), "build failed");

    // verify：干净 → 0
    let output = forma()
        .args(["artifacts", "verify"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "verify failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("all"), "{stdout}");

    // conflicts：干净 → 0
    let output = forma()
        .args(["artifacts", "conflicts"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "conflicts failed");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("0 conflict"),
        "{:?}",
        String::from_utf8_lossy(&output.stdout)
    );

    // 篡改一个产物 → verify 非 0，conflicts 非 0
    let target = dir.join("generated/backend/src/main.rs");
    fs::write(&target, "// tampered\n").unwrap();
    let output = forma()
        .args(["artifacts", "verify"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!output.status.success(), "verify should fail after tamper");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("modified"), "{stderr}");

    let output = forma()
        .args(["artifacts", "conflicts"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "conflicts should fail after tamper"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("modified"), "{stdout}");
}

#[test]
fn cli_capability_list_check_show() {
    let dir = tmp("cap");
    write_project(&dir, "app cliops { version = \"0.1.0\" }\nentity Order { id Uuid @id @default(uuid) status String(20) @required }\napi Orders { entity Order operations [list get] }\n");

    let output = forma()
        .args(["capability", "list"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("registered"), "{stdout}");

    let output = forma()
        .args(["capability", "check"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "check failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("capability check"), "{stdout}");

    let output = forma()
        .args(["capability", "show", "visualization.echarts"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "show failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("visualization.echarts"), "{stdout}");
}

// ── CLI: validate / doctor / graph / docs / migrate status/verify ──

fn write_project_io(dir: &std::path::Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: cliops\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const IO_FM: &str = "app cliops { version = \"0.1.0\" }\nentity Order {\n  id     Uuid @id @default(uuid)\n  status String(20) @required\n}\napi Orders { entity Order operations [list get create update delete] }\npage Orders {\n  route \"/orders\"\n  title { en \"Orders\" }\n  table { query Orders.list columns [status] actions { row [delete → Orders.delete confirm] } }\n}\n";

#[test]
fn cli_validate_and_doctor_agree_on_clean_project() {
    let dir = tmp("validate");
    write_project_io(&dir, IO_FM);

    let output = forma().args(["validate"]).arg(&dir).output().unwrap();
    assert!(output.status.success(), "validate failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("errors"), "{stdout}");

    let output = forma().args(["doctor"]).arg(&dir).output().unwrap();
    assert!(output.status.success(), "doctor failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.to_lowercase().contains("diagnostic")
            || stdout.contains("errors")
            || stdout.contains("✓"),
        "{stdout}"
    );
}

#[test]
fn cli_validate_nonzero_exit_on_semantic_error() {
    let dir = tmp("validate-bad");
    write_project_io(&dir, "app cliops { version = \"0.1.0\" }\nentity Order { id Uuid @id @default(uuid) status Nope }\n");
    let output = forma().args(["validate"]).arg(&dir).output().unwrap();
    assert!(!output.status.success(), "validate should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[ERROR]") || stderr.contains("error"),
        "{stderr}"
    );
}

#[test]
fn cli_graph_outputs_nodes_and_edges() {
    let dir = tmp("graph");
    write_project_io(&dir, IO_FM);
    // graph 无 --json：文本模型（包含实体名）
    let output = forma().args(["graph"]).arg(&dir).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Order"), "{stdout}");

    // graph --json：机读图（需要显式目录）
    let output = forma()
        .args(["graph", "--json"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("nodes") || stdout.contains("\"nodes\""),
        "{stdout}"
    );
}

#[test]
fn cli_docs_validate_lint_graph() {
    // docs 命令在仓库根运行（docs/architecture 存在）
    // docs 命令需要显式目录（治理文档根 = 仓库根，内含 docs/architecture）
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    assert!(
        root.join("docs/architecture").is_dir(),
        "root must contain docs/architecture: {}",
        root.display()
    );
    let output = forma()
        .args(["docs", "validate"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "docs validate failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("governed doc"), "{stdout}");

    let output = forma().args(["docs", "lint"]).arg(&root).output().unwrap();
    assert!(output.status.success(), "docs lint failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("governed doc"), "{stdout}");

    let output = forma().args(["docs", "graph"]).arg(&root).output().unwrap();
    assert!(output.status.success(), "docs graph failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.to_lowercase().contains("edge") || stdout.contains("node"),
        "{stdout}"
    );
}

#[test]
fn cli_migrate_status_and_verify() {
    let dir = tmp("migrate");
    write_project_io(&dir, IO_FM);
    // 先 build 生成迁移链
    let status = forma().args(["build"]).arg(&dir).status().unwrap();
    assert!(status.success(), "build failed");

    let output = forma()
        .args(["migrate", "status"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "migrate status failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("migration(s)"), "{stdout}");

    let output = forma()
        .args(["migrate", "verify"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "migrate verify failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("迁移链") || stdout.contains("一致"),
        "{stdout}"
    );
}

// ── CLI: check / seed / init / bench / import openapi ──

fn write_io(dir: &std::path::Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: clifull\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

#[test]
fn cli_check_clean_and_error_paths() {
    let dir = tmp("check");
    write_io(&dir, IO_FM);
    let output = forma().args(["check"]).arg(&dir).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 errors"), "{stdout}");

    let bad = tmp("check-bad");
    write_io(&bad, "app clifull { version = \"0.1.0\" }\nentity Order { id Uuid @id @default(uuid) status Nope }\n");
    let output = forma().args(["check"]).arg(&bad).output().unwrap();
    assert!(!output.status.success(), "check should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[ERROR]"), "{stderr}");
}

#[test]
fn cli_seed_writes_and_verifies_deterministic_data() {
    let dir = tmp("seed");
    write_io(&dir, IO_FM);
    let status = forma().args(["seed"]).arg(&dir).status().unwrap();
    assert!(status.success(), "seed failed");
    // 种子落盘
    let seed_dir = dir.join("generated/db/seed");
    assert!(seed_dir.is_dir(), "seed dir missing");
    let entries: Vec<_> = fs::read_dir(&seed_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "no seed files");

    // --verify：磁盘种子与 DSL 推导一致 → 0
    let output = forma()
        .args(["seed", "--verify"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "seed verify failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("seed verify ok"), "{stdout}");

    // 篡改 → verify 非 0
    let victim = seed_dir
        .read_dir()
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .unwrap();
    fs::write(victim.path(), "-- tampered\n").unwrap();
    let output = forma()
        .args(["seed", "--verify"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!output.status.success(), "verify should fail after tamper");
}

#[test]
fn cli_init_scaffolds_working_project() {
    let dir = tmp("init");
    let status = forma().args(["init"]).arg(&dir).status().unwrap();
    assert!(status.success(), "init failed");
    assert!(dir.join("forma.yaml").is_file(), "manifest missing");
    assert!(dir.join("forma/app.fm").is_file(), "app.fm missing");
    // init 产物是可通过 check 的工程
    let output = forma().args(["check"]).arg(&dir).output().unwrap();
    assert!(
        output.status.success(),
        "init project should pass check: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_bench_runs_matrix_and_exits_zero() {
    let dir = tmp("bench");
    write_io(&dir, IO_FM);
    let output = forma()
        .args(["bench", "--skip-backend"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bench failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cases"), "{stdout}");
    assert!(stdout.contains("checks"), "{stdout}");

    // --json 机读
    let output = forma()
        .args(["bench", "--skip-backend", "--json"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("casesTotal") || stdout.contains("cases_total"),
        "{stdout}"
    );
}

#[test]
fn cli_import_openapi_writes_dsl_and_loss_report() {
    let dir = tmp("oas");
    let spec = dir.join("petstore.yaml");
    fs::write(
        &spec,
        r#"openapi: "3.0.0"
info:
  title: Pet Store
  version: "1.0.0"
paths:
  /pets:
    get:
      tags: [Pet]
      responses: {'200': {description: ok}}
components:
  schemas:
    Pet:
      type: object
      required: [name]
      properties:
        id: {type: string, format: uuid}
        name: {type: string, maxLength: 50}
        photos: {type: array, items: {type: string}}
"#,
    )
    .unwrap();
    let out = dir.join("app.fm");
    let report = dir.join("loss.json");
    let status = forma()
        .args(["import", "openapi"])
        .arg(&spec)
        .args(["--out"])
        .arg(&out)
        .args(["--loss-report"])
        .arg(&report)
        .status()
        .unwrap();
    assert!(status.success(), "import openapi failed");
    let dsl = fs::read_to_string(&out).unwrap();
    assert!(dsl.contains("entity Pet"), "{dsl}");
    let loss = fs::read_to_string(&report).unwrap();
    assert!(loss.contains("openapi"), "{loss}");
}

// ── CLI: docs check-links / check-dependencies / check-compatibility ──

fn write_doc_cli(root: &std::path::Path, rel: &str, content: &str) {
    let p = root.join("docs/architecture").join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn full_rfc_cli(id: &str, deps: &str) -> String {
    let mut s = format!(
        "---\nid: {id}\ntitle: {id} 测试\nstatus: Accepted\nversion: 1.0\nauthors: [tester]\ncreated: 2026-11-04\nupdated: 2026-11-04\ndomain: architecture\ndepends_on: [{deps}]\nrelated_rfcs: []\nrelated_adrs: []\nbreaking_change: false\n---\n\n# {id}\n\n"
    );
    for (n, title) in [
        (1, "Metadata"),
        (2, "Abstract"),
        (3, "Motivation"),
        (4, "Goals"),
        (5, "Non-goals"),
        (6, "Terminology"),
        (7, "Architectural Context"),
        (8, "Design"),
        (9, "Compatibility"),
        (10, "Security Considerations"),
        (11, "Performance and Reliability"),
        (12, "Alternatives Considered"),
        (13, "Trade-offs"),
        (14, "Migration Plan"),
        (15, "Implementation Plan"),
        (16, "Open Questions"),
        (17, "Conformance Requirements"),
        (18, "Acceptance Criteria"),
        (19, "References"),
        (20, "Change Log"),
    ] {
        s.push_str(&format!("## {n}. {title}\n\n正文。\n\n"));
    }
    s
}

#[test]
fn cli_docs_check_links_clean_and_broken() {
    let root = tmp("docs-links");
    write_doc_cli(
        &root,
        "rfc/RFC-0001-a.zh-CN.md",
        &full_rfc_cli("RFC-0001", ""),
    );

    // 干净 → 0
    let output = forma()
        .args(["docs", "check-links"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "check-links clean failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("governed doc"), "{stdout}");

    // 断链 → warning（exit 0），但 stderr 含 docs-link-broken
    write_doc_cli(
        &root,
        "rfc/RFC-0002-b.zh-CN.md",
        &full_rfc_cli("RFC-0002", "").replace(
            "## 19. References",
            "## 19. References\n\n- [missing](missing.md)",
        ),
    );
    let output = forma()
        .args(["docs", "check-links"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(output.status.success(), "broken link is a warning, exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("docs-link-broken"), "{stderr}");
}

#[test]
fn cli_docs_check_dependencies_self_and_cycle() {
    let root = tmp("docs-deps");
    // 自依赖 → 非 0
    write_doc_cli(
        &root,
        "rfc/RFC-0001-a.zh-CN.md",
        &full_rfc_cli("RFC-0001", "RFC-0001"),
    );
    let output = forma()
        .args(["docs", "check-dependencies"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!output.status.success(), "self-dep should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("docs-dep-self"), "{stderr}");

    // 干净依赖 → 0
    let root2 = tmp("docs-deps-ok");
    write_doc_cli(
        &root2,
        "rfc/RFC-0001-a.zh-CN.md",
        &full_rfc_cli("RFC-0001", ""),
    );
    write_doc_cli(
        &root2,
        "rfc/RFC-0002-b.zh-CN.md",
        &full_rfc_cli("RFC-0002", "RFC-0001"),
    );
    let output = forma()
        .args(["docs", "check-dependencies"])
        .arg(&root2)
        .output()
        .unwrap();
    assert!(output.status.success(), "clean deps failed");
}

#[test]
fn cli_docs_check_compatibility_breaking_without_migration() {
    let root = tmp("docs-compat");
    // breaking_change: true 且删掉 Migration Plan 章节 → 非 0
    let doc = full_rfc_cli("RFC-0001", "")
        .replace("breaking_change: false", "breaking_change: true")
        .replace("## 14. Migration Plan\n\n正文。\n\n", "");
    write_doc_cli(&root, "rfc/RFC-0001-a.zh-CN.md", &doc);
    let output = forma()
        .args(["docs", "check-compatibility"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "breaking without migration should fail"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("docs-compat-breaking-without-migration")
            || stderr.contains("docs-compat-breaking-without-migration"),
        "{stdout} {stderr}"
    );

    // 干净 → 0
    let root2 = tmp("docs-compat-ok");
    write_doc_cli(
        &root2,
        "rfc/RFC-0001-a.zh-CN.md",
        &full_rfc_cli("RFC-0001", ""),
    );
    let output = forma()
        .args(["docs", "check-compatibility"])
        .arg(&root2)
        .output()
        .unwrap();
    assert!(output.status.success(), "clean compat failed");
}
