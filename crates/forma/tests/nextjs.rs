//! Next.js（App Router / SSR 变体）target 回归测试（M10）。
//! 验证门：`tsc --noEmit` + `next build`（本地）；cargo test 覆盖产物结构。

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
        "forma-next-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app next {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
  qty Int
}
api Items {
  entity Item
  operations [list get create update delete]
}
api ItemApprove {
  POST "/items/{id}/approve"
  label { zh "审批" en "Approve" }
  pre { approved == false }
  effect { set Item.approved = true }
}
page ItemList {
  route "/items"
  title { zh "条目" en "Items" }
  table { query Items.list columns [name, amount] }
}
"#;

fn setup(dir: &Path, framework: &str) {
    let yaml = format!(
        "project:\n  name: next\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  frontend:\n    framework: {framework}\n    out: generated/web\n"
    );
    fs::write(dir.join("forma.yaml"), yaml).unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn nextjs_target_generates_app_router_project() {
    let dir = tmp_dir("project");
    setup(&dir, "nextjs");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"frontend-nextjs"), "{names:?}");
    assert!(!names.contains(&"web-react"), "{names:?}");
    // vite+vitest 模板的通用 web-engineering 对 Next 不适用，不生成
    assert!(!names.contains(&"web-engineering"), "{names:?}");

    // App Router 路由结构
    assert!(dir.join("generated/web/app/layout.tsx").exists());
    assert!(dir.join("generated/web/app/page.tsx").exists());
    assert!(dir.join("generated/web/app/actions.ts").exists());
    assert!(dir.join("generated/web/app/items/page.tsx").exists());
    assert!(dir.join("generated/web/app/items/new/page.tsx").exists());
    assert!(dir.join("generated/web/app/items/[id]/page.tsx").exists());

    // Server Actions
    let actions = fs::read_to_string(dir.join("generated/web/app/actions.ts")).unwrap();
    assert!(actions.starts_with("\"use server\";"), "{actions}");
    assert!(actions.contains("createItem"), "{actions}");
    assert!(actions.contains("updateItem"), "{actions}");
    assert!(actions.contains("deleteItem"), "{actions}");
    assert!(actions.contains("getItem"), "{actions}");
    assert!(actions.contains("item_approve"), "{actions}");
    assert!(actions.contains("revalidatePath"), "{actions}");

    // 列表页 = Server Component（async + force-dynamic）
    let list = fs::read_to_string(dir.join("generated/web/app/items/page.tsx")).unwrap();
    assert!(list.contains("export default async function"), "{list}");
    assert!(list.contains("force-dynamic"), "{list}");
    assert!(list.contains("item_approve"), "{list}");
    assert!(list.contains("deleteItem"), "{list}");

    // 表单页 = client component + validators 复用
    let new_page = fs::read_to_string(dir.join("generated/web/app/items/new/page.tsx")).unwrap();
    assert!(new_page.starts_with("\"use client\";"), "{new_page}");
    assert!(new_page.contains("validateCreateItem"), "{new_page}");
    let edit = fs::read_to_string(dir.join("generated/web/app/items/[id]/page.tsx")).unwrap();
    assert!(edit.contains("validateUpdateItem"), "{edit}");

    // 共享 TS 层复用 web_react 契约（客户端 api.ts 满足 validators 的 FieldError 导入）
    let api_client = fs::read_to_string(dir.join("generated/web/src/api.ts")).unwrap();
    assert!(api_client.contains("ItemApi"), "{api_client}");
    let api_server = fs::read_to_string(dir.join("generated/web/src/api-server.ts")).unwrap();
    assert!(api_server.contains("API_BASE"), "{api_server}");
    assert!(api_server.contains("cache: \"no-store\""), "{api_server}");
    let validators = fs::read_to_string(dir.join("generated/web/src/validators.ts")).unwrap();
    assert!(validators.contains("validateCreateItem"), "{validators}");

    // next.config rewrites + Next 自带 CI
    let config = fs::read_to_string(dir.join("generated/web/next.config.mjs")).unwrap();
    assert!(config.contains("rewrites"), "{config}");
    let ci = fs::read_to_string(dir.join("generated/web/.github/workflows/ci.yml")).unwrap();
    assert!(ci.contains("next") || ci.contains("Build"), "{ci}");
    assert!(!ci.contains("vitest"), "{ci}");
}

#[test]
fn nextjs_target_keeps_default_react_target_unchanged() {
    let dir = tmp_dir("default");
    setup(&dir, "react");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"web-react"), "{names:?}");
    assert!(!names.contains(&"frontend-nextjs"), "{names:?}");
}
