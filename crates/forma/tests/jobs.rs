//! 定时任务（scheduled jobs）target 回归测试（M10/P1）。
//! 覆盖：DSL 解析、cron/op 检查诊断、MySQL 建表、迁移链、seed、Rust 后端编译、
//! IR bridge 节点、job 管理端点。

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
        "forma-jobs-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: jobs\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: rust\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        "app jobs { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n approved Bool\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\napi ItemApprove {\n  POST \"/items/{id}/approve\"\n  label { zh \"审批\" en \"Approve\" }\n  pre { approved == false }\n  message { zh \"该条目已审批\" en \"Item already approved\" }\n  effect { set Item.approved = true }\n}\njob cleanup {\n cron \"0 2 * * *\"\n label { zh \"每日清理\" en \"Daily cleanup\" }\n op ItemApprove\n}\n",
    )
    .unwrap();
}

#[test]
fn job_decl_parses_and_checks() {
    let dir = tmp_dir("parse");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-rust"), "{names:?}");

    // forma_jobs 表生成
    let ddl = fs::read_to_string(dir.join("generated/db/migrations/0001_init.sql")).unwrap();
    assert!(ddl.contains("CREATE TABLE forma_jobs"), "{ddl}");
    assert!(ddl.contains("cron VARCHAR(50)"), "{ddl}");
    assert!(ddl.contains("op VARCHAR(100)"), "{ddl}");

    // seed 含 job 行
    let seed = fs::read_to_string(dir.join("generated/db/seed/seed_0_1_0.sql")).unwrap();
    assert!(seed.contains("INSERT IGNORE INTO forma_jobs"), "{seed}");
    assert!(seed.contains("'cleanup'"), "{seed}");
    assert!(seed.contains("'0 2 * * *'"), "{seed}");

    // Rust 后端：jobs.rs 生成 + main 接线
    assert!(dir.join("generated/backend/src/jobs.rs").exists());
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(main.contains("mod jobs;"), "{main}");
    assert!(main.contains("spawn_scheduler"), "{main}");
    let jobs = fs::read_to_string(dir.join("generated/backend/src/jobs.rs")).unwrap();
    assert!(jobs.contains("cron_matches"), "{jobs}");
    assert!(jobs.contains("CommandTarget"), "{jobs}");
    assert!(jobs.contains("ItemApprove"), "{jobs}");
    assert!(jobs.contains("/api/jobs"), "{jobs}");

    // routes 接线
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(routes.contains("crate::jobs::router()"), "{routes}");

    // TARGETS 静态展开（表中含目标 command）
    assert!(jobs.contains("CommandTarget {"), "{jobs}");
    assert!(
        jobs.contains("set_cols: &[(\"approved\", \"TRUE\")]"),
        "{jobs}"
    );
    assert!(
        jobs.contains("pre: Some((\"approved\", \"=\", \"FALSE\"))"),
        "{jobs}"
    );
}

#[test]
fn job_check_rejects_bad_cron_and_unknown_op() {
    let dir = tmp_dir("check");
    setup(&dir);
    // 坏 cron + 未知 op
    fs::write(
        dir.join("forma/app.fm"),
        "app jobs { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\njob broken {\n cron \"99 * * * *\"\n label { zh \"坏\" en \"broken\" }\n op NoSuchApi\n}\n",
    )
    .unwrap();
    let report = forma::build::run_build(&dir);
    let err = report.expect_err("build must fail on diagnostics");
    let msg = format!("{err:#}");
    assert!(msg.contains("dsl-job-cron-format"), "{msg}");
    assert!(msg.contains("dsl-job-unknown-op"), "{msg}");
}

#[test]
fn job_check_rejects_input_dependent_effect() {
    let dir = tmp_dir("input-effect");
    setup(&dir);
    fs::write(
        dir.join("forma/app.fm"),
        "app jobs { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n note Text\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\napi ItemNote {\n  POST \"/items/{id}/note\"\n  input {\n    reason Text\n  }\n  effect { set Item.note = input.reason }\n}\njob note_daily {\n cron \"0 3 * * *\"\n label { zh \"备注\" en \"Note\" }\n op ItemNote\n}\n",
    )
    .unwrap();
    let err = forma::build::run_build(&dir).expect_err("input effect must be rejected");
    let msg = format!("{err:#}");
    assert!(msg.contains("dsl-job-input-effect"), "{msg}");
}

#[test]
fn no_jobs_means_zero_job_artifacts() {
    let dir = tmp_dir("zero");
    setup(&dir);
    fs::write(
        dir.join("forma/app.fm"),
        "app jobs { version = \"0.1.0\" }\nentity Item {\n id Uuid @id @default(uuid)\n name String(50) @required\n}\napi Items {\n entity Item\n operations [list get create update delete]\n}\n",
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    assert!(!dir.join("generated/backend/src/jobs.rs").exists());
    let ddl = fs::read_to_string(dir.join("generated/db/migrations/0001_init.sql")).unwrap();
    assert!(!ddl.contains("forma_jobs"), "{ddl}");
}

#[test]
fn job_ir_bridge_has_nodes_and_edges() {
    let dir = tmp_dir("ir");
    setup(&dir);
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".to_string()]).unwrap();
    let graph = forma::dsl::lower::lower_to_graph(&program).unwrap().graph;
    let uri = "forma://project/jobs/job/cleanup";
    assert!(graph.has(uri), "job node missing: {uri}");
    // 边：job → command（references）
    let edges = graph.edges_from(uri);
    assert!(
        edges
            .iter()
            .any(|e| e.to.starts_with("forma://project/jobs/api/ItemApprove")),
        "job→command edge missing: {edges:?}"
    );
}
