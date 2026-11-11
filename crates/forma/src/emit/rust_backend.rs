//! backend-rust 适配层：Program → 可编译的 axum + sqlx 后端工程。

use super::{pick_message, snake, EntityDecl, FieldDecl, Program};
use crate::dsl::{pluralize, BinOp, Expr, FieldConstraint};
use crate::util::{sha256_hex, write_text};
use anyhow::Result;
use std::path::Path;

/// 生成后端的 Cargo 包名 / 二进制名——**唯一**命名来源。
/// Dockerfile 的 `cp target/release/<binary>` 必须与此处一致，否则
/// camelCase 项目名（如 `shopProd`）会因 to_snake_case 与 to_lowercase 不一致而构建失败。
pub fn backend_package_name(app_name: &str) -> String {
    format!("{}-backend", app_name.to_lowercase())
}

pub fn emit_rust_backend(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    app_name: &str,
) -> Result<Vec<crate::generators::GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    // 设计宪法 6.2：未知分页模式显式报错，不做静默降级。
    // `cursor` 已实现（P1-6），语义约束由 check_program 在任何 emitter 写盘前执行。
    for api in &program.apis {
        if let Some(list) = &api.list {
            match list.paginate.as_deref() {
                Some("offset") | None => {}
                Some("cursor") => {
                    let Some(entity) = super::find_entity(program, &api.entity) else {
                        anyhow::bail!("api {}.list cursor references unknown entity", api.name);
                    };
                    if list.sort.is_empty() {
                        anyhow::bail!(
                            "api {}.list: paginate cursor requires at least one non-null, comparable sort field; use `paginate offset` or add `sort [...]`",
                            api.entity
                        );
                    }
                    let mut seen = std::collections::HashSet::new();
                    for (field_name, _) in &list.sort {
                        if !seen.insert(field_name) {
                            anyhow::bail!(
                                "api {}.list: cursor sort field `{field_name}` is duplicated",
                                api.entity
                            );
                        }
                        let field = entity
                            .fields
                            .iter()
                            .find(|f| f.name == *field_name)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "api {}.list: unknown cursor sort field `{field_name}`",
                                    api.entity
                                )
                            })?;
                        if field.optional {
                            anyhow::bail!(
                                "api {}.list: cursor sort field `{field_name}` is nullable",
                                api.entity
                            );
                        }
                        let supported = matches!(
                            field.type_name.as_str(),
                            "Uuid"
                                | "String"
                                | "Text"
                                | "Email"
                                | "Int"
                                | "Float"
                                | "Decimal"
                                | "Timestamp"
                                | "Date"
                        ) || super::is_enum(program, &field.type_name);
                        if !supported {
                            anyhow::bail!(
                                "api {}.list: cursor sort field `{field_name}` has unsupported type `{}`",
                                api.entity, field.type_name
                            );
                        }
                    }
                }
                Some(mode) => anyhow::bail!(
                    "api {}.list: unsupported paginate mode `{mode}` (supported: offset | cursor)",
                    api.entity
                ),
            }
        }
    }
    let pkg = backend_package_name(app_name);
    let mut files: Vec<crate::generators::GeneratedFile> = Vec::new();

    // 命令类操作（spec §10）：有命令 → 生成输入模型/校验器/仓储/路由/审计钩子
    let has_commands = !program.commands.is_empty();
    let has_audit = program.commands.iter().any(|c| c.audit.is_some());

    files.push(file(
        out_dir,
        "Cargo.toml",
        cargo_toml(&pkg, program),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/request_id.rs",
        request_id_rs(),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/error.rs",
        error_rs(
            has_any_auth_rules(program)
                || crate::dsl::tenant_enabled(program)
                || !crate::dsl::field_policy_roles(program).is_empty(),
            program.auth.is_some(),
            has_conflict_errors(program),
        ),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/models.rs",
        format!("{}{}", models_rs(program), commands_rs(program)),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/validators.rs",
        format!(
            "{}{}",
            validators_rs(program),
            command_validators_rs(program)
        ),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/repo.rs",
        format!("{}{}", repo_rs(program), command_repo_rs(program)?),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/routes.rs",
        routes_rs_with_commands(program, has_commands)?,
        "gen-rust",
    ));
    if has_audit {
        files.push(file(out_dir, "src/audit.rs", audit_rs(), "gen-rust"));
    }
    if program.auth.is_some() {
        files.push(file(
            out_dir,
            "src/auth.rs",
            auth_rs(program, app_name),
            "gen-rust",
        ));
        files.push(file(out_dir, "src/secret.rs", secret_rs(), "gen-rust"));
    }
    let has_jobs = !program.jobs.is_empty();
    let has_notify = program.entities.iter().any(|e| !e.notify.is_empty());
    if has_jobs {
        files.push(file(out_dir, "src/jobs.rs", jobs_rs(program), "gen-rust"));
    }
    if has_notify {
        files.push(file(
            out_dir,
            "src/notify.rs",
            notify_rs(program),
            "gen-rust",
        ));
    }
    let has_live = !program.lives.is_empty();
    if has_live {
        files.push(file(out_dir, "src/live.rs", live_rs(program), "gen-rust"));
    }
    if has_storage(program) {
        files.push(file(
            out_dir,
            "src/storage.rs",
            storage_rs(program),
            "gen-rust",
        ));
    }
    if has_search(program) {
        files.push(file(
            out_dir,
            "src/search.rs",
            search_rs(program),
            "gen-rust",
        ));
    }
    if !program.exports.is_empty() {
        files.push(file(
            out_dir,
            "src/export.rs",
            export_rs(program),
            "gen-rust",
        ));
    }
    files.push(file(
        out_dir,
        "src/main.rs",
        main_rs(program, has_commands, has_audit),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        ".env.example",
        "# @generated by Forma (gen-rust) — do not edit manually.\nDATABASE_URL=mysql://root:root@localhost:3306/school\n".into(),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "README.md",
        format!(
            "<!-- @generated by Forma (gen-rust) — do not edit manually. Regenerate with: forma build -->\n\n# {app_name} backend (generated by Forma)\n\n```bash\n# 1. create schema\nmysql -u root -p < ../db/migrations/0001_init.sql\n# 2. run\nexport DATABASE_URL=mysql://root:root@localhost:3306/school\ncargo run\n```\nAPI base: http://localhost:8080/api\n"
        ),
        "gen-rust",
    ));

    // 写盘（内容不变则跳过）
    let mut written = Vec::new();
    for f in files {
        let abs = project_dir.join(&f.path);
        let hash = sha256_hex(&f.content);
        let unchanged = abs.is_file()
            && crate::util::read_text(&abs)
                .map(|c| sha256_hex(&c) == hash)
                .unwrap_or(false);
        if !unchanged {
            write_text(&abs, &f.content)?;
        }
        written.push(f);
    }
    Ok(written)
}

fn file(
    out_dir: &str,
    rel: &str,
    content: String,
    generator: &str,
) -> crate::generators::GeneratedFile {
    crate::generators::GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content,
        generator: generator.into(),
    }
}

fn cargo_toml(pkg: &str, program: &Program) -> String {
    let has_auth = program.auth.is_some();
    let has_live = !program.lives.is_empty();
    let has_storage = has_storage(program);
    // error.rs uses tracing::error! for internal failures (Runtime Contract E5), so tracing is always present.
    // tracing 恒存在（M10/P1）：error.rs / 审计 / 请求 id 统一走 tracing；
    // tracing-subscriber 带 env-filter + json features（RUST_LOG + LOG_FORMAT=json）
    let tracing_deps = "\ntracing = \"0.1\"\ntracing-subscriber = { version = \"0.3\", features = [\"env-filter\", \"json\"] }";
    let live_deps = if has_live {
        "\ntokio-stream = { version = \"0.1\", features = [\"sync\"] }"
    } else {
        ""
    };
    let auth_deps = if has_auth {
        "\nargon2 = \"0.5\"\njsonwebtoken = \"9\""
    } else {
        ""
    };
    // M9/P1：Attachment 字段存在时启用 axum multipart（POST /api/uploads）
    let axum_dep = if has_storage {
        "axum = { version = \"0.7\", features = [\"multipart\"] }"
    } else {
        "axum = \"0.7\""
    };
    // ── 条件依赖（仅当对应能力声明存在时引入——零新增依赖原则；BTreeSet 去重）──
    let has_xlsx = program.exports.iter().any(|e| e.format == "xlsx");
    let has_webhook = program
        .entities
        .iter()
        .any(|e| e.notify.iter().any(|n| n.channel == "webhook"));
    let has_sms = program
        .entities
        .iter()
        .any(|e| e.notify.iter().any(|n| n.channel == "sms"));
    let has_email = program
        .entities
        .iter()
        .any(|e| e.notify.iter().any(|n| n.channel == "email"));
    let storage_providers: Vec<&str> = program
        .entities
        .iter()
        .flat_map(|e| e.fields.iter())
        .filter(|f| f.type_name == "Attachment")
        .filter_map(|f| f.attrs.iter().find(|a| a.name == "storage"))
        .filter_map(|a| a.args.first().map(|s| s.as_str()))
        .collect();
    let has_s3 = storage_providers.contains(&"s3");
    let has_oss = storage_providers.contains(&"oss");
    let has_search = program.entities.iter().any(|e| !e.searchable.is_empty());

    let mut extra: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if has_xlsx {
        extra.insert("rust_xlsxwriter = \"0.79\"".into());
    }
    if has_webhook || has_oss || has_sms || has_search {
        extra.insert(
            "reqwest = { version = \"0.12\", default-features = false, features = [\"json\", \"rustls-tls\"] }"
                .into(),
        );
    }
    if has_email {
        extra.insert(
            "lettre = { version = \"0.11\", default-features = false, features = [\"smtp-transport\", \"rustls-tls\", \"builder\"] }"
                .into(),
        );
    }
    if has_s3 {
        extra.insert("aws-config = \"1\"".into());
        extra.insert("aws-sdk-s3 = \"1\"".into());
    }
    if has_oss {
        extra.insert("hmac = \"0.12\"".into());
        extra.insert("sha1 = \"0.10\"".into());
        extra.insert("base64 = \"0.22\"".into());
    }
    let extra_deps = extra.iter().map(|d| format!("\n{d}")).collect::<String>();
    format!(
        r#"# @generated by Forma (gen-rust) — do not edit manually.
[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[features]
default = []
# OTLP trace 导出（opt-in：cargo run --features otlp）；默认不编译、不引入运行时依赖
otlp = ["dep:opentelemetry", "dep:opentelemetry_sdk", "dep:opentelemetry-otlp", "dep:tracing-opentelemetry"]

[dependencies]
{axum_dep}
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
sqlx = {{ version = "0.8", features = ["runtime-tokio", "mysql", "uuid", "chrono", "rust_decimal"] }}
uuid = {{ version = "1", features = ["v4", "serde"] }}{tracing_deps}{auth_deps}{live_deps}{extra_deps}
chrono = {{ version = "0.4", features = ["serde"] }}
rust_decimal = {{ version = "1", features = ["serde"] }}
tower-http = {{ version = "0.5", features = ["cors", "timeout"] }}
opentelemetry = {{ version = "0.28", optional = true }}
opentelemetry_sdk = {{ version = "0.28", features = ["rt-tokio"], optional = true }}
opentelemetry-otlp = {{ version = "0.28", features = ["grpc-tonic"], optional = true }}
tracing-opentelemetry = {{ version = "0.29", optional = true }}

# 独立于 Forma 工作区编译
[workspace]
"#
    )
}

/// 工程存在 Attachment 字段（M9/P1 文件上传）
fn has_storage(program: &Program) -> bool {
    program
        .entities
        .iter()
        .any(|e| e.fields.iter().any(|f| f.type_name == "Attachment"))
}

/// 工程存在 `searchable` 声明的实体（外部搜索引擎适配层）。
fn has_search(program: &Program) -> bool {
    program.entities.iter().any(|e| !e.searchable.is_empty())
}

/// 搜索引擎写路径钩子：索引文档（仅当实体有 searchable 字段时生成）。
fn search_index_hook(program: &Program, entity: &str, input_var: &str, id_expr: &str) -> String {
    let Some(decl) = program.entities.iter().find(|e| e.name == entity) else {
        return String::new();
    };
    if decl.searchable.is_empty() {
        return String::new();
    }
    let table = crate::identity::to_snake_case(&crate::dsl::pluralize(entity));
    let mut obj = format!("\"id\": {id_expr}");
    for f in &decl.searchable {
        let Some(field) = decl.fields.iter().find(|x| &x.name == f) else {
            continue;
        };
        // @id / @readonly 不在输入体 → 不索引
        if crate::dsl::has_attr(field, "readonly") || crate::dsl::has_attr(field, "id") {
            continue;
        }
        let col = crate::identity::to_snake_case(&field.name);
        obj.push_str(&format!(", \"{col}\": {input_var}.{col}"));
    }
    format!(
        "    crate::search::index_document(\"{table}\", &{id_expr}, serde_json::json!({{ {obj} }})).await;\n"
    )
}

/// 搜索引擎写路径钩子：删文档（仅当实体有 searchable 字段时生成）。
fn search_delete_hook(program: &Program, entity: &str) -> String {
    let Some(decl) = program.entities.iter().find(|e| e.name == entity) else {
        return String::new();
    };
    if decl.searchable.is_empty() {
        return String::new();
    }
    let table = crate::identity::to_snake_case(&crate::dsl::pluralize(entity));
    format!("    crate::search::delete_document(\"{table}\", &id).await;\n")
}

fn has_conflict_errors(program: &Program) -> bool {
    program.auth.is_some()
        || program.commands.iter().any(|c| c.pre.is_some())
        || program
            .rules
            .iter()
            .flat_map(|r| &r.guards)
            .any(|g| !g.denies.is_empty())
}

/// 编译行级访问守卫（A-2：任意深度关系遍历）。
///
/// 对每个跨关系路径生成逐跳 `repo::find_{E}` 取数；**任一行缺失即 403**（fail-closed，
/// 不用兜底值把条件判成允许）；最终字段 clone 后绑定到 `__relN` 变量供表达式引用。
fn compile_access_guard(
    program: &Program,
    entity: &EntityDecl,
    cond: &Expr,
    tenant_arg: &str,
) -> String {
    let mut pre = String::new();
    let mut bindings = String::new();
    let mut path_bindings: Vec<(Vec<String>, String)> = Vec::new();
    let mut idx = 0usize;
    let mut paths: Vec<Vec<String>> = Vec::new();
    for p in crate::dsl::expr_refs(cond) {
        if p.len() >= 2 && !paths.contains(&p) {
            paths.push(p);
        }
    }
    for path in paths {
        let mut current: &EntityDecl = entity;
        let mut row_var = "row".to_string();
        let mut ok = true;
        for seg in &path[..path.len() - 1] {
            let Some(field) = current.fields.iter().find(|f| f.name == *seg) else {
                ok = false;
                break;
            };
            let Some(target) = program.entities.iter().find(|e| e.name == field.type_name) else {
                ok = false;
                break;
            };
            idx += 1;
            let next = format!("__row{idx}");
            let fk = format!("{}_id", snake(seg));
            let repo_fn = format!("find_{}", snake(&target.name));
            pre.push_str(&format!(
                "    let {next} = repo::{repo_fn}(&db, &{row_var}.{fk}, {tenant_arg}).await?.ok_or(AppError::Forbidden(\"access relation row missing\".to_string()))?;\n"
            ));
            row_var = next;
            current = target;
        }
        if !ok {
            continue;
        }
        let last = path.last().expect("non-empty path");
        let Some(field) = current.fields.iter().find(|f| f.name == *last) else {
            continue;
        };
        let accessor = format!("{row_var}.{}", snake(&field.name));
        idx += 1;
        let var = format!("__rel{idx}");
        if field.optional {
            let fallback = typed_fallback(program, &field.type_name);
            pre.push_str(&format!(
                "    let {var} = {accessor}.clone().unwrap_or({fallback});\n"
            ));
        } else {
            pre.push_str(&format!("    let {var} = {accessor}.clone();\n"));
        }
        path_bindings.push((path, var));
    }
    let vars: &[(&str, &str)] = &[("role", "role")];
    let expr = expr_to_rust_access(
        cond,
        program,
        entity,
        "row",
        &path_bindings,
        vars,
        &mut bindings,
        &mut idx,
    );
    format!(
        "{pre}{bindings}    let role = user.role.clone();\n    if !({expr}) {{ return Err(AppError::Forbidden(\"operation not permitted for this role\".to_string())); }}\n"
    )
}

struct OpGuard {
    ext_mid: String,
    ext_last: String,
    pre: String,
    after_row: String,
}

impl OpGuard {
    fn empty() -> Self {
        OpGuard {
            ext_mid: String::new(),
            ext_last: String::new(),
            pre: String::new(),
            after_row: String::new(),
        }
    }
}

fn op_guard_bundle(
    program: &Program,
    entity: &EntityDecl,
    api_roles: &std::collections::HashMap<String, Vec<String>>,
    access: &std::collections::HashMap<String, Expr>,
    op: &str,
    tenant_arg: &str,
    force_ext: bool,
) -> OpGuard {
    let mut pre = String::new();
    let mut after_row = String::new();
    if let Some(roles) = api_roles.get(op) {
        if !roles.is_empty() {
            let list = roles
                .iter()
                .map(|r| format!("\"{}\"", escape_rust(r)))
                .collect::<Vec<_>>()
                .join(", ");
            pre.push_str(&format!(
                "    crate::auth::require_role(&[{list}], &user)?;\n"
            ));
        }
    }
    if let Some(cond) = access.get(op) {
        let is_any = matches!(cond, Expr::Ref { path, .. } if path.len() == 1 && path[0] == "any");
        if !is_any {
            let code = compile_access_guard(program, entity, cond, tenant_arg);
            if access_needs_row(cond) {
                after_row.push_str(&code);
            } else {
                pre.push_str(&code);
            }
        }
    }
    // 租户维度（M10 收尾）：force_ext（@tenant 实体全操作）也要注入 Extension<CurrentUser>
    let needs_ext =
        !pre.is_empty() || !after_row.is_empty() || (force_ext && program.auth.is_some());
    if !needs_ext {
        return OpGuard::empty();
    }
    OpGuard {
        ext_mid: "Extension(user): Extension<CurrentUser>, ".to_string(),
        ext_last: ", Extension(user): Extension<CurrentUser>".to_string(),
        pre,
        after_row,
    }
}

/// A-2：行级访问规则跨关系遍历涉及的目标实体——这些实体即使没有
/// get/update/delete API，也必须生成 `find_{E}` 仓储函数供守卫逐跳取数。
fn access_relation_targets(program: &Program) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for rules in &program.rules {
        for access in &rules.access {
            let Some(start) = program.entities.iter().find(|e| e.name == access.entity) else {
                continue;
            };
            for rule in &access.rules {
                for path in crate::dsl::expr_refs(&rule.expr) {
                    if path.len() < 2 {
                        continue;
                    }
                    // 逐跳收集目标实体名（最后一段是标量字段，不产生实体跳）
                    let mut current = start;
                    for seg in &path[..path.len() - 1] {
                        let Some(field) = current.fields.iter().find(|f| f.name == *seg) else {
                            break;
                        };
                        let Some(target) =
                            program.entities.iter().find(|e| e.name == field.type_name)
                        else {
                            break;
                        };
                        out.insert(target.name.clone());
                        current = target;
                    }
                }
            }
        }
    }
    out
}

fn error_rs(has_forbidden: bool, has_auth: bool, has_conflict: bool) -> String {
    let base = r#"// @generated by Forma (gen-rust) — do not edit manually.
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

// 当前请求的追踪 id（§4.4）——request_id.rs 中间件写入，错误体读取。
tokio::task_local! {
    pub static REQUEST_ID: String;
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    /// 面向用户的文案（Runtime Contract E2：来自规格 message / i18n）
    error: String,
    /// 稳定错误码（Runtime Contract E1：Agent 可据此分支处理）
    code: String,
    /// 字段级错误（仅校验失败）
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<Vec<FieldError>>,
    /// 请求追踪 id（Runtime Contract 预留，当前不生成）
    #[serde(skip_serializing_if = "Option::is_none", rename = "requestId")]
    request_id: Option<String>,
}

/// 稳定错误码（Runtime Contract · RFC-0005 E1）。
///
/// 这些字符串是**公开 API**——Agent 与前端可据此分支处理，改动视为破坏性变更。
pub mod error_code {
    pub const VALIDATION_FAILED: &str = "FORMA_VALIDATION_FAILED";
    pub const NOT_FOUND: &str = "FORMA_NOT_FOUND";
    pub const CONFLICT: &str = "FORMA_CONFLICT";
    pub const UNAUTHORIZED: &str = "FORMA_UNAUTHORIZED";
    pub const FORBIDDEN: &str = "FORMA_FORBIDDEN";
    pub const INTERNAL: &str = "FORMA_INTERNAL";
    /// 认证端点限速触发（RFC-0005 E1 新增稳定码；429 + Retry-After）
    pub const RATE_LIMITED: &str = "FORMA_RATE_LIMITED";
}

/// 错误码 → HTTP 状态码（Runtime Contract · RFC-0005 E3：映射只在此处声明）。
pub fn status_for(code: &str) -> StatusCode {
    match code {
        error_code::VALIDATION_FAILED => StatusCode::BAD_REQUEST,
        error_code::UNAUTHORIZED => StatusCode::UNAUTHORIZED,
        error_code::FORBIDDEN => StatusCode::FORBIDDEN,
        error_code::NOT_FOUND => StatusCode::NOT_FOUND,
        error_code::CONFLICT => StatusCode::CONFLICT,
        error_code::RATE_LIMITED => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub enum AppError {
    Validation(Vec<FieldError>),
    NotFound,
    //CONFLICT_VARIANT
    //AUTH_VARIANT
    //FORBIDDEN_VARIANT
    /// 认证失败限速（429）：客户端应在 Retry-After 秒后重试
    RateLimited(u64),
    Db(sqlx::Error),
    /// 序列化失败（P0-2 字段裁剪；细节只写日志）
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (code, error, fields) = match self {
            AppError::Validation(fields) => (
                error_code::VALIDATION_FAILED,
                "validation failed".to_string(),
                Some(fields),
            ),
            AppError::NotFound => (error_code::NOT_FOUND, "not found".to_string(), None),
            //CONFLICT_MATCH
            //AUTH_MATCH
            //EXPORT_MATCH
            //FORBIDDEN_MATCH
            AppError::Internal(msg) => {
                tracing::error!(error = %msg, "internal error");
                (error_code::INTERNAL, "internal server error".to_string(), None)
            }
            AppError::RateLimited(retry_after) => {
                tracing::warn!(retry_after_secs = retry_after, "auth rate limited");
                (error_code::RATE_LIMITED, "too many attempts".to_string(), None)
            }
            AppError::Db(e) => {
                if matches!(e, sqlx::Error::RowNotFound) {
                    (error_code::NOT_FOUND, "not found".to_string(), None)
                } else if let sqlx::Error::Database(db) = &e {
                    if db.message().contains("Duplicate entry") {
                        (error_code::CONFLICT, "duplicate entry".to_string(), None)
                    } else {
                        // E5：不把内部细节（SQL / 内部路径）回显给调用方，只写日志
                        tracing::error!(error = %e, "internal error");
                        (error_code::INTERNAL, "internal server error".to_string(), None)
                    }
                } else {
                    tracing::error!(error = %e, "internal error");
                    (error_code::INTERNAL, "internal server error".to_string(), None)
                }
            }
        };
        let status = status_for(code);
        let body = ErrorBody {
            error,
            code: code.to_string(),
            fields,
            // §4.4：task-local 里的请求 id（中间件挂最外层，总有值）
            request_id: crate::request_id::REQUEST_ID.try_with(|id| id.clone()).ok(),
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Db(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        // 字段级策略（P0-2）：裁剪路径的 ? 需要
        AppError::Internal(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
"#
    .to_string();
    const VARIANT_MARK: &str = "    //FORBIDDEN_VARIANT\n";
    const MATCH_MARK: &str = "            //FORBIDDEN_MATCH\n";
    const CONFLICT_VARIANT: &str = "    //CONFLICT_VARIANT\n";
    const CONFLICT_MATCH: &str = "            //CONFLICT_MATCH\n";
    const AUTH_VARIANT: &str = "    //AUTH_VARIANT\n";
    const AUTH_MATCH: &str = "            //AUTH_MATCH\n";
    const EXPORT_VARIANT: &str = "    //EXPORT_VARIANT\n";
    const EXPORT_MATCH: &str = "            //EXPORT_MATCH\n";
    let base = if has_forbidden {
        base.replace(
            VARIANT_MARK,
            "    /// 授权失败（403）：身份有效但角色无权执行该操作\n    Forbidden(String),\n",
        )
        .replace(
            MATCH_MARK,
            "            AppError::Forbidden(msg) => (error_code::FORBIDDEN, msg, None),\n",
        )
    } else {
        base.replace(VARIANT_MARK, "").replace(MATCH_MARK, "")
    };
    let base = if has_conflict {
        base.replace(CONFLICT_VARIANT, "    Conflict(String),\n")
            .replace(
                CONFLICT_MATCH,
                "            AppError::Conflict(msg) => (error_code::CONFLICT, msg, None),\n",
            )
    } else {
        base.replace(CONFLICT_VARIANT, "")
            .replace(CONFLICT_MATCH, "")
    };
    let base = if has_auth {
        base.replace(
            AUTH_VARIANT,
            "    /// 认证失败（401）：缺失/无效 token 或错误凭据\n    Auth(String),\n",
        )
        .replace(
            AUTH_MATCH,
            "            AppError::Auth(msg) => (error_code::UNAUTHORIZED, msg, None),\n",
        )
    } else {
        base.replace(AUTH_VARIANT, "").replace(AUTH_MATCH, "")
    };
    base.replace(EXPORT_VARIANT, "").replace(EXPORT_MATCH, "")
}

/// 是否有 api 声明了操作级授权（决定是否生成 `Forbidden` 错误与 `require_role` 守卫）。
/// 没有授权规则时**不生成**——否则生成工程会出现「never used」警告。
fn has_api_auth_rules(program: &Program) -> bool {
    program.apis.iter().any(|a| !a.auth.is_empty())
}

/// 是否有实体声明了访问规则（`rules { access ... }`）。
fn has_access_rules(program: &Program) -> bool {
    program.rules.iter().any(|r| {
        r.access.iter().any(|a| {
            a.rules.iter().any(|rule| {
                !matches!(&rule.expr, Expr::Ref { path, .. } if path.len() == 1 && path[0] == "any")
            })
        })
    })
}

/// 任一授权规则存在（api 级或实体级）→ 需要 `Forbidden` 错误变体。
fn has_any_auth_rules(program: &Program) -> bool {
    has_api_auth_rules(program) || has_access_rules(program)
}

/// 实体级访问规则：`op → 条件表达式`（`read` 同时覆盖 list / get）。
fn access_conditions(
    program: &Program,
    entity_name: &str,
) -> std::collections::HashMap<String, Expr> {
    let mut map: std::collections::HashMap<String, Expr> = std::collections::HashMap::new();
    for rules in &program.rules {
        for access in &rules.access {
            if access.entity != entity_name {
                continue;
            }
            for rule in &access.rules {
                match rule.op.as_str() {
                    "read" => {
                        map.entry("list".to_string())
                            .or_insert_with(|| rule.expr.clone());
                        map.entry("get".to_string())
                            .or_insert_with(|| rule.expr.clone());
                    }
                    op => {
                        map.insert(op.to_string(), rule.expr.clone());
                    }
                }
            }
        }
    }
    map
}

/// Decimal 字面量——必须显式类型，否则 `Decimal > 5` 的整数字面量类型不明确。
fn decimal_literal(v: f64) -> String {
    if v.fract() == 0.0 {
        format!(
            "rust_decimal::Decimal::try_from({}i64).unwrap_or_default()",
            v as i64
        )
    } else {
        format!("rust_decimal::Decimal::try_from({v}f64).unwrap_or_default()")
    }
}

/// 访问条件中操作数的类型（用于数字字面量类型化；A-2 支持任意深度路径）。
fn access_operand_type(program: &Program, entity: &EntityDecl, expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ref { path, .. } => {
            let field = crate::dsl::resolve_access_field(program, entity, path).ok()?;
            if super::is_enum(program, &field.type_name) {
                // 枚举在模型侧落为 String
                Some("String".to_string())
            } else {
                Some(field.type_name.clone())
            }
        }
        Expr::Bin(_, a, _) => access_operand_type(program, entity, a),
        _ => None,
    }
}

/// 访问条件 → Rust 表达式（**字段类型感知 + Option 安全**）。
///
/// 与通用表达式编译器的差异：
/// - `role` 绑定为当前用户角色（String）；
/// - 跨关系路径（`a.b.c`）由 `path_bindings` 提供已取数的变量（A-2：任意深度）；
/// - Option 字段**克隆后**取兜底值——既不移动出 `row`（否则 `Ok(Json(row))` 报部分移动），
///   也不移动出引用；
/// - 数值字面量按对侧字段类型渲染（Decimal 必须显式类型）。
#[allow(clippy::too_many_arguments)]
fn expr_to_rust_access(
    expr: &Expr,
    program: &Program,
    entity: &EntityDecl,
    row: &str,
    path_bindings: &[(Vec<String>, String)],
    vars: &[(&str, &str)],
    bindings: &mut String,
    idx: &mut usize,
) -> String {
    match expr {
        Expr::Ref { path, .. } => {
            // 绑定变量（role 等）优先
            if path.len() == 1 {
                if let Some((_, code)) = vars.iter().find(|(n, _)| *n == path[0]) {
                    return code.to_string();
                }
            }
            // 跨关系：由编译器在守卫前置代码里完成取数，这里直接引用绑定变量
            if path.len() >= 2 {
                if let Some((_, var)) = path_bindings.iter().find(|(p, _)| p == path) {
                    return var.clone();
                }
                // 检查层保证绑定存在；缺绑定时 fail-closed 返回默认（不生成不可判定表达式）
                return "Default::default()".to_string();
            }
            let Some(field) = entity.fields.iter().find(|f| f.name == path[0]) else {
                return "Default::default()".to_string();
            };
            let col = snake(&field.name);
            let accessor = format!("{row}.{col}");
            if !field.optional {
                return accessor;
            }
            *idx += 1;
            let var = format!("__a{idx}");
            let fallback = typed_fallback(program, &field.type_name);
            bindings.push_str(&format!(
                "    let {var} = {accessor}.clone().unwrap_or({fallback});\n"
            ));
            var
        }
        Expr::Num(v) => {
            if v.fract() == 0.0 {
                format!("{}", *v as i64)
            } else {
                format!("{v}")
            }
        }
        Expr::Str(s) => format!("\"{}\".to_string()", escape_rust(s)),
        Expr::Bool(b) => format!("{b}"),
        Expr::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|i| {
                    expr_to_rust_access(i, program, entity, row, path_bindings, vars, bindings, idx)
                })
                .collect();
            format!("vec![{}]", parts.join(", "))
        }
        Expr::Not(inner) => format!(
            "!({})",
            expr_to_rust_access(
                inner,
                program,
                entity,
                row,
                path_bindings,
                vars,
                bindings,
                idx
            )
        ),
        Expr::Call(_, _) => "true".to_string(),
        Expr::Null => "Default::default()".to_string(),
        Expr::Bin(op, a, b) => {
            let l =
                expr_to_rust_access(a, program, entity, row, path_bindings, vars, bindings, idx);
            let l_type = access_operand_type(program, entity, a);
            let r = match &**b {
                Expr::Num(v) if l_type.as_deref() == Some("Decimal") => decimal_literal(*v),
                other => expr_to_rust_access(
                    other,
                    program,
                    entity,
                    row,
                    path_bindings,
                    vars,
                    bindings,
                    idx,
                ),
            };
            let r_type = access_operand_type(program, entity, b);
            let l_final = match &**a {
                Expr::Num(v) if r_type.as_deref() == Some("Decimal") => decimal_literal(*v),
                _ => l,
            };
            match op {
                BinOp::In => format!("{r}.contains(&{l_final})"),
                BinOp::Eq => format!("({l_final} == {r})"),
                BinOp::Ne => format!("({l_final} != {r})"),
                BinOp::Lt => format!("({l_final} < {r})"),
                BinOp::Le => format!("({l_final} <= {r})"),
                BinOp::Gt => format!("({l_final} > {r})"),
                BinOp::Ge => format!("({l_final} >= {r})"),
                BinOp::And => format!("({l_final} && {r})"),
                BinOp::Or => format!("({l_final} || {r})"),
                BinOp::Add => format!("({l_final} + {r})"),
                BinOp::Sub => format!("({l_final} - {r})"),
                BinOp::Mul => format!("({l_final} * {r})"),
                BinOp::Div => format!("({l_final} / {r})"),
            }
        }
    }
}

/// 按字段类型给出带类型注解的兜底表达式（避免 Default 推断失败）。
fn typed_fallback(program: &Program, type_name: &str) -> String {
    if super::is_enum(program, type_name) {
        let first = program
            .enums
            .iter()
            .find(|e| e.name == type_name)
            .and_then(|e| e.variants.first())
            .map(|v| v.name.clone())
            .unwrap_or_default();
        return format!("\"{}\".to_string()", first);
    }
    match type_name {
        "Timestamp" => "chrono::Utc::now()".to_string(),
        "Date" => "chrono::Utc::now().date_naive()".to_string(),
        "Decimal" => "rust_decimal::Decimal::ZERO".to_string(),
        "Int" => "0i64".to_string(),
        "Float" => "0.0".to_string(),
        "Bool" => "false".to_string(),
        "Uuid" => "Uuid::nil()".to_string(),
        _ => "String::new()".to_string(),
    }
}

fn escape_rust(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// ── repo.rs ─────────────────────────────────────────────

fn table_of(entity: &EntityDecl) -> String {
    crate::identity::to_snake_case(&pluralize(&entity.name))
}

fn cursor_fields<'a>(program: &'a Program, api: &crate::dsl::ApiDecl) -> Vec<&'a FieldDecl> {
    let Some(list) = api.list.as_ref() else {
        return Vec::new();
    };
    if list.paginate.as_deref() != Some("cursor") {
        return Vec::new();
    }
    let Some(entity) = super::find_entity(program, &api.entity) else {
        return Vec::new();
    };
    list.sort
        .iter()
        .filter_map(|(name, _)| entity.fields.iter().find(|f| f.name == *name))
        .collect()
}

/// Cursor 值在 handler / repository 中的具体 Rust 类型（全限定名，避免导入漂移）。
fn cursor_rust_type(program: &Program, field: &FieldDecl) -> String {
    if super::is_enum(program, &field.type_name) {
        return "String".to_string();
    }
    match field.type_name.as_str() {
        "Uuid" | "String" | "Text" | "Email" => "String".to_string(),
        "Int" => "i64".to_string(),
        "Float" => "f64".to_string(),
        "Decimal" => "rust_decimal::Decimal".to_string(),
        "Timestamp" => "chrono::DateTime<chrono::Utc>".to_string(),
        "Date" => "chrono::NaiveDate".to_string(),
        _other => rust_type(program, field)
            .replace("Decimal", "rust_decimal::Decimal")
            .replace("DateTime<Utc>", "chrono::DateTime<chrono::Utc>")
            .replace("NaiveDate", "chrono::NaiveDate"),
    }
}

fn cursor_value_expr(field: &FieldDecl) -> String {
    let col = snake(&field.name);
    match field.type_name.as_str() {
        "Timestamp" => format!("last.{col}.to_rfc3339()"),
        _ => format!("last.{col}.to_string()"),
    }
}

/// Keyset lexicographic seek expression and binding sequence.
/// Each sort key uses its own direction; `id` is appended in the final key direction.
fn cursor_seek(sort: &[(String, bool)]) -> (String, Vec<usize>) {
    let cols: Vec<String> = sort
        .iter()
        .map(|(f, _)| format!("s.{}", snake(f)))
        .collect();
    let mut terms = Vec::new();
    let mut binds = Vec::new();
    for i in 0..cols.len() {
        let mut pieces = Vec::new();
        for (j, col) in cols.iter().enumerate().take(i) {
            pieces.push(format!("{col} = ?"));
            binds.push(j);
        }
        let op = if sort[i].1 { "<" } else { ">" };
        pieces.push(format!("{} {op} ?", cols[i]));
        binds.push(i);
        terms.push(format!("({})", pieces.join(" AND ")));
    }
    let mut tie = Vec::new();
    for (i, col) in cols.iter().enumerate() {
        tie.push(format!("{col} = ?"));
        binds.push(i);
    }
    let tie_op = if sort.last().map(|(_, d)| *d).unwrap_or(false) {
        "<"
    } else {
        ">"
    };
    tie.push(format!("s.id {tie_op} ?"));
    binds.push(usize::MAX); // cursor id
    terms.push(format!("({})", tie.join(" AND ")));
    (format!("({})", terms.join(" OR ")), binds)
}

fn repo_rs(program: &Program) -> String {
    let mut out = String::from("// @generated by Forma (gen-rust) — do not edit manually.\n");
    // 导入按需（P2-4）：entity-only（无 api）→ repo 无任何函数 → 零 import
    let any_repo_api = program.apis.iter().any(|a| !a.operations.is_empty());
    let any_create_op = program
        .apis
        .iter()
        .any(|a| a.operations.iter().any(|o| o == "create"));
    if !any_repo_api {
        return out;
    }
    let any_list_op = program
        .apis
        .iter()
        .any(|a| a.operations.iter().any(|o| o == "list"));
    let mut imports = String::from("use crate::models::*;\nuse sqlx::MySqlPool;\n");
    if any_list_op {
        imports.insert_str(0, "use serde::Deserialize;\n");
    }
    out.push_str(&imports);
    if any_create_op {
        out.push_str("use uuid::Uuid;\n");
    }
    out.push('\n');
    if program
        .apis
        .iter()
        .any(|a| a.list.as_ref().and_then(|l| l.paginate.as_deref()) == Some("cursor"))
    {
        out.push_str("#[derive(serde::Deserialize)]\npub struct CursorPayload { pub values: Vec<String>, pub id: String }\n\n");
    }

    for entity in &program.entities {
        let name = &entity.name;
        let table = table_of(entity);
        let snake_name = snake(&entity.name);
        let s = "s"; // 主别名
        let writable: Vec<&FieldDecl> = entity
            .fields
            .iter()
            .filter(|f| !super::has_attr(f, "id") && !super::has_attr(f, "readonly"))
            .collect();
        // 只生成 api 声明的 op 对应的仓储函数——否则生成后端出现 dead_code 警告
        let api = program.apis.iter().find(|a| a.entity == *name);
        let ops = api.map(|a| a.operations.clone()).unwrap_or_default();
        let has_op = |op: &str| ops.iter().any(|o| o == op);
        let list_cfg = api.and_then(|a| a.list.clone());

        // JOIN 列
        let mut select_cols = String::new();
        let mut joins = String::new();
        let mut alias_n = 0usize;
        for field in &entity.fields {
            let col = snake(&field.name);
            if super::is_relation(program, &field.type_name) {
                select_cols.push_str(&format!("        {s}.{col}_id,\n"));
                alias_n += 1;
                let alias = format!("t{alias_n}");
                // 表名必须与 gen-mysql 的 plan_table 完全一致（snake_case 复数）：
                // 否则生成 `LEFT JOIN Schools` → MySQL（Linux）表名大小写敏感 → 1064
                let ref_table = crate::identity::to_snake_case(&pluralize(&field.type_name));
                let display = display_column(program, &field.type_name);
                joins.push_str(&format!(
                    "        LEFT JOIN {ref_table} AS {alias} ON {s}.{col}_id = {alias}.id\n"
                ));
                select_cols.push_str(&format!("        {alias}.{display} AS {col}_name,\n"));
            } else {
                select_cols.push_str(&format!("        {s}.{col},\n"));
            }
        }
        // 末尾是 "col,\n" → 先去空白再去尾逗号（否则 `s.created_at,\nFROM` → 1064），
        // 再补回换行（否则 `s.created_at        FROM` 仍是非法 SQL）
        let select_cols = format!("{}\n", select_cols.trim_end().trim_end_matches(','));

        // WHERE 条件（list cfg）
        let mut wheres = String::new();
        let mut search_where = String::new();
        let mut param_fields = String::new();
        let binds = String::new();
        if let Some(list) = &program
            .apis
            .iter()
            .find(|a| a.entity == *name)
            .and_then(|a| a.list.clone())
        {
            for (field, contains) in &list.filters {
                let is_relation = entity
                    .fields
                    .iter()
                    .find(|f| f.name == *field)
                    .map(|f| super::is_relation(program, &f.type_name))
                    .unwrap_or(false);
                let col = if is_relation {
                    format!("{s}.{}_id", snake(field))
                } else {
                    format!("{s}.{}", snake(field))
                };
                let ptype = if is_relation {
                    "Option<String>".to_string()
                } else {
                    let f = entity.fields.iter().find(|f| f.name == *field);
                    match f.map(|f| rust_type(program, f)).as_deref() {
                        Some("Option<i64>") | Some("i64") => "Option<i64>".to_string(),
                        Some(t) if t.starts_with("Option<") => t.to_string(),
                        Some(t) => format!("Option<{t}>"),
                        None => "Option<String>".to_string(),
                    }
                };
                let pname = snake(field);
                param_fields.push_str(&format!("    pub {pname}: {ptype},\n"));
                // 运行时 sqlx::query() 只支持位置占位符 `?`（命名参数仅 query! 宏支持）
                if *contains {
                    wheres.push_str(&format!(
                        "        (? IS NULL OR {col} LIKE CONCAT('%', ?, '%')) AND\n"
                    ));
                } else {
                    wheres.push_str(&format!("        (? IS NULL OR {col} = ?) AND\n"));
                }
            }
            // 搜索：多字段 OR 语义；COALESCE 处理 NULL 列
            // （`FALSE OR NULL` = NULL → 整行被三值逻辑过滤掉）
            let mut search_conds: Vec<String> = Vec::new();
            for field in &list.search {
                let col = format!("{s}.{}", snake(field));
                search_conds.push(format!("COALESCE({col} LIKE CONCAT('%', ?, '%'), 0) = 1"));
            }
            if !search_conds.is_empty() {
                search_where = format!("(? IS NULL OR ({}))", search_conds.join(" OR "));
            }
            let _ = &binds;
        }
        // 租户维度（M10 收尾）：实体含 @tenant 字段 → 查询强制限当前租户。
        // tenant=None → `s.{col} = NULL` 恒不命中 → 空集（安全且确定，无绕过路径）。
        let tenant_col = crate::dsl::tenant_field_of(program, &entity.name).map(|f| snake(&f));
        // 无租户列时形参改名 `_tenant`，避免生成工程 unused_variables 警告
        let tenant_param: &str = if tenant_col.is_some() {
            "tenant"
        } else {
            "_tenant"
        };
        if let Some(col) = &tenant_col {
            wheres.push_str(&format!("        (s.{col} = ?) AND\n"));
        }
        // 去掉末尾 " AND\n"；core = filters + tenant；search 单独（外部引擎开启时替换 LIKE）
        let core_where = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.trim_end_matches("AND\n").trim_end())
        };
        let where_clause_no_search = core_where.clone();
        let where_clause = if search_where.is_empty() {
            core_where
        } else if core_where.is_empty() {
            format!("WHERE {search_where}")
        } else {
            format!("{core_where} AND {search_where}")
        };
        let _ = binds;

        // 多字段排序（spec：sort [createdAt desc, amount]）——逐项生成，不做静默截断
        let order = program
            .apis
            .iter()
            .find(|a| a.entity == *name)
            .and_then(|a| a.list.as_ref())
            .map(|l| {
                if l.sort.is_empty() {
                    format!("{s}.id")
                } else {
                    l.sort
                        .iter()
                        .map(|(f, desc)| {
                            format!("{s}.{} {}", snake(f), if *desc { "DESC" } else { "ASC" })
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            })
            .unwrap_or_else(|| format!("{s}.id"));

        // 外部搜索引擎（opt-in）：工程存在 searchable 实体时，list 的 q 可经引擎取 id。
        // 未配置引擎 / 查询失败 → engine_ids=None → 回退 SQL LIKE（行为零变化）。
        let engine_preamble = if has_search(program) {
            format!(
                "    let engine_ids: Option<Vec<String>> = if params.q.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {{\n        crate::search::search_ids(\"{table}\", params.q.as_deref().unwrap_or(\"\")).await\n    }} else {{\n        None\n    }};\n    let use_engine = engine_ids.is_some();\n    let base_where: &str = if use_engine {{ {wns:?} }} else {{ {w:?} }};\n    let search_filter = match &engine_ids {{\n        Some(ids) if !ids.is_empty() => {{\n            let ph = vec![\"?\"; ids.len()].join(\",\");\n            if base_where.is_empty() {{ format!(\" WHERE s.id IN ({{ph}})\") }} else {{ format!(\" AND s.id IN ({{ph}})\") }}\n        }}\n        Some(_) => {{\n            if base_where.is_empty() {{ \" WHERE 1 = 0\".to_string() }} else {{ \" AND 1 = 0\".to_string() }}\n        }}\n        None => String::new(),\n    }};\n",
                table = table,
                wns = where_clause_no_search,
                w = where_clause
            )
        } else {
            format!(
                "    let base_where: &str = {w:?};\n    let search_filter = String::new();\n",
                w = where_clause
            )
        };

        // ListParams（offset 与 cursor 是互斥协议形状）
        if has_op("list") {
            let q_field = if list_cfg
                .as_ref()
                .map(|l| !l.search.is_empty())
                .unwrap_or(false)
            {
                "    #[serde(default)]\n    pub q: Option<String>,\n"
            } else {
                ""
            };
            let cursor_mode =
                list_cfg.as_ref().and_then(|l| l.paginate.as_deref()) == Some("cursor");
            let pagination_fields = if cursor_mode {
                "    #[serde(default, rename = \"pageSize\", alias = \"page_size\")]\n    pub page_size: Option<u64>,\n    #[serde(default)]\n    pub cursor: Option<String>,\n"
            } else {
                "    #[serde(default)]\n    pub page: Option<u64>,\n    #[serde(default, rename = \"pageSize\", alias = \"page_size\")]\n    pub page_size: Option<u64>,\n"
            };
            out.push_str(&format!(
                "#[derive(Debug, Default, Deserialize)]\npub struct {name}ListParams {{\n{pagination_fields}{q_field}{param_fields}}}\n\n"
            ));
        }

        // list
        if has_op("list") {
            let api_decl = program.apis.iter().find(|a| a.entity == *name).unwrap();
            let cursor_mode =
                list_cfg.as_ref().and_then(|l| l.paginate.as_deref()) == Some("cursor");
            let mut cursor_bind_code = String::new();
            if cursor_mode {
                let sort_fields: Vec<(String, bool)> = api_decl.list.as_ref().unwrap().sort.clone();
                let cfields = cursor_fields(program, api_decl);
                let mut cursor_types: Vec<String> = cfields
                    .iter()
                    .map(|field| cursor_rust_type(program, field))
                    .collect();
                cursor_types.push("String".to_string()); // id tie-breaker
                let cursor_ty = format!("({})", cursor_types.join(", "));
                let (seek_expr, bind_order) = cursor_seek(&sort_fields);
                let prefix = if where_clause.is_empty() {
                    "WHERE"
                } else {
                    "AND"
                };
                let cursor_sql = format!("{prefix} {seek_expr}");
                let mut order_parts: Vec<String> = sort_fields
                    .iter()
                    .map(|(f, desc)| {
                        format!("{s}.{} {}", snake(f), if *desc { "DESC" } else { "ASC" })
                    })
                    .collect();
                let last_desc = sort_fields.last().map(|(_, d)| *d).unwrap_or(false);
                order_parts.push(format!("{s}.id {}", if last_desc { "DESC" } else { "ASC" }));
                let cursor_order = order_parts.join(", ");
                let cursor_vars: Vec<String> = (0..cfields.len())
                    .map(|i| format!("cursor_value_{i}"))
                    .collect();
                let mut tuple_vars = cursor_vars.clone();
                tuple_vars.push("cursor_id".to_string());
                let tuple_binding = tuple_vars.join(", ");
                let cursor_binds = bind_order
                    .iter()
                    .map(|i| {
                        if *i == usize::MAX {
                            "query = query.bind(cursor_id);\n".to_string()
                        } else {
                            format!("query = query.bind({}.clone());\n", cursor_vars[*i])
                        }
                    })
                    .collect::<String>();
                let frag = format!(
                    r#"pub async fn list__SNAKE__(db: &MySqlPool, params: &__NAME__ListParams, cursor: Option<{cursor_ty}>, __TENANT_PARAM__: Option<&str>) -> sqlx::Result<Vec<__NAME__ListRow>> {{
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);
    let cursor_where = if cursor.is_some() {{ {cursor_sql:?}.to_string() }} else {{ String::new() }};
__ENGINE_PREAMBLE__    let sql = format!(
        "SELECT
__SELECT__        FROM __TABLE__ AS s
__JOINS__        {{base_where}}{{search_filter}} {{}}\n        ORDER BY __ORDER__
        LIMIT ?",
        cursor_where
    );
    let mut query = sqlx::query_as::<_, __NAME__ListRow>(&sql);
"#,
                    cursor_ty = cursor_ty,
                    cursor_sql = cursor_sql,
                )
                .replace("__ENGINE_PREAMBLE__", &engine_preamble)
                .replace("__TENANT_PARAM__", tenant_param)
                .replace("__SNAKE__", &format!("_{}", snake_name))
                .replace("__NAME__", name)
                .replace("__SELECT__", &select_cols)
                .replace("__TABLE__", &table)
                .replace("__JOINS__", &joins)
                .replace("__ORDER__", &cursor_order);
                out.push_str(&frag);
                cursor_bind_code = format!(
                    "    if let Some(({tuple_binding})) = cursor {{\n{cursor_binds}    }}\n"
                );
            } else {
                let frag = r#"pub async fn list__SNAKE__(db: &MySqlPool, params: &__NAME__ListParams, __TENANT_PARAM__: Option<&str>) -> sqlx::Result<Vec<__NAME__ListRow>> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);
__ENGINE_PREAMBLE__    let sql = format!(
        "SELECT
__SELECT__        FROM __TABLE__ AS s
__JOINS__        {base_where}{search_filter}
        ORDER BY __ORDER__
        LIMIT ? OFFSET ?",
    );
    let mut query = sqlx::query_as::<_, __NAME__ListRow>(&sql);
"#;
                let frag = frag
                    .replace("__ENGINE_PREAMBLE__", &engine_preamble)
                    .replace("__TENANT_PARAM__", tenant_param)
                    .replace("__SNAKE__", &format!("_{}", snake_name))
                    .replace("__NAME__", name)
                    .replace("__SELECT__", &select_cols)
                    .replace("__TABLE__", &table)
                    .replace("__JOINS__", &joins)
                    .replace("__ORDER__", &order);
                out.push_str(&frag);
            }

            // bind filters/search in declared placeholder order
            if let Some(list) = &list_cfg {
                for (field, _contains) in &list.filters {
                    let pname = snake(field);
                    out.push_str(&format!(
                        "    query = query.bind(params.{pname}.clone()).bind(params.{pname}.clone());\n"
                    ));
                }
                if !list.search.is_empty() {
                    if has_search(program) {
                        // 外部引擎开启 → 绑定引擎返回的 id；否则绑定 q（LIKE）
                        out.push_str(
                            "    if use_engine {\n        if let Some(ids) = &engine_ids {\n            for id in ids {\n                query = query.bind(id.clone());\n            }\n        }\n    } else {\n        query = query.bind(params.q.clone());\n",
                        );
                        for _ in &list.search {
                            out.push_str("        query = query.bind(params.q.clone());\n");
                        }
                        out.push_str("    }\n");
                    } else {
                        out.push_str("    query = query.bind(params.q.clone());\n");
                        for _ in &list.search {
                            out.push_str("    query = query.bind(params.q.clone());\n");
                        }
                    }
                }
            }
            // 租户维度（M10 收尾）：wheres 尾部的 `s.{col} = ?` 条件对应绑定
            if tenant_col.is_some() {
                out.push_str("    query = query.bind(tenant);\n");
            }
            if cursor_mode {
                out.push_str(&cursor_bind_code);
                out.push_str(
                    "    query = query.bind(page_size + 1);\n    query.fetch_all(db).await\n}\n\n",
                );
            } else {
                out.push_str("    query = query.bind(page_size).bind((page - 1) * page_size);\n    query.fetch_all(db).await\n}\n\n");
            }
        }

        // get：路由的 GET handler 使用 find_X（返回实体本身），此处不再重复生成
        // get_X——否则生成后端出现 dead_code 警告。

        // insert
        let mut insert_cols = vec!["id".to_string()];
        let mut insert_vals = vec!["?".to_string()];
        let mut insert_binds = vec!["id".to_string()];
        for field in &writable {
            // DB 列名：关系 → {col}_id（与 gen-mysql DDL 一致）；输入体字段名：裸关系名
            let col = snake(&field.name);
            let db_col = if super::is_relation(program, &field.type_name) {
                format!("{col}_id")
            } else {
                col.clone()
            };
            insert_cols.push(db_col);
            insert_vals.push("?".to_string());
            if super::has_attr(field, "default") {
                let attr = super::field_attr(field, "default").unwrap();
                match attr.args.first().map(|s| s.as_str()) {
                    Some("now") => {
                        insert_binds.push(format!("input.{col}.unwrap_or_else(chrono::Utc::now)"))
                    }
                    Some("uuid") => insert_binds.push(format!(
                        "input.{col}.unwrap_or_else(|| Uuid::new_v4().to_string())"
                    )),
                    Some(lit) => {
                        let fallback = if field.type_name == "Bool" {
                            // @default(true|false)：字面量不是数值，parse::<f64>() 会得到 0.0，
                            // 渲染成整数 0 与 Option<bool> 类型不匹配（真库编译抓出）。
                            lit.eq_ignore_ascii_case("true").to_string()
                        } else if matches!(field.type_name.as_str(), "String" | "Email" | "Text")
                            || super::is_enum(program, &field.type_name)
                        {
                            format!("\"{}\".to_string()", lit)
                        } else {
                            literal_for(&rust_type(program, field), lit.parse().unwrap_or(0.0))
                        };
                        insert_binds
                            .push(format!("input.{col}.clone().unwrap_or_else(|| {fallback})"));
                    }
                    _ => insert_binds.push(format!("input.{col}.clone()")),
                }
            } else if super::is_relation(program, &field.type_name) {
                // 关系字段在输入体里是文本 UUID（CHAR(36)）→ 直接绑定
                insert_binds.push(format!("input.{col}.clone()"));
            } else {
                insert_binds.push(format!("input.{col}.clone()"));
            }
        }
        // createdAt/updatedAt 之类 readonly+default(now) 也入库（now）
        for field in &entity.fields {
            if super::has_attr(field, "id") || !super::has_attr(field, "readonly") {
                continue;
            }
            let col = snake(&field.name);
            let attr = super::field_attr(field, "default");
            let is_now = attr.and_then(|a| a.args.first().cloned()).as_deref() == Some("now");
            insert_cols.push(col.clone());
            insert_vals.push("?".to_string());
            if is_now {
                insert_binds.push("chrono::Utc::now()".to_string());
            } else if let Some(attr) = attr {
                match attr.args.first().map(|s| s.as_str()) {
                    Some("uuid") => insert_binds.push("Uuid::new_v4().to_string()".to_string()),
                    Some(lit) => insert_binds.push(
                        literal_for(&rust_type(program, field), lit.parse().unwrap_or(0.0))
                            .to_string(),
                    ),
                    _ => insert_binds.push("Default::default()".to_string()),
                }
            } else {
                insert_binds.push("Default::default()".to_string());
            }
        }
        // 租户维度（M10 收尾）：create 时租户来自 JWT claim（服务端权威绑定），
        // 输入体里的同名字段被忽略（防越权指定租户）。
        if let Some(col) = &tenant_col {
            insert_cols.push(col.clone());
            insert_vals.push("?".to_string());
        }

        if has_op("create") {
            let frag = r#"pub async fn insert__SNAKE__(db: &MySqlPool, input: &Create__NAME__, __TENANT_PARAM__: Option<&str>) -> sqlx::Result<String> {
    let id = Uuid::new_v4().to_string();
    let sql = "INSERT INTO __TABLE__ (__COLS__) VALUES (__VALS__)";
    sqlx::query(sql)
        .bind(&id)
"#;
            let frag = frag
                .replace("__TENANT_PARAM__", tenant_param)
                .replace("__SNAKE__", &format!("_{}", snake_name))
                .replace("__NAME__", name)
                .replace("__TABLE__", &table)
                .replace("__COLS__", &insert_cols.join(", "))
                .replace("__VALS__", &insert_vals.join(", "));
            out.push_str(&frag);
            for b in &insert_binds[1..] {
                out.push_str(&format!("        .bind({b})\n"));
            }
            if tenant_col.is_some() {
                out.push_str("        .bind(tenant)\n");
            }
            out.push_str("        .execute(db)\n        .await?;\n    Ok(id)\n}\n\n");
        }

        // update
        // 输入体里 optional / @default / readonly 字段是 Option → 调用方可能不提供。
        // 直接 `col = ?` 会把缺省字段写成 NULL（NOT NULL 列 → 1048 Column cannot be null；
        // 有默认值的列则被默认值抹掉）。partial update 语义：缺省即「保持原值」。
        let set_clause = writable
            .iter()
            .map(|f| {
                let col = if super::is_relation(program, &f.type_name) {
                    format!("{}_id", snake(&f.name))
                } else {
                    snake(&f.name)
                };
                let optional_input =
                    f.optional || super::has_attr(f, "default") || super::has_attr(f, "readonly");
                if optional_input {
                    format!("{col} = COALESCE(?, {col})")
                } else {
                    format!("{col} = ?")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        if has_op("update") {
            let frag = r#"pub async fn update__SNAKE__(db: &MySqlPool, id: &str, input: &Update__NAME__) -> sqlx::Result<()> {
    let sql = "UPDATE __TABLE__ SET __SET__ WHERE id = ?";
    sqlx::query(sql)
"#;
            let frag = frag
                .replace("__SNAKE__", &format!("_{}", snake_name))
                .replace("__NAME__", name)
                .replace("__TABLE__", &table)
                .replace("__SET__", &set_clause);
            out.push_str(&frag);
            for field in &writable {
                let col = snake(&field.name);
                if super::is_relation(program, &field.type_name) {
                    // FK 列是 CHAR(36) → 绑字符串
                    out.push_str(&format!("        .bind(input.{col}.clone())\n"));
                } else {
                    out.push_str(&format!("        .bind(input.{col}.clone())\n"));
                }
            }
            out.push_str(
                "        .bind(id)\n        .execute(db)\n        .await?;\n    Ok(())\n}\n\n",
            );
        }

        // delete + get raw（find 供 get/update/delete 与守卫复用）
        if has_op("delete") {
            out.push_str(&format!(
                r#"pub async fn delete_{snake_name}(db: &MySqlPool, id: &str) -> sqlx::Result<()> {{
    sqlx::query("DELETE FROM {table} WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}}

"#
            ));
        }
        // A-2：关系遍历守卫需要 find_X（即使该实体没有 get/update/delete API）
        let relation_targets = access_relation_targets(program);
        if has_op("get") || has_op("update") || has_op("delete") || relation_targets.contains(name)
        {
            let tenant_find = match &tenant_col {
                Some(col) => format!(" AND {col} = ?"),
                None => String::new(),
            };
            let tenant_param = if tenant_col.is_some() {
                "tenant"
            } else {
                "_tenant"
            };
            let tenant_bind = if tenant_col.is_some() {
                ".bind(tenant)"
            } else {
                ""
            };
            out.push_str(&format!(
                r#"pub async fn find_{snake_name}(db: &MySqlPool, id: &str, {tenant_param}: Option<&str>) -> sqlx::Result<Option<{name}>> {{
    let sql = "SELECT * FROM {table} WHERE id = ?{tenant_find}";
    let query = sqlx::query_as::<_, {name}>(sql).bind(id);
    query{tenant_bind}.fetch_optional(db)
        .await
}}

"#,
                tenant_bind = tenant_bind,
                tenant_param = tenant_param,
            ));
        }
    }
    out
}

// ── routes.rs ───────────────────────────────────────────

/// routes + 命令 handlers/路由（命令路由挂到 router 汇总处）。
fn routes_rs_with_commands(program: &Program, has_commands: bool) -> anyhow::Result<String> {
    let mut out = routes_rs(program);
    // 字段级策略（P0-2）：有策略时在 handlers 末尾注入裁剪 helper
    let trim_helpers = field_trim_helpers(program);
    if !trim_helpers.is_empty() {
        out.push('\n');
        out.push_str(&trim_helpers);
    }
    if has_commands {
        let (handlers, routes) = command_routes_rs(program)?;
        // 命令路由插在 router() 的 layer 之前
        let mut route_lines = String::new();
        for (path, handler) in &routes {
            route_lines.push_str(&format!(
                "            .route(\"{path}\", post({handler}))\n"
            ));
        }
        let commands_router = format!(
            "    protected = protected.merge(\n        Router::new()\n{route_lines}            .with_state(db.clone()),\n    );\n"
        );
        out = out.replace(
            "    app = app.merge(protected);",
            &format!("{commands_router}    app = app.merge(protected);"),
        );
        // 审计查询（倒序分页）：独立 router（自有 Query 提取器）——插到 layer 之前
        let has_audit = program.commands.iter().any(|c| c.audit.is_some());
        if has_audit {
            let audit_router = "    app = app.merge(\n        Router::new()\n            .route(\"/api/audit-logs\", get(list_audit_logs_handler))\n            .with_state(db.clone()),\n    );\n";
            out = out.replace("    app.layer(", &format!("{audit_router}    app.layer("));
            out.push_str(
                r#"
#[derive(Debug, Default, serde::Deserialize)]
pub struct AuditLogListParams {
    #[serde(default)]
    pub page: Option<u64>,
    #[serde(default)]
    pub page_size: Option<u64>,
}

async fn list_audit_logs_handler(
    State(db): State<Db>,
    Query(params): Query<AuditLogListParams>,
) -> AppResult<Json<Vec<crate::audit::AuditLogRow>>> {
    Ok(Json(crate::audit::list_audit_logs(
        &db,
        params.page.unwrap_or(1),
        params.page_size.unwrap_or(50),
    )
    .await?))
}

"#,
            );
        }
        // handlers 追加到文件尾部
        out.push('\n');
        out.push_str(&handlers);
    }
    // 实时推送端点（M10/P1）：SSE 订阅（挂公开组）
    if !program.lives.is_empty() {
        let live_router =
            "    app = app.merge(\n        crate::live::router().with_state(db.clone()),\n    );\n";
        out = out.replace("    app.layer(", &format!("{live_router}    app.layer("));
    }
    // 文件上传端点（M9/P1）：挂公开组（v1 不做端点级鉴权；业务写路径仍走各实体守卫）
    if has_storage(program) {
        let storage_router = "    app = app.merge(\n        crate::storage::router().with_state(db.clone()),\n    );\n";
        out = out.replace(
            "    app = app.merge(protected);",
            &format!("{storage_router}    app = app.merge(protected);"),
        );
    }
    // 数据导出端点（M9/P1）：挂公开组（v1 不做端点级鉴权）
    if !program.exports.is_empty() {
        let export_router = "    app = app.merge(\n        crate::export::router().with_state(db.clone()),\n    );\n";
        out = out.replace(
            "    app = app.merge(protected);",
            &format!("{export_router}    app = app.merge(protected);"),
        );
    }
    // 租户管理端点（M10 收尾）：挂公开组（授权走 TENANT_ADMIN_TOKEN 环境变量，
    // 不依赖业务登录/角色；未设置 env 时端点 403 关闭）
    if crate::dsl::tenant_enabled(program) {
        let tenant_admin_router = "    app = app.merge(\n        Router::new()\n            .route(\"/api/admin/tenants/:tenant/revoke\", post(crate::auth::revoke_tenant))\n            .route(\"/api/admin/tenants/:tenant/transfer\", post(crate::auth::transfer_tenant))\n            .with_state(db.clone()),\n    );\n";
        out = out.replace(
            "    app.layer(",
            &format!("{tenant_admin_router}    app.layer("),
        );
    }
    // 定时任务管理端点（M10/P1）：挂受保护路由组
    if !program.jobs.is_empty() {
        let jobs_router = "    protected = protected.merge(\n        crate::jobs::router().with_state(db.clone()),\n    );\n";
        out = out.replace(
            "    app = app.merge(protected);",
            &format!("{jobs_router}    app = app.merge(protected);"),
        );
    }
    // 租户管理端点（M10 收尾）：挂公开组（授权走 TENANT_ADMIN_TOKEN env）
    if crate::dsl::tenant_enabled(program) {
        let tenant_admin_router = "    app = app.merge(\n        Router::new()\n            .route(\"/api/admin/tenants/:tenant/revoke\", post(crate::auth::revoke_tenant))\n            .route(\"/api/admin/tenants/:tenant/transfer\", post(crate::auth::transfer_tenant))\n            .with_state(db.clone()),\n    );\n";
        out = out.replace(
            "    app.layer(",
            &format!("{tenant_admin_router}    app.layer("),
        );
    }
    // /auth/me 需要当前用户 → 挂在受保护路由上（与是否有命令无关）
    if program.auth.is_some() {
        out = out.replace(
            "    app = app.merge(protected);",
            "    protected = protected.merge(\n        Router::new()\n            .route(\"/api/auth/me\", get(crate::auth::me))\n            .with_state(db.clone()),\n    );\n    app = app.merge(protected);",
        );
    }
    // 保护层：所有受保护路由 merge 完成后统一挂载（axum layer 只影响已挂载路由）
    let protect = if program.auth.is_some() {
        "    protected = protected.layer(axum::middleware::from_fn(crate::auth::require_auth));\n"
    } else {
        ""
    };
    out = out.replace(
        "    app = app.merge(protected);",
        &format!("{protect}    app = app.merge(protected);"),
    );
    Ok(out)
}

/// 该 api 的操作级授权：`op → 允许的角色`（多条规则合并去重）。
fn api_auth_roles(api: &crate::dsl::ApiDecl) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for rule in &api.auth {
        for op in &rule.operations {
            let entry = map.entry(op.clone()).or_default();
            for role in &rule.roles {
                if !entry.contains(role) {
                    entry.push(role.clone());
                }
            }
        }
    }
    map
}

// ── 字段级策略（P0-2）：服务端输出裁剪 ────────────────────

/// 生成 routes.rs 头部的字段裁剪 helper：
/// `fn trim_{entity}_fields(v: &mut serde_json::Value, role: &str)` —— 按 FieldRule 表裁剪字段。
fn field_trim_helpers(program: &Program) -> String {
    let policies = crate::dsl::field_policy_roles(program);
    if policies.is_empty() {
        return String::new();
    }
    let mut by_entity: std::collections::BTreeMap<String, Vec<(String, Vec<String>)>> =
        Default::default();
    for (entity, field, roles) in &policies {
        by_entity
            .entry(entity.clone())
            .or_default()
            .push((crate::identity::to_snake_case(field), roles.clone()));
    }
    let mut out = String::new();
    for (entity, fields) in &by_entity {
        let snake_e = snake(entity);
        let mut table_rows = String::new();
        for (col, roles) in fields {
            let roles_lit: Vec<String> = roles
                .iter()
                .map(|r| format!("\"{}\"", escape_rust(r)))
                .collect();
            table_rows.push_str(&format!(
                "        ({}, &[{}]),\n",
                rust_string_lit(col),
                roles_lit.join(", ")
            ));
        }
        // 生成 trim_{snake}_fields 函数体——不用 format!(r#""#) 避免 { } 转义冲突
        out.push_str("/// 字段级策略裁剪（P0-2）：按允许角色表移除调用方不可见的字段。\n");
        out.push_str(&format!(
            "fn trim_{snake_e}_fields(v: &mut serde_json::Value, role: &str) {{\n"
        ));
        out.push_str("    let obj = match v.as_object_mut() {\n        Some(o) => o,\n        None => return,\n    };\n");
        out.push_str("    const POLICY: &[(&str, &[&str])] = &[\n");
        out.push_str(&table_rows);
        out.push_str("    ];\n");
        out.push_str("    for (col, allowed) in POLICY {\n");
        out.push_str("        if !allowed.is_empty() && !allowed.contains(&role) {\n");
        out.push_str("            obj.remove(col);\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }
    out
}

fn routes_rs(program: &Program) -> String {
    let mut out = String::from("// @generated by Forma (gen-rust) — do not edit manually.\n");
    // 字段级策略（P0-2）：有策略时生成裁剪 helper
    let trim_helpers = field_trim_helpers(program);
    out.push_str(&trim_helpers);
    // entity-only（P2-4）：无 api/auth/commands → 极简 router + 零多余导入
    if program.apis.is_empty()
        && program.commands.is_empty()
        && program.auth.is_none()
        && program.jobs.is_empty()
    {
        out.push_str(
            "use axum::Router;\nuse std::sync::Arc;\n\npub type Db = Arc<sqlx::MySqlPool>;\n",
        );
        out.push_str(health_routes_rs());
        out.push_str(
            "\npub fn router(db: sqlx::MySqlPool) -> Router {\n    health_routes(Arc::new(db))\n}\n",
        );
        return out;
    }
    // 导入按需：无 create op → 不需要 post；无 create/update op → 不需要 validators；
    // 无 list op → 不需要 Query 提取器（否则生成后端出现 unused_imports 警告）
    let any_list = program
        .apis
        .iter()
        .any(|a| a.operations.iter().any(|o| o == "list"));
    // 只有「有 create 但没有 list」的实体才需要自由函数 post()；
    // list+create 组合走 MethodRouter 链式 .post(...) → 不需要导入 post
    let any_create_only = program.apis.iter().any(|a| {
        a.operations.iter().any(|o| o == "create") && !a.operations.iter().any(|o| o == "list")
    });
    // 操作级授权（api { auth { ... } } 或 rules { access { ... } }）
    // ——存在即需要 Extension 提取器与 CurrentUser
    // 租户维度（M10 收尾）：@tenant 实体全操作注入当前用户 → 同样需要
    let any_tenant_entity = program
        .entities
        .iter()
        .any(|e| crate::dsl::tenant_field_of(program, &e.name).is_some());
    let any_auth = has_any_auth_rules(program) || any_tenant_entity;
    // 命令一律 POST；auth module 的 register/login 也是 POST
    let has_commands = !program.commands.is_empty();
    let routing = if any_create_only || has_commands || program.auth.is_some() {
        "use axum::routing::{get, post};"
    } else {
        "use axum::routing::get;"
    };
    let any_id_op = program.apis.iter().any(|a| {
        a.operations
            .iter()
            .any(|o| o == "get" || o == "update" || o == "delete")
    });
    let ext_list = if any_auth {
        "Extension, Query, State"
    } else {
        "Query, State"
    };
    let ext_id = if any_auth {
        "Extension, Path, State"
    } else {
        "Path, State"
    };
    let extract = if any_list && any_id_op {
        format!("use axum::extract::{{{ext_list}, Path}};")
    } else if any_list {
        format!("use axum::extract::{{{ext_list}}};")
    } else if any_id_op {
        format!("use axum::extract::{{{ext_id}}};")
    } else {
        "use axum::extract::State;".to_string()
    };
    // 操作级授权需要当前用户扩展（由 require_auth 注入）
    let auth_import = if any_auth {
        "use crate::auth::CurrentUser;\n"
    } else {
        ""
    };
    // 校验器走全限定路径 crate::validators::validate_* → 不需要 glob 导入
    let validators_import = "";
    out.push_str(&format!(
        "{extract}\n{routing}\nuse axum::{{Json, Router}};\n"
    ));
    // 按需导入（P2-4）：entity-only / auth-only 不需要 models/repo
    let needs_repo = !program.apis.is_empty();
    let needs_models = needs_repo;
    // repo glob 仅用于 `Query<{Entity}ListParams>` 的未限定类型名
    let needs_repo_glob = program
        .apis
        .iter()
        .any(|a| a.operations.iter().any(|o| o == "list"));
    let needs_error =
        !program.apis.is_empty() || program.auth.is_some() || !program.commands.is_empty();
    let mut header = String::new();
    if needs_error {
        header.push_str("use crate::error::{AppError, AppResult};\n");
    }
    if needs_models {
        header.push_str("use crate::models::*;\n");
    }
    if needs_repo {
        header.push_str("use crate::repo;\n");
        header.push_str(validators_import);
        if needs_repo_glob {
            header.push_str("use crate::repo::*;\n");
        }
    }
    header.push_str(auth_import);
    header.push_str("use std::sync::Arc;\n\npub type Db = Arc<sqlx::MySqlPool>;\n\n");
    out.push_str(&header);

    // guards 汇总（delete 时校验）
    let mut guard_helpers = String::new();
    for entity in &program.entities {
        let guards: Vec<&crate::dsl::GuardDecl> = program
            .rules
            .iter()
            .flat_map(|r| r.guards.iter())
            .filter(|g| g.entity == entity.name)
            .collect();
        if guards.is_empty() {
            continue;
        }
        let mut idx = 0usize;
        let mut body = String::new();
        for g in &guards {
            if g.op != "delete" {
                continue;
            }
            for deny in &g.denies {
                let mut bindings = String::new();
                let expr = expr_to_rust_mode(
                    &deny.expr,
                    program,
                    entity,
                    "row",
                    &mut bindings,
                    &mut idx,
                    false,
                    true,
                );
                let reason = pick_message(&deny.reason, "operation denied");
                body.push_str(&bindings);
                body.push_str(&format!(
                    "    if {expr} {{ return Err(AppError::Conflict(\"{}\".to_string())); }}\n",
                    escape_rust(&reason),
                    expr = strip_outer_parens(&expr)
                ));
            }
        }
        if !body.is_empty() {
            let snake_name = snake(&entity.name);
            let ename = &entity.name;
            guard_helpers.push_str(&format!(
                "fn guard_delete_{snake_name}(row: &{ename}) -> AppResult<()> {{\n{body}    Ok(())\n}}\n\n"
            ));
        }
    }
    out.push_str(&guard_helpers);

    for entity in &program.entities {
        let name = &entity.name;
        let snake_name = snake(&entity.name);
        let _table = table_of(entity);
        let api = program.apis.iter().find(|a| a.entity == *name);
        let ops = api.map(|a| a.operations.clone()).unwrap_or_default();
        let has = |op: &str| ops.iter().any(|o| o == op);
        // 操作级授权：op → 允许的角色（api 级）
        let auth_map = api.map(api_auth_roles).unwrap_or_default();
        // 实体级访问规则：op → 条件表达式
        let access_map = access_conditions(program, name);
        // 字段级策略（P0-2）：list/get 需要角色做显隐裁剪
        let has_field_policies = !crate::dsl::field_policy_roles(program)
            .iter()
            .filter(|(e, _, _)| e == name)
            .collect::<Vec<_>>()
            .is_empty();
        // 租户维度（M10 收尾）：@tenant 实体全操作需要当前用户（claim 的 tenant）
        let has_tenant_col = crate::dsl::tenant_field_of(program, name).is_some();
        // 调用参数：仅该实体带租户列且启用了 auth 时，handler 才有 `user` 提取器。
        // 无租户列的实体传 None——避免「auth 工程无租户列却引用 user」的存量编译错误。
        let tenant_arg = if has_tenant_col && program.auth.is_some() {
            "user.tenant.as_deref()"
        } else {
            "None"
        };
        // 守卫里的关系行取数：仅多租户工程按 claim 租户过滤（fail-closed）；
        // 无租户维度时 CurrentUser 没有 tenant 字段，必须传 None。
        let guard_tenant_arg = if crate::dsl::tenant_enabled(program) {
            "user.tenant.as_deref()"
        } else {
            "None"
        };
        // 实时推送（M10/P1）：实体有 live 通道时，CRUD 成功后 publish
        let live_table = crate::identity::to_snake_case(&crate::dsl::pluralize(name));
        let live_publish = if has_live_channel(program, &live_table) {
            format!(
                "    crate::live::publish({}, \"{{event}}\");\n",
                rust_string_lit(&live_table)
            )
        } else {
            String::new()
        };
        // 租户维度（M10 收尾）：@tenant 实体全操作需要当前用户（claim 的 tenant）
        let guard = |op: &str| {
            let force_ext = (has_field_policies && matches!(op, "list" | "get")) || has_tenant_col;
            op_guard_bundle(
                program,
                entity,
                &auth_map,
                &access_map,
                op,
                guard_tenant_arg,
                force_ext,
            )
        };

        let frag = "pub fn __SNAKE___routes(db: Db) -> Router {\n    Router::new()\n"
            .replace("__SNAKE___routes", &format!("{}_routes", snake_name));
        out.push_str(&frag);
        if has("list") && has("create") {
            out.push_str(&format!(
                "        .route(\"/api/{plural}\", get(list_{snake_name}).post(create_{snake_name}))\n",
                plural = table_of(entity)
            ));
        } else if has("list") {
            out.push_str(&format!(
                "        .route(\"/api/{plural}\", get(list_{snake_name}))\n",
                plural = table_of(entity)
            ));
        } else if has("create") {
            out.push_str(&format!(
                "        .route(\"/api/{plural}\", post(create_{snake_name}))\n",
                plural = table_of(entity)
            ));
        }
        if has("get") || has("update") || has("delete") {
            let mut methods: Vec<String> = Vec::new();
            if has("get") {
                methods.push(format!("get(get_{snake_name})"));
            }
            if has("update") {
                methods.push(format!("put(update_{snake_name})"));
            }
            if has("delete") {
                methods.push(format!("delete(delete_{snake_name})"));
            }
            // axum 0.7 多方法链：get(h).put(h).delete(h)
            let chain = methods.join(".");

            // 需要导入 post 时一并处理
            has("create");
            out.push_str(&format!(
                "        .route(\"/api/{plural}/:id\", {chain})\n",
                plural = table_of(entity)
            ));
        }
        out.push_str("        .with_state(db)\n}\n\n");

        if has("list") {
            let g = guard("list");
            let cursor_mode = api
                .and_then(|a| a.list.as_ref())
                .and_then(|l| l.paginate.as_deref())
                == Some("cursor");
            if cursor_mode {
                let api_decl = api.expect("cursor api");
                let cfields = cursor_fields(program, api_decl);
                let mut parse_values = String::new();
                let mut cursor_vars = Vec::new();
                let mut cursor_tuple_types = Vec::new();
                let mut next_values = Vec::new();
                for (i, field) in cfields.iter().enumerate() {
                    let var = format!("cursor_value_{i}");
                    let ty = cursor_rust_type(program, field);
                    parse_values.push_str(&format!(
                        "            let {var}: {ty} = payload.values.get({i}).ok_or_else(|| AppError::Validation(vec![crate::error::FieldError::new(\"cursor\", \"cursor sort values do not match this endpoint\")]))?.parse().map_err(|_| AppError::Validation(vec![crate::error::FieldError::new(\"cursor\", \"invalid cursor sort value\")]))?;\n"
                    ));
                    cursor_vars.push(var);
                    cursor_tuple_types.push(ty);
                    next_values.push(cursor_value_expr(field));
                }
                cursor_vars.push("payload.id".to_string());
                cursor_tuple_types.push("String".to_string());
                let tuple_expr = cursor_vars.join(", ");
                let tuple_type = format!("({})", cursor_tuple_types.join(", "));
                let values_expr = next_values.join(", ");
                let values_len = cfields.len();
                out.push_str(&format!(
                    r#"async fn list_{snake_name}(State(db): State<Db>, Query(params): Query<{name}ListParams>{ext_last}) -> AppResult<Json<serde_json::Value>> {{
{pre}    let cursor: Option<{tuple_type}> = match params.cursor.as_deref() {{
        Some(raw) => {{
            let payload: repo::CursorPayload = serde_json::from_str(raw).map_err(|_| AppError::Validation(vec![crate::error::FieldError::new("cursor", "invalid cursor")]))?;
            if payload.values.len() != {values_len} || payload.id.is_empty() {{
                return Err(AppError::Validation(vec![crate::error::FieldError::new("cursor", "cursor shape does not match this endpoint")]));
            }}
{parse_values}            Some(({tuple_expr}))
        }}
        None => None,
    }};
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200) as usize;
    let tenant = {tenant_arg};
    let mut items = repo::list_{snake_name}(&db, &params, cursor, tenant).await?;
    let has_more = items.len() > page_size;
    items.truncate(page_size);
    let next_cursor = if has_more {{
        let last = items.last().expect("has_more implies at least one item");
        let payload = serde_json::json!({{ "values": [{values_expr}], "id": last.id }});
        Some(serde_json::to_string(&payload).map_err(|e| AppError::Db(sqlx::Error::Protocol(e.to_string())))?)
    }} else {{ None }};
    Ok(Json(serde_json::json!({{ "items": items, "nextCursor": next_cursor }})))
}}

"#,
                    ext_last = g.ext_last,
                    tenant_arg = tenant_arg,
                    pre = g.pre,
                    tuple_type = tuple_type,
                    values_len = values_len,
                    parse_values = parse_values,
                    tuple_expr = tuple_expr,
                    values_expr = values_expr,
                ));
            } else if crate::dsl::field_policy_roles(program)
                .iter()
                .any(|(e, _, _)| e == name)
            {
                // 字段级策略（P0-2）：list 走裁剪路径（返回 Value）
                out.push_str(&format!(
                    r#"async fn list_{snake_name}(State(db): State<Db>, Query(params): Query<{name}ListParams>{ext_last}) -> AppResult<Json<Vec<serde_json::Value>>> {{
{pre}    let mut rows = repo::list_{snake_name}(&db, &params, {tenant_arg}).await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {{
        let mut v = serde_json::to_value(row).map_err(AppError::Internal)?;
        trim_{snake_name}_fields(&mut v, &user.role);
        result.push(v);
    }}
    Ok(Json(result))
}}

"#,
                    ext_last = g.ext_last,
                    tenant_arg = tenant_arg,
                    pre = g.pre
                ));
            } else {
                out.push_str(&format!(
                    r#"async fn list_{snake_name}(State(db): State<Db>, Query(params): Query<{name}ListParams>{ext_last}) -> AppResult<Json<Vec<{name}ListRow>>> {{
{pre}    Ok(Json(repo::list_{snake_name}(&db, &params, {tenant_arg}).await?))
}}

"#,
                    ext_last = g.ext_last,
                    tenant_arg = tenant_arg,
                    pre = g.pre
                ));
            }
        }
        if has("get") {
            let g = guard("get");
            if crate::dsl::field_policy_roles(program)
                .iter()
                .any(|(e, _, _)| e == name)
            {
                // 字段级策略（P0-2）：get 走裁剪路径（返回 Value）
                out.push_str(&format!(
                    r#"async fn get_{snake_name}(State(db): State<Db>, Path(id): Path<String>{ext_last}) -> AppResult<Json<serde_json::Value>> {{
{pre}    let row = repo::find_{snake_name}(&db, &id, {tenant_arg}).await?.ok_or(AppError::NotFound)?;
{after_row}    let mut value = serde_json::to_value(row).map_err(AppError::Internal)?;
    trim_{snake_name}_fields(&mut value, &user.role);
    Ok(Json(value))
}}

"#,
                    ext_last = g.ext_last,
                    tenant_arg = tenant_arg,
                    pre = g.pre,
                    after_row = g.after_row
                ));
            } else {
                out.push_str(&format!(
                    r#"async fn get_{snake_name}(State(db): State<Db>, Path(id): Path<String>{ext_last}) -> AppResult<Json<{name}>> {{
{pre}    let row = repo::find_{snake_name}(&db, &id, {tenant_arg}).await?.ok_or(AppError::NotFound)?;
{after_row}    Ok(Json(row))
}}

"#,
                    ext_last = g.ext_last,
                    tenant_arg = tenant_arg,
                    pre = g.pre,
                    after_row = g.after_row
                ));
            }
        }
        if has("create") {
            let g = guard("create");
            let search_index_create = search_index_hook(program, name, "input", "id");
            // 无搜索引擎钩子时不绑定 id（避免 unused variable 警告）
            let id_binding = if search_index_create.is_empty() {
                ""
            } else {
                "let id = "
            };
            out.push_str(&format!(
                r#"async fn create_{snake_name}(State(db): State<Db>, {ext_mid}Json(input): Json<Create{name}>) -> AppResult<Json<serde_json::Value>> {{
{pre}    let errs = crate::validators::validate_create_{snake_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
    {id_binding}repo::insert_{snake_name}(&db, &input, {tenant_arg}).await?;
{search_index}{live_publish}    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
                ext_mid = g.ext_mid,
                pre = g.pre,
                id_binding = id_binding,
                search_index = search_index_create,
                live_publish = live_publish.replace("{event}", "create"),
                tenant_arg = tenant_arg,
            ));
        }
        if has("update") {
            let g = guard("update");
            let search_index_update = search_index_hook(program, name, "input", "id");
            // 有行级访问条件时必须先取到实体行（判定需要行数据）→ 先取行、再授权、最后校验
            let body = if g.after_row.is_empty() {
                format!(
                    r#"    let errs = crate::validators::validate_update_{snake_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
    if repo::find_{snake_name}(&db, &id, {tenant_arg}).await?.is_none() {{
        return Err(AppError::NotFound);
    }}
"#
                )
            } else {
                format!(
                    r#"    let row = repo::find_{snake_name}(&db, &id, {tenant_arg}).await?.ok_or(AppError::NotFound)?;
{after_row}    let errs = crate::validators::validate_update_{snake_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
"#,
                    after_row = g.after_row
                )
            };
            out.push_str(&format!(
                r#"async fn update_{snake_name}(State(db): State<Db>, Path(id): Path<String>, {ext_mid}Json(input): Json<Update{name}>) -> AppResult<Json<serde_json::Value>> {{
{pre}{body}    repo::update_{snake_name}(&db, &id, &input).await?;
{search_index}{live_publish}    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
                ext_mid = g.ext_mid,
                pre = g.pre,
                body = body,
                search_index = search_index_update,
                live_publish = live_publish.replace("{event}", "update"),
            ));
        }
        if has("delete") {
            let has_guard = program
                .rules
                .iter()
                .flat_map(|r| r.guards.iter())
                .any(|g| g.entity == *name && g.op == "delete");
            let search_delete = search_delete_hook(program, name);
            let guard_call = if has_guard {
                format!("    guard_delete_{snake_name}(&row)?;\n")
            } else {
                String::new()
            };
            let g = guard("delete");
            // 无守卫且无行级访问条件时 row 只用于存在性检查 → 避免 unused variable 警告
            let row_binding = if has_guard || !g.after_row.is_empty() {
                "row"
            } else {
                "_row"
            };
            out.push_str(&format!(
                r#"async fn delete_{snake_name}(State(db): State<Db>, Path(id): Path<String>{ext_last}) -> AppResult<Json<serde_json::Value>> {{
{pre}    let {row_binding} = repo::find_{snake_name}(&db, &id, {tenant_arg}).await?.ok_or(AppError::NotFound)?;
{after_row}{guard_call}    repo::delete_{snake_name}(&db, &id).await?;
{search_delete}{live_publish}    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
                ext_last = g.ext_last,
                pre = g.pre,
                after_row = g.after_row,
                row_binding = row_binding,
                guard_call = guard_call,
                search_delete = search_delete,
                live_publish = live_publish.replace("{event}", "delete"),
                tenant_arg = tenant_arg,
            ));
        }
    }

    // 认证路由（公开：注册/登录；受保护：me）
    if program.auth.is_some() {
        out.push_str(
            "pub fn auth_routes(db: Db) -> Router {\n    Router::new()\n        .route(\"/api/auth/register\", post(crate::auth::register))\n        .route(\"/api/auth/login\", post(crate::auth::login))\n        .with_state(db)\n}\n\n",
        );
    }

    // RFC-0005 §16：存活 / 就绪探针（公开，不挂 require_auth）
    out.push_str(health_routes_rs());
    // router 汇总
    out.push_str("pub fn router(db: sqlx::MySqlPool) -> Router {\n");
    out.push_str("    let db = Arc::new(db);\n");
    out.push_str("    let mut app = Router::new();\n");
    out.push_str("    app = app.merge(health_routes(db.clone()));\n");
    out.push_str("    let mut protected = Router::new();\n");
    // 认证：公开端点独立挂载；实体与命令路由统一挂 require_auth 中间件
    let has_auth = program.auth.is_some();
    if has_auth {
        out.push_str("    app = app.merge(auth_routes(db.clone()));\n");
    }
    for entity in &program.entities {
        let snake_name = snake(&entity.name);
        out.push_str(&format!(
            "    protected = protected.merge({snake_name}_routes(db.clone()));\n"
        ));
    }
    // 受保护路由整体并入；保护层在 routes_rs_with_commands 中、所有受保护路由
    // merge 之后才加（axum 的 layer 只作用于调用时已挂载的路由）。
    out.push_str("    app = app.merge(protected);\n");
    out.push_str(
        "    app.layer(axum::middleware::from_fn(crate::request_id::attach_request_id))\n        .layer(cors_layer())\n        .layer(tower_http::timeout::TimeoutLayer::new(std::time::Duration::from_secs(\n            crate::env_u64(\"REQUEST_TIMEOUT_SECS\", 30),\n        )))\n        .layer(axum::extract::DefaultBodyLimit::max(\n            crate::env_u64(\"MAX_BODY_BYTES\", 2_097_152) as usize,\n        ))\n}\n\n\
/// CORS 策略（生产安全默认）：\n\
/// - 未设置 `CORS_ORIGINS` → 不允许任何跨域来源（同源部署的默认形态）；\n\
/// - `CORS_ORIGINS=*` → 允许全部（**仅限本地开发**）；\n\
/// - `CORS_ORIGINS=https://a.com,https://b.com` → 精确允许这些来源。\n\
/// 绝不默认 `permissive()`——那会让任意站点都能读取 API 响应。\n\
fn cors_layer() -> tower_http::cors::CorsLayer {\n    use tower_http::cors::{AllowOrigin, CorsLayer};\n    let origins = std::env::var(\"CORS_ORIGINS\").unwrap_or_default();\n    let allow_origin = if origins.trim() == \"*\" {\n        AllowOrigin::any()\n    } else if origins.trim().is_empty() {\n        AllowOrigin::predicate(|_, _| false)\n    } else {\n        let list: Vec<_> = origins\n            .split(',')\n            .map(|s| s.trim())\n            .filter(|s| !s.is_empty())\n            .filter_map(|s| s.parse().ok())\n            .collect();\n        AllowOrigin::list(list)\n    };\n    CorsLayer::new()\n        .allow_origin(allow_origin)\n        .allow_methods(tower_http::cors::Any)\n        .allow_headers(tower_http::cors::Any)\n}\n",
    );
    out
}

/// RFC-0005 §16：存活 / 就绪探针（公开端点，不经过业务鉴权）。
///
/// `/health` 只证明进程存活；`/ready` 真实探测数据库，不可用时返回 503（fail-closed，
/// 不谎报就绪）。两个分支（完整 router / entity-only 极简 router）共用同一份生成代码。
fn health_routes_rs() -> &'static str {
    r#"
/// 存活探针：不依赖外部服务。
async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

/// 就绪探针：数据库不可达 → 503。
async fn ready_handler(
    axum::extract::State(db): axum::extract::State<Db>,
) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    match sqlx::query("SELECT 1").execute(db.as_ref()).await {
        Ok(_) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({ "ready": true })),
        ),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "ready": false })),
        ),
    }
}

pub fn health_routes(db: Db) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health_handler))
        .route("/ready", axum::routing::get(ready_handler))
        .with_state(db)
}
"#
}

fn main_rs(program: &Program, _has_commands: bool, has_audit: bool) -> String {
    // RFC-0005 §16：配置缺失 / 数据库不可达 → 结构化 fail-closed（非 panic）。
    // 用原始字符串构造生成代码：JSON 内嵌引号无需转义，emitter 侧不经过 format!。
    // RFC-0005 §16：配置缺失 / 数据库不可达 → 结构化 fail-closed（非 panic）。
    // 结构化 JSON 由 tracing 日志承担（LOG_FORMAT=json 即 JSON Lines，含 code 字段）；
    // stderr 输出稳定错误码文本（无插值，规避 emitter 模板转义）。
    let fail_closed_init = r#"
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            tracing::error!(code = "FORMA_CONFIG_MISSING", field = "DATABASE_URL", "database url not configured");
            eprintln!("FORMA_CONFIG_MISSING: DATABASE_URL not configured (expected mysql://user:***@host:3306/db)");
            std::process::exit(1);
        }
    };
    let db = match sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(env_u64("DB_MAX_CONNECTIONS", 8) as u32)
        .acquire_timeout(std::time::Duration::from_secs(env_u64(
            "DB_ACQUIRE_TIMEOUT_SECS",
            10,
        )))
        .connect(&database_url)
        .await
    {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(code = "FORMA_DB_UNREACHABLE", detail = format!("{e}"));
            eprintln!("FORMA_DB_UNREACHABLE: cannot connect to database (see logs)");
            std::process::exit(1);
        }
    };
"#;
    let has_jobs = !program.jobs.is_empty();
    let has_notify = program.entities.iter().any(|e| !e.notify.is_empty());
    let has_live = !program.lives.is_empty();
    let app_name = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "forma-app".into());
    let audit_mod = if has_audit { "\nmod audit;" } else { "" };
    let jobs_mod = if has_jobs { "\nmod jobs;" } else { "" };
    let notify_mod = if has_notify { "\nmod notify;" } else { "" };
    let live_mod = if has_live { "\nmod live;" } else { "" };
    let storage_mod = if has_storage(program) {
        "\nmod storage;"
    } else {
        ""
    };
    let search_mod = if has_search(program) {
        "\nmod search;"
    } else {
        ""
    };
    let export_mod = if !program.exports.is_empty() {
        "\nmod export;"
    } else {
        ""
    };
    let jobs_spawn = if has_jobs {
        "\n    // 定时任务调度器（M10/P1）：后台轮询，30s tick\n    crate::jobs::spawn_scheduler(db.clone());\n"
    } else {
        ""
    };
    let auth_mod = if program.auth.is_some() {
        "\nmod auth;\nmod secret;"
    } else {
        ""
    };
    // auth 模块存在时，启动期校验 JWT 密钥已配置（缺失 → 进程拒绝启动，而非首个请求才 panic）
    let auth_startup_check = if program.auth.is_some() {
        "\n    crate::auth::assert_secret_configured();"
    } else {
        ""
    };
    // M10/P1：结构化日志恒存在（RUST_LOG 过滤 + LOG_FORMAT=json 输出 JSON Lines）。
    // OTLP 导出为 opt-in feature（cargo run --features otlp）：默认不编译 OTLP 代码。
    let tracing_init = r#"
#[cfg(feature = "otlp")]
fn init_tracing() {
    // OTLP（opt-in）：traces 经 gRPC 导出到 collector（OTEL_EXPORTER_OTLP_ENDPOINT）。
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("otlp exporter init");
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name(
                    std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "forma-app".to_string()),
                )
                .build(),
        )
        .build();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("forma")))
        .init();
}

#[cfg(not(feature = "otlp"))]
fn init_tracing() {
    // 默认：human-readable；LOG_FORMAT=json 时输出 JSON Lines；RUST_LOG 控制过滤级别。
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::fmt().json().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
"#;
    // entity-only 工程：error / models / request_id 等契约模块按契约生成，
    // 但极简 router 不引用它们 → crate 级允许 dead_code（不是掩盖错误，是声明有意保留）。
    let dead_code_attr = if program.apis.is_empty()
        && program.commands.is_empty()
        && program.auth.is_none()
        && program.jobs.is_empty()
    {
        "#![allow(dead_code)] // entity-only: contract modules are intentionally generated\n"
    } else {
        ""
    };
    format!(
        r#"// @generated by Forma (gen-rust) — do not edit manually.
{dead_code_attr}mod error;
mod models;
mod repo;
mod request_id;
mod routes;
mod validators;{audit_mod}{auth_mod}{jobs_mod}{notify_mod}{live_mod}{storage_mod}{search_mod}{export_mod}

use routes::router;

{tracing_init}
/// 整数型环境变量（非法值回退到默认值）。
fn env_u64(key: &str, default: u64) -> u64 {{
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}}

/// 优雅关闭：收到 SIGTERM / SIGINT 后停止接受新连接，等待在途请求完成。
/// （容器编排滚动更新依赖它；否则每次发布会丢一批在途请求。）
async fn shutdown_signal() {{
    let ctrl_c = async {{
        let _ = tokio::signal::ctrl_c().await;
    }};
    #[cfg(unix)]
    let terminate = async {{
        if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {{
            sig.recv().await;
        }}
    }};
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {{
        _ = ctrl_c => {{}}
        _ = terminate => {{}}
    }}
    tracing::info!("shutdown signal received — draining in-flight requests");
}}

#[tokio::main]
async fn main() {{
    init_tracing();{fail_closed_init}{auth_startup_check}{jobs_spawn}
    let app = router(db);

    // 端口可用 PORT 环境变量覆盖（默认 8080，与 Web proxy / Flutter 默认 API_BASE 对齐）
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = "0.0.0.0:".to_string() + &port;
    let listener = tokio::net::TcpListener::bind(addr.as_str()).await.expect("bind addr");
    println!("{app_name} backend listening on http://{{}}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("serve");
}}
"#,
        app_name = app_name,
        dead_code_attr = dead_code_attr,
        fail_closed_init = fail_closed_init,
        auth_startup_check = auth_startup_check,
        audit_mod = audit_mod,
        auth_mod = auth_mod,
        jobs_mod = jobs_mod,
        notify_mod = notify_mod,
        live_mod = live_mod,
        jobs_spawn = jobs_spawn,
        tracing_init = tracing_init,
    )
}

// ── 命令类操作（spec §10）：真实业务的状态机动作 ──────────

/// 命令输入体的 Rust 类型（标量；输入不承载关系）。
fn command_input_rust_type(type_name: &str, type_args: &[String], optional: bool) -> String {
    let base = match type_name {
        "Uuid" | "String" | "Email" | "Text" => "String".to_string(),
        "Int" => "i64".to_string(),
        "Float" => "f64".to_string(),
        "Decimal" => "Decimal".to_string(),
        "Bool" => "bool".to_string(),
        "Timestamp" => "DateTime<Utc>".to_string(),
        "Date" => "NaiveDate".to_string(),
        other => {
            if other == "Json" {
                "serde_json::Value".to_string()
            } else {
                "String".to_string() // 枚举 → 变体名文本（与实体模型一致）
            }
        }
    };
    let _ = type_args;
    if optional {
        format!("Option<{base}>")
    } else {
        base
    }
}

/// effect 值 → Rust 绑定表达式（按目标字段类型）。
fn effect_bind_expr(program: &Program, effect: &crate::dsl::Effect) -> anyhow::Result<String> {
    use crate::dsl::EffectValue;
    let Some(entity) = super::find_entity(program, &effect.entity) else {
        anyhow::bail!(
            "effect {}.{}: entity `{}` 不存在",
            effect.entity,
            effect.field,
            effect.entity
        );
    };
    let Some(field) = entity.fields.iter().find(|f| f.name == effect.field) else {
        anyhow::bail!(
            "effect {}.{}: 字段不存在于实体 {}",
            effect.entity,
            effect.field,
            effect.entity
        );
    };
    Ok(match (&effect.value, field.type_name.as_str()) {
        (EffectValue::Ident(v), t) if super::is_enum(program, t) => {
            // 枚举列存变体名文本（VARCHAR(64)）
            if !super::is_enum(program, t) {
                unreachable!()
            }
            let valid = program
                .enums
                .iter()
                .find(|e| e.name == *t)
                .map(|e| e.variants.iter().any(|x| x.name == *v))
                .unwrap_or(false);
            if !valid {
                anyhow::bail!(
                    "effect {}.{}: 变体 `{v}` 不是枚举 {t} 的成员",
                    effect.entity,
                    effect.field
                );
            }
            format!("\"{v}\".to_string()")
        }
        (EffectValue::Ident(v), t) => {
            anyhow::bail!(
                "effect {}.{}: 裸标识符 `{v}` 仅支持枚举变体，但字段类型是 {t}（字符串请加引号）",
                effect.entity,
                effect.field
            );
        }
        (EffectValue::Now, "Timestamp") => "chrono::Utc::now()".to_string(),
        (EffectValue::Now, t) => {
            anyhow::bail!(
                "effect {}.{}: now 仅支持 Timestamp 字段，当前 {t}",
                effect.entity,
                effect.field
            )
        }
        (EffectValue::Str(v), "String" | "Email" | "Text") => format!("\"{v}\".to_string()"),
        (EffectValue::Str(v), t) if super::is_enum(program, t) => format!("\"{v}\".to_string()"),
        (EffectValue::Str(_), t) => {
            anyhow::bail!(
                "effect {}.{}: 字符串值不适用于 {t} 字段",
                effect.entity,
                effect.field
            )
        }
        (EffectValue::Num(n), "Int") => format!("{}i64", *n as i64),
        (EffectValue::Num(n), "Float") => format!("{n}f64"),
        (EffectValue::Num(n), "Decimal") => {
            let scale = field
                .type_args
                .get(1)
                .and_then(|a| a.parse::<usize>().ok())
                .unwrap_or(2);
            format!("\"{n:.scale$}\".to_string()")
        }
        (EffectValue::Num(_), t) => {
            anyhow::bail!(
                "effect {}.{}: 数字值不适用于 {t} 字段",
                effect.entity,
                effect.field
            )
        }
        (EffectValue::Bool(b), "Bool") => format!("{b}"),
        (EffectValue::Bool(_), t) => {
            anyhow::bail!(
                "effect {}.{}: bool 值不适用于 {t} 字段",
                effect.entity,
                effect.field
            )
        }
        (EffectValue::InputRef(f), _) => {
            // 输入体字段直接克隆绑定（Option 字段绑 NULL/值均可）
            format!("input.{}.clone()", snake(f))
        }
    })
}

/// 命令目标实体：从第一个 effect 推导（所有 effect 必须同一实体）。
fn command_entity<'a>(
    program: &'a Program,
    cmd: &crate::dsl::CommandDecl,
) -> anyhow::Result<&'a crate::dsl::EntityDecl> {
    let Some(first) = cmd.effects.first() else {
        anyhow::bail!("api {}: 命令缺少 effect（无法确定目标实体）", cmd.name);
    };
    for e in &cmd.effects {
        if e.entity != first.entity {
            anyhow::bail!(
                "api {}: 所有 effect 必须指向同一实体（{} vs {}）",
                cmd.name,
                first.entity,
                e.entity
            );
        }
    }
    let Some(entity) = super::find_entity(program, &first.entity) else {
        anyhow::bail!("api {}: effect 实体 {} 不存在", cmd.name, first.entity);
    };
    Ok(entity)
}

fn commands_rs(program: &Program) -> String {
    // 命令输入模型（models 的一部分，单独函数便于审计入口按需引入）
    let mut out = String::new();
    for cmd in &program.commands {
        let struct_name = &cmd.name;
        let mut fields = String::new();
        for input in &cmd.input {
            let t = command_input_rust_type(&input.type_name, &input.type_args, input.optional);
            fields.push_str(&format!("    pub {}: {},\n", snake(&input.name), t));
        }
        out.push_str(&format!(
            "#[derive(Debug, Deserialize, Serialize)]\n#[allow(dead_code)] // 契约字段可能只参与审计或 effect 引用\npub struct {struct_name}Input {{\n{fields}}}\n\n"
        ));
    }
    out
}

fn command_validators_rs(program: &Program) -> String {
    let mut out = String::new();
    for cmd in &program.commands {
        let fn_name = snake(&cmd.name);
        let struct_name = &cmd.name;
        let mut body = String::new();
        for input in &cmd.input {
            let col = snake(&input.name);
            let is_text = matches!(input.type_name.as_str(), "String" | "Email" | "Text");
            if !input.optional && is_text {
                body.push_str(&format!(
                    "    if input.{col}.trim().is_empty() {{ errs.push(FieldError::new(\"{col}\", \"{col} 必填\")); }}\n"
                ));
            }
        }
        let mut_kw = if body.contains("errs.push") {
            "mut "
        } else {
            ""
        };
        let arg = if body.contains("input.") {
            "input"
        } else {
            "_input"
        };
        out.push_str(&format!(
            "pub fn validate_{fn_name}({arg}: &{struct_name}Input) -> Vec<FieldError> {{\n    let {mut_kw}errs: Vec<FieldError> = Vec::new();\n{body}    errs\n}}\n\n"
        ));
    }
    out
}

fn command_repo_rs(program: &Program) -> anyhow::Result<String> {
    let mut out = String::new();
    for cmd in &program.commands {
        let entity = command_entity(program, cmd)?;
        let table = table_of(entity);
        let fn_name = snake(&cmd.name);
        let struct_name = &cmd.name;

        let mut set_cols = Vec::new();
        let mut binds = String::new();
        for effect in &cmd.effects {
            let col = if super::is_relation(program, &effect.field) {
                format!("{}_id", snake(&effect.field))
            } else {
                snake(&effect.field)
            };
            set_cols.push(format!("{col} = ?"));
            binds.push_str(&format!(
                "        .bind({})\n",
                effect_bind_expr(program, effect)?
            ));
        }
        if set_cols.is_empty() {
            anyhow::bail!("api {}: 命令缺少 effect", cmd.name);
        }

        // 无 InputRef effect 时 input 不被读取 → _input
        let arg = if binds.contains("input.") {
            "input"
        } else {
            "_input"
        };
        out.push_str(&format!(
            r#"pub async fn {fn_name}(db: &MySqlPool, id: &str, {arg}: &{struct_name}Input) -> sqlx::Result<()> {{
    let sql = "UPDATE {table} SET {} WHERE id = ?";
    sqlx::query(sql)
{binds}        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}}

"#,
            set_cols.join(", ")
        ));
    }
    Ok(out)
}

fn command_routes_rs(program: &Program) -> anyhow::Result<(String, Vec<(String, String)>)> {
    // (handlers 源码, [(axum 路径, handler 名)])
    let mut handlers = String::new();
    let mut routes: Vec<(String, String)> = Vec::new();
    for cmd in &program.commands {
        let entity = command_entity(program, cmd)?;
        let entity_name = &entity.name;
        let fn_name = snake(&cmd.name);
        let struct_name = &cmd.name;
        let axum_path = format!(
            "/api{}",
            cmd.path
                .replace("{id}", ":id")
                .replace('{', ":")
                .replace('}', "")
        );

        // pre 条件（复用 guard 的表达式编译；row = 当前行）
        let pre_check = if let Some(pre) = &cmd.pre {
            let mut idx = 0usize;
            let mut bindings = String::new();
            let expr_code = expr_to_rust_mode(
                pre,
                program,
                entity,
                "row",
                &mut bindings,
                &mut idx,
                false,
                true,
            );
            let msg = cmd
                .message
                .as_ref()
                .map(|m| pick_message(m, "precondition failed"))
                .unwrap_or_else(|| "precondition failed".to_string());
            format!(
                "{bindings}    if !({expr_code}) {{\n        return Err(AppError::Conflict(\"{}\".to_string()));\n    }}\n",
                escape_rust(&msg),
            )
        } else {
            String::new()
        };

        let user_arg = if program.auth.is_some() {
            "req_user: axum::Extension<crate::auth::CurrentUser>, "
        } else {
            ""
        };
        // headers 仅在审计钩子读取 idempotency-key 时需要；
        // row 仅在 pre 条件求值时需要——两者按需生成，避免未使用参数警告。
        let headers_param = if cmd.audit.is_some() {
            "headers: axum::http::HeaderMap, "
        } else {
            ""
        };
        // A-2 连带修复：命令 handler 的 find_X 也带 tenant 形参（与 repo 契约一致）
        let cmd_tenant_arg = if crate::dsl::tenant_enabled(program) {
            "req_user.tenant.as_deref()"
        } else {
            "None"
        };
        let row_let = if cmd.pre.is_some() {
            format!(
                "    let row = repo::find_{}(&db, &id, {}).await?.ok_or(AppError::NotFound)?;\n",
                snake(entity_name),
                cmd_tenant_arg
            )
        } else {
            String::new()
        };
        let actor = if program.auth.is_some() {
            "Some(req_user.username.as_str())"
        } else {
            "None"
        };
        let audit_call = cmd
            .audit
            .as_ref()
            .map(|a| {
                let payload = if cmd.input.is_empty() {
                    "None".to_string()
                } else {
                    // 输入体序列化进审计载荷（可追溯操作参数）
                    "Some(serde_json::to_value(&input).unwrap_or(serde_json::Value::Null))".to_string()
                };
                format!(
                    "    let idempotency_key = headers.get(\"idempotency-key\").and_then(|v| v.to_str().ok());\n    crate::audit::log_action(&db, \"{a}\", \"{entity_name}\", &id, {actor}, idempotency_key, {payload}).await?;\n"
                )
            })
            .unwrap_or_default();

        let notify_call = if has_notify_rules(program, entity_name, &cmd.name) {
            format!(
                "    crate::notify::fire(\"{}\", \"{}\", &id);\n",
                escape_rust(entity_name),
                escape_rust(&cmd.name)
            )
        } else {
            String::new()
        };
        handlers.push_str(&format!(
            r#"async fn {fn_name}_handler(State(db): State<Db>, {user_arg}{headers_param}Path(id): Path<String>, Json(input): Json<{struct_name}Input>) -> AppResult<Json<serde_json::Value>> {{
    let errs = crate::validators::validate_{fn_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
{row_let}{pre_check}{audit_call}    repo::{fn_name}(&db, &id, &input).await?;
{notify_call}    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
            headers_param = headers_param,
            row_let = row_let,
        ));
        routes.push((axum_path, format!("{fn_name}_handler")));
    }
    Ok((handlers, routes))
}

// ── 请求追踪 id（Runtime Contract §4.4 / spec:protocols §7）────────

/// 生成 src/request_id.rs：x-request-id 透传/生成 + 响应头回写 + task-local 暴露。
fn request_id_rs() -> String {
    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-rust) — do not edit manually.\n");
    s.push_str("//! 请求追踪 id（§4.4）：x-request-id 透传/生成 + 响应头回写。\n");
    s.push_str("//! 错误体经 task-local 拿同一 id（对账日志与错误响应对得上）。\n\n");
    s.push_str("use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};\n\n");
    s.push_str("/// 追踪 id 的请求/响应头名。\npub const HEADER: &str = \"x-request-id\";\n\n");
    s.push_str("tokio::task_local! {\n    /// 当前请求的追踪 id（错误体填充用）。\n    pub static REQUEST_ID: String;\n}\n\n");
    s.push_str("/// 透传调用方 x-request-id（非空时），否则生成 uuid v4；\n");
    s.push_str("/// 挂到响应头，并在 task-local 作用域内暴露给错误体。\n");
    s.push_str("pub async fn attach_request_id(req: Request, next: Next) -> Response {\n");
    s.push_str("    let incoming = req\n        .headers()\n        .get(HEADER)\n        .and_then(|v| v.to_str().ok())\n        .map(|s| s.trim().to_string())\n        .filter(|s| !s.is_empty());\n");
    s.push_str("    let id = incoming.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());\n");
    s.push_str("    let mut req = req;\n    req.extensions_mut().insert(id.clone());\n");
    s.push_str("    let mut res = REQUEST_ID\n        .scope(id.clone(), next.run(req))\n        .await;\n");
    s.push_str("    if let Ok(v) = HeaderValue::from_str(&id) {\n        res.headers_mut().insert(HEADER, v);\n    }\n    res\n}\n");
    s
}

// ── 定时任务（M10/P1）调度器生成器 ─────────────────────────

/// effect 值 → SQL 字面量。
fn effect_value_sql(v: &crate::dsl::EffectValue) -> Option<String> {
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    Some(match v {
        crate::dsl::EffectValue::Bool(true) => "TRUE".into(),
        crate::dsl::EffectValue::Bool(false) => "FALSE".into(),
        crate::dsl::EffectValue::Str(s) => q(s),
        crate::dsl::EffectValue::Num(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        crate::dsl::EffectValue::Now => "NOW()".into(),
        crate::dsl::EffectValue::Ident(v) => q(v),
        crate::dsl::EffectValue::InputRef(_) => return None,
    })
}

/// pre 右值 → SQL 字面量。
fn pre_rhs_sql(expr: &crate::dsl::Expr) -> Option<String> {
    match expr {
        crate::dsl::Expr::Str(s) => Some(format!("'{}'", s.replace('\'', "''"))),
        crate::dsl::Expr::Num(n) => {
            if n.fract() == 0.0 {
                Some(format!("{}", *n as i64))
            } else {
                Some(format!("{n}"))
            }
        }
        crate::dsl::Expr::Bool(b) => Some(if *b { "TRUE".into() } else { "FALSE".into() }),
        _ => None,
    }
}

fn bin_op_sql(op: &crate::dsl::BinOp) -> Option<&'static str> {
    Some(match op {
        crate::dsl::BinOp::Eq => "=",
        crate::dsl::BinOp::Ne => "<>",
        crate::dsl::BinOp::Lt => "<",
        crate::dsl::BinOp::Le => "<=",
        crate::dsl::BinOp::Gt => ">",
        crate::dsl::BinOp::Ge => ">=",
        _ => return None,
    })
}

/// 生成 src/jobs.rs：TARGETS 由 DSL 静态展开（运行时零 DSL 依赖）。
fn jobs_rs(program: &Program) -> String {
    let mut target_items = String::new();
    for cmd in &program.commands {
        if !program.jobs.iter().any(|j| j.op == cmd.name) {
            continue;
        }
        let Some(first) = cmd.effects.first() else {
            continue;
        };
        let table = snake(&crate::dsl::pluralize(&first.entity));
        let mut sets: Vec<String> = Vec::new();
        let mut ok = true;
        for eff in &cmd.effects {
            match effect_value_sql(&eff.value) {
                Some(val) => {
                    let col = snake(&eff.field);
                    sets.push(format!(
                        "({}, {})",
                        rust_string_lit(&col),
                        rust_string_lit(&val)
                    ));
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let pre = cmd.pre.as_ref().and_then(|p| match p {
            crate::dsl::Expr::Bin(op, a, b) => {
                let op_sql = bin_op_sql(op)?;
                let crate::dsl::Expr::Ref { path, old: false } = a.as_ref() else {
                    return None;
                };
                if path.len() != 1 {
                    return None;
                }
                let col = snake(&path[0]);
                let rhs = pre_rhs_sql(b.as_ref())?;
                Some((col, op_sql.to_string(), rhs))
            }
            _ => None,
        });
        let pre_lit = match pre {
            Some((c, o, v)) => format!(
                "Some(({}, {}, {}))",
                rust_string_lit(&c),
                rust_string_lit(&o),
                rust_string_lit(&v)
            ),
            None => "None".to_string(),
        };
        target_items.push_str(&format!(
            "        CommandTarget {{ op: {}, table: {}, set_cols: &[{}], pre: {} }},\n",
            rust_string_lit(&cmd.name),
            rust_string_lit(&table),
            sets.join(", "),
            pre_lit
        ));
    }
    if target_items.is_empty() {
        target_items.push_str("        // （无可静态执行的目标：api 类 job 仅记录执行时间）\n");
    }

    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-rust) — do not edit manually.\n");
    s.push_str("// 定时任务（M10/P1）：进程内轮询调度器 + 管理端点。\n");
    s.push_str("// 调度器每 30s tick：对 enabled 且 cron 命中的 job 批量应用目标 command 的\n");
    s.push_str("// effect（纯 SQL WHERE pre，服务端权威语义）；api 类 job 仅记录执行时间。\n\n");
    s.push_str("use axum::extract::{Path, State};\n");
    s.push_str("use axum::routing::{get, post};\n");
    s.push_str("use axum::{Json, Router};\n");
    s.push_str("use chrono::{Datelike, Timelike};\n");
    s.push_str("use sqlx::MySqlPool;\n\n");
    s.push_str("use crate::error::{AppError, AppResult};\n\n");
    s.push_str("type Pool = std::sync::Arc<MySqlPool>;\n\n");
    s.push_str("struct CommandTarget {\n    op: &'static str,\n    table: &'static str,\n    set_cols: &'static [(&'static str, &'static str)],\n    pre: Option<(&'static str, &'static str, &'static str)>,\n}\n\n");
    s.push_str("fn command_targets() -> Vec<CommandTarget> {\n    vec![\n");
    s.push_str(&target_items);
    s.push_str("    ]\n}\n\n");
    // DSL job 声明的静态登记（name, cron, label, op）：启动时 INSERT IGNORE 自注册。
    // 此前 job 行只存在于 seed.sql，而生产镜像不导入 seed → forma_jobs 表为空、
    // 调度器读不到任何 enabled job，声明了定时任务却从不执行（假绿）。
    let mut job_specs = String::new();
    for job in &program.jobs {
        let label = super::pick_message(&job.label, &job.name);
        job_specs.push_str(&format!(
            "        ({}, {}, {}, {}),\n",
            rust_string_lit(&job.name),
            rust_string_lit(&job.cron),
            rust_string_lit(&label),
            rust_string_lit(&job.op),
        ));
    }
    s.push_str("/// DSL 声明的定时任务（运行时自注册到 forma_jobs，避免依赖 seed 数据）。\n");
    s.push_str("static JOB_SPECS: &[(&str, &str, &str, &str)] = &[\n");
    s.push_str(&job_specs);
    s.push_str("];\n\n");
    s.push_str("/// cron 5 字段求值（minute hour dom month dow；支持 `*` `,` `-`）。\n");
    s.push_str("pub fn cron_matches(cron: &str, now: chrono::DateTime<chrono::Local>) -> bool {\n");
    s.push_str("    let fields: Vec<&str> = cron.split_whitespace().collect();\n");
    s.push_str("    if fields.len() != 5 {\n        return false;\n    }\n");
    s.push_str("    let values = [\n        now.minute(),\n        now.hour(),\n        now.day(),\n        now.month(),\n        now.weekday().num_days_from_sunday(),\n    ];\n");
    s.push_str("    for (i, raw) in fields.iter().enumerate() {\n        if !cron_field_matches(raw, values[i]) {\n            return false;\n        }\n    }\n    true\n}\n\n");
    s.push_str("fn cron_field_matches(raw: &str, value: u32) -> bool {\n    if raw == \"*\" {\n        return true;\n    }\n    for part in raw.split(',') {\n        if let Some((a, b)) = part.split_once('-') {\n            if let (Ok(lo), Ok(hi)) = (a.parse::<u32>(), b.parse::<u32>()) {\n                if value >= lo && value <= hi {\n                    return true;\n                }\n            }\n        } else if let Ok(v) = part.parse::<u32>() {\n            if v == value {\n                return true;\n            }\n        }\n    }\n    false\n}\n\n");
    s.push_str("async fn run_job(pool: &MySqlPool, id: &str, name: &str, op: &str) {\n");
    s.push_str("    let target = command_targets().into_iter().find(|t| t.op == op);\n");
    s.push_str("    match target {\n");
    s.push_str("        Some(t) => {\n");
    s.push_str("            let sets: Vec<String> = t\n                .set_cols\n                .iter()\n                .map(|(c, v)| format!(\"{c} = {v}\"))\n                .collect();\n");
    s.push_str(
        "            let mut sql = format!(\"UPDATE {} SET {}\", t.table, sets.join(\", \"));\n",
    );
    s.push_str("            if let Some((pc, pop, pv)) = t.pre {\n");
    s.push_str("                sql.push_str(&format!(\" WHERE {pc} {pop} {pv}\"));\n");
    s.push_str("            }\n");
    s.push_str("            match sqlx::query(&sql).execute(pool).await {\n");
    s.push_str("                Ok(res) => tracing::info!(job = name, affected = res.rows_affected(), \"job effect applied\"),\n");
    s.push_str("                Err(e) => tracing::error!(job = name, error = %e, \"job effect failed\"),\n");
    s.push_str("            }\n");
    s.push_str("        }\n");
    s.push_str("        None => {\n");
    s.push_str("            tracing::warn!(job = name, op, \"no static target for op; recorded run only\");\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str(
        "    let _ = sqlx::query(\"UPDATE forma_jobs SET last_run_at = NOW() WHERE id = ?\")\n",
    );
    s.push_str("        .bind(id)\n        .execute(pool)\n        .await;\n}\n\n");
    s.push_str("/// 每 30s tick：检查 enabled job 的 cron 是否命中当前分钟。\n");
    s.push_str("pub fn spawn_scheduler(pool: MySqlPool) {\n    tokio::spawn(async move {\n        // 自注册：seed 属于案例数据，生产镜像不导入；job 行必须随服务启动幂等写入，\n        // 否则调度器永远读到空表（INSERT IGNORE + name 唯一键，重启/多副本均安全）。\n        for (name, cron, label, op) in JOB_SPECS {\n            let id = uuid::Uuid::new_v4().to_string();\n            let _ = sqlx::query(\n                \"INSERT IGNORE INTO forma_jobs (id, name, cron, label, op, status) VALUES (?, ?, ?, ?, ?, 'enabled')\",\n            )\n            .bind(&id)\n            .bind(name)\n            .bind(cron)\n            .bind(label)\n            .bind(op)\n            .execute(&pool)\n            .await;\n        }\n        loop {\n");
    s.push_str("            tokio::time::sleep(std::time::Duration::from_secs(30)).await;\n");
    s.push_str("            let rows = sqlx::query_as::<_, (String, String, String, String)>(\n");
    s.push_str(
        "                \"SELECT id, name, cron, op FROM forma_jobs WHERE status = 'enabled'\",\n",
    );
    s.push_str("            )\n            .fetch_all(&pool)\n            .await;\n");
    s.push_str(
        "            let Ok(rows) = rows else {\n                continue;\n            };\n",
    );
    s.push_str("            let now = chrono::Local::now();\n");
    s.push_str("            for (id, name, cron, op) in rows {\n");
    s.push_str("                if cron_matches(&cron, now) {\n");
    s.push_str("                    tracing::info!(job = %name, \"job firing\");\n");
    s.push_str("                    run_job(&pool, &id, &name, &op).await;\n");
    s.push_str("                }\n            }\n        }\n    });\n}\n\n");
    s.push_str(
        "async fn list_jobs(State(db): State<Pool>) -> AppResult<Json<serde_json::Value>> {\n",
    );
    s.push_str("    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String, String, Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>)>(\n");
    s.push_str("        \"SELECT id, name, cron, label, op, status, last_run_at, next_run_at FROM forma_jobs ORDER BY name\",\n");
    s.push_str("    )\n    .fetch_all(&*db)\n    .await?;\n");
    s.push_str("    let jobs: Vec<serde_json::Value> = rows\n        .into_iter()\n");
    s.push_str("        .map(|(id, name, cron, label, op, status, last, next)| {\n");
    s.push_str("            serde_json::json!({ \"id\": id, \"name\": name, \"cron\": cron, \"label\": label, \"op\": op, \"status\": status, \"lastRunAt\": last.map(|d| d.to_string()), \"nextRunAt\": next.map(|d| d.to_string()) })\n");
    s.push_str("        })\n        .collect();\n");
    s.push_str("    Ok(Json(serde_json::json!({ \"jobs\": jobs })))\n}\n\n");
    s.push_str("async fn trigger_job(State(db): State<Pool>, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {\n");
    s.push_str("    let row =\n        sqlx::query_as::<_, (String, String, String)>(\"SELECT name, cron, op FROM forma_jobs WHERE id = ?\")\n            .bind(&id)\n            .fetch_optional(&*db)\n            .await?\n            .ok_or(AppError::NotFound)?;\n");
    s.push_str("    run_job(&db, &id, &row.0, &row.2).await;\n");
    s.push_str("    Ok(Json(serde_json::json!({ \"ok\": true })))\n}\n\n");
    s.push_str(
        "async fn set_job_status(db: &MySqlPool, id: &str, status: &str) -> AppResult<()> {\n",
    );
    s.push_str("    let res = sqlx::query(\"UPDATE forma_jobs SET status = ? WHERE id = ?\")\n");
    s.push_str(
        "        .bind(status)\n        .bind(id)\n        .execute(db)\n        .await?;\n",
    );
    s.push_str("    if res.rows_affected() == 0 {\n        return Err(AppError::NotFound);\n    }\n    Ok(())\n}\n\n");
    s.push_str("async fn enable_job(State(db): State<Pool>, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {\n");
    s.push_str("    set_job_status(&db, &id, \"enabled\").await?;\n");
    s.push_str("    Ok(Json(serde_json::json!({ \"ok\": true, \"status\": \"enabled\" })))\n}\n\n");
    s.push_str("async fn disable_job(State(db): State<Pool>, Path(id): Path<String>) -> AppResult<Json<serde_json::Value>> {\n");
    s.push_str("    set_job_status(&db, &id, \"disabled\").await?;\n");
    s.push_str(
        "    Ok(Json(serde_json::json!({ \"ok\": true, \"status\": \"disabled\" })))\n}\n\n",
    );
    s.push_str("pub fn router() -> Router<Pool> {\n");
    s.push_str("    Router::new()\n");
    s.push_str("        .route(\"/api/jobs\", get(list_jobs))\n");
    s.push_str("        .route(\"/api/jobs/:id/trigger\", post(trigger_job))\n");
    s.push_str("        .route(\"/api/jobs/:id/enable\", post(enable_job))\n");
    s.push_str("        .route(\"/api/jobs/:id/disable\", post(disable_job))\n}\n");
    s
}

/// Rust 字符串字面量（debug 转义）。
fn rust_string_lit(v: &str) -> String {
    format!("{:?}", v)
}

// ── 外部搜索引擎（Meilisearch / Elasticsearch）生成器 ─────────

/// 生成 src/search.rs：外部搜索引擎适配层（opt-in：SEARCH_ENGINE 环境变量）。
///
/// - 引擎选择：`SEARCH_ENGINE=meilisearch|elasticsearch`（未设置 → 全部 no-op，不影响 SQL LIKE）。
/// - 写路径：`index_document` / `delete_document`（create/update/delete 后同步）。
/// - 读路径：`search_ids`（list 的 q 经引擎取 id；失败/未配置 → None → 回退 LIKE）。
/// - 依赖：reqwest（仅当工程存在 searchable 实体时条件引入）。
fn search_rs(program: &Program) -> String {
    let mut specs = String::new();
    for entity in &program.entities {
        let fields = &entity.searchable;
        if fields.is_empty() {
            continue;
        }
        let table = crate::identity::to_snake_case(&crate::dsl::pluralize(&entity.name));
        specs.push_str(&format!("    SearchSpec {{ table: \"{table}\" }},\n"));
    }
    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-rust) — do not edit manually.\n");
    s.push_str("//! 外部搜索引擎适配层（opt-in：SEARCH_ENGINE 环境变量）。\n");
    s.push_str("//!\n");
    s.push_str(
        "//! - `SEARCH_ENGINE=meilisearch|elasticsearch`（未设置 → 全部 no-op，不影响 SQL LIKE）\n",
    );
    s.push_str(
        "//! - Meilisearch: MEILISEARCH_URL / MEILISEARCH_API_KEY / MEILISEARCH_INDEX_PREFIX\n",
    );
    s.push_str("//! - Elasticsearch: ELASTICSEARCH_URL / ELASTICSEARCH_API_KEY / ELASTICSEARCH_INDEX_PREFIX\n\n");
    s.push_str("use serde_json::Value;\n\n");
    s.push_str("pub struct SearchSpec {\n    pub table: &'static str,\n}\n\n");
    s.push_str("pub static SEARCH_SPECS: &[SearchSpec] = &[\n");
    s.push_str(&specs);
    s.push_str("];\n\n");
    s.push_str(
        r#"/// 当前引擎（未配置 → None）。引擎选择完全运行时化（构建产物与引擎解耦）。
fn engine() -> Option<&'static str> {
    match std::env::var("SEARCH_ENGINE").ok().as_deref() {
        Some("meilisearch") => Some("meilisearch"),
        Some("elasticsearch") => Some("elasticsearch"),
        _ => None,
    }
}


fn index_prefix(engine: &str) -> String {
    let var = if engine == "meilisearch" { "MEILISEARCH_INDEX_PREFIX" } else { "ELASTICSEARCH_INDEX_PREFIX" };
    std::env::var(var).unwrap_or_default()
}

fn spec(table: &str) -> Option<&'static SearchSpec> {
    SEARCH_SPECS.iter().find(|s| s.table == table)
}

/// 引擎检索：返回命中 id（未配置 / 失败 → None，调用方回退 SQL LIKE）。
pub async fn search_ids(table: &str, q: &str) -> Option<Vec<String>> {
    let engine = engine()?;
    if spec(table).is_none() {
        return None;
    }
    let result = match engine {
        "meilisearch" => meilisearch_search(table, q).await,
        "elasticsearch" => elasticsearch_search(table, q).await,
        _ => return None,
    };
    match result {
        Ok(ids) => Some(ids),
        Err(e) => {
            tracing::warn!(error = %e, "search engine query failed; falling back to SQL LIKE");
            None
        }
    }
}

/// 写路径同步：create/update 后索引文档（未配置引擎 → no-op，错误只记日志）。
pub async fn index_document(table: &str, id: &str, doc: Value) {
    let Some(engine) = engine() else { return };
    if spec(table).is_none() {
        return;
    }
    let result = match engine {
        "meilisearch" => meilisearch_index(table, id, doc).await,
        "elasticsearch" => elasticsearch_index(table, id, doc).await,
        _ => return,
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "search index sync failed");
    }
}

/// 写路径同步：delete 后删文档（未配置引擎 → no-op，错误只记日志）。
pub async fn delete_document(table: &str, id: &str) {
    let Some(engine) = engine() else { return };
    if spec(table).is_none() {
        return;
    }
    let result = match engine {
        "meilisearch" => meilisearch_delete(table, id).await,
        "elasticsearch" => elasticsearch_delete(table, id).await,
        _ => return,
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "search delete sync failed");
    }
}

async fn meilisearch_search(index: &str, q: &str) -> Result<Vec<String>, String> {
    let url = std::env::var("MEILISEARCH_URL").map_err(|_| "MEILISEARCH_URL not configured".to_string())?;
    let key = std::env::var("MEILISEARCH_API_KEY").unwrap_or_default();
    let prefix = index_prefix("meilisearch");
    let resp = reqwest::Client::new()
        .post(format!("{url}/indexes/{prefix}{index}/search"))
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({ "q": q }))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp["hits"]
        .as_array()
        .map(|hits| {
            hits.iter()
                .filter_map(|h| h["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}

async fn meilisearch_index(index: &str, id: &str, mut doc: Value) -> Result<(), String> {
    if let Value::Object(ref mut map) = doc {
        map.insert("id".to_string(), Value::String(id.to_string()));
    }
    let url = std::env::var("MEILISEARCH_URL").map_err(|_| "MEILISEARCH_URL not configured".to_string())?;
    let key = std::env::var("MEILISEARCH_API_KEY").unwrap_or_default();
    let prefix = index_prefix("meilisearch");
    reqwest::Client::new()
        .post(format!("{url}/indexes/{prefix}{index}/documents"))
        .header("Authorization", format!("Bearer {key}"))
        .json(&vec![doc])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn meilisearch_delete(index: &str, id: &str) -> Result<(), String> {
    let url = std::env::var("MEILISEARCH_URL").map_err(|_| "MEILISEARCH_URL not configured".to_string())?;
    let key = std::env::var("MEILISEARCH_API_KEY").unwrap_or_default();
    let prefix = index_prefix("meilisearch");
    reqwest::Client::new()
        .delete(format!("{url}/indexes/{prefix}{index}/documents/{id}"))
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn elasticsearch_search(index: &str, q: &str) -> Result<Vec<String>, String> {
    let url = std::env::var("ELASTICSEARCH_URL").map_err(|_| "ELASTICSEARCH_URL not configured".to_string())?;
    let key = std::env::var("ELASTICSEARCH_API_KEY").unwrap_or_default();
    let prefix = index_prefix("elasticsearch");
    let mut req = reqwest::Client::new()
        .post(format!("{url}/{prefix}{index}/_search"))
        .json(&serde_json::json!({ "query": { "query_string": { "query": q } } }));
    if !key.is_empty() {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp["hits"]["hits"]
        .as_array()
        .map(|hits| {
            hits.iter()
                .filter_map(|h| h["_id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}

async fn elasticsearch_index(index: &str, id: &str, doc: Value) -> Result<(), String> {
    let url = std::env::var("ELASTICSEARCH_URL").map_err(|_| "ELASTICSEARCH_URL not configured".to_string())?;
    let key = std::env::var("ELASTICSEARCH_API_KEY").unwrap_or_default();
    let prefix = index_prefix("elasticsearch");
    let mut req = reqwest::Client::new()
        .put(format!("{url}/{prefix}{index}/_doc/{id}"))
        .json(&doc);
    if !key.is_empty() {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    req.send().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn elasticsearch_delete(index: &str, id: &str) -> Result<(), String> {
    let url = std::env::var("ELASTICSEARCH_URL").map_err(|_| "ELASTICSEARCH_URL not configured".to_string())?;
    let key = std::env::var("ELASTICSEARCH_API_KEY").unwrap_or_default();
    let prefix = index_prefix("elasticsearch");
    let mut req = reqwest::Client::new().delete(format!("{url}/{prefix}{index}/_doc/{id}"));
    if !key.is_empty() {
        req = req.header("Authorization", format!("Bearer {key}"));
    }
    req.send().await.map_err(|e| e.to_string())?;
    Ok(())
}
"#,
    );
    s
}

// ── 通知派发（M10/P1）生成器 ───────────────────────────────

/// 生成 src/notify.rs：NotifyRule 静态表 + fire()（规则由 DSL 展开）。
fn notify_rs(program: &Program) -> String {
    let mut rule_items = String::new();
    for entity in &program.entities {
        for n in &entity.notify {
            let target = n.target.as_deref().unwrap_or("-");
            rule_items.push_str(&format!(
                "        NotifyRule {{ entity: {}, event: {}, channel: {}, target: {} }},\n",
                rust_string_lit(&entity.name),
                rust_string_lit(&n.event.clone().unwrap_or_else(|| "*".into())),
                rust_string_lit(&n.channel),
                rust_string_lit(target),
            ));
        }
    }
    if rule_items.is_empty() {
        rule_items.push_str("        // （无通知规则）\n");
    }
    let mut s = String::new();
    let has_webhook = program
        .entities
        .iter()
        .any(|e| e.notify.iter().any(|n| n.channel == "webhook"));
    let has_email = program
        .entities
        .iter()
        .any(|e| e.notify.iter().any(|n| n.channel == "email"));
    let has_sms = program
        .entities
        .iter()
        .any(|e| e.notify.iter().any(|n| n.channel == "sms"));
    s.push_str("// @generated by Forma (gen-rust) — do not edit manually.\n");
    s.push_str("// 通知派发（M10/P1）：字段级 @notify 声明的事件订阅。\n");
    let mut drivers: Vec<&str> = vec!["log（tracing）"];
    if has_webhook {
        drivers.push("webhook（reqwest POST JSON）");
    }
    if has_email {
        drivers.push("email（lettre SMTP）");
    }
    if has_sms {
        drivers.push("sms（HTTP 网关）");
    }
    s.push_str(&format!(
        "// 驱动：{}；未列出的渠道为占位（外部集成点）。\n",
        drivers.join(" / ")
    ));
    s.push_str("// 规则由 DSL 静态展开——运行时零 DSL 依赖（spec:ir）。\n\n");
    s.push_str("struct NotifyRule {\n    entity: &'static str,\n    event: &'static str,\n    channel: &'static str,\n    target: &'static str,\n}\n\n");
    s.push_str("fn notify_rules() -> &'static [NotifyRule] {\n    &[\n");
    s.push_str(&rule_items);
    s.push_str("    ]\n}\n\n");
    s.push_str("/// 事件触发点：command handler 成功后调用。\n");
    s.push_str("pub fn fire(entity: &str, event: &str, entity_id: &str) {\n");
    s.push_str("    for rule in notify_rules() {\n");
    s.push_str("        if rule.entity != entity {\n            continue;\n        }\n");
    s.push_str("        if rule.event != \"*\" && rule.event != event {\n            continue;\n        }\n");
    s.push_str("        match rule.channel {\n");
    s.push_str("            \"log\" => tracing::info!(\n                entity, event, id = entity_id, target = rule.target,\n                \"notify: log channel\"\n            ),\n");
    if has_email {
        s.push_str("            \"email\" => {\n");
        s.push_str(
            "                // 真实驱动：lettre SMTP（spawn_blocking，不阻塞异步运行时）\n",
        );
        s.push_str("                if let Ok(handle) = tokio::runtime::Handle::try_current() {\n");
        s.push_str("                    let to = rule.target.to_string();\n");
        s.push_str("                    let entity = entity.to_string();\n");
        s.push_str("                    let event = event.to_string();\n");
        s.push_str("                    let id = entity_id.to_string();\n");
        s.push_str("                    handle.spawn_blocking(move || {\n");
        s.push_str("                        match send_email(&to, &entity, &event, &id) {\n");
        s.push_str(
            "                            Ok(()) => tracing::info!(to = %to, \"notify: email delivered\"),\n",
        );
        s.push_str(
            "                            Err(err) => tracing::warn!(error = %err, \"notify: email delivery failed\"),\n",
        );
        s.push_str("                        }\n");
        s.push_str("                    });\n");
        s.push_str("                } else {\n");
        s.push_str(
            "                    tracing::warn!(\"notify: email skipped (no async runtime)\");\n",
        );
        s.push_str("                }\n");
        s.push_str("            }\n");
    } else {
        s.push_str(
            "            \"email\" => tracing::warn!(\n                entity, event, id = entity_id, to = rule.target,\n                \"notify: email driver not configured (integration point)\"\n            ),\n",
        );
    }
    if has_sms {
        s.push_str("            \"sms\" => {\n");
        s.push_str(
            "                // 真实驱动：通用 HTTP 网关（SMS_GATEWAY_URL + 可选 SMS_GATEWAY_TOKEN）\n",
        );
        s.push_str("                if let Ok(handle) = tokio::runtime::Handle::try_current() {\n");
        s.push_str("                    let to = rule.target.to_string();\n");
        s.push_str(
            "                    let text = format!(\"{}: {} id={}\", entity, event, entity_id);\n",
        );
        s.push_str("                    handle.spawn(async move {\n");
        s.push_str(
            "                        let url = match std::env::var(\"SMS_GATEWAY_URL\") {\n                            Ok(u) if !u.trim().is_empty() => u,\n                            _ => {\n                                tracing::warn!(\"notify: SMS_GATEWAY_URL not configured\");\n                                return;\n                            }\n                        };\n",
        );
        s.push_str("                        let mut req = reqwest::Client::new().post(&url).json(&serde_json::json!({\n");
        s.push_str("                            \"to\": to, \"text\": text,\n");
        s.push_str("                        }));\n");
        s.push_str(
            "                        if let Ok(token) = std::env::var(\"SMS_GATEWAY_TOKEN\") {\n                            if !token.trim().is_empty() {\n                                req = req.header(\"Authorization\", format!(\"Bearer {token}\"));
                            }\n                        }\n",
        );
        s.push_str("                        match req.send().await {\n");
        s.push_str(
            "                            Ok(res) => tracing::info!(status = res.status().as_u16(), \"notify: sms delivered\"),\n",
        );
        s.push_str(
            "                            Err(err) => tracing::warn!(error = %err, \"notify: sms delivery failed\"),\n",
        );
        s.push_str("                        }\n");
        s.push_str("                    });\n");
        s.push_str("                } else {\n");
        s.push_str(
            "                    tracing::warn!(\"notify: sms skipped (no async runtime)\");\n",
        );
        s.push_str("                }\n");
        s.push_str("            }\n");
    } else {
        s.push_str(
            "            \"sms\" => tracing::warn!(\n                entity, event, id = entity_id, to = rule.target,\n                \"notify: sms driver not configured (integration point)\"\n            ),\n",
        );
    }
    if has_webhook {
        s.push_str("            \"webhook\" => {\n");
        s.push_str(
            "                // 真实驱动：POST JSON 到目标 URL（spawn 异步任务，不阻塞请求）\n",
        );
        s.push_str("                if let Ok(handle) = tokio::runtime::Handle::try_current() {\n");
        s.push_str("                    let url = rule.target.to_string();\n");
        s.push_str("                    let payload = serde_json::json!({\n");
        s.push_str(
            "                        \"entity\": entity, \"event\": event, \"id\": entity_id,\n",
        );
        s.push_str("                    });\n");
        s.push_str("                    handle.spawn(async move {\n");
        s.push_str("                        let client = reqwest::Client::new();\n");
        s.push_str(
            "                        match client.post(&url).json(&payload).send().await {\n",
        );
        s.push_str(
            "                            Ok(res) => tracing::info!(url = %url, status = res.status().as_u16(), \"notify: webhook delivered\"),\n",
        );
        s.push_str(
            "                            Err(err) => tracing::warn!(url = %url, error = %err, \"notify: webhook delivery failed\"),\n",
        );
        s.push_str("                        }\n");
        s.push_str("                    });\n");
        s.push_str("                } else {\n");
        s.push_str(
            "                    tracing::warn!(\"notify: webhook skipped (no async runtime)\");\n",
        );
        s.push_str("                }\n");
        s.push_str("            }\n");
    } else {
        s.push_str(
            "            \"webhook\" => tracing::warn!(\n                entity, event, id = entity_id, url = rule.target,\n                \"notify: webhook driver not configured (integration point)\"\n            ),\n",
        );
    }
    s.push_str(
        "            other => tracing::error!(channel = other, \"notify: unknown channel\"),\n",
    );
    s.push_str("        }\n    }\n}\n");
    if has_email {
        s.push_str(
            r#"
/// 真实 email 驱动（lettre SMTP）：配置经环境变量注入。
/// - SMTP_HOST / SMTP_PORT（默认 587）/ SMTP_USER / SMTP_PASS / SMTP_FROM
/// - 未配置 SMTP_HOST → 返回 Err（不 panic，调用方记录告警）
#[allow(dead_code)]
fn send_email(to: &str, entity: &str, event: &str, id: &str) -> Result<(), String> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let host = std::env::var("SMTP_HOST").map_err(|_| "SMTP_HOST not configured".to_string())?;
    let port: u16 = std::env::var("SMTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(587);
    let user = std::env::var("SMTP_USER").unwrap_or_default();
    let pass = std::env::var("SMTP_PASS").unwrap_or_default();
    let from = std::env::var("SMTP_FROM").unwrap_or_else(|_| "no-reply@example.com".to_string());
    let email = Message::builder()
        .from(from.as_str().parse().map_err(|e| format!("invalid SMTP_FROM: {e}"))?)
        .to(to.parse().map_err(|e| format!("invalid target: {e}"))?)
        .subject(format!("[{entity}] {event}"))
        .body(format!("entity={entity} event={event} id={id}"))
        .map_err(|e| format!("email build failed: {e}"))?;
    let transport = SmtpTransport::starttls_relay(&host)
        .map_err(|e| format!("smtp relay error: {e}"))?
        .port(port)
        .credentials(Credentials::new(user, pass))
        .build();
    transport
        .send(&email)
        .map(|_| ())
        .map_err(|e| format!("smtp send failed: {e}"))
}
"#,
        );
    }
    s
}

// ── 实时推送（M10/P1）：SSE 通道 ─────────────────────────

/// 生成 src/live.rs：SSE 广播通道 + 订阅端点。
/// 通道按 live 块名命名；handler 成功后调用 publish() 通知订阅者。
/// 零新增依赖：axum::response::sse + tokio broadcast。
fn live_rs(program: &Program) -> String {
    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-rust) — do not edit manually.\n");
    s.push_str("// 实时推送（M10/P1）：SSE 通道。订阅端点 + 广播发布。\n");
    s.push_str("// 零新增依赖：axum SSE + tokio broadcast channel。\n\n");
    s.push_str("use axum::extract::{State};\n");
    s.push_str("use axum::response::sse::{Event, KeepAlive, Sse};\n");
    s.push_str("use axum::Router;\n");
    s.push_str("use sqlx::MySqlPool;\n\n");
    s.push_str("type Pool = std::sync::Arc<MySqlPool>;\n\n");

    // 静态通道表：live 名 → 目标实体表（事件载荷 = 实体刷新提示）
    let mut chan_items = String::new();
    let mut entities = Vec::new();
    for live in &program.lives {
        let entity = program
            .apis
            .iter()
            .find(|a| a.name == live.api)
            .map(|a| a.entity.clone())
            .unwrap_or_default();
        let table = crate::identity::to_snake_case(&crate::dsl::pluralize(&entity));
        if !entities.contains(&table) {
            entities.push(table.clone());
        }
        chan_items.push_str(&format!(
            "        ChannelSpec {{ name: {}, table: {} }},\n",
            rust_string_lit(&live.name),
            rust_string_lit(&table),
        ));
    }
    s.push_str("struct ChannelSpec {\n    name: &'static str,\n    table: &'static str,\n}\n\n");
    s.push_str("fn channel_specs() -> &'static [ChannelSpec] {\n    &[\n");
    s.push_str(&chan_items);
    s.push_str("    ]\n}\n\n");

    // 全局广播总线（每表一条 broadcast channel，容量 64）
    s.push_str("/// 全局事件总线：entity table → broadcast sender。\n");
    s.push_str("static BUS: std::sync::OnceLock<std::collections::BTreeMap<&'static str, tokio::sync::broadcast::Sender<String>>> = std::sync::OnceLock::new();\n\n");
    s.push_str("fn bus() -> &'static std::collections::BTreeMap<&'static str, tokio::sync::broadcast::Sender<String>> {\n");
    s.push_str("    BUS.get_or_init(|| {\n");
    s.push_str("        let mut map = std::collections::BTreeMap::new();\n");
    s.push_str("        for spec in channel_specs() {\n");
    s.push_str("            map.insert(spec.table, tokio::sync::broadcast::channel(64).0);\n");
    s.push_str("        }\n        map\n    })\n}\n\n");

    // publish：业务操作成功后调用（按表广播）
    s.push_str("/// 业务操作成功后调用：向订阅该表的 SSE 客户端广播一条刷新事件。\n");
    s.push_str("pub fn publish(table: &str, event: &str) {\n");
    s.push_str("    if let Some(tx) = bus().get(table) {\n");
    s.push_str("        let payload = format!(\"{{\\\\\\\"table\\\\\\\": \\\\\\\"{}\\\\\\\", \\\\\\\"event\\\\\\\": \\\\\\\"{}\\\\\\\"}}\", table, event);\n");
    s.push_str("        let _ = tx.send(payload);\n");
    s.push_str("    }\n}\n\n");

    // SSE 订阅端点：GET /api/live/:name
    s.push_str("async fn subscribe(\n    State(_db): State<Pool>,\n    axum::extract::Path(name): axum::extract::Path<String>,\n) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {\n");
    s.push_str("    let spec = channel_specs().iter().find(|c| c.name == name);\n");
    s.push_str("    let table = spec.map(|c| c.table).unwrap_or(\"\");\n");
    s.push_str("    let rx = match bus().get(table) {\n");
    s.push_str("        Some(tx) => tx.subscribe(),\n");
    s.push_str("        None => {\n");
    s.push_str("            // 未知通道：返回空流（连接立即结束）\n");
    s.push_str("            let (_tx, rx) = tokio::sync::broadcast::channel(1);\n");
    s.push_str("            rx\n        }\n    };\n");
    s.push_str("    use tokio_stream::StreamExt;\n");
    s.push_str("    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)\n");
    s.push_str("        .filter_map(|msg| match msg {\n");
    s.push_str("                Ok(data) => Some(Ok::<_, std::convert::Infallible>(Event::default().data(data))),\n");
    s.push_str("                Err(_) => None,\n");
    s.push_str("            }\n        );\n");
    s.push_str("    Sse::new(stream).keep_alive(KeepAlive::default())\n}\n\n");

    // router
    s.push_str("pub fn router() -> Router<Pool> {\n");
    // 动态路径段：handler 用 Path<String> 提取通道名并在 channel_specs 里查表。
    // 两个已修的真实缺陷：
    // ① 为每个通道挂静态路径却仍用 Path 提取器 → axum 500
    //    (Wrong number of path arguments for Path)；
    // ② axum 0.7 的路径参数语法是 `:name`（`{name}` 是 axum 0.8；写错 → 404）。
    // 未知通道在 handler 内返回空流（不 500）。
    s.push_str("    Router::new().route(\"/api/live/:name\", axum::routing::get(subscribe))\n");
    s.push_str("}\n");
    s
}

/// 该实体是否有 live 通道（handler publish 钩子按需生成）。
fn has_live_channel(program: &Program, entity_table: &str) -> bool {
    program.lives.iter().any(|l| {
        let entity = program
            .apis
            .iter()
            .find(|a| a.name == l.api)
            .map(|a| a.entity.clone())
            .unwrap_or_default();
        crate::identity::to_snake_case(&crate::dsl::pluralize(&entity)) == entity_table
    })
}

/// 该 (entity, command) 是否有 @notify 订阅（handler fire 钩子按需生成）。
fn has_notify_rules(program: &Program, entity: &str, event: &str) -> bool {
    program.entities.iter().any(|e| {
        e.name == entity
            && e.notify
                .iter()
                .any(|n| n.event.as_deref().map(|ev| ev == event).unwrap_or(false))
    })
}

/// auth.rs：注册/登录/me + JWT 签发校验 + 保护中间件。
/// 租户维度（M10 收尾）的条件代码片段——te=false 时全空（产物零变化）。
struct TenantBlocks {
    tenant_let: String,
    /// issue_token 的 tenant 形参名（无租户维度时前缀 _ 避免 unused warning）
    token_param: String,
    /// login 里 issue_token 的第三参数（te=false 时不能用 user.tenant）
    login_arg: String,
    field_tail: String,
    login_tail: String,
    token_field: String,
    userrow: String,
    claim: String,
    extract: String,
    user_field: String,
    register_field: String,
    register_cols: String,
    register_vals: String,
    insert_bind: String,
    login_select: String,
    disabled_check: String,
    me: String,
}

fn tenant_block(te: bool) -> TenantBlocks {
    if !te {
        return TenantBlocks {
            // 无租户维度：register 仍需一个 Option<String> 绑定供 issue_token 使用
            tenant_let: "    let tenant: Option<String> = None;\n".into(),
            token_param: "_tenant".into(),
            login_arg: "None".into(),
            field_tail: String::new(),
            login_tail: String::new(),
            token_field: String::new(),
            userrow: String::new(),
            claim: String::new(),
            extract: String::new(),
            user_field: String::new(),
            register_field: String::new(),
            register_cols: String::new(),
            register_vals: String::new(),
            insert_bind: String::new(),
            login_select: String::new(),
            disabled_check: String::new(),
            me: String::new(),
        };
    }
    TenantBlocks {
        tenant_let: "    let tenant = input.tenant.clone();\n".into(),
        token_param: "tenant".into(),
        login_arg: "user.tenant.as_deref()".into(),
        field_tail: ", tenant".into(),
        login_tail: ", tenant: user.tenant".into(),
        token_field: "\n    pub tenant: Option<String>,".into(),
        userrow: "\n    pub tenant: Option<String>,\n    pub disabled: bool,".into(),
        claim: "        \"tenant\": tenant,\n".into(),
        extract: "    let tenant = data.claims[\"tenant\"].as_str().map(|s| s.to_string());\n".into(),
        user_field: "\n    pub tenant: Option<String>,".into(),
        register_field: "\n    #[serde(default)]\n    pub tenant: Option<String>,".into(),
        register_cols: ", tenant".into(),
        register_vals: ", ?".into(),
        insert_bind: "        .bind(&input.tenant)\n".into(),
        login_select: ", tenant, disabled".into(),
        disabled_check: "    if user.disabled {\n        return Err(AppError::Auth(\"账号已停用\".into()));\n    }\n".into(),
        me: "        \"tenant\": req_user.tenant,".into(),
    }
}

/// 租户管理（M10 收尾）：吊销 = 停用租户全部用户；迁移 = 业务行改绑新租户。
/// 授权：TENANT_ADMIN_TOKEN 环境变量 + x-admin-token 头匹配；未设置时管理面关闭（403）。
fn tenant_admin_block(te: bool, tenant_tables: &[(String, String)]) -> String {
    if !te {
        return String::new();
    }
    let mut items = String::new();
    for (t, c) in tenant_tables {
        items.push_str(&format!("        ({t:?}, {c:?}),\n"));
    }
    let mut f = String::new();
    f.push_str("\nconst TENANT_TABLES: [(&str, &str); ");
    f.push_str(&tenant_tables.len().to_string());
    f.push_str("] = [\n");
    f.push_str(&items);
    f.push_str("];\n\n");
    f.push_str("/// 授权：TENANT_ADMIN_TOKEN 环境变量 + x-admin-token 头匹配；未设置时管理面关闭（403）。\n");
    f.push_str("fn require_tenant_admin(headers: &axum::http::HeaderMap) -> AppResult<()> {\n");
    f.push_str(
        "    let expected = crate::secret::resolve(\"TENANT_ADMIN_TOKEN\").ok().flatten().unwrap_or_default();\n",
    );
    f.push_str("    if expected.is_empty() {\n        return Err(AppError::Forbidden(\n            \"tenant admin is disabled (TENANT_ADMIN_TOKEN not set)\".into(),\n        ));\n    }\n");
    f.push_str("    let provided = headers\n        .get(\"x-admin-token\")\n        .and_then(|v| v.to_str().ok())\n        .unwrap_or(\"\");\n");
    f.push_str("    if provided != expected {\n        return Err(AppError::Forbidden(\"invalid tenant admin token\".into()));\n    }\n    Ok(())\n}\n\n");
    f.push_str("#[derive(Debug, Deserialize)]\npub struct TenantTransferInput {\n    pub to: String,\n}\n\n");
    f.push_str("/// 吊销租户：停用该租户全部用户（登录与鉴权立即失效）。\n");
    f.push_str("pub async fn revoke_tenant(\n    State(db): State<Db>,\n    headers: axum::http::HeaderMap,\n    axum::extract::Path(tenant): axum::extract::Path<String>,\n) -> AppResult<Json<serde_json::Value>> {\n");
    f.push_str("    require_tenant_admin(&headers)?;\n");
    f.push_str("    let res = sqlx::query(\"UPDATE users SET disabled = 1 WHERE tenant = ?\")\n        .bind(&tenant)\n        .execute(db.as_ref())\n        .await\n        .map_err(AppError::Db)?;\n");
    f.push_str("    tracing::warn!(tenant = %tenant, disabled = res.rows_affected(), \"tenant revoked\");\n");
    f.push_str("    Ok(Json(serde_json::json!({ \"ok\": true, \"disabledUsers\": res.rows_affected() })))\n}\n\n");
    f.push_str("/// 迁移租户：把 from 租户的全部业务行（含 users）改绑到 to。\n");
    f.push_str("/// 数据面操作（可回滚：反向调用 transfer(to→from)）；审计日志不迁移。\n");
    f.push_str("pub async fn transfer_tenant(\n    State(db): State<Db>,\n    headers: axum::http::HeaderMap,\n    axum::extract::Path(tenant): axum::extract::Path<String>,\n    Json(input): Json<TenantTransferInput>,\n) -> AppResult<Json<serde_json::Value>> {\n");
    f.push_str("    require_tenant_admin(&headers)?;\n");
    f.push_str("    if input.to.trim().is_empty() || input.to == tenant {\n        return Err(AppError::Validation(vec![crate::error::FieldError::new(\n            \"to\",\n            \"target tenant must be a non-empty value different from the source tenant\",\n        )]));\n    }\n");
    f.push_str("    let mut moved = 0u64;\n");
    f.push_str("    for (table, col) in TENANT_TABLES {\n");
    f.push_str("        let sql = format!(\"UPDATE {table} SET {col} = ? WHERE {col} = ?\");\n");
    f.push_str("        let res = sqlx::query(&sql)\n            .bind(&input.to)\n            .bind(&tenant)\n            .execute(db.as_ref())\n            .await\n            .map_err(AppError::Db)?;\n        moved += res.rows_affected();\n    }\n");
    f.push_str(
        "    tracing::warn!(from = %tenant, to = %input.to, moved, \"tenant transferred\");\n",
    );
    f.push_str("    Ok(Json(serde_json::json!({ \"ok\": true, \"movedRows\": moved })))\n}\n");
    f
}

/// Secret Provider（RFC-0005 §18 / G-4 收口，T3 契约）：`env` + `file` 两种实现。
///
/// - `env`：环境变量 `{name}`；
/// - `file`：`{name}_FILE` 指向挂载文件（Docker/K8s secrets 约定）；
/// - `{name}_FILE` 已配置但不可读/为空 → Err（fail-closed，**不回显值**）；
/// - `resolve`：Ok(Some) 成功 / Ok(None) 未配置 / Err 已配置不可读；
/// - `require`：缺失/不可读 → `FORMA_CONFIG_MISSING`（fail-closed，不 panic）；
/// - **v1 不缓存**（每次读 env/file）：避免密钥驻留进程内存（RFC-0005 §18-4 的权衡结论）。
fn secret_rs() -> String {
    r#"// @generated by Forma (gen-rust) — do not edit manually.
//! Secret Provider（RFC-0005 §18）：`env` + `file` 两种实现（v1 不缓存）。

/// 读取密钥：`{name}_FILE`（挂载文件）优先，其次 `{name}` 环境变量。
///
/// - `Ok(Some(v))`：解析成功
/// - `Ok(None)`：未配置（调用方决定降级或 fail-closed）
/// - `Err(msg)`：已配置但不可读（fail-closed，**不回显值**）
pub fn resolve(name: &str) -> Result<Option<String>, String> {
    if let Ok(path) = std::env::var(format!("{name}_FILE")) {
        let path = path.trim();
        if !path.is_empty() {
            return match std::fs::read_to_string(path) {
                Ok(content) if !content.trim().is_empty() => Ok(Some(content.trim().to_string())),
                _ => Err(format!("{name} file provider unreadable (FORMA_SECRET_UNREADABLE)")),
            };
        }
    }
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Ok(Some(v)),
        _ => Ok(None),
    }
}

/// 必须存在的密钥；缺失/不可读 → fail-closed（错误码 FORMA_CONFIG_MISSING，不 panic）。
#[allow(dead_code)] // 公共入口：使用 require 的工程（fail-closed 密钥）启用
pub fn require(name: &str) -> Result<String, crate::error::AppError> {
    match resolve(name) {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(crate::error::AppError::Forbidden(format!(
            "{name} not configured (FORMA_CONFIG_MISSING)"
        ))),
        Err(msg) => Err(crate::error::AppError::Forbidden(msg)),
    }
}
"#
    .to_string()
}

fn auth_rs(program: &Program, app_name: &str) -> String {
    let auth = program.auth.as_ref().unwrap();
    let first_role = auth.roles.first().cloned().unwrap_or_else(|| "user".into());
    // 引导角色 = 第一个角色（管理员）；默认注册角色 = 最后一个角色（最低权限）。
    let last_role = auth.roles.last().cloned().unwrap_or_else(|| "user".into());
    let token_hours = auth.token_hours.to_string();
    let n_roles = auth.roles.len().to_string();
    // 租户维度（M10 收尾）：@tenant 字段存在 → JWT claim / CurrentUser / users 表 tenant 列
    let te = crate::dsl::tenant_enabled(program);
    let tenant_tables: Vec<(String, String)> = crate::dsl::tenant_fields(program)
        .iter()
        .map(|(e, _)| {
            (
                crate::identity::to_snake_case(&crate::dsl::pluralize(e)),
                crate::dsl::tenant_field_of(program, e)
                    .map(|f| crate::identity::to_snake_case(&f))
                    .unwrap_or_default(),
            )
        })
        .collect();
    let tenant_block = tenant_block(te);
    let tenant_admin = tenant_admin_block(te, &tenant_tables);
    // 操作级授权守卫——只在确实有 api 授权规则时生成（否则「never used」警告）。
    let require_role_block = if has_api_auth_rules(program) {
        r#"/// 操作级授权（服务端权威执行，ADR-0004）：调用方角色必须在允许列表内。
///
/// 前端按同一规则隐藏按钮只是体验层，**不构成**安全边界——本函数才是边界。
pub fn require_role(allowed: &[&str], user: &CurrentUser) -> AppResult<()> {
    if !allowed.contains(&user.role.as_str()) {
        return Err(AppError::Forbidden(format!(
            "role '{}' is not permitted for this operation",
            user.role
        )));
    }
    Ok(())
}
"#
        .to_string()
    } else {
        String::new()
    };
    let roles_list = auth
        .roles
        .iter()
        .map(|r| format!("\"{r}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let template = r#"// @generated by Forma (gen-rust) — do not edit manually.
//! 认证：注册/登录（argon2 哈希 + JWT）、Bearer 校验中间件、当前用户。

use axum::{
    extract::State,
    http::HeaderMap,
    middleware::Next,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::routes::Db;

pub const ROLES: [&str; __N_ROLES__] = [__ROLES_LIST__];

/// 角色由**服务端分配**（客户端传入的 `role` 一律忽略）：
/// - 库中还没有任何用户 → 引导角色（默认 = DSL 声明的第一个角色，通常是管理员）；
/// - 已有用户 → 默认角色（默认 = DSL 声明的最后一个角色，即最低权限）。
/// 管理员创建其他账号应走管理流程，不靠开放注册。
pub const BOOTSTRAP_ROLE: &str = "__BOOTSTRAP_ROLE__";
pub const DEFAULT_ROLE: &str = "__DEFAULT_ROLE__";

#[derive(Debug, Deserialize)]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
    /// 兼容旧客户端保留字段：服务端分配角色，此值一律忽略
    #[allow(dead_code)]
    #[serde(default)]
    pub role: Option<String>,
__TENANT_REGISTER_FIELD__
}

#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub username: String,
    pub role: String,
__TENANT_TOKEN_FIELD__
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    #[allow(dead_code)] // 行标识（当前仅用于扩展）
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: String,
__TENANT_USERROW__
}

fn jwt_secret() -> String {
    // Secret Provider（RFC-0005 §18）：
    // - 已配置（JWT_SECRET / JWT_SECRET_FILE）→ 使用配置值；
    // - 已配置但不可读 → 显式失败，不回显值；
    // - 未配置 → **fail-closed**：不使用任何内置默认密钥——产物源码里的公开默认值
    //   一旦成为生产签名密钥，任何读过源码的人都能伪造任意角色的 token。
    //   仅本地开发可显式设 FORMA_DEV_INSECURE_JWT=1 使用开发密钥。
    match crate::secret::resolve("JWT_SECRET") {
        Ok(Some(v)) => v,
        Ok(None) => {
            if std::env::var("FORMA_DEV_INSECURE_JWT").as_deref() == Ok("1") {
                "__APP__-dev-secret-change-me".into()
            } else {
                panic!(
                    "JWT_SECRET is not configured — refusing to start with a development secret. \
                     Set JWT_SECRET (or JWT_SECRET_FILE); for local development only set \
                     FORMA_DEV_INSECURE_JWT=1."
                );
            }
        }
        Err(msg) => panic!("{msg}"),
    }
}

/// 启动期校验（main 在 auth 模块存在时调用）：没有签名密钥就拒绝启动，
/// 而不是等到第一个请求才 panic。
pub fn assert_secret_configured() {
    let _ = jwt_secret();
}

/// 口令最小长度（生产默认 8；可用 AUTH_MIN_PASSWORD_LEN 覆盖，下限 6）。
/// 6 位下限对生产过弱（离线爆破成本极低），故默认收紧到 8。
fn min_password_len() -> usize {
    std::env::var("AUTH_MIN_PASSWORD_LEN")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n >= 6)
        .unwrap_or(8)
}

// ── 认证失败限速（按用户名 + 进程全局固定窗口；无额外依赖）──────────
// 多副本部署时为「每副本」独立计数——全局限速应在网关层做。
struct AttemptWindow {
    started: std::time::Instant,
    failures: u32,
}

static ATTEMPTS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, AttemptWindow>>,
> = std::sync::OnceLock::new();

fn attempts_map() -> &'static std::sync::Mutex<std::collections::HashMap<String, AttemptWindow>> {
    ATTEMPTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn rate_limit_per_min() -> u32 {
    std::env::var("AUTH_RATE_LIMIT_PER_MIN")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(20)
}

fn rate_limit_total() -> u32 {
    std::env::var("AUTH_RATE_LIMIT_TOTAL")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(200)
}

fn rate_limit_window_secs() -> u64 {
    std::env::var("AUTH_RATE_LIMIT_WINDOW_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(60)
}

/// 是否允许本次尝试（失败计数超限 → false；限速器自身故障时不阻断业务）。
fn allow_attempt(username: &str) -> bool {
    let per_min = rate_limit_per_min();
    let total = rate_limit_total();
    if per_min == 0 && total == 0 {
        return true;
    }
    let Ok(mut map) = attempts_map().lock() else {
        return true;
    };
    let now = std::time::Instant::now();
    let window = std::time::Duration::from_secs(rate_limit_window_secs());
    map.retain(|_, w| now.duration_since(w.started) < window);
    let sum: u32 = map.values().map(|w| w.failures).sum();
    if total > 0 && sum >= total {
        return false;
    }
    let entry = map
        .entry(username.to_string())
        .or_insert(AttemptWindow { started: now, failures: 0 });
    if now.duration_since(entry.started) >= window {
        entry.started = now;
        entry.failures = 0;
    }
    per_min == 0 || entry.failures < per_min
}

fn record_failure(username: &str) {
    if let Ok(mut map) = attempts_map().lock() {
        let now = std::time::Instant::now();
        let entry = map
            .entry(username.to_string())
            .or_insert(AttemptWindow { started: now, failures: 0 });
        if now.duration_since(entry.started)
            >= std::time::Duration::from_secs(rate_limit_window_secs())
        {
            entry.started = now;
            entry.failures = 0;
        }
        entry.failures += 1;
    }
}

fn clear_failures(username: &str) {
    if let Ok(mut map) = attempts_map().lock() {
        map.remove(username);
    }
}
// ── 限速结束 ──────────────────────────────────────────────

fn issue_token(username: &str, role: &str, __TENANT_PARAM__: Option<&str>) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": username,
        "role": role,
__TENANT_CLAIM__
        "exp": (chrono::Utc::now() + chrono::Duration::hours(__TOKEN_HOURS__)).timestamp(),
    });
    encode(&Header::default(), &claims, &EncodingKey::from_secret(jwt_secret().as_bytes()))
        .expect("sign jwt")
}

fn hash_password(pw: &str) -> String {
    use argon2::password_hash::{rand_core::OsRng, SaltString};
    use argon2::{Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("hash password")
        .to_string()
}

fn verify_password(pw: &str, hash: &str) -> bool {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(pw.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// 受保护路由的中间件：校验 Bearer token 并注入当前用户。
pub async fn require_auth(
    headers: HeaderMap,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Auth("missing bearer token".into()))?;
    let data = jsonwebtoken::decode::<serde_json::Value>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_secret().as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .map_err(|_| AppError::Auth("invalid token".into()))?;
    let username = data.claims["sub"].as_str().unwrap_or("").to_string();
    let role = data.claims["role"].as_str().unwrap_or("").to_string();
__TENANT_EXTRACT__
    let mut req = req;
    req.extensions_mut().insert(CurrentUser { username, role__TENANT_FIELD_TAIL__ });
    Ok(next.run(req).await)
}

/// 当前用户（由 require_auth 注入请求扩展）。
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub username: String,
    pub role: String,
__TENANT_USER__
}

//REQUIRE_ROLE_BLOCK

pub async fn register(
    State(db): State<Db>,
    Json(input): Json<RegisterInput>,
) -> AppResult<Json<TokenResponse>> {
    if !allow_attempt(&input.username) {
        return Err(AppError::RateLimited(rate_limit_window_secs()));
    }
    if input.username.trim().is_empty() || input.password.len() < min_password_len() {
        record_failure(&input.username);
        return Err(AppError::Validation(vec![crate::error::FieldError::new(
            "password",
            "用户名不能为空且密码至少 ".to_string() + &min_password_len().to_string() + " 位",
        )]));
    }
    // 角色由服务端分配（客户端传值一律忽略）：无用户 → 引导角色；否则默认（最低权限）角色。
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(db.as_ref())
        .await
        .map_err(AppError::Db)?;
    let role = if user_count == 0 { BOOTSTRAP_ROLE } else { DEFAULT_ROLE };
    if !ROLES.contains(&role) {
        return Err(AppError::Validation(vec![crate::error::FieldError::new(
            "role",
            format!(
                "configured roles must contain both bootstrap ({}) and default ({})",
                BOOTSTRAP_ROLE,
                DEFAULT_ROLE
            ),
        )]));
    }
__TENANT_LET__    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = ?")
            .bind(&input.username)
            .fetch_optional(db.as_ref())
            .await
            .map_err(AppError::Db)?;
    if exists.is_some() {
        record_failure(&input.username);
        return Err(AppError::Conflict("用户名已存在".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at__TENANT_COLS__) VALUES (?, ?, ?, ?, ?__TENANT_VALS__)")        .bind(&id)
        .bind(&input.username)
        .bind(hash_password(&input.password))
        .bind(&role)
        .bind(chrono::Utc::now())
__TENANT_INSERT_BIND__
        .execute(db.as_ref())
        .await
        .map_err(AppError::Db)?;
    let token = issue_token(&input.username, &role, tenant.as_deref());
    Ok(Json(TokenResponse {
        token,
        username: input.username,
        role: role.to_string()__TENANT_FIELD_TAIL__,
    }))
}

pub async fn login(
    State(db): State<Db>,
    Json(input): Json<LoginInput>,
) -> AppResult<Json<TokenResponse>> {
    if !allow_attempt(&input.username) {
        return Err(AppError::RateLimited(rate_limit_window_secs()));
    }
    let row: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, role__TENANT_SELECT__ FROM users WHERE username = ?",
    )
    .bind(&input.username)
    .fetch_optional(db.as_ref())
    .await
    .map_err(AppError::Db)?;
    let Some(user) = row else {
        record_failure(&input.username);
        return Err(AppError::Auth("用户名或密码错误".into()));
    };
    if !verify_password(&input.password, &user.password_hash) {
        record_failure(&input.username);
        return Err(AppError::Auth("用户名或密码错误".into()));
    }
    clear_failures(&input.username);
__DISABLED_CHECK__    let token = issue_token(&user.username, &user.role, __TENANT_LOGIN_ARG__);
    Ok(Json(TokenResponse { token, username: user.username, role: user.role__TENANT_LOGIN_TAIL__ }))
}

/// 当前用户信息（要求已登录）。
pub async fn me(req_user: axum::Extension<CurrentUser>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "username": req_user.username,
        "role": req_user.role,
__TENANT_ME__
    }))
}
//REQUIRE_TENANT_ADMIN_BLOCK
"#;
    template
        .replace("__N_ROLES__", &n_roles)
        .replace("__ROLES_LIST__", &roles_list)
        .replace("__TOKEN_HOURS__", &token_hours)
        .replace("__BOOTSTRAP_ROLE__", &first_role)
        .replace("__DEFAULT_ROLE__", &last_role)
        .replace("__FIRST_ROLE__", &first_role)
        .replace("__APP__", app_name)
        .replace("//REQUIRE_ROLE_BLOCK\n", &require_role_block)
        .replace("__TENANT_TABLES__\n", &tenant_admin)
        .replace("__TENANT_LET__", tenant_block.tenant_let.as_str())
        .replace("__TENANT_PARAM__", tenant_block.token_param.as_str())
        .replace("__TENANT_LOGIN_ARG__", tenant_block.login_arg.as_str())
        .replace("__TENANT_FIELD_TAIL__", tenant_block.field_tail.as_str())
        .replace("__TENANT_LOGIN_TAIL__", tenant_block.login_tail.as_str())
        .replace("__TENANT_TOKEN_FIELD__", tenant_block.token_field.as_str())
        .replace("__TENANT_USERROW__", tenant_block.userrow.as_str())
        .replace("__TENANT_CLAIM__", tenant_block.claim.as_str())
        .replace("__TENANT_EXTRACT__", tenant_block.extract.as_str())
        .replace("__TENANT_USER__", tenant_block.user_field.as_str())
        .replace(
            "__TENANT_REGISTER_FIELD__",
            tenant_block.register_field.as_str(),
        )
        .replace("__TENANT_COLS__", tenant_block.register_cols.as_str())
        .replace("__TENANT_VALS__", tenant_block.register_vals.as_str())
        .replace("__TENANT_INSERT_BIND__", tenant_block.insert_bind.as_str())
        .replace("__TENANT_SELECT__", tenant_block.login_select.as_str())
        .replace("__DISABLED_CHECK__", tenant_block.disabled_check.as_str())
        .replace("__TENANT_ME__", tenant_block.me.as_str())
        .replace("//REQUIRE_TENANT_ADMIN_BLOCK\n", &tenant_admin)
}

/// audit.rs：审计钩子 = 结构化日志 + audit_logs 表落库（actor 待 auth 模块接入）。
fn audit_rs() -> String {
    r#"// @generated by Forma (gen-rust) — do not edit manually.
//! 命令审计：结构化日志（可观测）+ audit_logs 表（可追溯）。
//! actor 在 auth 模块接入后自动填充当前用户。

use serde::Serialize;
use sqlx::MySqlPool;
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditLogRow {
    pub id: String,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub actor: Option<String>,
    /// 命令幂等键（spec:protocols §10.5）：来自客户端 `Idempotency-Key` 头
    pub idempotency_key: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn log_action(
    db: &MySqlPool,
    action: &str,
    entity: &str,
    entity_id: &str,
    actor: Option<&str>,
    idempotency_key: Option<&str>,
    payload: Option<serde_json::Value>,
) -> sqlx::Result<()> {
    tracing::info!(action, entity, id = entity_id, actor = actor.unwrap_or("anonymous"), idempotency_key = idempotency_key.unwrap_or(""), "forma.audit");
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO audit_logs (id, action, entity, entity_id, actor, idempotency_key, payload, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(id)
        .bind(action)
        .bind(entity)
        .bind(entity_id)
        .bind(actor)
        .bind(idempotency_key)
        .bind(payload)
        .bind(chrono::Utc::now())
        .execute(db)
        .await?;
    Ok(())
}

pub async fn list_audit_logs(db: &MySqlPool, page: u64, page_size: u64) -> sqlx::Result<Vec<AuditLogRow>> {
    let page_size = page_size.clamp(1, 200);
    let offset = (page.max(1) - 1) * page_size;
    sqlx::query_as::<_, AuditLogRow>(
        "SELECT id, action, entity, entity_id, actor, idempotency_key, payload, created_at FROM audit_logs ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(db)
    .await
}
"#
    .to_string()
}

fn check_rule_code(program: &Program, entity_name: &str, input: &str, is_create: bool) -> String {
    let mut out = String::new();
    let Some(entity) = super::find_entity(program, entity_name) else {
        return out;
    };
    let mut idx = 0usize;
    for rules in &program.rules {
        for check in &rules.checks {
            if check.entity != *entity_name {
                continue;
            }
            // Create 输入包含所有非 @id 字段；Update 同。readonly 字段取 default 兜底。
            let fields = expr_fields(&check.expr);
            let mut bindings = String::new();
            let expr_code = expr_to_rust(
                &check.expr,
                program,
                entity,
                input,
                &mut bindings,
                &mut idx,
                is_create,
            );
            let _ = fields;
            out.push_str(&bindings);
            let msg = pick_message(&check.message, "check failed");
            out.push_str(&format!(
                "    if !({expr_code}) {{ errs.push(FieldError::new(\"__check\", \"{}\")); }}\n",
                escape_rust(&msg)
            ));
        }
    }
    let _ = entity;
    out
}

fn display_column(program: &Program, entity_name: &str) -> String {
    let Some(entity) = super::find_entity(program, entity_name) else {
        return "id".into();
    };
    for f in &entity.fields {
        if matches!(f.type_name.as_str(), "String" | "Email") && super::has_attr(f, "required") {
            return crate::identity::to_snake_case(&f.name);
        }
    }
    "id".into()
}

fn enum_membership_checks(
    program: &Program,
    entity: &EntityDecl,
    input: &str,
    _is_create: bool,
) -> String {
    let mut out = String::new();
    for field in &entity.fields {
        if super::is_enum(program, &field.type_name) {
            let Some(e) = program.enums.iter().find(|x| x.name == field.type_name) else {
                continue;
            };
            let variants: Vec<String> = e
                .variants
                .iter()
                .map(|v| format!("\"{}\".to_string()", v.name))
                .collect();
            let col = snake(&field.name);
            out.push_str(&format!(
                "    let allowed_{col} = [{}];\n",
                variants.join(", ")
            ));
            if field.optional || super::has_attr(field, "default") {
                out.push_str(&format!(
                    "    if let Some(v) = &{input}.{col} {{ if !allowed_{col}.contains(v) {{ errs.push(FieldError::new(\"{col}\", \"must be one of {}\")); }} }}\n",
                    e.variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join("|")
                ));
            } else {
                out.push_str(&format!(
                    "    if !allowed_{col}.contains(&{input}.{col}) {{ errs.push(FieldError::new(\"{col}\", \"must be one of {}\")); }}\n",
                    e.variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join("|")
                ));
            }
        }
    }
    out
}

fn expr_fields(expr: &Expr) -> Vec<Vec<String>> {
    match expr {
        Expr::Ref { path, .. } => vec![path.clone()],
        Expr::Bin(_, a, b) => {
            let mut v = expr_fields(a);
            v.extend(expr_fields(b));
            v
        }
        Expr::Not(a) => expr_fields(a),
        Expr::Call(_, args) => {
            let mut v = Vec::new();
            for a in args {
                v.extend(expr_fields(a));
            }
            v
        }
        Expr::List(items) => {
            let mut v = Vec::new();
            for a in items {
                v.extend(expr_fields(a));
            }
            v
        }
        _ => vec![],
    }
}

fn expr_to_rust(
    expr: &Expr,
    program: &Program,
    entity: &EntityDecl,
    input: &str,
    bindings: &mut String,
    idx: &mut usize,
    is_create: bool,
) -> String {
    expr_to_rust_mode(
        expr, program, entity, input, bindings, idx, is_create, false,
    )
}

#[allow(clippy::too_many_arguments)]
fn expr_to_rust_mode(
    expr: &Expr,
    program: &Program,
    entity: &EntityDecl,
    input: &str,
    bindings: &mut String,
    idx: &mut usize,
    is_create: bool,
    plain: bool,
) -> String {
    match expr {
        Expr::Num(v) => {
            if v.fract() == 0.0 {
                format!("{}", *v as i64)
            } else {
                format!("{v}")
            }
        }
        Expr::Str(s) => format!("\"{}\".to_string()", escape_rust(s)),
        Expr::Bool(b) => b.to_string(),
        Expr::Null => "Default::default()".to_string(),
        Expr::Ref { path, old: _ } => {
            let field = entity.fields.iter().find(|f| f.name == path[0]);
            let Some(field) = field else {
                return "Default::default()".to_string();
            };
            let col = snake(&field.name);
            let readonly = super::has_attr(field, "readonly");
            // 多级路径（跨关系）v1 不在输入校验中支持 JOIN 值；编译为输入自身
            let _ = path.len() > 1;
            let accessor = format!("{input}.{col}");
            let is_option =
                !plain && (field.optional || super::has_attr(field, "default") || readonly);
            if !is_option {
                return accessor;
            }
            // Option 字段：绑定临时变量并给出兜底
            *idx += 1;
            let var = format!("__r{idx}");
            let _base_ty_known = true;
            let ty = String::new();
            let base_ty = match field.type_name.as_str() {
                "Decimal" => "Decimal".to_string(),
                "Int" => "i64".to_string(),
                "Timestamp" => "chrono::DateTime<chrono::Utc>".to_string(),
                _ => "String".to_string(),
            };
            let fallback = if readonly {
                let attr = super::field_attr(field, "default").unwrap();
                match attr.args.first().map(|s| s.as_str()) {
                    Some("now") => "chrono::Utc::now()".to_string(),
                    Some("uuid") => "Uuid::new_v4()".to_string(),
                    Some(lit) => literal_for(&base_ty, lit.parse().unwrap_or(0.0)),
                    _ => typed_fallback(program, &field.type_name),
                }
            } else {
                typed_fallback(program, &field.type_name)
            };
            let _ = (ty, is_create);
            bindings.push_str(&format!(
                "    let {var} = {accessor}.unwrap_or({fallback});\n"
            ));
            var
        }
        Expr::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|i| {
                    expr_to_rust_mode(i, program, entity, input, bindings, idx, is_create, plain)
                })
                .collect();
            format!("vec![{}]", parts.join(", "))
        }
        Expr::Call(name, args) => {
            let rendered: Vec<String> = args
                .iter()
                .map(|a| {
                    expr_to_rust_mode(a, program, entity, input, bindings, idx, is_create, plain)
                })
                .collect();
            match name.as_str() {
                "matches" => format!(
                    "{}.matches(&regex::Regex::new({}).unwrap())",
                    rendered.first().cloned().unwrap_or_default(),
                    rendered
                        .get(1)
                        .cloned()
                        .unwrap_or_else(|| "\"\".to_string()".into())
                ),
                _ => "true".to_string(),
            }
        }
        Expr::Not(inner) => format!(
            "!({})",
            expr_to_rust_mode(inner, program, entity, input, bindings, idx, is_create, plain)
        ),
        Expr::Bin(op, a, b) => {
            let l = expr_to_rust_mode(a, program, entity, input, bindings, idx, is_create, plain);
            let r = expr_to_rust_mode(b, program, entity, input, bindings, idx, is_create, plain);
            let op_str = match op {
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::In => {
                    return format!("{}.contains(&{})", r, l);
                }
            };
            format!("({l} {op_str} {r})")
        }
    }
}

/// 按字段类型给出带类型注解的兜底表达式（避免 Default 推断失败）。
fn field_constraint_checks(
    program: &Program,
    entity: &EntityDecl,
    input: &str,
    is_create: bool,
) -> String {
    let mut out = String::new();
    // 收集该实体的字段规则
    let mut rules_by_field: std::collections::HashMap<String, Vec<&FieldConstraint>> =
        Default::default();
    for rules in &program.rules {
        for fr in &rules.field_rules {
            if fr.entity == entity.name {
                rules_by_field
                    .entry(fr.field.clone())
                    .or_default()
                    .extend(fr.constraints.iter());
            }
        }
    }

    for field in &entity.fields {
        if super::has_attr(field, "id") {
            continue;
        }
        let col = snake(&field.name);
        let is_relation = super::is_relation(program, &field.type_name);
        // @required（关系字段 NOT NULL 在 repo 层保证；字符串/数值必填校验在这里）
        if !is_relation
            && !field.optional
            && matches!(field.type_name.as_str(), "String" | "Email" | "Text")
        {
            out.push_str(&format!(
                "    if {input}.{col}.trim().is_empty() {{ errs.push(FieldError::new(\"{col}\", \"必填 / required\")); }}\n"
            ));
        }
        // @min/@max 属性
        for attr in &field.attrs {
            if matches!(attr.name.as_str(), "min" | "max") {
                if let Some(v) = attr.args.first().and_then(|a| a.parse::<f64>().ok()) {
                    let cmp = if attr.name == "min" { "<" } else { ">" };
                    out.push_str(&numeric_check(
                        &col,
                        &rust_type(program, field),
                        v,
                        cmp,
                        &format!(
                            "{} must be {} {}",
                            col,
                            if attr.name == "min" { ">=" } else { "<=" },
                            v
                        ),
                    ));
                }
            }
        }
        // rules.field 约束
        if let Some(constraints) = rules_by_field.get(&field.name) {
            for c in constraints {
                match c {
                    FieldConstraint::Required(msg) => {
                        if matches!(field.type_name.as_str(), "String" | "Email" | "Text") {
                            out.push_str(&format!(
                                "    if {input}.{col}.trim().is_empty() {{ errs.push(FieldError::new(\"{col}\", \"{}\")); }}\n",
                                escape_rust(&pick_message(msg, "required"))
                            ));
                        }
                    }
                    FieldConstraint::Min(v, msg) => {
                        out.push_str(&numeric_check(
                            &col,
                            &rust_type(program, field),
                            *v,
                            "<",
                            &pick_message(msg, "value too small"),
                        ));
                    }
                    FieldConstraint::Max(v, msg) => {
                        out.push_str(&numeric_check(
                            &col,
                            &rust_type(program, field),
                            *v,
                            ">",
                            &pick_message(msg, "value too large"),
                        ));
                    }
                    FieldConstraint::Length(min, max, msg) => {
                        if matches!(field.type_name.as_str(), "String" | "Email" | "Text") {
                            let mut cond = String::new();
                            if let Some(mn) = min {
                                cond.push_str(&format!(
                                    "{input}.{col}.trim().chars().count() < {mn}"
                                ));
                            }
                            if let Some(mx) = max {
                                if !cond.is_empty() {
                                    cond.push_str(" || ");
                                }
                                cond.push_str(&format!(
                                    "{input}.{col}.trim().chars().count() > {mx}"
                                ));
                            }
                            if !cond.is_empty() {
                                out.push_str(&format!(
                                    "    if {cond} {{ errs.push(FieldError::new(\"{col}\", \"{}\")); }}\n",
                                    escape_rust(&pick_message(msg, "invalid length"))
                                ));
                            }
                        }
                    }
                }
            }
        }
        let _ = is_create;
    }
    out
}

fn input_fields(program: &Program, entity: &EntityDecl, is_create: bool) -> String {
    let mut out = String::new();
    for field in &entity.fields {
        if super::has_attr(field, "id") {
            continue; // id 由服务端生成（update 走路径参数）
        }
        let readonly = super::has_attr(field, "readonly");
        if readonly && !is_create {
            // update 仍带 readonly 字段（Option，供规则校验引用），repo 忽略
        }
        let is_relation = super::is_relation(program, &field.type_name);
        let base = if is_relation {
            "String".to_string()
        } else {
            let rt = rust_type(program, field);
            // 只剥一层 Option<T>（DateTime<Utc> 等内部泛型的 `>` 不能被误剥）
            match rt
                .strip_prefix("Option<")
                .and_then(|inner| inner.strip_suffix('>'))
            {
                Some(inner) => inner.to_string(),
                None => rt,
            }
        };
        let has_default = super::has_attr(field, "default");
        let required = super::has_attr(field, "required");
        let is_option = field.optional || has_default || readonly;
        if is_relation {
            let t = if field.optional || (has_default && !required) {
                format!("Option<{base}>")
            } else {
                base
            };
            out.push_str(&format!("    pub {}: {},\n", snake(&field.name), t));
            continue;
        }
        let t = match base.as_str() {
            "Option<T>" => unreachable!(),
            "DateTime<Utc>" | "NaiveDate" => {
                if is_option || (is_create && has_default) || readonly {
                    format!("Option<{base}>")
                } else {
                    base.clone()
                }
            }
            _ => {
                if is_option {
                    format!("Option<{base}>")
                } else {
                    base.clone()
                }
            }
        };
        out.push_str(&format!("    pub {}: {},\n", snake(&field.name), t));
    }
    out
}

/// 目标实体的展示列：第一个 required 的 String/Email 字段，否则 id。
fn literal_for(ty: &str, v: f64) -> String {
    match ty {
        // Decimal 与整数字面量直接比较会产生**多个候选 impl**（i32/i64/…）→ 类型不明确。
        // 必须显式类型化（与访问条件编译器的 decimal_literal 同一口径）。
        "Decimal" => decimal_literal(v),
        "f64" => format!("{v}"),
        "i64" => format!("{}", v as i64),
        _ => format!("{v}"),
    }
}

/// 枚举字段：值必须 ∈ variants。
fn models_rs(program: &Program) -> String {
    let mut out = String::from("// @generated by Forma (gen-rust) — do not edit manually.\n");
    out.push_str(&uses_header(program));
    // 模型层只承载文本 UUID（CHAR(36)）→ 不需要 uuid crate
    out.push_str("use serde::{Deserialize, Serialize};\nuse sqlx::FromRow;\n\n");

    for entity in &program.entities {
        let struct_name = &entity.name;
        out.push_str(&format!(
            "#[derive(Debug, Serialize, FromRow)]\npub struct {struct_name} {{\n"
        ));
        for field in &entity.fields {
            let (name, col_type) = if super::is_relation(program, &field.type_name) {
                (
                    format!("{}_id", snake(&field.name)),
                    if field.optional {
                        "Option<String>".to_string()
                    } else {
                        "String".to_string()
                    },
                )
            } else {
                (snake(&field.name), rust_type(program, field))
            };
            out.push_str(&format!("    pub {name}: {col_type},\n"));
        }
        out.push_str("}\n\n");

        // 列表行：关系字段带显示名列
        let mut row_fields = String::new();
        for field in &entity.fields {
            let (name, col_type) = if super::is_relation(program, &field.type_name) {
                (
                    format!("{}_id", snake(&field.name)),
                    if field.optional {
                        "Option<String>".to_string()
                    } else {
                        "String".to_string()
                    },
                )
            } else {
                (snake(&field.name), rust_type(program, field))
            };
            row_fields.push_str(&format!("    pub {name}: {col_type},\n"));
            if super::is_relation(program, &field.type_name) {
                row_fields.push_str(&format!(
                    "    pub {}_name: Option<String>,\n",
                    snake(&field.name)
                ));
            }
        }
        out.push_str(&format!(
            "#[derive(Debug, Serialize, FromRow)]\n#[allow(dead_code)] // 无 list API 的实体仍保留 ListRow 契约\npub struct {struct_name}ListRow {{\n{row_fields}}}\n\n"
        ));

        // Create / Update 输入
        out.push_str(&format!(
            "#[derive(Debug, Deserialize)]\n#[allow(dead_code)] // readonly/契约字段可能只参与 JSON 与规则引用\npub struct Create{struct_name} {{\n{}\n}}\n\n",
            input_fields(program, entity, true)
        ));
        out.push_str(&format!(
            "#[derive(Debug, Deserialize)]\n#[allow(dead_code)] // readonly/契约字段可能只参与 JSON 与规则引用\npub struct Update{struct_name} {{\n{}\n}}\n\n",
            input_fields(program, entity, false)
        ));
    }
    out
}

fn numeric_check(col: &str, ty: &str, v: f64, cmp: &str, message: &str) -> String {
    let access = format!("input.{col}");
    if ty.starts_with("Option<") {
        let inner = ty
            .trim_start_matches("Option<")
            .trim_end_matches('>')
            .to_string();
        let literal = literal_for(&inner, v);
        format!(
            "    if let Some(v) = {access} {{ if v {cmp} {literal} {{ errs.push(FieldError::new(\"{col}\", \"{}\")); }} }}\n",
            escape_rust(message)
        )
    } else {
        let literal = literal_for(ty, v);
        format!(
            "    if {access} {cmp} {literal} {{ errs.push(FieldError::new(\"{col}\", \"{}\")); }}\n",
            escape_rust(message)
        )
    }
}

pub fn rust_type(_program: &Program, field: &FieldDecl) -> String {
    let base = match field.type_name.as_str() {
        // Uuid 列在 MySQL 侧是 CHAR(36)（gen-mysql）→ sqlx 的 Uuid 解码仅支持 16 字节二进制，
        // 故 Rust 侧统一用 String 承载文本 UUID（API JSON 不变：仍是带连字符的字符串）
        "Uuid" => "String".to_string(),
        "String" | "Email" | "Text" => "String".to_string(),
        "Int" => "i64".to_string(),
        "Float" => "f64".to_string(),
        "Decimal" => "Decimal".to_string(),
        "Bool" => "bool".to_string(),
        "Timestamp" => "DateTime<Utc>".to_string(),
        "Date" => "NaiveDate".to_string(),
        "Json" => "serde_json::Value".to_string(),
        // 枚举与关系字段在 Rust 侧都是文本（枚举存变体名，关系存 CHAR(36) 文本 UUID）
        _ => "String".to_string(),
    };
    if field.optional {
        format!("Option<{base}>")
    } else {
        base
    }
}

fn strip_outer_parens(s: &str) -> String {
    let t = s.trim();
    if !t.starts_with('(') || !t.ends_with(')') {
        return t.to_string();
    }
    let inner = &t[1..t.len() - 1];
    let mut depth = 0i32;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i + 1 != inner.len() {
                    return t.to_string(); // 括号未包住整个表达式
                }
            }
            _ => {}
        }
    }
    inner.to_string()
}

fn uses_header(program: &Program) -> String {
    let mut needs_decimal = false;
    let mut needs_date = false;
    let mut needs_naive = false;
    for e in &program.entities {
        for f in &e.fields {
            match f.type_name.as_str() {
                "Decimal" => needs_decimal = true,
                "Timestamp" => needs_date = true,
                "Date" => needs_naive = true,
                _ => {}
            }
        }
    }
    let mut out = String::new();
    if needs_decimal {
        out.push_str("use rust_decimal::Decimal;\n");
    }
    if needs_date {
        out.push_str("use chrono::{DateTime, Utc};\n");
    }
    if needs_naive {
        out.push_str("use chrono::NaiveDate;\n");
    }
    out
}

fn validators_rs(program: &Program) -> String {
    let mut out = String::from("// @generated by Forma (gen-rust) — do not edit manually.\n");
    // validators 只用 fully-qualified 的 chrono/decimal 路径（rust_decimal::Decimal::try_from），
    // → 不导入 chrono/Uuid/Decimal 类型（否则部分工程报 unused_imports——踩过）
    let any_validate_op = program
        .apis
        .iter()
        .any(|a| a.operations.iter().any(|o| o == "create" || o == "update"));
    if !any_validate_op {
        out.push_str("// （无 create/update 操作 → 不生成校验器）\n");
        return out;
    }
    out.push_str("use crate::error::FieldError;\n");
    out.push_str("use crate::models::*;\n\n");

    for entity in &program.entities {
        let name = &entity.name;
        let snake_name = snake(&entity.name);
        let api = program.apis.iter().find(|a| a.entity == *name);
        let ops = api.map(|a| a.operations.clone()).unwrap_or_default();
        for (kind, struct_name, is_create) in
            [("create", "Create", true), ("update", "Update", false)]
        {
            if !ops.iter().any(|o| o == kind) {
                continue; // 未声明该 op → 不生成对应校验器（避免 dead_code）
            }
            let mut body = String::new();
            body.push_str(&field_constraint_checks(
                program, entity, "input", is_create,
            ));
            body.push_str(&enum_membership_checks(program, entity, "input", is_create));
            body.push_str(&check_rule_code(program, name, "input", is_create));
            // 无校验项时 input 不被读取 → 改名 _input 消除 unused variable 警告
            let arg = if body.contains("input.") {
                "input"
            } else {
                "_input"
            };
            let mut_kw = if body.contains("errs.push") {
                "mut "
            } else {
                ""
            };
            out.push_str(&format!(
                "pub fn validate_{kind}_{snake_name}({arg}: &{struct_name}{name}) -> Vec<FieldError> {{\n    let {mut_kw}errs: Vec<FieldError> = Vec::new();\n"
            ));
            out.push_str(&body);
            out.push_str("    errs\n}\n\n");
        }
    }
    out
}

/// 字段级约束（rules.field + DSL 属性 @min/@max）→ Rust 校验。
fn access_needs_row(cond: &Expr) -> bool {
    crate::dsl::expr_refs(cond)
        .iter()
        .any(|p| !(p.len() == 1 && crate::dsl::ACCESS_VARS.contains(&p[0].as_str())))
}

/// 文件上传（M9/P1）：Attachment 字段 + @storage 属性的服务端执行。
///
/// 语义：Attachment 字段保存文件引用（URL / 相对路径）。上传走统一端点
/// `POST /api/uploads`（multipart，字段名 `file`），按 `field` 查询参数选择
/// 字段级 @storage 配置（缺省 = 第一个 Attachment 字段的配置）。
///
/// 驱动策略：
/// - `local`：真实落盘 `./uploads/<uuid>.<ext>`，`GET /api/uploads/:name` 读回；
/// - `s3`：aws-sdk-s3 上传（AWS_REGION + AWS_* 凭据环境变量）；
/// - `oss`：OSS-HMAC-SHA1 签名上传（OSS_* 环境变量）。
///   后两者为条件依赖（仅当存在对应 provider 时引入）。
fn storage_rs(program: &Program) -> String {
    let mut entries = String::new();
    for entity in &program.entities {
        for f in &entity.fields {
            if f.type_name != "Attachment" {
                continue;
            }
            let attr = f.attrs.iter().find(|a| a.name == "storage");
            let provider = attr
                .and_then(|a| a.args.first())
                .map(|s| s.as_str())
                .unwrap_or("local");
            let bucket = attr
                .and_then(|a| a.args.get(1))
                .map(|s| s.as_str())
                .unwrap_or("");
            entries.push_str(&format!(
                "    StorageField {{ entity: \"{}\", field: \"{}\", provider: \"{}\", bucket: \"{}\" }},\n",
                entity.name, f.name, provider, bucket
            ));
        }
    }
    let mut out = String::from(
        r#"// @generated by Forma (gen-rust) — do not edit manually.
//! 文件上传（M9/P1）：Attachment 字段 + @storage 属性的服务端执行。
//!
//! - `POST /api/uploads`（multipart，字段 `file`；可选 `?field=Entity.field` 选存储配置）
//! - `GET /api/uploads/:name` 读回（local）；s3/oss 返回直链 URL
//! - local 落盘 `./uploads/<uuid>.<ext>`；s3 经 aws-sdk-s3 上传；oss 经 OSS-HMAC-SHA1 签名上传（均为条件依赖）

use axum::{
    extract::{Multipart, Path as AxumPath, Query},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

const UPLOAD_DIR: &str = "./uploads";

/// (entity, field, provider, bucket) —— 由 DSL `@storage` 静态展开（运行时零 DSL 依赖）
pub struct StorageField {
    pub entity: &'static str,
    pub field: &'static str,
    pub provider: &'static str,
    pub bucket: &'static str,
}

pub static STORAGE_FIELDS: &[StorageField] = &[
"#,
    );
    out.push_str(&entries);
    out.push_str(
        r#"];

#[derive(Deserialize)]
pub struct UploadQuery {
    pub field: Option<String>,
}

/// 驱动策略：local 落盘；s3/oss 为条件依赖的真实驱动（凭据经环境变量注入；
/// 未声明的 provider 零产物、零依赖）。
async fn save_upload(
    field: &StorageField,
    filename: &str,
    data: &[u8],
) -> Result<String, crate::error::AppError> {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    if !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(crate::error::AppError::Validation(vec![
            crate::error::FieldError::new("file", "invalid file extension"),
        ]));
    }
    match field.provider {
        "local" => {
            let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            tokio::fs::create_dir_all(UPLOAD_DIR)
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("create upload dir: {e}")))?;
            let path = std::path::Path::new(UPLOAD_DIR).join(&name);
            tokio::fs::write(&path, data)
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("write upload: {e}")))?;
            Ok(format!("/api/uploads/{name}"))
        }
__S3_ARM____OSS_ARM__        other => Err(crate::error::AppError::Internal(format!(
            "unknown storage provider: {other}"
        ))),
    }
}

__OSS_HELPER__

pub async fn upload_handler(
    Query(q): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, crate::error::AppError> {
    let target = q
        .field
        .as_deref()
        .and_then(|f| {
            STORAGE_FIELDS.iter().find(|s| {
                format!("{}.{}", s.entity, s.field) == f
                    || format!("{}.{}", s.entity.to_lowercase(), s.field) == f
            })
        })
        .or_else(|| STORAGE_FIELDS.first());
    let Some(target) = target else {
        return Err(crate::error::AppError::Internal(
            "no storage fields declared".into(),
        ));
    };
    while let Some(item) = multipart
        .next_field()
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("multipart error: {e}")))?
    {
        let Some(filename) = item.file_name().map(|s| s.to_string()) else {
            continue;
        };
        let data = item
            .bytes()
            .await
            .map_err(|e| crate::error::AppError::Internal(format!("read upload body: {e}")))?;
        let url = save_upload(target, &filename, &data).await?;
        return Ok(Json(serde_json::json!({
            "url": url,
            "provider": target.provider,
            "bytes": data.len(),
        })));
    }
    Err(crate::error::AppError::Validation(vec![
        crate::error::FieldError::new("file", "multipart body must contain a file"),
    ]))
}

pub async fn download_handler(
    AxumPath(name): AxumPath<String>,
) -> Result<(StatusCode, [(&'static str, &'static str); 1], Vec<u8>), crate::error::AppError> {
    // 路径穿越防护：只接受纯文件名（uuid.ext）
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(crate::error::AppError::NotFound);
    }
    let path = std::path::Path::new(UPLOAD_DIR).join(&name);
    let data = tokio::fs::read(&path)
        .await
        .map_err(|_| crate::error::AppError::NotFound)?;
    Ok((
        StatusCode::OK,
        [("content-type", "application/octet-stream")],
        data,
    ))
}

pub fn router() -> Router<crate::routes::Db> {
    Router::new()
        .route("/api/uploads", post(upload_handler))
        .route("/api/uploads/:name", get(download_handler))
}
"#,
    );
    // 条件驱动：仅当存在对应 provider 声明时引入 s3 / oss 代码（依赖也同步条件化）
    let providers: Vec<&str> = program
        .entities
        .iter()
        .flat_map(|e| e.fields.iter())
        .filter(|f| f.type_name == "Attachment")
        .filter_map(|f| f.attrs.iter().find(|a| a.name == "storage"))
        .filter_map(|a| a.args.first().map(|s| s.as_str()))
        .collect();
    let has_s3 = providers.contains(&"s3");
    let has_oss = providers.contains(&"oss");
    let s3_arm = if has_s3 {
        r#"        "s3" => {
            let key = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            let region = std::env::var("AWS_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string());
            // 显式 region + 环境变量凭据（避免 IMDS 探测）；桶来自 @storage 声明
            let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(region.clone()))
                .load()
                .await;
            let client = aws_sdk_s3::Client::new(&cfg);
            client
                .put_object()
                .bucket(field.bucket)
                .key(key.as_str())
                .body(aws_sdk_s3::primitives::ByteStream::from(data.to_vec()))
                .content_type("application/octet-stream")
                .send()
                .await
                .map_err(|e| {
                    crate::error::AppError::Internal(format!("s3 put failed: {e}"))
                })?;
            let base = std::env::var("S3_PUBLIC_BASE")
                .ok()
                .filter(|s| !s.trim().is_empty());
            Ok(match base {
                Some(b) => format!("{b}/{key}"),
                None => format!("https://{}.s3.{}.amazonaws.com/{}", field.bucket, region, key),
            })
        }
"#
        .to_string()
    } else {
        String::new()
    };
    let oss_arm = if has_oss {
        r#"        "oss" => {
            // 真实驱动：阿里云 OSS（OSS-HMAC-SHA1 签名；OSS_* 环境变量）
            let key = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            let endpoint = std::env::var("OSS_ENDPOINT")
                .unwrap_or_else(|_| "oss-cn-hangzhou.aliyuncs.com".to_string());
            let date = chrono::Utc::now()
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string();
            let auth = oss_sign("PUT", &field.bucket, &key, "application/octet-stream", &date)?;
            let url = format!("https://{}.{}/{}", field.bucket, endpoint, key);
            let resp = reqwest::Client::new()
                .put(&url)
                .header("Authorization", auth)
                .header("Date", date)
                .header("Content-Type", "application/octet-stream")
                .body(data.to_vec())
                .send()
                .await
                .map_err(|e| {
                    crate::error::AppError::Internal(format!("oss put failed: {e}"))
                })?;
            if !resp.status().is_success() {
                return Err(crate::error::AppError::Internal(format!(
                    "oss put failed: HTTP {}",
                    resp.status()
                )));
            }
            Ok(url)
        }
"#
        .to_string()
    } else {
        String::new()
    };
    let oss_helper = if has_oss {
        r#"
/// 阿里云 OSS 签名（OSS-HMAC-SHA1 v1）：base64(hmac-sha1(stringToSign, secret))。
/// StringToSign = VERB \n\n Content-Type \n Date \n /{bucket}/{key}
#[allow(dead_code)]
fn oss_sign(
    verb: &str,
    bucket: &str,
    key: &str,
    content_type: &str,
    date: &str,
) -> Result<String, crate::error::AppError> {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    let string_to_sign = format!("{verb}\n\n{content_type}\n{date}\n/{bucket}/{key}");
    let secret = std::env::var("OSS_ACCESS_KEY_SECRET")
        .map_err(|_| crate::error::AppError::Internal("OSS_ACCESS_KEY_SECRET not configured".into()))?;
    let access_key = std::env::var("OSS_ACCESS_KEY_ID")
        .map_err(|_| crate::error::AppError::Internal("OSS_ACCESS_KEY_ID not configured".into()))?;
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
        .map_err(|e| crate::error::AppError::Internal(format!("hmac init: {e}")))?;
    mac.update(string_to_sign.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    Ok(format!("OSS {access_key}:{signature}"))
}
"#
        .to_string()
    } else {
        String::new()
    };
    out = out
        .replace("__S3_ARM__", &s3_arm)
        .replace("__OSS_ARM__", &oss_arm)
        .replace("__OSS_HELPER__", &oss_helper);
    out
}

/// 数据导出（M9/P1）：`export <Api> as csv|xlsx`。
///
/// 语义：从所引用 api 的 list 读取**全量**行输出 CSV 或 XLSX（`SELECT *` 无分页）。
/// CSV 为流式拼装；XLSX 由 `rust_xlsxwriter` 生成（仅当存在 xlsx 导出声明时引入依赖）。
///
/// `EXPORT_TARGETS` 由 DSL 声明静态展开（运行时零 DSL 依赖）；列名取自
/// `plan_tables`（与 MySQL DDL 列名完全一致，含 `{rel}_id` 关系列）。
fn export_rs(program: &Program) -> String {
    // 每个实体只展开一个条目（csv + xlsx 两条路由共享条目，避免 axum 重复路由 panic）
    let mut entries: Vec<(String, String, Vec<String>)> = Vec::new();
    for e in &program.exports {
        let Some(api) = program.apis.iter().find(|a| a.name == e.api) else {
            continue;
        };
        let Some(entity) = program.entities.iter().find(|x| x.name == api.entity) else {
            continue;
        };
        if !matches!(e.format.as_str(), "csv" | "xlsx") {
            continue; // 检查层已拦截，防御
        }
        let plural = crate::identity::to_snake_case(&crate::dsl::pluralize(&entity.name));
        if entries.iter().any(|(p, ..)| *p == plural) {
            continue;
        }
        let cols: Vec<String> = crate::emit::mysql::plan_tables(program)
            .iter()
            .find(|t| t.entity == entity.name)
            .map(|t| t.columns.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_else(|| {
                entity
                    .fields
                    .iter()
                    .map(|f| crate::identity::to_snake_case(&f.name))
                    .collect()
            });
        entries.push((
            plural,
            crate::identity::to_snake_case(&crate::dsl::pluralize(&entity.name)),
            cols,
        ));
    }
    let mut targets = String::new();
    for (plural, table, cols) in &entries {
        let col_lit = cols
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", ");
        targets.push_str(&format!(
            "    (&\"{plural}\", &\"{table}\", &[{col_lit}]),\n"
        ));
    }
    let has_xlsx = program.exports.iter().any(|e| e.format == "xlsx");
    let mut out = String::from(
        r#"// @generated by Forma (gen-rust) — do not edit manually.
//! 数据导出（M9/P1）：`export <Api> as csv|xlsx`。
//!
//! - CSV：`GET /api/{plural}/export.csv` —— 从表读取全量行，流式拼装；
__XLSX_DOC__

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use sqlx::mysql::MySqlRow;
use sqlx::Row;

/// (plural, table, columns) —— 由 DSL `export` 声明静态展开（运行时零 DSL 依赖）
pub static EXPORT_TARGETS: &[(&str, &str, &[&str])] = &[
"#,
    );
    out.push_str(&targets);
    out.push_str(
        r#"];

async fn export_csv(
    State(db): State<crate::routes::Db>,
    AxumPath(plural): AxumPath<String>,
) -> Result<Response, crate::error::AppError> {
    let Some((_api, table, cols)) = EXPORT_TARGETS
        .iter()
        .find(|(p, _, _)| *p == plural)
    else {
        return Err(crate::error::AppError::NotFound);
    };
    let sql = format!("SELECT * FROM `{table}` ORDER BY id");
    let rows = sqlx::query(&sql)
        .fetch_all(&*db)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("export query failed: {e}")))?;
    let mut csv = String::new();
    csv.push_str(&cols.join(","));
    csv.push('\n');
    for row in &rows {
        let vals: Vec<String> = cols.iter().map(|c| decode_cell(row, c)).collect();
        csv.push_str(&vals.join(","));
        csv.push('\n');
    }
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{plural}.csv\""),
        )
        .body(axum::body::Body::from(csv.into_bytes()))
        .map_err(|_| crate::error::AppError::Internal("export response build".into()))?)
}

__XLSX_HANDLER__
/// 单元格值（按 try_get 类型降级解码；生成端不知列类型，运行时逐类型尝试）。
enum CellValue {
    None,
    Str(String),
    Num(f64),
    Bool(bool),
}

fn decode_cell_value(row: &MySqlRow, col: &str) -> CellValue {
    if let Ok(Some(v)) = row.try_get::<Option<String>, _>(col) {
        return CellValue::Str(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<bool>, _>(col) {
        return CellValue::Bool(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<i8>, _>(col) {
        return CellValue::Num(v as f64);
    }
    if let Ok(Some(v)) = row.try_get::<Option<i64>, _>(col) {
        return CellValue::Num(v as f64);
    }
    if let Ok(Some(v)) = row.try_get::<Option<f64>, _>(col) {
        return CellValue::Num(v);
    }
    if let Ok(Some(v)) = row.try_get::<Option<rust_decimal::Decimal>, _>(col) {
        return CellValue::Num(v.to_string().parse().unwrap_or(0.0));
    }
    if let Ok(Some(v)) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(col) {
        return CellValue::Str(v.to_rfc3339());
    }
    if let Ok(Some(v)) = row.try_get::<Option<chrono::NaiveDate>, _>(col) {
        return CellValue::Str(v.to_string());
    }
    if let Ok(Some(v)) = row.try_get::<Option<serde_json::Value>, _>(col) {
        return CellValue::Str(v.to_string());
    }
    if let Ok(Some(v)) = row.try_get::<Option<Vec<u8>>, _>(col) {
        return CellValue::Str(String::from_utf8_lossy(&v).to_string());
    }
    CellValue::None // NULL
}

/// CSV 单元格字符串（与 xlsx 共用类型解码，保证两端同值）。
fn decode_cell(row: &MySqlRow, col: &str) -> String {
    match decode_cell_value(row, col) {
        CellValue::None => String::new(),
        CellValue::Str(s) => escape_csv(&s),
        CellValue::Num(n) => n.to_string(),
        CellValue::Bool(b) => (b as u8).to_string(),
    }
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn router() -> Router<crate::routes::Db> {
    let mut r = Router::new();
    for &(plural, _table, _cols) in EXPORT_TARGETS {
        r = r.route(&format!("/api/{plural}/export.csv"), get(export_csv));
        __XLSX_ROUTE__
    }
    r
}
"#,
    );
    let xlsx_handler = if has_xlsx {
        r#"async fn export_xlsx(
    State(db): State<crate::routes::Db>,
    AxumPath(plural): AxumPath<String>,
) -> Result<Response, crate::error::AppError> {
    let Some((_api, table, cols)) = EXPORT_TARGETS
        .iter()
        .find(|(p, _, _)| *p == plural)
    else {
        return Err(crate::error::AppError::NotFound);
    };
    let sql = format!("SELECT * FROM `{table}` ORDER BY id");
    let rows = sqlx::query(&sql)
        .fetch_all(&*db)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("export query failed: {e}")))?;
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let worksheet = workbook.add_worksheet();
    let xlsx_err = |msg: String| crate::error::AppError::Internal(msg);
    for (c, col) in cols.iter().enumerate() {
        worksheet
            .write_string(0, c as u16, *col)
            .map_err(|e| xlsx_err(format!("xlsx header write failed: {e}")))?;
    }
    for (r, row) in rows.iter().enumerate() {
        let rr = (r + 1) as u32;
        for (c, col) in cols.iter().enumerate() {
            let cc = c as u16;
            match decode_cell_value(row, col) {
                CellValue::None => {}
                CellValue::Str(s) => {
                    worksheet
                        .write_string(rr, cc, s.as_str())
                        .map_err(|e| xlsx_err(format!("xlsx cell write failed: {e}")))?;
                }
                CellValue::Num(n) => {
                    worksheet
                        .write_number(rr, cc, n)
                        .map_err(|e| xlsx_err(format!("xlsx cell write failed: {e}")))?;
                }
                CellValue::Bool(b) => {
                    worksheet
                        .write_boolean(rr, cc, b)
                        .map_err(|e| xlsx_err(format!("xlsx cell write failed: {e}")))?;
                }
            }
        }
    }
    let buf = workbook
        .save_to_buffer()
        .map_err(|e| xlsx_err(format!("xlsx build failed: {e}")))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            axum::http::header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{plural}.xlsx\""),
        )
        .body(axum::body::Body::from(buf))
        .map_err(|_| crate::error::AppError::Internal("export response build".into()))?)
}

"#
        .to_string()
    } else {
        String::new()
    };
    let xlsx_route = if has_xlsx {
        "r = r.route(&format!(\"/api/{plural}/export.xlsx\"), get(export_xlsx));\n"
    } else {
        ""
    };
    let xlsx_doc = if has_xlsx {
        "//! - XLSX：`GET /api/{plural}/export.xlsx` —— rust_xlsxwriter 生成（条件依赖）。\n"
    } else {
        ""
    };
    out = out
        .replace("__XLSX_DOC__", xlsx_doc)
        .replace("__XLSX_HANDLER__", &xlsx_handler)
        .replace("__XLSX_ROUTE__", xlsx_route);
    out
}
