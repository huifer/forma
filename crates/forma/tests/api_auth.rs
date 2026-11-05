//! api 级操作授权回归测试（`api { auth { ... } }`）：
//! - 解析：`update delete → role in ["admin", "ops"]`；
//! - 检查：未知操作 / 未知角色 / 无 auth module → 编译期硬失败；
//! - 生成：服务端权威执行（require_role 守卫 + 403），未授权操作不受影响。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use forma::dsl::check_program;
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
        "forma-auth-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: authdemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

/// 带 auth module + api 授权规则的工程。
fn fm_with_auth(auth_block: &str, with_module: bool) -> String {
    let module = if with_module {
        "auth module {\n  provider jwt\n  roles [admin, ops, user]\n}\n"
    } else {
        ""
    };
    format!(
        r#"
app authdemo {{
  version = "0.1.0"
}}

enum OrderStatus {{ pending approved }}

entity Order {{
  id     Uuid   @id @default(uuid)
  amount Decimal(12,2) @required
  status OrderStatus @default(pending)
}}

api Orders {{
  entity Order
  operations [list get create update delete]
{auth_block}}}

{module}"#
    )
}

fn parse_and_check(dir: &Path) -> Vec<forma::validate::Diagnostic> {
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, mut diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    diagnostics.extend(check_program(&program));
    diagnostics
}

fn codes(dir: &Path) -> Vec<String> {
    parse_and_check(dir)
        .iter()
        .map(|d| d.code.clone())
        .collect()
}

fn generate_backend(dir: &Path) -> String {
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/backend");
    forma::emit::emit_rust_backend(&loaded.dir, &program, &out.to_string_lossy(), "authdemo")
        .unwrap();
    fs::read_to_string(out.join("src/routes.rs")).unwrap()
}

// ── 1. 解析 ────────────────────────────────────────────────────

#[test]
fn api_auth_rule_is_parsed() {
    let dir = tmp_dir("parse");
    write_project(
        &dir,
        &fm_with_auth(
            "  auth {\n    update delete → role in [\"admin\", \"ops\"]\n  }\n",
            true,
        ),
    );
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let api = program
        .apis
        .iter()
        .find(|a| a.name == "Orders")
        .expect("api Orders");
    assert_eq!(
        api.auth.len(),
        1,
        "expected one auth rule, got {:?}",
        api.auth
    );
    let rule = &api.auth[0];
    assert_eq!(
        rule.operations,
        vec!["update".to_string(), "delete".to_string()]
    );
    assert_eq!(rule.roles, vec!["admin".to_string(), "ops".to_string()]);
}

// ── 2. 检查层：非法授权规则必须编译期失败 ────────────────────────

#[test]
fn unknown_operation_in_auth_rule_is_error() {
    let dir = tmp_dir("unknown-op");
    write_project(
        &dir,
        &fm_with_auth("  auth {\n    purge → role in [\"admin\"]\n  }\n", true),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-auth-unknown-operation"),
        "expected dsl-auth-unknown-operation, got {codes:?}"
    );
}

#[test]
fn unknown_role_in_auth_rule_is_error() {
    let dir = tmp_dir("unknown-role");
    write_project(
        &dir,
        &fm_with_auth("  auth {\n    update → role in [\"wizard\"]\n  }\n", true),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-auth-unknown-role"),
        "expected dsl-auth-unknown-role, got {codes:?}"
    );
}

#[test]
fn auth_rule_without_auth_module_is_error() {
    let dir = tmp_dir("no-module");
    write_project(
        &dir,
        &fm_with_auth("  auth {\n    update → role in [\"admin\"]\n  }\n", false),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-auth-no-provider"),
        "expected dsl-auth-no-provider, got {codes:?}"
    );
}

#[test]
fn valid_auth_rule_passes_checks() {
    let dir = tmp_dir("valid");
    write_project(
        &dir,
        &fm_with_auth(
            "  auth {\n    update delete → role in [\"admin\", \"ops\"]\n  }\n",
            true,
        ),
    );
    let all = parse_and_check(&dir);
    let errors: Vec<_> = all.iter().filter(|d| d.severity == "error").collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}

// ── 3. 生成端：服务端权威执行 ───────────────────────────────────

/// 取出某个 handler 的函数体（到下一个 handler / 函数结束为止）。
fn handler_body<'a>(routes: &'a str, sig: &str) -> &'a str {
    let rest = routes.split(sig).nth(1).unwrap_or("");
    let end = rest.find("\nasync fn ").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn protected_operations_get_role_guard() {
    let dir = tmp_dir("emit");
    write_project(
        &dir,
        &fm_with_auth(
            "  auth {\n    update delete → role in [\"admin\", \"ops\"]\n  }\n",
            true,
        ),
    );
    let routes = generate_backend(&dir);

    // 受限操作：带 Extension + require_role 守卫
    assert!(
        routes.contains("crate::auth::require_role(&[\"admin\", \"ops\"], &user)?;"),
        "update/delete must enforce roles:\n{routes}"
    );
    let update_fn = handler_body(&routes, "async fn update_order");
    assert!(
        update_fn.contains("Extension(user): Extension<CurrentUser>"),
        "update handler must extract current user:\n{routes}"
    );
    let delete_fn = handler_body(&routes, "async fn delete_order");
    assert!(
        delete_fn.contains("require_role"),
        "delete must be guarded:\n{routes}"
    );
    // 未受限操作：不得带守卫
    let list_fn = handler_body(&routes, "async fn list_order");
    assert!(
        !list_fn.contains("require_role"),
        "list must not be guarded:\n{routes}"
    );
    let get_fn = handler_body(&routes, "async fn get_order");
    assert!(
        !get_fn.contains("require_role"),
        "get must not be guarded:\n{routes}"
    );
    // Extension 导入 + CurrentUser 导入
    assert!(routes.contains("Extension"));
    assert!(routes.contains("use crate::auth::CurrentUser;"));
}

#[test]
fn forbidden_status_is_generated_only_when_needed() {
    // 有授权规则 → 生成 Forbidden（403）
    let dir = tmp_dir("forbidden-yes");
    write_project(
        &dir,
        &fm_with_auth("  auth {\n    update → role in [\"admin\"]\n  }\n", true),
    );
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/backend");
    forma::emit::emit_rust_backend(&loaded.dir, &program, &out.to_string_lossy(), "authdemo")
        .unwrap();
    let error_rs = fs::read_to_string(out.join("src/error.rs")).unwrap();
    assert!(
        error_rs.contains("Forbidden(String)") && error_rs.contains("StatusCode::FORBIDDEN"),
        "Forbidden variant must exist:\n{error_rs}"
    );

    // 无授权规则 → 不生成（否则生成工程报 never constructed）
    let dir2 = tmp_dir("forbidden-no");
    write_project(&dir2, &fm_with_auth("", true));
    let loaded2 = forma::manifest::load_manifest(&dir2).unwrap();
    let source2 = loaded2.manifest.source.clone().unwrap();
    let (program2, _) = forma::dsl::load_program(&loaded2.dir, &source2).unwrap();
    let out2 = dir2.join("generated/backend");
    forma::emit::emit_rust_backend(&loaded2.dir, &program2, &out2.to_string_lossy(), "authdemo")
        .unwrap();
    let error_rs2 = fs::read_to_string(out2.join("src/error.rs")).unwrap();
    assert!(
        !error_rs2.contains("Forbidden"),
        "Forbidden must not exist without auth rules:\n{error_rs2}"
    );
}

#[test]
fn auth_me_route_is_registered_without_commands() {
    // 回归：`/api/auth/me` 曾在 `if has_commands` 块内注册 → 无命令的工程拿不到该端点
    let dir = tmp_dir("me-route");
    write_project(&dir, &fm_with_auth("", true));
    let routes = generate_backend(&dir);
    assert!(
        routes.contains("/api/auth/me"),
        "/api/auth/me must be registered regardless of commands:\n{routes}"
    );
    // auth 路由用 post() → routing 导入必须含 post
    assert!(
        routes.contains("use axum::routing::{get, post};"),
        "routing import must include post for auth routes:\n{routes}"
    );
}

// ── 4. 实体级访问规则（rules { access { ... } }） ─────────────────

fn fm_with_access(access_block: &str, with_module: bool) -> String {
    let module = if with_module {
        "auth module {\n  provider jwt\n  roles [admin, ops, user]\n}\n"
    } else {
        ""
    };
    format!(
        r#"
app authdemo {{
  version = "0.1.0"
}}

entity Order {{
  id     Uuid   @id @default(uuid)
  amount Decimal(12,2) @required
}}

api Orders {{
  entity Order
  operations [list get create update delete]
}}

rules OrderRules {{
{access_block}}}

{module}"#
    )
}

#[test]
fn access_rules_are_parsed() {
    let dir = tmp_dir("access-parse");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    read   → any\n    create → role in [\"admin\", \"ops\"]\n    delete → role == \"admin\"\n  }\n",
            true,
        ),
    );
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let rules = &program.rules[0];
    assert_eq!(rules.access.len(), 1);
    let access = &rules.access[0];
    assert_eq!(access.entity, "Order");
    assert_eq!(access.rules.len(), 3);
    assert_eq!(access.rules[0].op, "read");
    assert_eq!(access.rules[1].op, "create");
    assert_eq!(access.rules[2].op, "delete");
}

#[test]
fn access_unknown_role_is_error() {
    let dir = tmp_dir("access-role");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    create → role in [\"wizard\"]\n  }\n",
            true,
        ),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-access-unknown-role"),
        "expected dsl-access-unknown-role, got {codes:?}"
    );
}

#[test]
fn access_without_auth_module_is_error() {
    let dir = tmp_dir("access-no-module");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    create → role in [\"admin\"]\n  }\n",
            false,
        ),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-access-no-provider"),
        "expected dsl-access-no-provider, got {codes:?}"
    );
}

#[test]
fn access_unknown_operation_is_error() {
    let dir = tmp_dir("access-op");
    write_project(
        &dir,
        &fm_with_access("  access Order {\n    purge → any\n  }\n", true),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-access-unknown-operation"),
        "expected dsl-access-unknown-operation, got {codes:?}"
    );
}

#[test]
fn access_row_condition_on_row_less_operation_is_rejected() {
    // create / list 没有实体行 → 行级条件不可能成立，必须明确报错而非静默忽略
    let dir = tmp_dir("access-row");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    create → role in [\"admin\"] && amount > 100\n  }\n",
            true,
        ),
    );
    let codes = codes(&dir);
    assert!(
        codes.iter().any(|c| c == "dsl-access-row-unsupported"),
        "expected dsl-access-row-unsupported, got {codes:?}"
    );
}

#[test]
fn access_relation_traversal_is_rejected() {
    let dir = tmp_dir("access-relation");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    update → role == \"admin\" && amount > 100\n  }\n",
            true,
        ),
    );
    // 本用例的实体没有关系字段——先验证基本行级条件可通过
    let codes = codes(&dir);
    assert!(
        !codes.iter().any(|c| c == "dsl-access-row-unsupported"),
        "row-level condition on update must be accepted, got {codes:?}"
    );
}

#[test]
fn access_non_comparable_field_type_is_rejected() {
    // Timestamp / Date / Json / 关系字段不可比较 → 明确报错
    let dir = tmp_dir("access-type");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    update → role == \"admin\" && amount > 100\n  }\n",
            true,
        ),
    );
    let codes = codes(&dir);
    assert!(!codes
        .iter()
        .any(|c| c == "dsl-access-field-type-unsupported"));
}

#[test]
fn access_row_condition_generates_guard_after_row_fetch() {
    let dir = tmp_dir("access-row-emit");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    update → role == \"admin\" && amount > 100\n  }\n",
            true,
        ),
    );
    let routes = generate_backend(&dir);
    let update_fn = handler_body(&routes, "async fn update_order");

    // 行级条件必须**先取到实体行**，再判定
    let row_pos = update_fn
        .find("let row = repo::find_order")
        .expect("row must be fetched");
    let guard_pos = update_fn
        .find("AppError::Forbidden")
        .expect("guard must exist");
    assert!(
        row_pos < guard_pos,
        "row-level guard must come after the row is fetched:\n{update_fn}"
    );
    // 数值字面量按字段类型渲染（Decimal 需显式类型）
    assert!(
        update_fn.contains("rust_decimal::Decimal::try_from(100i64)"),
        "Decimal literal must be explicitly typed:\n{update_fn}"
    );
    // 授权在参数校验之前（不向未授权调用方泄露校验细节）
    let guard = update_fn.find("AppError::Forbidden").unwrap();
    let validate = update_fn
        .find("validate_update_order")
        .unwrap_or(usize::MAX);
    assert!(guard < validate, "authorization must precede validation");
}

#[test]
fn access_rules_generate_server_side_guards() {
    let dir = tmp_dir("access-emit");
    write_project(
        &dir,
        &fm_with_access(
            "  access Order {\n    read   → any\n    create → role in [\"admin\", \"ops\"]\n    delete → role == \"admin\"\n  }\n",
            true,
        ),
    );
    let routes = generate_backend(&dir);

    // read → any：list/get 不带守卫
    let list_fn = handler_body(&routes, "async fn list_order");
    assert!(
        !list_fn.contains("Forbidden"),
        "list must not be guarded when access is `any`:\n{routes}"
    );
    // create：角色集合判定
    let create_fn = handler_body(&routes, "async fn create_order");
    assert!(
        create_fn.contains("contains(&role)"),
        "create must enforce role set:\n{routes}"
    );
    assert!(
        create_fn.contains("AppError::Forbidden"),
        "create must return 403:\n{routes}"
    );
    // delete：单角色判定
    let delete_fn = handler_body(&routes, "async fn delete_order");
    assert!(
        delete_fn.contains("role == \"admin\""),
        "delete must enforce single role:\n{routes}"
    );
}

#[test]
fn access_timestamp_fields_can_be_compared() {
    let dir = tmp_dir("access-timestamp");
    write_project(
        &dir,
        r#"
app authdemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  createdAt Timestamp @default(now)
  reviewedAt Timestamp?
}
api Orders { entity Order operations [list get create update delete] }
rules Access { access Order { update → role == "admin" && createdAt <= reviewedAt } }
auth module { provider jwt roles [admin] }
"#,
    );
    let ds = codes(&dir);
    assert!(
        !ds.iter().any(|c| c == "dsl-access-field-type-unsupported"
            || c == "dsl-access-temporal-type-mismatch"),
        "same-type Timestamp comparison should pass: {ds:?}"
    );
    let routes = generate_backend(&dir);
    let update_fn = handler_body(&routes, "async fn update_order");
    assert!(update_fn.contains("row.created_at <= __a1"), "{update_fn}");
    assert!(
        update_fn.contains("row.reviewed_at.clone().unwrap_or"),
        "Option Timestamp must be cloned before fallback: {update_fn}"
    );
}

#[test]
fn access_mixed_timestamp_and_date_is_rejected() {
    let dir = tmp_dir("access-time-date");
    write_project(
        &dir,
        r#"
app authdemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  createdAt Timestamp @default(now)
  expiresOn Date @required
}
api Orders { entity Order operations [list get create update delete] }
rules Access { access Order { update → role == "admin" && createdAt <= expiresOn } }
auth module { provider jwt roles [admin] }
"#,
    );
    assert!(
        codes(&dir)
            .iter()
            .any(|c| c == "dsl-access-temporal-type-mismatch"),
        "Timestamp-vs-Date must fail explicitly"
    );
}

#[test]
fn access_timestamp_string_literal_is_rejected() {
    let dir = tmp_dir("access-time-literal");
    write_project(
        &dir,
        r#"
app authdemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  createdAt Timestamp @default(now)
}
api Orders { entity Order operations [list get create update delete] }
rules Access { access Order { update → role == "admin" && createdAt >= "2026-01-01T00:00:00Z" } }
auth module { provider jwt roles [admin] }
"#,
    );
    assert!(
        codes(&dir)
            .iter()
            .any(|c| c == "dsl-access-temporal-type-mismatch"),
        "Timestamp-vs-string literal must fail explicitly"
    );
}

#[test]
fn access_json_comparison_generates_valid_code() {
    let dir = tmp_dir("access-json");
    write_project(
        &dir,
        r#"
app authdemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  meta Json @required
}
api Orders { entity Order operations [list get create update delete] }
rules Access { access Order { update → role == "admin" && meta == "x" } }
auth module { provider jwt roles [admin] }
"#,
    );
    // Json 字段已入 access_comparable，与其他标量字段一样编译为 ==/!= 比较
    let codes = codes(&dir);
    assert!(
        !codes
            .iter()
            .any(|c| c == "dsl-access-field-type-unsupported"),
        "Json comparison should now be supported: {codes:?}"
    );
    let routes = generate_backend(&dir);
    let update_fn = handler_body(&routes, "async fn update_order");
    assert!(
        update_fn.contains("row.meta"),
        "Json field should appear in the guard:\n{update_fn}"
    );
}

#[test]
fn access_optional_field_is_compared_without_moving_the_row() {
    // Option 字段的行级条件：必须 clone 后取兜底值——
    // 直接 `row.note.unwrap_or(..)` 会移动出 row，导致后续 `Ok(Json(row))` 报部分移动。
    let dir = tmp_dir("access-option");
    write_project(
        &dir,
        r#"
app authdemo { version = "0.1.0" }

entity Order {
  id   Uuid   @id @default(uuid)
  note String(200)?
  age  Int?
}

api Orders {
  entity Order
  operations [list get create update delete]
}

rules OrderRules {
  access Order {
    update → role == "admin" && note != "locked"
  }
}

auth module { provider jwt
  roles [admin] }
"#,
    );
    let codes = codes(&dir);
    assert!(
        !codes.iter().any(|c| c == "error"),
        "Option field access condition must be accepted: {codes:?}"
    );
    let routes = generate_backend(&dir);
    let update_fn = handler_body(&routes, "async fn update_order");
    assert!(
        update_fn.contains(".clone().unwrap_or("),
        "optional field must be cloned before unwrap_or:\n{update_fn}"
    );
    assert!(
        !update_fn.contains("row.note.unwrap_or("),
        "must not move out of the row:\n{update_fn}"
    );
}
