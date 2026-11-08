//! 统一错误响应体契约回归测试（Runtime Contract · RFC-0005 E1–E5）：
//! - 后端错误体：`{ error, code, fields?, requestId? }` 同形（E4）；
//! - `code` 是稳定 API（E1）：`FORMA_*` 前缀，Agent 可据此分支处理；
//! - 错误码 → 状态码映射只声明一次（E3）；
//! - 内部错误**不泄露** SQL / 内部细节（E5）；
//! - 前端（Web / Flutter）按同形解析；
//! - OpenAPI 用 `components.schemas.Error` + 统一错误响应声明契约。

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
        "forma-errc-{tag}-{}-{seq}-{nanos}",
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
        "project:\n  name: errc\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const FM: &str = r#"
app errc {
  version = "0.1.0"
}

enum OrderStatus { pending approved }

entity Order {
  id     Uuid   @id @default(uuid)
  amount Decimal(12,2) @required
  status OrderStatus @default(pending)
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [status] }
}

page OrderList {
  route "/orders"
  title { zh "订单" en "Orders" }
  table {
    query   Orders.list
    columns [amount, status]
    actions {
      create → OrderEdit
      row    [edit → OrderEdit, delete → Orders.delete confirm]
    }
  }
}

page OrderEdit {
  route "/orders/{id}"
  title { zh "编辑订单" en "Edit order" }
  form {
    query  Orders.get
    submit Orders.update
    fields [amount]
  }
}
"#;

const ERROR_CODES: &[&str] = &[
    "FORMA_VALIDATION_FAILED",
    "FORMA_NOT_FOUND",
    "FORMA_CONFLICT",
    "FORMA_UNAUTHORIZED",
    "FORMA_FORBIDDEN",
    "FORMA_INTERNAL",
];

fn generate(dir: &Path) -> (PathBuf, String) {
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated");
    let error_rs = {
        let backend = out.join("backend");
        forma::emit::emit_rust_backend(&loaded.dir, &program, &backend.to_string_lossy(), "errc")
            .unwrap();
        fs::read_to_string(backend.join("src/error.rs")).unwrap()
    };
    forma::emit::emit_web_react(
        &loaded.dir,
        &program,
        &out.join("web").to_string_lossy(),
        "errc",
        false,
        false,
        "echarts",
    )
    .unwrap();
    forma::emit::emit_openapi(
        &loaded.dir,
        &program,
        &out.join("api-docs").to_string_lossy(),
    )
    .unwrap();
    (out, error_rs)
}

// ── 1. 后端：错误体与错误码（E1/E4） ─────────────────────────────

#[test]
fn error_body_has_stable_code_field() {
    let dir = tmp_dir("body");
    write_project(&dir, FM);
    let (_, error_rs) = generate(&dir);
    // E4 同形：error / code / fields / requestId
    assert!(
        error_rs.contains("error: String,")
            && error_rs.contains("code: String,")
            && error_rs.contains("fields: Option<Vec<FieldError>>,")
            && error_rs.contains("request_id: Option<String>,"),
        "ErrorBody must carry the unified contract fields:\n{error_rs}"
    );
}

#[test]
fn error_codes_are_declared_and_covered() {
    let dir = tmp_dir("codes");
    write_project(&dir, FM);
    let (_, error_rs) = generate(&dir);
    // E1：全部错误码都以 FORMA_ 前缀声明
    for code in ERROR_CODES {
        assert!(
            error_rs.contains(&format!("\"{code}\"")),
            "error code {code} must be declared:\n{error_rs}"
        );
    }
    // E3：映射只声明一次（status_for）
    assert!(
        error_rs.contains("pub fn status_for"),
        "status mapping must be declared once (status_for):\n{error_rs}"
    );
    // 映射覆盖全部错误码
    for code in [
        "VALIDATION_FAILED",
        "NOT_FOUND",
        "CONFLICT",
        "UNAUTHORIZED",
        "FORBIDDEN",
    ] {
        assert!(
            error_rs.contains(&format!("error_code::{code} => StatusCode::")),
            "status_for must map {code}"
        );
    }
}

// ── 2. 后端：E5 不泄露内部细节 ──────────────────────────────────

#[test]
fn internal_errors_do_not_leak_sql_details() {
    let dir = tmp_dir("e5");
    write_project(&dir, FM);
    let (_, error_rs) = generate(&dir);
    // E5：内部错误 → 日志记录 + 通用文案（不得把 SQL / 内部细节回显给调用方）
    assert!(
        error_rs.contains("tracing::error!"),
        "internal detail must be logged, not returned:\n{error_rs}"
    );
    assert!(
        error_rs.contains("\"internal server error\".to_string(), None)"),
        "Db error must return a generic message (E5):\n{error_rs}"
    );
    assert!(
        !error_rs.contains("e.to_string(), None)"),
        "must not echo the internal error to the client:\n{error_rs}"
    );
}

// ── 3. 前端（Web / Flutter）按同形解析（E4） ─────────────────────

#[test]
fn web_client_parses_code_and_fields() {
    let dir = tmp_dir("web");
    write_project(&dir, FM);
    let (out, _) = generate(&dir);
    let api_ts = fs::read_to_string(out.join("web/src/api.ts")).unwrap();
    assert!(
        api_ts.contains("err.code = body.code;"),
        "web client must surface the stable error code:\n{api_ts}"
    );
    assert!(
        api_ts.contains("FORMA_VALIDATION_FAILED"),
        "error codes must be documented for the client:\n{api_ts}"
    );
}

#[test]
fn flutter_client_parses_code_and_fields() {
    let dir = tmp_dir("flutter");
    write_project(&dir, FM);
    let (_, _) = generate(&dir);
    // Flutter 目标未配置 → 直接生成 api.dart 到临时工程
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let flutter_out = dir.join("generated/flutter");
    forma::emit::emit_flutter(
        &loaded.dir,
        &program,
        &flutter_out.to_string_lossy(),
        "errc",
    )
    .unwrap();
    let api = fs::read_to_string(flutter_out.join("lib/api/api.dart")).unwrap();
    assert!(
        api.contains("final String code;"),
        "Flutter ApiException must carry the stable error code:\n{api}"
    );
    assert!(
        api.contains("err['code']"),
        "Flutter client must read the code from the error body:\n{api}"
    );
}

// ── 4. OpenAPI 声明契约 ─────────────────────────────────────────

#[test]
fn openapi_declares_error_schema_and_responses() {
    let dir = tmp_dir("openapi");
    write_project(&dir, FM);
    let (out, _) = generate(&dir);
    let spec = fs::read_to_string(out.join("api-docs/openapi.json")).unwrap();
    // E4：所有错误同形 → components.schemas.Error
    assert!(
        spec.contains("\"Error\""),
        "OpenAPI must declare the Error schema:\n{spec}"
    );
    // E1：错误码是稳定 API → 枚举进 schema
    for code in ERROR_CODES {
        assert!(
            spec.contains(code),
            "OpenAPI must declare error code {code}:\n{spec}"
        );
    }
    // 每个操作都声明统一错误响应
    assert!(
        spec.contains("FORMA_VALIDATION_FAILED"),
        "operations must declare the unified error responses"
    );
}

// ── requestId（§4.4 · spec:protocols §7 缺口 8 收口）────────────────

#[test]
fn backend_generates_request_id_middleware_and_fills_body() {
    let dir = tmp_dir("request-id");
    write_project(&dir, FM);
    let (out, _) = generate(&dir);

    // 中间件模块：透传/生成 + 响应头回写 + task-local 作用域
    let module = fs::read_to_string(out.join("backend/src/request_id.rs")).unwrap();
    assert!(
        module.contains("pub const HEADER: &str = \"x-request-id\""),
        "{module}"
    );
    assert!(module.contains("attach_request_id"), "{module}");
    assert!(module.contains("REQUEST_ID"), "{module}");
    assert!(module.contains("uuid::Uuid::new_v4()"), "{module}");
    // 响应头回写
    assert!(module.contains("headers_mut().insert(HEADER"), "{module}");

    // error.rs：task-local 声明 + 错误体从作用域取 id
    let error = fs::read_to_string(out.join("backend/src/error.rs")).unwrap();
    assert!(error.contains("pub static REQUEST_ID: String;"), "{error}");
    assert!(
        error.contains("REQUEST_ID.try_with(|id| id.clone()).ok()"),
        "{error}"
    );
    // 不再是无条件 None
    assert!(!error.contains("request_id: None"), "{error}");

    // 路由汇总：中间件挂载在最外层（覆盖 auth 层与所有错误响应）
    let routes = fs::read_to_string(out.join("backend/src/routes.rs")).unwrap();
    assert!(
        routes.contains("axum::middleware::from_fn(crate::request_id::attach_request_id)"),
        "{routes}"
    );

    // main.rs 声明模块
    let main = fs::read_to_string(out.join("backend/src/main.rs")).unwrap();
    assert!(main.contains("mod request_id;"), "{main}");

    // OpenAPI：requestId 不再是「预留」
    let spec = fs::read_to_string(out.join("api-docs/openapi.json")).unwrap();
    assert!(spec.contains("requestId"), "{spec}");
    assert!(
        !spec.contains("预留"),
        "requestId must no longer be reserved-only:\n{spec}"
    );
}

#[test]
fn clients_surface_request_id() {
    let dir = tmp_dir("request-id-clients");
    write_project(&dir, FM);
    let (out, _) = generate(&dir);

    let web = fs::read_to_string(out.join("web/src/api.ts")).unwrap();
    assert!(web.contains("err.requestId = body.requestId"), "{web}");

    let api = fs::read_to_string(out.join("backend/src/request_id.rs")).unwrap();
    assert!(api.contains("x-request-id"), "{api}");
}

#[test]
fn flutter_client_surfaces_request_id() {
    let dir = tmp_dir("request-id-flutter");
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: errc\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  mobile:\n    framework: flutter\n    out: generated/mobile\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app errc {
  version = "0.1.0"
}

entity Order {
  id    Uuid      @id @default(uuid)
  title String(50) @required
}

api Orders {
  entity Order
  operations [list get create update delete]
}
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let api = fs::read_to_string(dir.join("generated/mobile/lib/api/api.dart")).unwrap();
    assert!(api.contains("final String? requestId;"), "{api}");
    assert!(
        api.contains("requestId: (err['requestId'] as String?) ?? res.headers['x-request-id']"),
        "{api}"
    );
}
