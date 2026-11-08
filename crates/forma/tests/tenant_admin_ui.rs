//! A-5：多租户管理 UI（吊销 / 迁移）回归。
//!
//! 零变化原则：仅 `tenant_enabled`（@tenant + auth module）工程生成管理页；
//! 普通工程不产生页面 / 路由 / 导航。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-tenantui-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

fn manifest() -> &'static str {
    "project:\n  name: tenantui\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n"
}

const TENANT_FM: &str = r#"
app tenantui {
  version = "0.1.0"
}

auth module {
  provider jwt
  roles [admin]
  tokenHours 12
}

entity Item {
  id     Uuid       @id @default(uuid)
  name   String(50) @required
  tenant String     @tenant @required
}

api Items {
  entity Item
  operations [list get create update delete]
}

page ItemList {
  route "/items"
  title { en "Items" }
  table {
    query   Items.list
    columns [name]
    actions {
      create → ItemEdit
      row    [edit → ItemEdit, delete → Items.delete confirm]
    }
  }
}

page ItemEdit {
  route "/items/{id}"
  title { en "Edit" }
  form Item {
    layout stack {
      name Input @required
    }
    submit → Items.update redirect "/items"
    submit create → Items.create redirect "/items"
  }
}
"#;

const PLAIN_FM: &str = r#"
app tenantui {
  version = "0.1.0"
}

entity Item {
  id   Uuid       @id @default(uuid)
  name String(50) @required
}

api Items {
  entity Item
  operations [list get create update delete]
}

page ItemList {
  route "/items"
  title { en "Items" }
  table {
    query   Items.list
    columns [name]
    actions {
      row [delete → Items.delete confirm]
    }
  }
}
"#;

fn project(dir: &Path, fm: &str) {
    fs::write(dir.join("forma.yaml"), manifest()).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

fn read(dir: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(dir.join(rel)).ok()
}

#[test]
fn tenant_project_generates_admin_ui_with_backend_endpoints() {
    let dir = tmp("tenant");
    project(&dir, TENANT_FM);
    let report = forma::build::run_build(&dir).unwrap();
    assert!(report.total_files() > 0);

    let page = read(&dir, "generated/web/src/pages/TenantAdmin.tsx").unwrap();
    assert!(page.contains("/api/admin/tenants/"), "{page}");
    assert!(page.contains("/revoke"), "{page}");
    assert!(page.contains("/transfer"), "{page}");
    assert!(page.contains("x-admin-token"), "{page}");
    assert!(page.contains("TENANT_ADMIN_TOKEN"), "{page}");
    assert!(page.contains("{ to: target.trim() }"), "{page}");

    let router = read(&dir, "generated/web/src/router.tsx").unwrap();
    assert!(router.contains("TenantAdmin"), "{router}");
    assert!(router.contains("\"/tenant-admin\""), "{router}");
    let layout = read(&dir, "generated/web/src/layout.tsx").unwrap();
    assert!(layout.contains("/tenant-admin"), "{layout}");

    // 后端端点与管理 UI 同时存在（同一判定条件）
    let routes = read(&dir, "generated/backend/src/routes.rs").unwrap();
    assert!(
        routes.contains("/api/admin/tenants/:tenant/revoke"),
        "{routes}"
    );
    assert!(
        routes.contains("/api/admin/tenants/:tenant/transfer"),
        "{routes}"
    );
}

#[test]
fn non_tenant_project_has_zero_admin_ui() {
    let dir = tmp("plain");
    project(&dir, PLAIN_FM);
    forma::build::run_build(&dir).unwrap();
    assert!(read(&dir, "generated/web/src/pages/TenantAdmin.tsx").is_none());
    let router = read(&dir, "generated/web/src/router.tsx").unwrap();
    assert!(!router.contains("TenantAdmin"), "{router}");
    let layout = read(&dir, "generated/web/src/layout.tsx").unwrap();
    assert!(!layout.contains("/tenant-admin"), "{layout}");
    let routes = read(&dir, "generated/backend/src/routes.rs").unwrap();
    assert!(!routes.contains("/api/admin/tenants/"), "{routes}");
}
