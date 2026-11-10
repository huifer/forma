//! Vue 3 frontend target 回归测试（M10）：脚手架、API 类型引用和页面路由。

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
        "forma-vue-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app vue {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
}
api Items {
  entity Item
  operations [list get create update delete]
}
page ItemList {
  route "/items"
  title { zh "条目" en "Items" }
  table { query Items.list columns [name, amount] }
}
"#;

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: vue\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  frontend:\n    framework: vue\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn vue_target_generates_vite_project_and_routes() {
    let dir = tmp_dir("project");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"frontend-vue"), "{names:?}");
    assert!(!names.contains(&"web-react"), "{names:?}");

    let package = fs::read_to_string(dir.join("generated/web/package.json")).unwrap();
    assert!(package.contains("\"vue\""), "{package}");
    assert!(
        package.contains("\"vue-tsc --noEmit && vite build\""),
        "{package}"
    );

    let api = fs::read_to_string(dir.join("generated/web/src/api.ts")).unwrap();
    assert!(
        api.contains("import { Item, ItemListRow, CreateItem, UpdateItem } from \"./types\";"),
        "{api}"
    );
    assert!(api.contains("export const ItemApi ="), "{api}");
    assert!(api.contains("req<{ ok: boolean }>(`/items/${id}`"), "{api}");
    assert!(!api.contains("{plural}"), "{api}");
    assert!(!api.contains("${{id}}"), "{api}");

    let page = fs::read_to_string(dir.join("generated/web/src/pages/item_list.vue")).unwrap();
    assert!(
        page.contains("import { ItemsApi } from \"../api\";"),
        "{page}"
    );
    // 列表页：vue-query 查询（filters/search/分页）+ Pinia store
    assert!(page.contains("ItemsApi.list({ ...store.filters"), "{page}");
    assert!(
        page.contains("import { useItemListStore } from \"../stores/item_list\";"),
        "{page}"
    );
    assert!(page.contains("<el-table-column prop=\"name\""), "{page}");
    assert!(page.contains("上一页"), "{page}");

    let router = fs::read_to_string(dir.join("generated/web/src/router.ts")).unwrap();
    assert!(router.contains("path: \"/items\""), "{router}");
    assert!(router.contains("./pages/item_list.vue"), "{router}");
    assert!(dir.join("generated/web/src/App.vue").exists());
    assert!(dir.join("generated/web/src/env.d.ts").exists());
}

#[test]
fn vue_target_keeps_default_react_target_unchanged() {
    let dir = tmp_dir("default");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: react\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"web-react"), "{names:?}");
    assert!(!names.contains(&"frontend-vue"), "{names:?}");
}

const FM_AUTH: &str = r#"
app vueauth {
  version = "0.1.0"
}
auth module { provider jwt roles [admin, student] }
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  approved Bool
}
api Items {
  entity Item
  operations [list get create update delete]
  auth { create delete update → role in ["admin"] }
}
rules ItemRules {
  access Item { read → role in ["student", "admin"] }
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
  table { query Items.list columns [name] }
}
"#;

#[test]
fn vue_target_with_auth_and_command_generates_vue_native_access() {
    let dir = tmp_dir("auth");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: vueauth\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  frontend:\n    framework: vue\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM_AUTH).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"frontend-vue"), "{names:?}");

    let api = fs::read_to_string(dir.join("generated/web/src/api.ts")).unwrap();
    assert!(api.contains("export const AuthApi ="), "{api}");
    assert!(api.contains("export const ItemApproveApi ="), "{api}");
    assert!(api.contains("execute:"), "{api}");
    assert!(
        !api.contains("useQuery"),
        "{api} — vue api.ts must not import React hooks"
    );

    let access = fs::read_to_string(dir.join("generated/web/src/access.ts")).unwrap();
    assert!(access.contains("export const ACCESS_RULES"), "{access}");
    assert!(access.contains("export const FIELD_RULES"), "{access}");
    assert!(access.contains("export function canSeeField"), "{access}");
    assert!(access.contains("useCurrentRole"), "{access}");
    assert!(access.contains("useCan"), "{access}");
    assert!(
        access.contains("from \"@tanstack/vue-query\""),
        "{access} — must use Vue Query, not react-query"
    );
    assert!(
        access.starts_with("// @generated"),
        "{access} — imports must be at the top level"
    );
}

const FM_FORM: &str = r#"
app vueform {
  version = "0.1.0"
}
entity School {
  id   Uuid @id @default(uuid)
  name String(50) @required
}
entity Student {
  id     Uuid @id @default(uuid)
  name   String(50) @required
  school School?
}
api Schools { entity School operations [list get create update delete] }
api Students { entity Student operations [list get create update delete] }
page StudentList {
  route "/students"
  title { zh "学生" en "Students" }
  table { query Students.list columns [name] }
}
page StudentEdit {
  route "/students/{id}"
  title { zh "编辑学生" en "Edit Student" }
  form Student {
    layout stack {
      name  Input @required
      school Select from School
    }
    submit → Students.update redirect "/students"
    submit create → Students.create redirect "/students"
  }
}
"#;

#[test]
fn vue_form_page_submits_with_relation_select() {
    let dir = tmp_dir("form");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: vueform\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  frontend:\n    framework: vue\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM_FORM).unwrap();
    forma::build::run_build(&dir).unwrap();

    let form = fs::read_to_string(dir.join("generated/web/src/pages/student_edit.vue")).unwrap();
    assert!(form.contains("SchoolOptions"), "{form}");
    assert!(form.contains("v-model=\"form.school\""), "{form}");
    assert!(
        form.contains("form.school = row[\"school_id\"]"),
        "{form} — 编辑页关系初始值映射"
    );
    assert!(
        form.contains("await StudentApi.create(form as any)"),
        "{form}"
    );
    assert!(
        form.contains("await StudentApi.update(id!, form as any)"),
        "{form}"
    );
    assert!(form.contains("validateCreateStudent"), "{form}");
    assert!(form.contains("validateUpdateStudent"), "{form}");
    assert!(form.contains("@click=\"submit\""), "{form}");

    // 列表页 Pinia store 与查询
    let list = fs::read_to_string(dir.join("generated/web/src/pages/student_list.vue")).unwrap();
    assert!(list.contains("useStudentListStore"), "{list}");
    let store = fs::read_to_string(dir.join("generated/web/src/stores/student_list.ts")).unwrap();
    assert!(store.contains("defineStore(\"student_list\""), "{store}");
    assert!(store.contains("function resetPage()"), "{store}");
}
