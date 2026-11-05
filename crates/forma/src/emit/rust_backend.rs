//! backend-rust 适配层：Program → 可编译的 axum + sqlx 后端工程。

use super::{pick_message, snake, EntityDecl, FieldDecl, Program};
use crate::dsl::{pluralize, BinOp, Expr, FieldConstraint};
use crate::util::{sha256_hex, write_text};
use anyhow::Result;
use std::path::Path;

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
    let pkg = format!("{}-backend", app_name.to_lowercase());
    let mut files: Vec<crate::generators::GeneratedFile> = Vec::new();

    // 命令类操作（spec §10）：有命令 → 生成输入模型/校验器/仓储/路由/审计钩子
    let has_commands = !program.commands.is_empty();
    let has_audit = program.commands.iter().any(|c| c.audit.is_some());

    files.push(file(
        out_dir,
        "Cargo.toml",
        cargo_toml(&pkg, has_commands, program.auth.is_some()),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "src/error.rs",
        error_rs(
            has_any_auth_rules(program),
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
        "DATABASE_URL=mysql://root:root@localhost:3306/school\n".into(),
        "gen-rust",
    ));
    files.push(file(
        out_dir,
        "README.md",
        format!(
            "# {app_name} backend (generated by Forma)\n\n```bash\n# 1. create schema\nmysql -u root -p < ../db/migrations/0001_init.sql\n# 2. run\nexport DATABASE_URL=mysql://root:root@localhost:3306/school\ncargo run\n```\nAPI base: http://localhost:8080/api\n"
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

fn cargo_toml(pkg: &str, has_commands: bool, has_auth: bool) -> String {
    // error.rs uses tracing::error! for internal failures (Runtime Contract E5), so tracing is always present.
    let tracing_deps = if has_commands {
        "\ntracing = \"0.1\"\ntracing-subscriber = \"0.3\""
    } else {
        "\ntracing = \"0.1\""
    };
    let auth_deps = if has_auth {
        "\nargon2 = \"0.5\"\njsonwebtoken = \"9\""
    } else {
        ""
    };
    format!(
        r#"[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
sqlx = {{ version = "0.8", features = ["runtime-tokio", "mysql", "uuid", "chrono", "rust_decimal"] }}
uuid = {{ version = "1", features = ["v4", "serde"] }}{tracing_deps}{auth_deps}
chrono = {{ version = "0.4", features = ["serde"] }}
rust_decimal = {{ version = "1", features = ["serde"] }}
tower-http = {{ version = "0.5", features = ["cors"] }}

# 独立于 Forma 工作区编译
[workspace]
"#
    )
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

fn compile_access_guard(
    program: &Program,
    entity: &EntityDecl,
    cond: &Expr,
    relations: &[(&str, String)],
) -> String {
    let mut bindings = String::new();
    let mut idx = 0usize;
    let vars: &[(&str, &str)] = &[("role", "role")];
    let expr = expr_to_rust_access(
        cond,
        program,
        entity,
        "row",
        relations,
        vars,
        &mut bindings,
        &mut idx,
    );
    format!(
        "{bindings}    let role = user.role.clone();\n    if !({expr}) {{ return Err(AppError::Forbidden(\"operation not permitted for this role\".to_string())); }}\n"
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
    relations: &[(&str, String)],
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
            let code = compile_access_guard(program, entity, cond, relations);
            if access_needs_row(cond) {
                after_row.push_str(&code);
            } else {
                pre.push_str(&code);
            }
        }
    }
    let needs_ext = !pre.is_empty() || !after_row.is_empty();
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

fn error_rs(has_forbidden: bool, has_auth: bool, has_conflict: bool) -> String {
    let base = r#"// @generated by Forma (gen-rust) — do not edit manually.
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

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
}

/// 错误码 → HTTP 状态码（Runtime Contract · RFC-0005 E3：映射只在此处声明）。
pub fn status_for(code: &str) -> StatusCode {
    match code {
        error_code::VALIDATION_FAILED => StatusCode::BAD_REQUEST,
        error_code::UNAUTHORIZED => StatusCode::UNAUTHORIZED,
        error_code::FORBIDDEN => StatusCode::FORBIDDEN,
        error_code::NOT_FOUND => StatusCode::NOT_FOUND,
        error_code::CONFLICT => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub enum AppError {
    Validation(Vec<FieldError>),
    NotFound,
    //CONFLICT_VARIANT
    //AUTH_VARIANT
    //FORBIDDEN_VARIANT
    Db(sqlx::Error),
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
            //FORBIDDEN_MATCH
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
            request_id: None,
        };
        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Db(e)
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
    if has_auth {
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
    }
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

/// 访问条件中操作数的类型（用于数字字面量类型化）。
fn access_operand_type(program: &Program, entity: &EntityDecl, expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ref { path, .. } if path.len() == 1 => {
            let field = entity.fields.iter().find(|f| f.name == path[0])?;
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
/// - Option 字段**克隆后**取兜底值——既不移动出 `row`（否则 `Ok(Json(row))` 报部分移动），
///   也不移动出引用；
/// - 数值字面量按对侧字段类型渲染（Decimal 必须显式类型）。
#[allow(clippy::too_many_arguments)]
fn expr_to_rust_access(
    expr: &Expr,
    program: &Program,
    entity: &EntityDecl,
    row: &str,
    relations: &[(&str, String)],
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
            // 跨关系：仅支持一层 `relation.field`，字段声明时已校验存在与类型
            if path.len() == 2 {
                if let Some((alias, _)) = relations.iter().find(|(name, _)| *name == path[0]) {
                    let target_field = {
                        let relation_field = entity.fields.iter().find(|f| f.name == path[0]);
                        let relation_field = match relation_field {
                            Some(f) => f,
                            None => return "Default::default()".to_string(),
                        };
                        let Some(target) = program
                            .entities
                            .iter()
                            .find(|e| e.name == relation_field.type_name)
                        else {
                            return "Default::default()".to_string();
                        };
                        target.fields.iter().find(|f| f.name == path[1])
                    };
                    let Some(target_field) = target_field else {
                        return "Default::default()".to_string();
                    };
                    let col = snake(&target_field.name);
                    let accessor = format!("{alias}.{col}");
                    *idx += 1;
                    let var = format!("__rel{idx}");
                    let fallback = typed_fallback(program, &target_field.type_name);
                    // 关系行可能缺失（LEFT JOIN）→ clone 后兜底，不移动出别名
                    bindings.push_str(&format!(
                        "    let {var} = {accessor}.clone().unwrap_or({fallback});\n"
                    ));
                    return var;
                }
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
                    expr_to_rust_access(i, program, entity, row, relations, vars, bindings, idx)
                })
                .collect();
            format!("vec![{}]", parts.join(", "))
        }
        Expr::Not(inner) => format!(
            "!({})",
            expr_to_rust_access(inner, program, entity, row, relations, vars, bindings, idx)
        ),
        Expr::Call(_, _) => "true".to_string(),
        Expr::Null => "Default::default()".to_string(),
        Expr::Bin(op, a, b) => {
            let l = expr_to_rust_access(a, program, entity, row, relations, vars, bindings, idx);
            let l_type = access_operand_type(program, entity, a);
            let r = match &**b {
                Expr::Num(v) if l_type.as_deref() == Some("Decimal") => decimal_literal(*v),
                other => {
                    expr_to_rust_access(other, program, entity, row, relations, vars, bindings, idx)
                }
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
    out.push_str(
        "use serde::Deserialize;\nuse crate::models::*;\nuse sqlx::MySqlPool;\nuse uuid::Uuid;\n\n",
    );
    if program
        .apis
        .iter()
        .any(|a| a.list.as_ref().and_then(|l| l.paginate.as_deref()) == Some("cursor"))
    {
        out.push_str("#[derive(Debug, Deserialize)]\npub struct CursorPayload { pub values: Vec<String>, pub id: String }\n\n");
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
                wheres.push_str(&format!(
                    "        (? IS NULL OR ({})) AND\n",
                    search_conds.join(" OR ")
                ));
            }
            let _ = &binds;
        }
        // 去掉末尾 " AND\n"
        let where_clause = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.trim_end_matches("AND\n").trim_end())
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
                    r#"pub async fn list__SNAKE__(db: &MySqlPool, params: &__NAME__ListParams, cursor: Option<{cursor_ty}>) -> sqlx::Result<Vec<__NAME__ListRow>> {{
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);
    let cursor_where = if cursor.is_some() {{ {cursor_sql:?}.to_string() }} else {{ String::new() }};
    let sql = format!(
        "SELECT
__SELECT__        FROM __TABLE__ AS s
__JOINS__        __WHERE__ {{}}\n        ORDER BY __ORDER__
        LIMIT ?",
        cursor_where
    );
    let mut query = sqlx::query_as::<_, __NAME__ListRow>(&sql);
"#,
                    cursor_ty = cursor_ty,
                    cursor_sql = cursor_sql,
                )
                .replace("__SNAKE__", &format!("_{}", snake_name))
                .replace("__NAME__", name)
                .replace("__SELECT__", &select_cols)
                .replace("__TABLE__", &table)
                .replace("__JOINS__", &joins)
                .replace("__WHERE__", &where_clause)
                .replace("__ORDER__", &cursor_order);
                out.push_str(&frag);
                cursor_bind_code = format!(
                    "    if let Some(({tuple_binding})) = cursor {{\n{cursor_binds}    }}\n"
                );
            } else {
                let frag = r#"pub async fn list__SNAKE__(db: &MySqlPool, params: &__NAME__ListParams) -> sqlx::Result<Vec<__NAME__ListRow>> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 200);
    let sql = format!(
        "SELECT
__SELECT__        FROM __TABLE__ AS s
__JOINS__        __WHERE__
        ORDER BY __ORDER__
        LIMIT ? OFFSET ?",
    );
    let mut query = sqlx::query_as::<_, __NAME__ListRow>(&sql);
"#;
                let frag = frag
                    .replace("__SNAKE__", &format!("_{}", snake_name))
                    .replace("__NAME__", name)
                    .replace("__SELECT__", &select_cols)
                    .replace("__TABLE__", &table)
                    .replace("__JOINS__", &joins)
                    .replace("__WHERE__", &where_clause)
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
                    out.push_str("    query = query.bind(params.q.clone());\n");
                    for _ in &list.search {
                        out.push_str("    query = query.bind(params.q.clone());\n");
                    }
                }
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
                        let fallback =
                            if matches!(field.type_name.as_str(), "String" | "Email" | "Text")
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

        if has_op("create") {
            let frag = r#"pub async fn insert__SNAKE__(db: &MySqlPool, input: &Create__NAME__) -> sqlx::Result<()> {
    let id = Uuid::new_v4().to_string();
    let sql = "INSERT INTO __TABLE__ (__COLS__) VALUES (__VALS__)";
    sqlx::query(sql)
        .bind(id)
"#;
            let frag = frag
                .replace("__SNAKE__", &format!("_{}", snake_name))
                .replace("__NAME__", name)
                .replace("__TABLE__", &table)
                .replace("__COLS__", &insert_cols.join(", "))
                .replace("__VALS__", &insert_vals.join(", "));
            out.push_str(&frag);
            for b in &insert_binds[1..] {
                out.push_str(&format!("        .bind({b})\n"));
            }
            out.push_str("        .execute(db)\n        .await?;\n    Ok(())\n}\n\n");
        }

        // update
        let set_clause = writable
            .iter()
            .map(|f| {
                let col = if super::is_relation(program, &f.type_name) {
                    format!("{}_id", snake(&f.name))
                } else {
                    snake(&f.name)
                };
                format!("{col} = ?")
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
        if has_op("get") || has_op("update") || has_op("delete") {
            out.push_str(&format!(
                r#"pub async fn find_{snake_name}(db: &MySqlPool, id: &str) -> sqlx::Result<Option<{name}>> {{
    let sql = "SELECT * FROM {table} WHERE id = ?";
    sqlx::query_as::<_, {name}>(sql)
        .bind(id)
        .fetch_optional(db)
        .await
}}

"#
            ));
        }
    }
    out
}

// ── routes.rs ───────────────────────────────────────────

/// routes + 命令 handlers/路由（命令路由挂到 router 汇总处）。
fn routes_rs_with_commands(program: &Program, has_commands: bool) -> anyhow::Result<String> {
    let mut out = routes_rs(program);
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

fn routes_rs(program: &Program) -> String {
    let mut out = String::from("// @generated by Forma (gen-rust) — do not edit manually.\n");
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
    let any_auth = has_any_auth_rules(program);
    // 命令一律 POST；auth module 的 register/login 也是 POST
    let has_commands = !program.commands.is_empty();
    let routing = if any_create_only || has_commands || program.auth.is_some() {
        "use axum::routing::{get, post};"
    } else {
        "use axum::routing::get;"
    };
    let extract = if any_list && any_auth {
        "use axum::extract::{Extension, Path, Query, State};"
    } else if any_list {
        "use axum::extract::{Path, Query, State};"
    } else if any_auth {
        "use axum::extract::{Extension, Path, State};"
    } else {
        "use axum::extract::{Path, State};"
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
    out.push_str(&format!(
        "use crate::error::{{AppError, AppResult}};\nuse crate::models::*;\nuse crate::repo;\n{validators_import}use crate::repo::*;\n{auth_import}use std::sync::Arc;\n\npub type Db = Arc<sqlx::MySqlPool>;\n\n"
    ));

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
        let relations: Vec<(&str, String)> = entity
            .fields
            .iter()
            .filter(|f| super::is_relation(program, &f.type_name))
            .map(|f| {
                let alias = format!(
                    "t{}",
                    1 + entity
                        .fields
                        .iter()
                        .take(
                            entity
                                .fields
                                .iter()
                                .position(|x| x.name == f.name)
                                .unwrap_or(0)
                        )
                        .filter(|x| super::is_relation(program, &x.type_name))
                        .count()
                );
                (f.name.as_str(), alias)
            })
            .collect();
        let guard =
            |op: &str| op_guard_bundle(program, entity, &auth_map, &access_map, op, &relations);

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
    let mut items = repo::list_{snake_name}(&db, &params, cursor).await?;
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
                    pre = g.pre,
                    tuple_type = tuple_type,
                    values_len = values_len,
                    parse_values = parse_values,
                    tuple_expr = tuple_expr,
                    values_expr = values_expr,
                ));
            } else {
                out.push_str(&format!(
                    r#"async fn list_{snake_name}(State(db): State<Db>, Query(params): Query<{name}ListParams>{ext_last}) -> AppResult<Json<Vec<{name}ListRow>>> {{
{pre}    Ok(Json(repo::list_{snake_name}(&db, &params).await?))
}}

"#,
                    ext_last = g.ext_last,
                    pre = g.pre
                ));
            }
        }
        if has("get") {
            let g = guard("get");
            out.push_str(&format!(
                r#"async fn get_{snake_name}(State(db): State<Db>, Path(id): Path<String>{ext_last}) -> AppResult<Json<{name}>> {{
{pre}    let row = repo::find_{snake_name}(&db, &id).await?.ok_or(AppError::NotFound)?;
{after_row}    Ok(Json(row))
}}

"#,
                ext_last = g.ext_last,
                pre = g.pre,
                after_row = g.after_row
            ));
        }
        if has("create") {
            let g = guard("create");
            out.push_str(&format!(
                r#"async fn create_{snake_name}(State(db): State<Db>, {ext_mid}Json(input): Json<Create{name}>) -> AppResult<Json<serde_json::Value>> {{
{pre}    let errs = crate::validators::validate_create_{snake_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
    repo::insert_{snake_name}(&db, &input).await?;
    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
                ext_mid = g.ext_mid,
                pre = g.pre
            ));
        }
        if has("update") {
            let g = guard("update");
            // 有行级访问条件时必须先取到实体行（判定需要行数据）→ 先取行、再授权、最后校验
            let body = if g.after_row.is_empty() {
                format!(
                    r#"    let errs = crate::validators::validate_update_{snake_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
    if repo::find_{snake_name}(&db, &id).await?.is_none() {{
        return Err(AppError::NotFound);
    }}
"#
                )
            } else {
                format!(
                    r#"    let row = repo::find_{snake_name}(&db, &id).await?.ok_or(AppError::NotFound)?;
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
    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
                ext_mid = g.ext_mid,
                pre = g.pre,
                body = body
            ));
        }
        if has("delete") {
            let has_guard = program
                .rules
                .iter()
                .flat_map(|r| r.guards.iter())
                .any(|g| g.entity == *name && g.op == "delete");
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
{pre}    let {row_binding} = repo::find_{snake_name}(&db, &id).await?.ok_or(AppError::NotFound)?;
{after_row}{guard_call}    repo::delete_{snake_name}(&db, &id).await?;
    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
                ext_last = g.ext_last,
                pre = g.pre,
                after_row = g.after_row,
                row_binding = row_binding,
                guard_call = guard_call
            ));
        }
    }

    // 认证路由（公开：注册/登录；受保护：me）
    if program.auth.is_some() {
        out.push_str(
            "pub fn auth_routes(db: Db) -> Router {\n    Router::new()\n        .route(\"/api/auth/register\", post(crate::auth::register))\n        .route(\"/api/auth/login\", post(crate::auth::login))\n        .with_state(db)\n}\n\n",
        );
    }

    // router 汇总
    out.push_str("pub fn router(db: sqlx::MySqlPool) -> Router {\n");
    out.push_str("    let db = Arc::new(db);\n");
    out.push_str("    let mut app = Router::new();\n");
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
    out.push_str("    app.layer(tower_http::cors::CorsLayer::permissive())\n}\n");
    out
}

fn main_rs(program: &Program, has_commands: bool, has_audit: bool) -> String {
    let app_name = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "forma-app".into());
    let audit_mod = if has_audit { "\nmod audit;" } else { "" };
    let auth_mod = if program.auth.is_some() {
        "\nmod auth;"
    } else {
        ""
    };
    let tracing_init = if has_commands {
        "\n    tracing_subscriber::fmt::init();"
    } else {
        ""
    };
    format!(
        r#"// @generated by Forma (gen-rust) — do not edit manually.
mod error;
mod models;
mod repo;
mod routes;
mod validators;{audit_mod}{auth_mod}

use routes::router;

#[tokio::main]
async fn main() {{{tracing_init}
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set (mysql://user:pass@host:3306/db)");
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
        .expect("failed to connect to MySQL");
    let app = router(db);
    // 端口可用 PORT 环境变量覆盖（默认 8080，与 Web proxy / Flutter 默认 API_BASE 对齐）
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = "0.0.0.0:".to_string() + &port;
    let listener = tokio::net::TcpListener::bind(addr.as_str()).await.expect("bind addr");
    println!("{app_name} backend listening on http://{{}}", addr);
    axum::serve(listener, app).await.expect("serve");
}}
"#,
        app_name = app_name,
        audit_mod = audit_mod,
        auth_mod = auth_mod,
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
                    "    crate::audit::log_action(&db, \"{a}\", \"{entity_name}\", &id, {actor}, {payload}).await?;\n"
                )
            })
            .unwrap_or_default();

        handlers.push_str(&format!(
            r#"async fn {fn_name}_handler(State(db): State<Db>, {user_arg}Path(id): Path<String>, Json(input): Json<{struct_name}Input>) -> AppResult<Json<serde_json::Value>> {{
    let errs = crate::validators::validate_{fn_name}(&input);
    if !errs.is_empty() {{
        return Err(AppError::Validation(errs));
    }}
    let row = repo::find_{0}(&db, &id).await?.ok_or(AppError::NotFound)?;
{pre_check}{audit_call}    repo::{fn_name}(&db, &id, &input).await?;
    Ok(Json(serde_json::json!({{ "ok": true }})))
}}

"#,
            snake(entity_name),
        ));
        routes.push((axum_path, format!("{fn_name}_handler")));
    }
    Ok((handlers, routes))
}

/// auth.rs：注册/登录/me + JWT 签发校验 + 保护中间件。
fn auth_rs(program: &Program, app_name: &str) -> String {
    let auth = program.auth.as_ref().unwrap();
    let first_role = auth.roles.first().cloned().unwrap_or_else(|| "user".into());
    let token_hours = auth.token_hours.to_string();
    let n_roles = auth.roles.len().to_string();
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

#[derive(Debug, Deserialize)]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub role: Option<String>,
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
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    #[allow(dead_code)] // 行标识（当前仅用于扩展）
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub role: String,
}

fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| "__APP__-dev-secret-change-me".into())
}

fn issue_token(username: &str, role: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = serde_json::json!({
        "sub": username,
        "role": role,
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
    let mut req = req;
    req.extensions_mut().insert(CurrentUser { username, role });
    Ok(next.run(req).await)
}

/// 当前用户（由 require_auth 注入请求扩展）。
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub username: String,
    pub role: String,
}

//REQUIRE_ROLE_BLOCK

pub async fn register(
    State(db): State<Db>,
    Json(input): Json<RegisterInput>,
) -> AppResult<Json<TokenResponse>> {
    if input.username.trim().is_empty() || input.password.len() < 6 {
        return Err(AppError::Validation(vec![crate::error::FieldError::new(
            "password",
            "用户名不能为空且密码至少 6 位",
        )]));
    }
    let role = input.role.clone().unwrap_or_else(|| "__FIRST_ROLE__".into());
    if !ROLES.contains(&role.as_str()) {
        return Err(AppError::Validation(vec![crate::error::FieldError::new(
            "role",
            format!("role 必须是 {}", ROLES.join("/")),
        )]));
    }
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = ?")
            .bind(&input.username)
            .fetch_optional(db.as_ref())
            .await
            .map_err(AppError::Db)?;
    if exists.is_some() {
        return Err(AppError::Conflict("用户名已存在".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&input.username)
        .bind(hash_password(&input.password))
        .bind(&role)
        .bind(chrono::Utc::now())
        .execute(db.as_ref())
        .await
        .map_err(AppError::Db)?;
    let token = issue_token(&input.username, &role);
    Ok(Json(TokenResponse { token, username: input.username, role }))
}

pub async fn login(
    State(db): State<Db>,
    Json(input): Json<LoginInput>,
) -> AppResult<Json<TokenResponse>> {
    let row: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, role FROM users WHERE username = ?",
    )
    .bind(&input.username)
    .fetch_optional(db.as_ref())
    .await
    .map_err(AppError::Db)?;
    let Some(user) = row else {
        return Err(AppError::Auth("用户名或密码错误".into()));
    };
    if !verify_password(&input.password, &user.password_hash) {
        return Err(AppError::Auth("用户名或密码错误".into()));
    }
    let token = issue_token(&user.username, &user.role);
    Ok(Json(TokenResponse { token, username: user.username, role: user.role }))
}

/// 当前用户信息（要求已登录）。
pub async fn me(req_user: axum::Extension<CurrentUser>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "username": req_user.username,
        "role": req_user.role,
    }))
}
"#;
    template
        .replace("__N_ROLES__", &n_roles)
        .replace("__ROLES_LIST__", &roles_list)
        .replace("__TOKEN_HOURS__", &token_hours)
        .replace("__FIRST_ROLE__", &first_role)
        .replace("__APP__", app_name)
        .replace("//REQUIRE_ROLE_BLOCK\n", &require_role_block)
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
    pub payload: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn log_action(
    db: &MySqlPool,
    action: &str,
    entity: &str,
    entity_id: &str,
    actor: Option<&str>,
    payload: Option<serde_json::Value>,
) -> sqlx::Result<()> {
    tracing::info!(action, entity, id = entity_id, actor = actor.unwrap_or("anonymous"), "forma.audit");
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO audit_logs (id, action, entity, entity_id, actor, payload, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(id)
        .bind(action)
        .bind(entity)
        .bind(entity_id)
        .bind(actor)
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
        "SELECT id, action, entity, entity_id, actor, payload, created_at FROM audit_logs ORDER BY created_at DESC LIMIT ? OFFSET ?",
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
            "#[derive(Debug, Serialize, FromRow)]\npub struct {struct_name}ListRow {{\n{row_fields}}}\n\n"
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

fn rust_type(_program: &Program, field: &FieldDecl) -> String {
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
    out.push_str("use crate::error::FieldError;\n");
    // validators 只用 fully-qualified 的 chrono 路径 → 不导入 DateTime/Utc/Uuid
    let needs_decimal = program
        .entities
        .iter()
        .any(|e| e.fields.iter().any(|f| f.type_name == "Decimal"));
    if needs_decimal {
        out.push_str("use rust_decimal::Decimal;\n");
    }
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
