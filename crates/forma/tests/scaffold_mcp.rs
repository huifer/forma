//! `forma init` 脚手架与 MCP 工具面的集成测试。
//!
//! init 必须给出「第一分钟就能跑通」的体验：validate/build/migrate verify 开箱全绿。
//! MCP 必须认识 v0.3 DSL 工程（此前全部工具走联邦引擎，对 DSL 工程返回空）。

use forma::build::run_build;
use forma::mcp::call_tool;
use forma::scaffold::scaffold_project;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "forma-mcp-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn school_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/school")
        .canonicalize()
        .unwrap()
}

fn args() -> serde_json::Map<String, serde_json::Value> {
    serde_json::Map::new()
}

// ── forma init（v0.3 DSL 脚手架） ────────────────────────

#[test]
fn init_scaffold_is_a_working_dsl_project() {
    let dir = tmp_dir("init");
    scaffold_project(&dir).unwrap();

    // 形状：DSL 工程四要素
    assert!(dir.join("forma.yaml").is_file());
    assert!(dir.join("forma/app.fm").is_file());
    let yaml = fs::read_to_string(dir.join("forma.yaml")).unwrap();
    assert!(yaml.contains("source: [\"forma/**/*.fm\"]"), "{yaml}");
    assert!(yaml.contains("mobile:"), "脚手架应含 mobile target");

    // 行为：validate → build → migrate verify 开箱全绿
    let report = run_build(&dir).unwrap();
    assert!(report.step_names().contains(&"db-mysql"));
    assert!(report.step_names().contains(&"backend-rust"));
    assert!(report.step_names().contains(&"web-react"));
    assert!(report.step_names().contains(&"mobile-flutter"));
    assert!(report.step_names().contains(&"seed"));
    assert!(report.total_files() > 30);

    // 迁移链可用
    let status = call_tool(&dir, "forma_migrate", &args()).unwrap();
    assert!(!status["ledger"].as_array().unwrap().is_empty());

    // 幂等：重复 init 报错
    assert!(scaffold_project(&dir).is_err());
}

// ── MCP：DSL 工程 ────────────────────────────────────────

#[test]
fn mcp_graph_understands_dsl_projects() {
    let dir = school_dir();
    let graph = call_tool(&dir, "forma_graph", &args()).unwrap();
    let summary = &graph["summary"];
    assert_eq!(summary["entityCount"], 7, "{graph}");
    assert_eq!(summary["enums"].as_array().unwrap().len(), 2);
    assert_eq!(summary["apis"].as_array().unwrap().len(), 7);
    assert_eq!(summary["pages"].as_array().unwrap().len(), 14); // 12 管理 + 2 图表页
                                                                // 实体带表名与关系字段信息
    let students = summary["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Student")
        .unwrap();
    assert_eq!(students["table"], "students");
    let school_field = students["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "school")
        .unwrap();
    assert_eq!(school_field["relation"], true);
}

#[test]
fn mcp_check_reports_diagnostics_and_lint() {
    let dir = school_dir();
    let check = call_tool(&dir, "forma_check", &args()).unwrap();
    assert_eq!(check["errors"], 0, "{check}");
    assert_eq!(check["version"], "0.3.0"); // DSL app.version（非 manifest 版本）
    assert!(!check["lint"].as_array().unwrap().is_empty());
}

#[test]
fn mcp_build_matches_cli_pipeline() {
    let dir = school_dir();
    let build = call_tool(&dir, "forma_build", &args()).unwrap();
    assert_eq!(build["totalFiles"], 89); // +audit.rs（命令）+ 2 图表页（flutter）+ viz 模板 5 + school 图表 2? 按实际
    let steps: Vec<&str> = build["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["emitter"].as_str().unwrap())
        .collect();
    assert_eq!(
        steps,
        vec![
            "db-mysql",
            "backend-rust",
            "web-react",
            "viz-web",
            "openapi",
            "diagrams",
            "migrate",
            "seed",
            "mobile-flutter"
        ]
    );
}

#[test]
fn mcp_migrate_reports_version_chain() {
    let dir = school_dir();
    let migrate = call_tool(&dir, "forma_migrate", &args()).unwrap();
    let ledger = migrate["ledger"].as_array().unwrap();
    let versions: Vec<&str> = ledger
        .iter()
        .map(|e| e["version"].as_str().unwrap())
        .collect();
    assert_eq!(versions, vec!["0.1.0", "0.2.0", "0.3.0", "0.3.0"]); // +audit_logs 迁移
}

// ── MCP：联邦工程向后兼容 ─────────────────────────────────

#[test]
fn mcp_keeps_federated_projects_working() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/order-management")
        .canonicalize()
        .unwrap();
    let graph = call_tool(&dir, "forma_graph", &args()).unwrap();
    assert_eq!(graph["entityCount"], 1, "{graph}");
    let validate = call_tool(&dir, "forma_validate", &args()).unwrap();
    assert_eq!(validate["ok"], true);
}

#[test]
fn mcp_rejects_federated_tools_on_dsl_projects_with_guidance() {
    let dir = school_dir();
    let err = call_tool(&dir, "forma_patch", &args()).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("DSL 工程"), "{msg}");
    assert!(msg.contains("forma_check"), "错误信息应给出替代工具");

    let err = call_tool(&dir, "forma_impact", &{
        let mut m = args();
        m.insert("target".into(), serde_json::json!("Order"));
        m
    })
    .unwrap_err();
    assert!(format!("{err:#}").contains("forma_graph"));
}
