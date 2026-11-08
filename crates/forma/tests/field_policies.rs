//! 字段级策略回归测试（P0-2 / ADR-0004 四层收口）：
//! - DSL 语法：`access Entity { fields { field → 角色表达式 } }`；
//! - Check：未知字段 / 关联字段 / 无 provider / 未知角色 / 非角色表达式 → 显式诊断；
//! - 生成：list/get 响应按当前角色裁剪（输出裁剪——角色不可见即移除该键）。
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
        "forma-fieldpol-{tag}-{}-{seq}-{nanos}",
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
        "project:\n  name: fp\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: db\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

fn check(fm: &str) -> Vec<forma::validate::Diagnostic> {
    let dir = tmp_dir("check");
    write_project(&dir, fm);
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    forma::dsl::check_program(&program)
}

const VALID_FM: &str = r#"
app fp {
  version = "0.1.0"
}

entity Employee {
  id    Uuid       @id @default(uuid)
  name  String(50) @required
  grade String(20)
}

api Employees {
  entity Employee
  operations [list get create update delete]
}

auth module {
  provider jwt
  roles [hr, manager, staff]
}

rules AccessRules {
  access Employee {
    read → any
    fields {
      grade → role in ["hr", "manager"]
    }
  }
}
"#;

fn find_code<'a>(
    diags: &'a [forma::validate::Diagnostic],
    code: &str,
) -> Option<&'a forma::validate::Diagnostic> {
    diags.iter().find(|d| d.code == code)
}

// ── Check 层 ────────────────────────────────────────────────────

#[test]
fn valid_field_policy_passes() {
    let diags = check(VALID_FM);
    assert!(
        find_code(&diags, "dsl-field-policy-unknown-field").is_none()
            && find_code(&diags, "dsl-field-policy-no-provider").is_none()
            && find_code(&diags, "dsl-field-policy-unknown-role").is_none(),
        "{diags:?}"
    );
}

#[test]
fn field_policy_diagnostics() {
    // 未知字段
    let diags = check(&VALID_FM.replace("grade → role in", "ghost → role in"));
    let d = find_code(&diags, "dsl-field-policy-unknown-field").expect("unknown field");
    assert_eq!(d.severity, "error");
    assert!(d.message.contains("Employee.ghost"), "{d:?}");

    // 关联字段
    let with_rel = VALID_FM.replace(
        "  grade String(20)",
        "  grade School\n}\n\nentity School {\n  id Uuid @id @default(uuid)\n  name String(50) @required",
    );
    let diags = check(&with_rel);
    assert!(find_code(&diags, "dsl-field-policy-relation-unsupported").is_some());

    // 无 provider
    let diags = check(&VALID_FM.replace(
        "auth module {\n  provider jwt\n  roles [hr, manager, staff]\n}",
        "",
    ));
    assert!(
        find_code(&diags, "dsl-field-policy-no-provider").is_some(),
        "{diags:?}"
    );

    // 未知角色
    let diags = check(&VALID_FM.replace("[\"hr\", \"manager\"]", "[\"hr\", \"ghost\"]"));
    let d = find_code(&diags, "dsl-field-policy-unknown-role").expect("unknown role");
    assert!(d.message.contains("ghost"), "{d:?}");

    // 非角色表达式
    let diags = check(&VALID_FM.replace(
        "grade → role in [\"hr\", \"manager\"]",
        "grade → name != \"\"",
    ));
    assert!(
        find_code(&diags, "dsl-field-policy-expression-unsupported").is_some(),
        "{diags:?}"
    );
}

// ── 生成层：裁剪 helper + handler 集成 ─────────────────────────

#[test]
fn backend_generates_trim_and_wires_handlers() {
    let dir = tmp_dir("backend");
    write_project(&dir, VALID_FM);
    let (program, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    let files = forma::emit::emit_rust_backend(&dir, &program, "generated/backend", "fp").unwrap();

    let routes = files
        .iter()
        .find(|f| f.path.ends_with("routes.rs"))
        .unwrap()
        .content
        .clone();

    // 裁剪函数：字段 → 允许角色表
    assert!(routes.contains("fn trim_employee_fields"), "{routes}");
    assert!(
        routes.contains("(\"grade\", &[\"hr\", \"manager\"])"),
        "{routes}"
    );
    // list/get 响应改走裁剪路径（返回 Value）
    assert!(
        routes.contains("trim_employee_fields(&mut v, &user.role)"),
        "{routes}"
    );
    assert!(
        routes.contains("trim_employee_fields(&mut value, &user.role)"),
        "{routes}"
    );
    // get/list 注入 Extension（需要当前角色）
    assert!(routes.contains(
        "list_employee(State(db): State<Db>, Query(params): Query<EmployeeListParams>, Extension(user): Extension<CurrentUser>)"
    ), "{routes}");
    // 错误转换：serde_json::Error → AppError（裁剪路径的 ? 需要）
    let error = files
        .iter()
        .find(|f| f.path.ends_with("error.rs"))
        .unwrap()
        .content
        .clone();
    assert!(
        error.contains("impl From<serde_json::Error> for AppError"),
        "{error}"
    );
}

#[test]
fn backend_without_policies_is_unchanged() {
    let dir = tmp_dir("no-policy");
    write_project(
        &dir,
        r#"
app fp {
  version = "0.1.0"
}

entity Item {
  id   Uuid      @id @default(uuid)
  name String(50) @required
}

api Items {
  entity Item
  operations [list get]
}
"#,
    );
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    let files = forma::emit::emit_rust_backend(&dir, &program, "generated/backend", "fp").unwrap();
    let routes = files
        .iter()
        .find(|f| f.path.ends_with("routes.rs"))
        .unwrap()
        .content
        .clone();
    // 无策略 → 零裁剪痕迹（零行为变化原则）
    assert!(!routes.contains("trim_"), "{routes}");
    assert!(routes.contains("Ok(Json(row))"), "{routes}");
    assert!(routes.contains("Json<Vec<ItemListRow>>"), "{routes}");
}

// ── MCP 集成：forma_check 报告字段策略无漂移 ─────────────────────

#[test]
fn field_policy_project_checks_clean() {
    let dir = tmp_dir("mcp");
    write_project(&dir, VALID_FM);
    let out = forma::mcp::call_tool(&dir, "forma_check", &serde_json::Map::new()).unwrap();
    assert_eq!(out["errors"], 0, "{out}");
}

// ── 前端显隐（体验层）：Web / Flutter 按角色隐藏字段 ─────────────────

#[test]
fn frontend_hides_fields_by_role() {
    let dir = tmp_dir("frontend");
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: fp\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web\n  mobile:\n    framework: flutter\n    out: generated/mobile\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FRONTEND_FM).unwrap();
    let (program, diags) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    forma::emit::emit_web_react(
        &dir,
        &program,
        "generated/web",
        "fp",
        false,
        false,
        "echarts",
    )
    .unwrap();
    forma::emit::emit_flutter(&dir, &program, "generated/mobile", "fp").unwrap();

    // ── Web：access.ts 提供字段规则 + canSeeField ──
    let access = fs::read_to_string(dir.join("generated/web/src/access.ts")).unwrap();
    assert!(access.contains("export const FIELD_RULES"), "{access}");
    assert!(
        access.contains("\"grade\": [\"hr\", \"manager\"]"),
        "{access}"
    );
    assert!(access.contains("export function canSeeField("), "{access}");
    // 角色未知 → 不隐藏
    assert!(access.contains("if (!role) return true;"), "{access}");

    // 列表页：列按角色隐藏
    let list = fs::read_to_string(dir.join("generated/web/src/pages/EmployeeList.tsx")).unwrap();
    assert!(
        list.contains("import { canSeeField, useCurrentRole } from \"../access\";"),
        "{list}"
    );
    assert!(list.contains("const role = useCurrentRole();"), "{list}");
    assert!(
        list.contains("canSeeField(\"Employee\", \"grade\", role)"),
        "list column must be role-gated:\n{list}"
    );
    // 未受约束的列不受影响
    assert!(list.contains("\"Name\""), "{list}");

    // 表单页：字段按角色隐藏
    let form = fs::read_to_string(dir.join("generated/web/src/pages/EmployeeEdit.tsx")).unwrap();
    assert!(
        form.contains("canSeeField(\"Employee\", \"grade\", role)"),
        "{form}"
    );

    // ── Flutter：access.dart 提供字段规则 + canSeeField ──
    let daccess = fs::read_to_string(dir.join("generated/mobile/lib/access.dart")).unwrap();
    assert!(
        daccess.contains("const Map<String, Map<String, List<String>>> fieldRules"),
        "{daccess}"
    );
    assert!(daccess.contains("'grade': ['hr', 'manager']"), "{daccess}");
    assert!(
        daccess.contains("bool canSeeField(String entity, String field, String? role)"),
        "{daccess}"
    );

    let dlist =
        fs::read_to_string(dir.join("generated/mobile/lib/pages/employee_list.dart")).unwrap();
    assert!(
        dlist.contains("canSeeField('Employee', 'grade', Api.currentRole)"),
        "{dlist}"
    );
    let dform =
        fs::read_to_string(dir.join("generated/mobile/lib/pages/employee_edit.dart")).unwrap();
    assert!(
        dform.contains("canSeeField('Employee', 'grade', Api.currentRole)"),
        "{dform}"
    );
}

#[test]
fn projects_without_field_policies_have_no_frontend_field_gating() {
    // 零变化原则：无字段策略 → 不生成 FIELD_RULES / canSeeField 引用
    let dir = tmp_dir("no-field-gate");
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: fp\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app fp {
  version = "0.1.0"
}

entity Item {
  id   Uuid      @id @default(uuid)
  name String(50) @required
}

api Items {
  entity Item
  operations [list get create update delete]
}

page ItemList {
  route "/items"
  title { zh "条目" en "Items" }
  table {
    query   Items.list
    columns [name]
  }
}
"#,
    )
    .unwrap();
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();
    forma::emit::emit_web_react(
        &dir,
        &program,
        "generated/web",
        "fp",
        false,
        false,
        "echarts",
    )
    .unwrap();
    assert!(
        !dir.join("generated/web/src/access.ts").exists(),
        "no policies → no access.ts"
    );
    let list = fs::read_to_string(dir.join("generated/web/src/pages/ItemList.tsx")).unwrap();
    assert!(!list.contains("canSeeField"), "{list}");
}

const FRONTEND_FM: &str = r#"
app fp {
  version = "0.1.0"
}

entity Employee {
  id    Uuid       @id @default(uuid)
  name  String(50) @required
  grade String(20)
  note  String(200)
}

api Employees {
  entity Employee
  operations [list get create update delete]
  list { filters [name] }
}

auth module {
  provider jwt
  roles [hr, manager, staff]
}

rules AccessRules {
  access Employee {
    read → any
    fields {
      grade → role in ["hr", "manager"]
    }
  }
}

page EmployeeList {
  route "/employees"
  title { zh "员工" en "Employees" }
  table {
    query   Employees.list
    columns [name, grade, note]
    actions {
      create → EmployeeEdit
      row    [edit → EmployeeEdit, delete → Employees.delete confirm]
    }
  }
}

page EmployeeEdit {
  route "/employees/{id}"
  title { zh "编辑员工" en "Edit employee" }
  form Employee {
    layout stack {
      name  Input @required
      grade Input
      note  Input
    }
    submit → Employees.update redirect "/employees"
    submit create → Employees.create redirect "/employees"
  }
}
"#;
