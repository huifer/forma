//! Agent 工程工具面回归测试（RFC-0006 P4）：
//! `forma_search` / `forma_get_context` / `forma_plan` / `forma_report`。
//!
//! 验收：一次 Agent 会话可完成 §8.5 全流程（发现 → 收敛 → 计划 → 应用 → 汇报），
//! **无需读取目标源码**——所有信息经由工具面返回。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "forma-agent-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const MANIFEST: &str = "project:\n  name: agentdemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n";

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(dir.join("forma.yaml"), MANIFEST).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

fn call(dir: &Path, tool: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    forma::mcp::call_tool(dir, tool, args.as_object().unwrap())
}

fn source(dir: &Path) -> String {
    fs::read_to_string(dir.join("forma/app.fm")).unwrap()
}

const BASE_FM: &str = r#"
app agentdemo {
  version = "0.1.0"
}

entity Student {
  id     Uuid       @id @default(uuid)
  name   String(50) @required
  status String(20)
}

entity School {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

api Students {
  entity Student
  operations [list get create update delete]
  list { filters [name] }
}

auth module {
  provider jwt
  roles [admin, teacher]
}

rules AccessRules {
  access Student {
    update → role in ["admin", "teacher"]
  }
}
"#;

// ── forma_search：发现 ─────────────────────────────────────────

#[test]
fn search_finds_nodes_by_keyword_and_kind() {
    let dir = tmp_dir("search");
    write_project(&dir, BASE_FM);
    let out = call(
        &dir,
        "forma_search",
        serde_json::json!({ "query": "Student" }),
    )
    .unwrap();
    assert!(out["total"].as_u64().unwrap() > 0, "{out}");
    let uris: Vec<&str> = out["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(
        uris.contains(&"forma://project/agentdemo/entity/Student"),
        "{out}"
    );
    // kind 过滤
    let fields = call(
        &dir,
        "forma_search",
        serde_json::json!({ "query": "status", "kind": "entity-field" }),
    )
    .unwrap();
    let hits: Vec<&str> = fields["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(hits.contains(&"status"), "{fields}");
    // 空结果给类型提示（Agent 不必猜词表）
    let none = call(
        &dir,
        "forma_search",
        serde_json::json!({ "query": "zzz-nothing" }),
    )
    .unwrap();
    assert!(none["hint"]
        .as_str()
        .unwrap_or_default()
        .contains("searchable kinds"));
    // 来源位置可定位（无需读源码）
    let first = &out["results"][0];
    assert!(first["file"].as_str().unwrap_or_default().contains(".fm"));
    assert!(first["line"].as_u64().unwrap_or(0) > 0);
}

// ── forma_get_context：收敛 ────────────────────────────────────

#[test]
fn get_context_returns_snippet_defines_and_impact() {
    let dir = tmp_dir("context");
    write_project(&dir, BASE_FM);
    let out = call(
        &dir,
        "forma_get_context",
        serde_json::json!({ "target": "Student" }),
    )
    .unwrap();
    assert_eq!(out["node"]["kind"], "entity", "{out}");
    // 声明块摘录：Agent 不读源码也能看到实体定义
    let snippet = out["snippet"].as_object().expect("snippet");
    let text = snippet["text"].as_str().unwrap();
    assert!(text.contains("entity Student {"), "{snippet:?}");
    assert!(text.contains("name   String(50) @required"), "{snippet:?}");
    // 定义产物：字段 + 表 + 列
    let defines: Vec<&str> = out["defines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap())
        .collect();
    assert!(defines.contains(&"entity-field"), "{out}");
    assert!(defines.contains(&"table"), "{out}");
    // 引用者：api 操作 + 页面引用它
    let referenced_by = out["referencedBy"].as_array().unwrap();
    let names: Vec<&str> = referenced_by
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert!(
        names.iter().any(|n| n.starts_with("Students.")),
        "api operations must reference the entity: {out}"
    );
    // 影响闭包
    assert!(out["impactCount"].as_u64().unwrap() > 0, "{out}");
}

// ── forma_plan：计划（零副作用）────────────────────────────────

#[test]
fn plan_is_side_effect_free_and_reports_migration_risk() {
    let dir = tmp_dir("plan");
    write_project(&dir, BASE_FM);
    // 先构建建立快照基线（plan 的迁移操作相对最近一次构建）
    let _ = call(&dir, "forma_build", serde_json::json!({}));
    let before = fs::read_to_string(dir.join("forma/app.fm")).unwrap();

    let out = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "couponAmount", "type": "Decimal(10,2)" } }
            ]
        }),
    )
    .unwrap();

    // 校验 + 编辑清单
    assert_eq!(out["operations"][0]["status"], "applied", "{out}");
    // 迁移风险：add_column → low → agent-safe
    assert_eq!(out["migrations"]["pendingOps"], 1, "{out}");
    assert_eq!(out["migrations"]["ops"][0]["kind"], "add_column", "{out}");
    assert_eq!(out["migrations"]["ops"][0]["risk"], "low", "{out}");
    assert_eq!(out["verdict"], "agent-safe", "{out}");
    assert_eq!(
        out["migrations"]["risk"]["approvalRequired"], false,
        "{out}"
    );
    // 影响面：新字段节点入图
    assert!(out["impact"]["newNodes"].as_u64().unwrap() > 0, "{out}");
    let new_uris = out["impact"]["newNodeUris"].as_array().unwrap();
    assert!(
        new_uris
            .iter()
            .any(|u| u.as_str().unwrap().contains("field/couponAmount")),
        "{out}"
    );
    // **零副作用**：真实工程的源码字节级不变
    assert_eq!(
        fs::read_to_string(dir.join("forma/app.fm")).unwrap(),
        before
    );
}

#[test]
fn plan_flags_high_risk_as_requires_approval() {
    let dir = tmp_dir("plan-high");
    write_project(&dir, BASE_FM);
    // 建立快照基线
    let _ = call(&dir, "forma_build", serde_json::json!({}));

    // 仅改校验（@required）不动 schema → 迁移零操作 → agent-safe
    let report = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "set-entity-field-required", "entity": "Student",
                  "field": "status", "required": true }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["verdict"], "agent-safe", "{report}");
    assert_eq!(report["migrations"]["pendingOps"], 0, "{report}");

    // 真正的 High：改字段类型（modify_column，High）
    fs::write(
        dir.join("forma/app.fm"),
        BASE_FM.replace("status String(20)", "status Text"),
    )
    .unwrap();
    let out2 = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    // 快照基线里 status 是 varchar(20)，规格已是 TEXT → pending 含 modify_column（High）
    assert_eq!(out2["verdict"], "requires-approval", "{out2}");
    assert_eq!(
        out2["migrations"]["risk"]["approvalRequired"], true,
        "{out2}"
    );
}

#[test]
fn plan_flags_access_rule_change_as_requires_approval() {
    // §8.7：权限规则变更 → High → plan 必须给出 requires-approval
    let dir = tmp_dir("plan-access");
    write_project(&dir, BASE_FM);
    let out = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-access-rule", "entity": "Student",
                  "accessOp": "delete", "roles": ["admin"] }
            ]
        }),
    )
    .unwrap();
    assert_eq!(out["verdict"], "requires-approval", "{out}");
    assert_eq!(out["migrations"]["risk"]["accessRuleChange"], true, "{out}");
}

#[test]
fn plan_blocks_when_operations_fail() {
    let dir = tmp_dir("plan-blocked");
    write_project(&dir, BASE_FM);
    let out = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Ghost",
                  "field": { "name": "x", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(out["verdict"], "blocked", "{out}");
    assert_eq!(out["operations"][0]["status"], "failed", "{out}");
    assert!(out["operations"][0]["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

// ── forma_report：汇报 ─────────────────────────────────────────

#[test]
fn report_aggregates_check_migrations_and_targets() {
    let dir = tmp_dir("report");
    write_project(&dir, BASE_FM);
    let out = call(&dir, "forma_report", serde_json::json!({})).unwrap();

    assert_eq!(out["check"]["status"], "pass", "{out}");
    assert_eq!(out["check"]["errors"], 0, "{out}");
    assert_eq!(out["model"]["entities"], 2, "{out}");
    assert_eq!(out["model"]["apis"], 1, "{out}");
    // 构建后：迁移链校验 + 待执行迁移为 0
    let _ = call(&dir, "forma_build", serde_json::json!({}));
    let after_build = call(&dir, "forma_report", serde_json::json!({})).unwrap();
    assert_eq!(
        after_build["verify"]["migrationChain"], "pass",
        "{after_build}"
    );
    assert_eq!(after_build["migrations"]["pendingOps"], 0, "{after_build}");

    // 手改规格（未 build）→ 报告必须暴露待执行迁移（add_column, low）
    fs::write(
        dir.join("forma/app.fm"),
        BASE_FM.replace("  status String(20)", "  status String(20)\n  memo  Text"),
    )
    .unwrap();
    let pending = call(&dir, "forma_report", serde_json::json!({})).unwrap();
    assert!(
        pending["migrations"]["pendingOps"].as_u64().unwrap() >= 1,
        "{pending}"
    );
    assert_eq!(pending["migrations"]["overallRisk"], "low", "{pending}");
    // check 仍然 pass（新字段合法）
    assert_eq!(pending["check"]["status"], "pass", "{pending}");
}

// ── 全流程（§8.5）：发现 → 收敛 → 计划 → 应用 → 汇报 ────────────

#[test]
fn full_agent_session_without_reading_source() {
    let dir = tmp_dir("session");
    write_project(&dir, BASE_FM);

    // 1-2. 发现 + 收敛
    let _ = call(
        &dir,
        "forma_search",
        serde_json::json!({ "query": "Student" }),
    )
    .unwrap();
    let _ = call(
        &dir,
        "forma_get_context",
        serde_json::json!({ "target": "Student" }),
    )
    .unwrap();

    // 3. 计划（Low 风险 → agent-safe）
    let plan = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "couponAmount", "type": "Decimal(10,2)" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(plan["verdict"], "agent-safe", "{plan}");

    // 4. 应用
    let patch = call(
        &dir,
        "forma_patch",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "couponAmount", "type": "Decimal(10,2)" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(patch["operations"][0]["status"], "applied", "{patch}");

    // 5. 汇报：变更已构建进产物，check pass，迁移链一致
    let report = call(&dir, "forma_report", serde_json::json!({})).unwrap();
    assert_eq!(report["check"]["status"], "pass", "{report}");
    assert_eq!(report["verify"]["migrationChain"], "pass", "{report}");
    // 影响闭包含优惠券字段（M3 桥接）
    let ctx = call(
        &dir,
        "forma_get_context",
        serde_json::json!({ "target": "Student" }),
    )
    .unwrap();
    let defines: Vec<&str> = ctx["defines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert!(defines.contains(&"couponAmount"), "{ctx}");
}

// ── 联邦工程的边界提示 ──────────────────────────────────────────

#[test]
fn plan_rejects_federal_projects_with_guidance() {
    let dir = tmp_dir("federal");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/order-management")
        .canonicalize()
        .unwrap();
    // 拷贝联邦原型
    copy_dir(&src, &dir);
    let err = call(&dir, "forma_plan", serde_json::json!({ "operations": [] })).unwrap_err();
    assert!(format!("{err:#}").contains("DSL 工程"), "{err}");
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            ".forma" | "generated" | "node_modules" | "target" | "dist"
        ) {
            continue;
        }
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn plan_reports_expected_conflict_tokens() {
    let dir = tmp_dir("plan-expected");
    write_project(&dir, BASE_FM);
    let out = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "expectation", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    assert!(
        out["expected"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| { e["entity"] == "Student" && e["sha256"].is_string() }),
        "{out}"
    );
}

#[test]
fn patch_with_expected_and_patch_id_is_idempotent_and_conflict_safe() {
    let dir = tmp_dir("patch-expected");
    write_project(&dir, BASE_FM);

    let plan = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    let sha = plan["expected"].as_array().unwrap()[0]["sha256"]
        .as_str()
        .unwrap()
        .to_string();

    let first = call(
        &dir,
        "forma_patch",
        serde_json::json!({
            "patchId": "OP-2026-11-05-01",
            "expected": [
                { "entity": "Student", "sha256": sha }
            ],
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(first["operations"][0]["status"], "applied", "{first}");
    assert_eq!(first["idempotent"], false, "{first}");

    let second = call(
        &dir,
        "forma_patch",
        serde_json::json!({
            "patchId": "OP-2026-11-05-01",
            "expected": [
                { "entity": "Student", "sha256": sha }
            ],
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(second["idempotent"], true, "{second}");
    assert_eq!(
        second["operations"][0]["status"], "already-applied",
        "{second}"
    );

    let stale = call(
        &dir,
        "forma_patch",
        serde_json::json!({
            "patchId": "OP-2026-11-05-02",
            "expected": [
                { "entity": "Student", "sha256": "stale-hash" }
            ],
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "newName", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    assert!(
        stale["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "patch-conflict"),
        "{stale}"
    );
    assert!(!source(&dir).contains("newName"), "conflict must not write");
}

#[test]
fn plan_returns_base_revision_and_patch_binds_to_it() {
    let dir = tmp_dir("base-revision");
    write_project(&dir, BASE_FM);
    let plan = call(
        &dir,
        "forma_plan",
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "baseBound", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    let revision = plan["baseRevision"].as_str().unwrap().to_string();
    assert_eq!(revision, "agentdemo@1.2.0", "{plan}");

    let applied = call(
        &dir,
        "forma_patch",
        serde_json::json!({
            "baseRevision": revision,
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "baseBound", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(applied["operations"][0]["status"], "applied", "{applied}");
}

#[test]
fn stale_base_revision_is_rejected_without_writes() {
    let dir = tmp_dir("stale-revision");
    write_project(&dir, BASE_FM);
    let before = source(&dir);
    let out = call(
        &dir,
        "forma_patch",
        serde_json::json!({
            "baseRevision": "agentdemo@0.9.0",
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "shouldNotWrite", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(out["operations"][0]["status"], "rejected", "{out}");
    assert!(
        out["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "patch-base-revision"),
        "{out}"
    );
    assert_eq!(source(&dir), before);
}
