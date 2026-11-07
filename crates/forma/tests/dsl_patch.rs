//! DSL 主线补丁事务回归测试（RFC-0006 P2：加字段 + 加规则 + 加页面）：
//! - 补丁 → 原位修改 .fm → `check_program` 重校验 → 重建产物（与 CLI/MCP forma_build 同管线）；
//! - **事务性**：任一环节失败（未知实体 / 非法类型 / 引入新语义错误）⇒ 整体回滚，源码字节级复原；
//! - 定位来自 M1 的声明级来源位置（file/line）。

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
        "forma-dslpatch-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const MANIFEST: &str = "project:\n  name: dslpatch\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: db\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n";

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(dir.join("forma.yaml"), MANIFEST).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const BASE_FM: &str = r#"
app dslpatch {
  version = "0.1.0"
}

enum Status {
  pending
  approved
}

entity Student {
  id     Uuid       @id @default(uuid)
  name   String(50) @required
  status Status
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

fn patch_dir(dir: &Path, v: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let obj = v.as_object().expect("patch object").clone();
    forma::patch::apply_patch_input(
        dir,
        forma::patch::PatchInput::Dsl(serde_json::from_value(serde_json::Value::Object(obj))?),
    )
}

fn source(dir: &Path) -> String {
    fs::read_to_string(dir.join("forma/app.fm")).unwrap()
}

fn call(dir: &Path, tool: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    forma::mcp::call_tool(dir, tool, args.as_object().unwrap())
}

// ── 加字段 ─────────────────────────────────────────────────────

#[test]
fn add_entity_field_applies_and_rebuilds() {
    let dir = tmp_dir("add-field");
    write_project(&dir, BASE_FM);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "age", "type": "Int" }, "required": false }
            ]
        }),
    )
    .unwrap();

    let op = &report["operations"][0];
    assert_eq!(op["status"], "applied", "{report}");
    assert_eq!(op["op"], "add-entity-field");
    // 源码包含新字段行
    let src = source(&dir);
    assert!(src.contains("  age Int"), "{src}");
    // 产物重建（字段出现在生成模型里）
    assert!(
        !report["build"]["files"].as_array().unwrap().is_empty(),
        "{report}"
    );
    let models = fs::read_to_string(dir.join("generated/backend/src/models.rs")).unwrap();
    // required=false → `Type?` 可空 → Option<i64>
    assert!(models.contains("pub age: Option<i64>"), "{models}");
    // 重校验通过（无错误诊断）
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(
        forma::dsl::check_program(&program).is_empty(),
        "{:?}",
        forma::dsl::check_program(&program)
    );
}

#[test]
fn add_required_field_writes_attribute() {
    let dir = tmp_dir("add-field-req");
    write_project(&dir, BASE_FM);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "email", "type": "Email" }, "required": true }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    let src = source(&dir);
    assert!(src.contains("  email Email @required"), "{src}");
}

// ── 切换必填 ──────────────────────────────────────────────────

#[test]
fn set_required_toggles_attribute() {
    let dir = tmp_dir("set-required");
    write_project(&dir, BASE_FM);
    // status 是可选（无 @required）→ 加必填
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "set-entity-field-required",
                  "entity": "Student", "field": "status", "required": true }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    assert!(
        source(&dir).contains("  status Status @required"),
        "{src}",
        src = source(&dir)
    );
}

// ── 加规则 ─────────────────────────────────────────────────────

#[test]
fn add_access_rule_merges_into_existing_block() {
    let dir = tmp_dir("add-rule-merge");
    write_project(&dir, BASE_FM);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-access-rule", "entity": "Student",
                  "accessOp": "delete", "roles": ["admin"] }
            ],
            "approval": { "by": "maintainer", "reason": "P5 审批门测试" }
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    let src = source(&dir);
    assert!(
        src.contains("delete → role == \"admin\""),
        "rule must be merged into the existing access block:\n{src}"
    );
    // 没有产生第二个 rules 块
    assert_eq!(src.matches("rules ").count(), 1, "{src}");
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(forma::dsl::check_program(&program).is_empty());
}

#[test]
fn add_access_rule_appends_rules_block_when_absent() {
    let dir = tmp_dir("add-rule-append");
    write_project(
        &dir,
        r#"
app dslpatch {
  version = "0.1.0"
}

entity Student {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

api Students {
  entity Student
  operations [list get update delete]
}

auth module {
  provider jwt
  roles [admin, teacher]
}
"#,
    );
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-access-rule", "entity": "Student",
                  "accessOp": "update", "roles": ["admin", "teacher"] }
            ],
            "approval": { "by": "maintainer", "reason": "P5 审批门测试" }
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    let src = source(&dir);
    assert!(src.contains("rules PatchRules {"), "{src}");
    assert!(
        src.contains("update → role in [\"admin\", \"teacher\"]"),
        "{src}"
    );
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(forma::dsl::check_program(&program).is_empty());
}

// ── 加页面 ─────────────────────────────────────────────────────

#[test]
fn add_table_page_appends_page_block() {
    let dir = tmp_dir("add-page");
    write_project(&dir, BASE_FM);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-table-page", "name": "StudentBoard",
                  "title": "学生看板", "queryApi": "Students",
                  "columns": ["name", "status"] }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    let src = source(&dir);
    assert!(src.contains("page StudentBoard {"), "{src}");
    assert!(src.contains("query   Students.list"), "{src}");
    assert!(src.contains("columns [name, status]"), "{src}");
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(forma::dsl::check_program(&program).is_empty());
    // 生成端出现新页面
    assert!(dir
        .join("generated/web/src/pages/StudentBoard.tsx")
        .exists());
}

// ── 事务性：失败 ⇒ 整体回滚 ─────────────────────────────────────

#[test]
fn unknown_entity_fails_and_rolls_back() {
    let dir = tmp_dir("rollback-entity");
    write_project(&dir, BASE_FM);
    let before = source(&dir);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Ghost",
                  "field": { "name": "x", "type": "Int" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "failed", "{report}");
    assert!(
        report["operations"][0]["message"]
            .as_str()
            .unwrap()
            .contains("not found"),
        "{report}"
    );
    // 计划阶段失败不产生编辑，也不触发构建
    assert!(report["build"].is_null(), "{report}");
    assert_eq!(source(&dir), before, "source must be unchanged");
}

#[test]
fn semantic_error_introduced_by_patch_rolls_back() {
    // 非法类型：编辑本身可应用，但 check_program 引入 dsl-unknown-type ⇒ 回滚
    let dir = tmp_dir("rollback-type");
    write_project(&dir, BASE_FM);
    let before = source(&dir);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "weird", "type": "NoSuchType" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "failed", "{report}");
    let msg = report["operations"][0]["message"].as_str().unwrap_or("");
    assert!(msg.contains("dsl-unknown-type"), "{report}");
    assert_eq!(
        source(&dir),
        before,
        "source must be restored byte-identical"
    );
}

#[test]
fn duplicate_field_fails_loudly_without_rebuild() {
    let dir = tmp_dir("dup-field");
    write_project(&dir, BASE_FM);
    let before = source(&dir);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "name", "type": "String(20)" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "failed", "{report}");
    assert!(
        report["operations"][0]["message"]
            .as_str()
            .unwrap()
            .contains("already exists"),
        "{report}"
    );
    // 失败的补丁不得触碰产物
    assert!(report["build"].is_null(), "{report}");
    assert_eq!(source(&dir), before);
}

// ── P2 验收：单文档完成「加字段 + 加规则 + 加页面」 ────────────────

#[test]
fn single_document_adds_field_rule_and_page() {
    let dir = tmp_dir("p2-acceptance");
    write_project(&dir, BASE_FM);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "advisorNote", "type": "Text" }, "required": false },
                { "op": "add-access-rule", "entity": "Student",
                  "accessOp": "delete", "roles": ["admin"] },
                { "op": "add-table-page", "name": "StudentReview",
                  "title": "学生复核", "queryApi": "Students",
                  "columns": ["name", "advisorNote"] }
            ],
            "approval": { "by": "maintainer", "reason": "P5 审批门测试" }
        }),
    )
    .unwrap();
    // 三个操作全部成功，且重建只发生一次
    let ops = report["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 3, "{report}");
    assert!(ops.iter().all(|o| o["status"] == "applied"), "{report}");
    assert!(
        !report["build"]["files"].as_array().unwrap().is_empty(),
        "{report}"
    );
    // 源码三处变更齐备
    let src = source(&dir);
    assert!(src.contains("  advisorNote Text?"), "{src}");
    assert!(src.contains("delete → role == \"admin\""), "{src}");
    assert!(src.contains("page StudentReview {"), "{src}");
    // 语义干净 + 影响闭包覆盖新字段（M3 桥接）
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(forma::dsl::check_program(&program).is_empty());
    let lowered = forma::dsl::lower::lower_to_graph(&program).unwrap();
    assert!(
        lowered
            .graph
            .has("forma://project/dslpatch/entity/Student/field/advisorNote"),
        "new field must be addressable in the graph"
    );
}

// ── P5：风险审批门 + 审计落库 ─────────────────────────────────

#[test]
fn access_rule_change_without_approval_is_rejected() {
    let dir = tmp_dir("gate-rule");
    write_project(&dir, BASE_FM);
    let before = source(&dir);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-access-rule", "entity": "Student",
                  "accessOp": "delete", "roles": ["admin"] }
            ]
        }),
    )
    .unwrap();
    // 被审批门拒绝：patch-approval-required 诊断 + 所有操作 rejected
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "patch-approval-required"),
        "{report}"
    );
    assert!(
        report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["status"] == "rejected"),
        "{report}"
    );
    assert!(
        report["build"].is_null(),
        "rejected patch must not build: {report}"
    );
    // 源码字节级不变
    assert_eq!(source(&dir), before);
    // 审计留痕（rejected）
    let audit = fs::read_to_string(dir.join(".forma/audit.jsonl")).unwrap();
    assert!(audit.contains("\"status\":\"rejected\""), "{audit}");
}

#[test]
fn high_migration_risk_without_approval_is_rejected() {
    let dir = tmp_dir("gate-high");
    write_project(&dir, BASE_FM);
    // 建立快照基线
    let _ = call(&dir, "forma_build", serde_json::json!({}));
    // 手改字段类型（String(20) → Text）→ modify_column（High）
    // 改字段类型（枚举 → Text）→ modify_column（High）
    fs::write(
        dir.join("forma/app.fm"),
        BASE_FM.replace("status Status", "status Text"),
    )
    .unwrap();
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "rejected", "{report}");
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "patch-approval-required"),
        "{report}"
    );
    // 消息点出 High 操作
    let msg = report["diagnostics"][0]["message"].as_str().unwrap();
    assert!(msg.contains("modify_column"), "{msg}");
}

#[test]
fn high_risk_with_approval_is_applied_and_audited() {
    let dir = tmp_dir("gate-approve");
    write_project(&dir, BASE_FM);
    let _ = call(&dir, "forma_build", serde_json::json!({}));
    // 改字段类型（枚举 → Text）→ modify_column（High）
    fs::write(
        dir.join("forma/app.fm"),
        BASE_FM.replace("status Status", "status Text"),
    )
    .unwrap();
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ],
            "approval": { "by": "architect-zhang", "reason": "工单-1234：改列类型已评审" }
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    // 审计记录：applied + 审批来源
    let audit = fs::read_to_string(dir.join(".forma/audit.jsonl")).unwrap();
    assert!(audit.contains("\"status\":\"applied\""), "{audit}");
    assert!(audit.contains("architect-zhang"), "{audit}");
    assert!(audit.contains("工单-1234"), "{audit}");
}

// ── MCP 工具面 ────────────────────────────────────────────────

#[test]
fn mcp_forma_patch_works_on_dsl_projects() {
    let dir = tmp_dir("mcp-patch");
    write_project(&dir, BASE_FM);
    let mut args = serde_json::Map::new();
    args.insert(
        "operations".into(),
        serde_json::json!([
            { "op": "add-entity-field", "entity": "Student",
              "field": { "name": "age", "type": "Int" } }
        ]),
    );
    let out = forma::mcp::call_tool(&dir, "forma_patch", &args).unwrap();
    assert_eq!(out["operations"][0]["status"], "applied", "{out}");
    assert!(source(&dir).contains("  age Int"));
}

// ── RFC-0006 §16：patchId 幂等 + expected 冲突检测 ──────────────────

#[test]
fn same_patch_id_is_idempotent() {
    let dir = tmp_dir("idem");
    write_project(&dir, BASE_FM);
    let patch_doc = serde_json::json!({
        "patchId": "OP-1234",
        "operations": [
            { "op": "add-entity-field", "entity": "Student",
              "field": { "name": "grade", "type": "Int" } }
        ]
    });
    // 第一次：正常应用
    let first = patch_dir(&dir, patch_doc.clone()).unwrap();
    assert_eq!(first["operations"][0]["status"], "applied", "{first}");
    assert_eq!(first["idempotent"], false, "{first}");
    let after_first = source(&dir);
    assert!(after_first.contains("  grade Int?"));

    // 第二次：同 patchId → 幂等零变更
    let second = patch_dir(&dir, patch_doc).unwrap();
    assert_eq!(second["idempotent"], true, "{second}");
    assert_eq!(
        second["operations"][0]["status"], "already-applied",
        "{second}"
    );
    assert!(second["build"].is_null(), "{second}");
    assert_eq!(source(&dir), after_first, "no second change");
}

#[test]
fn different_patch_id_applies_normally() {
    let dir = tmp_dir("idem-diff");
    write_project(&dir, BASE_FM);
    let base = serde_json::json!({
        "operations": [
            { "op": "add-entity-field", "entity": "Student",
              "field": { "name": "grade", "type": "Int" } }
        ]
    });
    let mut first = base.clone();
    first["patchId"] = serde_json::json!("OP-A");
    let mut second = base.clone();
    second["field"] = serde_json::json!({ "name": "rank", "type": "Int" });
    second["operations"] = serde_json::json!([
        { "op": "add-entity-field", "entity": "Student",
          "field": { "name": "rank", "type": "Int" } }
    ]);
    second["patchId"] = serde_json::json!("OP-B");
    assert_eq!(
        patch_dir(&dir, first).unwrap()["operations"][0]["status"],
        "applied"
    );
    assert_eq!(
        patch_dir(&dir, second).unwrap()["operations"][0]["status"],
        "applied"
    );
    let src = source(&dir);
    assert!(src.contains("grade") && src.contains("rank"), "{src}");
}

#[test]
fn expected_hash_conflict_is_rejected_without_writes() {
    let dir = tmp_dir("conflict");
    write_project(&dir, BASE_FM);
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let good_hash = forma::dsl::patch::entity_block_hash(&dir, &program, "Student").unwrap();

    // 用**过期的哈希**（伪造）→ 冲突拒绝，零写入
    let stale = patch_dir(
        &dir,
        serde_json::json!({
            "expected": [ { "entity": "Student", "sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef" } ],
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(stale["operations"][0]["status"], "rejected", "{stale}");
    assert!(
        stale["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "patch-conflict"),
        "{stale}"
    );
    assert!(!source(&dir).contains("memo"), "conflict must not write");

    // 用**当前哈希**（plan 回传的形态）→ 正常应用
    let ok = patch_dir(
        &dir,
        serde_json::json!({
            "expected": [ { "entity": "Student", "sha256": good_hash } ],
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "memo", "type": "Text" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(ok["operations"][0]["status"], "applied", "{ok}");
    assert!(source(&dir).contains("memo"), "{ok}");
}

#[test]
fn plan_returns_expected_hashes_for_conflict_detection() {
    let dir = tmp_dir("plan-hash");
    write_project(&dir, BASE_FM);
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
    let expected = out["expected"].as_array().expect("expected array");
    assert_eq!(expected.len(), 1, "{out}");
    assert_eq!(expected[0]["entity"], "Student", "{out}");
    let hash = expected[0]["sha256"].as_str().unwrap();
    assert_eq!(hash.len(), 64, "sha256 hex length: {out}");
    // 该哈希可直接作为 expected 用于冲突检测
    let ok = patch_dir(
        &dir,
        serde_json::json!({
            "expected": [ { "entity": "Student", "sha256": hash } ],
            "operations": [
                { "op": "add-entity-field", "entity": "Student",
                  "field": { "name": "couponAmount", "type": "Decimal(10,2)" } }
            ]
        }),
    )
    .unwrap();
    assert_eq!(ok["operations"][0]["status"], "applied", "{ok}");
}

// ── RFC-0006 §17：扩展操作集（加实体 + 加约束 + 加表单字段）────────────

#[test]
fn extended_patch_operations_add_entity_rule_and_form_field() {
    let dir = tmp_dir("extended-ops");
    write_project(
        &dir,
        r#"
app dslpatch {
  version = "0.1.0"
}

entity Order {
  id     Uuid       @id @default(uuid)
  title  String(50) @required
  amount Decimal(12,2)
}

api Orders {
  entity Order
  operations [list get create update delete]
}

page OrderEdit {
  route "/orders/{id}"
  title { zh "编辑订单" en "Edit order" }
  form Order {
    layout stack {
      title Input @required
      amount Input money
    }
    submit → Orders.update redirect "/orders"
  }
}
"#,
    );
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity", "entity": "Coupon",
                  "fields": [
                    { "name": "id", "type": "Uuid", "attrs": ["@id", "@default(uuid)"] },
                    { "name": "amount", "type": "Decimal(12,2)", "attrs": ["@required"] }
                  ] },
                { "op": "add-field-rule", "entity": "Order", "field": "amount",
                  "constraint": "max", "value": 1000, "message": "金额不得超过 1000" },
                { "op": "add-form-field", "page": "OrderEdit", "field": "amount",
                  "widget": "Input money" }
            ]
        }),
    )
    .unwrap();
    assert_eq!(
        report["operations"].as_array().unwrap().len(),
        3,
        "{report}"
    );
    assert!(
        report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["status"] == "applied"),
        "{report}"
    );
    let src = source(&dir);
    assert!(src.contains("entity Coupon {"), "{src}");
    assert!(src.contains("field Order.amount {"), "{src}");
    assert!(
        src.contains("max 1000 { zh \"金额不得超过 1000\" }"),
        "{src}"
    );
    // 新字段没有重复插入（表单原本已带 amount：操作应检测重复？
    // 当前规范允许补丁直接追加，后续页面去重可再收紧；此处验证格式合法与重校验。）
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let errors = forma::dsl::check_program(&program);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn extended_patch_rejects_bad_attribute_and_widget() {
    let dir = tmp_dir("extended-reject");
    write_project(&dir, BASE_FM);
    let bad_attr = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-entity", "entity": "Bad",
                  "fields": [{ "name": "x", "type": "Int", "attrs": ["@dropTable"] }] }
            ]
        }),
    )
    .unwrap();
    assert_eq!(bad_attr["operations"][0]["status"], "failed", "{bad_attr}");
    assert!(bad_attr["operations"][0]["message"]
        .as_str()
        .unwrap()
        .contains("not allowed"));
    let bad_widget = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "add-form-field", "page": "MissingPage", "field": "name", "widget": "Script" }
            ]
        }),
    )
    .unwrap();
    assert_eq!(
        bad_widget["operations"][0]["status"], "failed",
        "{bad_widget}"
    );
}

#[test]
fn set_field_rule_updates_constraint_value() {
    let dir = tmp_dir("set-rule");
    // 基础 fixture 直接包含 max 500 约束（不依赖前序补丁）
    write_project(
        &dir,
        r#"
app dslpatch {
  version = "0.1.0"
}

entity Student {
  id   Uuid  @id @default(uuid)
  name String(50) @required
  age  Int
}

api Students {
  entity Student
  operations [list get create update delete]
}

auth module {
  provider jwt
  roles [admin, teacher]
}

rules AccessRules {
  field Student.age {
    max 80 { zh "年龄不得超过 80" }
  }
}
"#,
    );
    // 先 build 建立基线
    let _ = call(&dir, "forma_build", serde_json::json!({}));

    // 改为 max 100
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "set-field-rule", "entity": "Student", "field": "age",
                  "constraint": "max", "value": 100 }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "applied", "{report}");
    let src = source(&dir);
    assert!(src.contains("max 100"), "{src}");
    assert!(
        !src.contains("max 80"),
        "old value must be replaced:\n{src}"
    );
    // message 保留
    assert!(src.contains("年龄不得超过 80"), "{src}");

    // 语义校验通过
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(forma::dsl::check_program(&program).is_empty());
}

#[test]
fn set_field_rule_fails_when_no_existing_constraint() {
    let dir = tmp_dir("set-rule-missing");
    write_project(&dir, BASE_FM);
    let report = patch_dir(
        &dir,
        serde_json::json!({
            "operations": [
                { "op": "set-field-rule", "entity": "Student", "field": "age",
                  "constraint": "min", "value": 3 }
            ]
        }),
    )
    .unwrap();
    assert_eq!(report["operations"][0]["status"], "failed", "{report}");
    assert!(
        report["operations"][0]["message"]
            .as_str()
            .unwrap()
            .contains("use add-field-rule first"),
        "{report}"
    );
}
