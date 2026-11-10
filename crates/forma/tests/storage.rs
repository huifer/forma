//! 文件上传（M9/P1）回归测试：`Attachment` 字段 + `@storage(provider, ...)` 属性。
//!
//! 验收：
//! - 检查层 3 条 `dsl-storage-*` 诊断（非 Attachment / 未知 provider / s3/oss 缺 bucket）；
//! - 生成层：src/storage.rs + multipart 路由 + Cargo multipart feature + main mod
//!   + OpenAPI /api/uploads + Web uploadFile 助手与文件输入；
//! - 无 Attachment 的工程全链路零产物（storage.rs / multipart / uploadFile 均不出现）。

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
        "forma-storage-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const MANIFEST: &str = r#"project:
  name: uploaddemo
  version: 0.1.0

source: ["forma/**/*.fm"]

targets:
  db:
    engine: mysql
    out: db
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
  frontend:
    framework: react
    out: generated/web
"#;

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(dir.join("forma.yaml"), MANIFEST).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const APP_WITH_STORAGE: &str = r#"app uploaddemo {
  version = "0.1.0"
}

entity Document {
  id     Uuid @id @default(uuid)
  title  String(200) @required
  file   Attachment @required @storage(local)
  avatar Attachment? @storage(s3, "forma-bucket")
}

api Documents {
  entity Document
  operations [list get create update delete]
}

page Documents {
  route "/documents"
  title { zh "文档管理" en "Documents" }
  table {
    query   Documents.list
    columns [title, file, createdAt: datetime]
  }
}

page DocumentEdit {
  route "/documents/{id}"
  title { zh "编辑文档" en "Edit Document" }
  form Document {
    layout stack {
      title  Input @required
      file   Input
      avatar Input
    }
    submit → Documents.update redirect "/documents"
    submit create → Documents.create redirect "/documents"
  }
}
"#;

fn load(dir: &Path) -> forma::dsl::Program {
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diags) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let errs: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
    assert!(errs.is_empty(), "fixture must parse: {errs:?}");
    program
}

#[test]
fn storage_attribute_checks() {
    // 合法：local / s3+bucket
    let dir = tmp_dir("ok");
    write_project(&dir, APP_WITH_STORAGE);
    let diags = forma::dsl::check_program(&load(&dir));
    let storage_errs: Vec<_> = diags
        .iter()
        .filter(|d| d.code.starts_with("dsl-storage"))
        .collect();
    assert!(
        storage_errs.is_empty(),
        "valid storage config must pass: {storage_errs:?}"
    );

    // @storage 用在非 Attachment 字段
    let dir = tmp_dir("nonattach");
    write_project(
        &dir,
        r#"app uploaddemo { version = "0.1.0" }
entity Doc {
  id    Uuid @id @default(uuid)
  title String(50) @storage(local)
}
"#,
    );
    let diags = forma::dsl::check_program(&load(&dir));
    assert!(
        diags.iter().any(|d| d.code == "dsl-storage-non-attachment"),
        "{diags:?}"
    );

    // 未知 provider
    let dir = tmp_dir("provider");
    write_project(
        &dir,
        r#"app uploaddemo { version = "0.1.0" }
entity Doc {
  id   Uuid @id @default(uuid)
  file Attachment @storage(ftp)
}
"#,
    );
    let diags = forma::dsl::check_program(&load(&dir));
    assert!(
        diags
            .iter()
            .any(|d| d.code == "dsl-storage-unknown-provider"),
        "{diags:?}"
    );

    // s3 缺 bucket
    let dir = tmp_dir("bucket");
    write_project(
        &dir,
        r#"app uploaddemo { version = "0.1.0" }
entity Doc {
  id   Uuid @id @default(uuid)
  file Attachment @storage(s3)
}
"#,
    );
    let diags = forma::dsl::check_program(&load(&dir));
    assert!(
        diags.iter().any(|d| d.code == "dsl-storage-missing-bucket"),
        "{diags:?}"
    );
}

#[test]
fn storage_backend_and_web_generation() {
    let dir = tmp_dir("gen");
    write_project(&dir, APP_WITH_STORAGE);
    forma::build::run_build(&dir).unwrap();

    // Rust 后端：storage.rs + STORAGE_FIELDS 静态展开
    let storage_rs = fs::read_to_string(dir.join("generated/backend/src/storage.rs")).unwrap();
    assert!(storage_rs.contains("pub static STORAGE_FIELDS"));
    assert!(storage_rs.contains(r#"entity: "Document", field: "file", provider: "local""#));
    assert!(storage_rs.contains(r#"provider: "s3", bucket: "forma-bucket""#));
    // S3 真实驱动（aws-sdk-s3，条件依赖）
    assert!(storage_rs.contains("aws_sdk_s3::Client"));
    assert!(storage_rs.contains("put_object"));
    // 无 oss 声明 → 不生成 oss 代码（条件依赖）
    assert!(!storage_rs.contains("fn oss_sign"));
    assert!(!storage_rs.contains("OSS_ACCESS_KEY_ID"));

    // Cargo multipart feature + main mod + 路由挂载 + S3 依赖
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(cargo.contains("features = [\"multipart\"]"), "{cargo}");
    assert!(cargo.contains("aws-sdk-s3"), "{cargo}");
    assert!(cargo.contains("aws-config"), "{cargo}");

    // main mod + 路由挂载
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(main.contains("mod storage;"));
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(routes.contains("crate::storage::router"));

    // OpenAPI 契约
    let openapi = fs::read_to_string(dir.join("generated/api-docs/openapi.json")).unwrap();
    assert!(openapi.contains("/api/uploads"));

    // Web：uploadFile 助手 + 文件输入 + import
    let api_ts = fs::read_to_string(dir.join("generated/web/src/api.ts")).unwrap();
    assert!(api_ts.contains("export async function uploadFile"));
    let pages_dir = dir.join("generated/web/src/pages");
    let page = fs::read_dir(&pages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| fs::read_to_string(e.path()).unwrap_or_default())
        .find(|c| c.contains("upload-field"))
        .expect("form page must render upload-field");
    assert!(page.contains("import { uploadFile } from \"../api\""));
}

#[test]
fn no_attachment_project_has_zero_storage_artifacts() {
    let dir = tmp_dir("zero");
    write_project(
        &dir,
        r#"app uploaddemo { version = "0.1.0" }
entity Note {
  id   Uuid @id @default(uuid)
  body Text
}
api Notes { entity Note operations [list get create update delete] }

page Notes {
  route "/notes"
  title { zh "笔记" en "Notes" }
  table {
    query   Notes.list
    columns [body, createdAt: datetime]
  }
}
"#,
    );
    forma::build::run_build(&dir).unwrap();

    assert!(!dir.join("generated/backend/src/storage.rs").exists());
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(!cargo.contains("multipart"), "{cargo}");
    assert!(cargo.contains("axum = \"0.7\""));
    let main = fs::read_to_string(dir.join("generated/backend/src/main.rs")).unwrap();
    assert!(!main.contains("mod storage;"));
    let openapi = fs::read_to_string(dir.join("generated/api-docs/openapi.json")).unwrap();
    assert!(!openapi.contains("/api/uploads"));
    let api_ts = fs::read_to_string(dir.join("generated/web/src/api.ts")).unwrap();
    assert!(!api_ts.contains("uploadFile"));
}

#[test]
fn oss_storage_generates_signed_upload_driver() {
    let dir = tmp_dir("oss");
    write_project(
        &dir,
        r#"app ossdemo { version = "0.1.0" }
entity File {
  id Uuid @id @default(uuid)
  name String(50) @required
  blob Attachment? @storage(oss, "forma-oss-bucket")
}
api Files { entity File operations [list get create update delete] }
"#,
    );
    forma::build::run_build(&dir).unwrap();
    let storage_rs = fs::read_to_string(dir.join("generated/backend/src/storage.rs")).unwrap();
    assert!(storage_rs.contains("fn oss_sign"), "{storage_rs}");
    assert!(storage_rs.contains("OSS_ACCESS_KEY_ID"), "{storage_rs}");
    assert!(storage_rs.contains("OSS_ENDPOINT"), "{storage_rs}");
    assert!(storage_rs.contains("Hmac::<Sha1>"), "{storage_rs}");
    // 无 s3 声明 → 不生成 s3 代码/依赖
    assert!(!storage_rs.contains("aws_sdk_s3::Client"), "{storage_rs}");
    let cargo = fs::read_to_string(dir.join("generated/backend/Cargo.toml")).unwrap();
    assert!(cargo.contains("reqwest"), "{cargo}");
    assert!(cargo.contains("hmac"), "{cargo}");
    assert!(cargo.contains("sha1"), "{cargo}");
    assert!(cargo.contains("base64"), "{cargo}");
    assert!(!cargo.contains("aws-sdk-s3"), "{cargo}");
}
