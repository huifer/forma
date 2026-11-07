//! 移动端角色感知回归测试（P2-6，与 web_access.rs 同构）：
//! - DSL 的 `api { auth }` / `rules { access }` → 生成的 Flutter 按当前用户角色隐藏操作；
//! - **仅体验层**：服务端才是安全边界（ADR-0004）——前端只在「已知角色不足」时隐藏；
//! - 无授权规则的工程**不受影响**（不生成 access.dart，不引入 import）。

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
        "forma-flutteraccess-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const WITH_RULES_FM: &str = r#"
app flaccess {
  version = "0.1.0"
}

entity Student {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

entity School {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

api Students {
  entity Student
  operations [list get create update delete]
  list { filters [name] }
}

api Schools {
  entity School
  operations [list get create update delete]
  list { filters [name] }
}

rules AccessRules {
  access Student {
    update → role in ["admin", "teacher"]
    delete → role == "admin"
  }
}

auth module {
  provider jwt
  roles [admin, teacher, student]
}

page StudentList {
  route "/students"
  title { zh "学生" en "Students" }
  table {
    query   Students.list
    columns [name]
    actions {
      create → StudentEdit
      row    [edit → StudentEdit, delete → Students.delete confirm]
    }
  }
}

page StudentEdit {
  route "/students/{id}"
  title { zh "编辑学生" en "Edit student" }
  form Student {
    layout stack {
      name Input @required
    }
    submit → Students.update redirect "/students"
    submit create → Students.create redirect "/students"
  }
}

page SchoolList {
  route "/schools"
  title { zh "学校" en "Schools" }
  table {
    query   Schools.list
    columns [name]
    actions {
      create → SchoolEdit
      row    [edit → SchoolEdit, delete → Schools.delete confirm]
    }
  }
}

page SchoolEdit {
  route "/schools/{id}"
  title { zh "编辑学校" en "Edit school" }
  form School {
    layout stack {
      name Input @required
    }
    submit → Schools.update redirect "/schools"
    submit create → Schools.create redirect "/schools"
  }
}
"#;

fn generate(dir: &Path, fm: &str) -> PathBuf {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
    let (program, diags) = forma::dsl::load_program(dir, &["forma/**/*.fm".to_string()]).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    let out = dir.join("app_mobile");
    forma::emit::emit_flutter(dir, &program, &out.to_string_lossy(), "flaccess").unwrap();
    out
}

fn read(out: &Path, rel: &str) -> String {
    fs::read_to_string(out.join(rel)).unwrap()
}

// ── 1. 有授权规则 → 生成 access.dart + 门控 ──────────────────────

#[test]
fn access_rules_produce_access_dart() {
    let out = generate(&tmp_dir("access-dart"), WITH_RULES_FM);
    let access = read(&out, "lib/access.dart");
    // 规则内容：实体 → 受约束操作 + 允许角色（api 级与实体级规则的角色并集）
    assert!(
        access.contains("'Student'"),
        "access.dart must contain the Student rule:\n{access}"
    );
    assert!(
        access.contains("'update'") && access.contains("'delete'"),
        "access.dart must contain gated ops:\n{access}"
    );
    assert!(
        access.contains("'admin'") && access.contains("'teacher'"),
        "access.dart must contain roles:\n{access}"
    );
    // 角色感知判定
    assert!(
        access.contains("bool can(String entity, String op)"),
        "access.dart must expose can():\n{access}"
    );
    // 关键语义：角色未知时**不隐藏**（宁可让服务端判定，也不误伤）
    assert!(
        access.contains("if (role == null) return true;"),
        "unknown role must not hide actions:\n{access}"
    );
}

#[test]
fn gated_buttons_are_wrapped_in_can_checks() {
    let out = generate(&tmp_dir("gating"), WITH_RULES_FM);
    let list = read(&out, "lib/pages/student_list.dart");
    // import + 门控
    assert!(
        list.contains("import '../access.dart';"),
        "page must import access.dart:\n{list}"
    );
    assert!(
        list.contains("if (can('Student', 'update'))"),
        "edit button must be gated:\n{list}"
    );
    assert!(
        list.contains("if (can('Student', 'delete'))"),
        "delete button must be gated:\n{list}"
    );
    // create 未受约束 → FAB 照常生成且不门控（0 行为变化原则）
    assert!(
        list.contains("floatingActionButton: FloatingActionButton.extended("),
        "unconstrained create FAB must stay ungated:\n{list}"
    );
    assert!(
        !list.contains("can('Student', 'create')"),
        "unconstrained op must not be gated:\n{list}"
    );
    // 无规则的实体：不得引入 access.dart / 不得包裹按钮
    let school = read(&out, "lib/pages/school_list.dart");
    assert!(
        !school.contains("access.dart"),
        "unrestricted page must not import access.dart:\n{school}"
    );
    assert!(
        !school.contains("can('School'"),
        "unrestricted page must not gate buttons:\n{school}"
    );
    // 编辑页：保存按钮按提交操作门控
    let edit = read(&out, "lib/pages/student_edit.dart");
    assert!(
        edit.contains("import '../access.dart';") && edit.contains("can('Student', 'update')"),
        "edit page must gate the save button:\n{edit}"
    );
    let school_edit = read(&out, "lib/pages/school_edit.dart");
    assert!(
        !school_edit.contains("access.dart"),
        "unrestricted edit page must not gate:\n{school_edit}"
    );
}

// ── 2. 登录后写入当前角色 ────────────────────────────────────────

#[test]
fn login_stores_current_role() {
    let out = generate(&tmp_dir("role"), WITH_RULES_FM);
    let api = read(&out, "lib/api/api.dart");
    assert!(
        api.contains("static String? currentRole;"),
        "Api must expose currentRole:\n{api}"
    );
    assert!(
        api.contains("Api.currentRole = token.role;"),
        "login/register must store the role:\n{api}"
    );
    let login = read(&out, "lib/pages/login.dart");
    assert!(
        login.contains("Api.currentRole = res.role;"),
        "login page must store the role:\n{login}"
    );
}

// ── 3. 无授权规则 → 完全不生成 access.dart ───────────────────────

#[test]
fn project_without_rules_has_no_access_dart() {
    let fm = r#"
app flaccess {
  version = "0.1.0"
}

entity Student {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

api Students {
  entity Student
  operations [list get create update delete]
  list { filters [name] }
}

page StudentList {
  route "/students"
  title { zh "学生" en "Students" }
  table {
    query   Students.list
    columns [name]
    actions {
      create → StudentEdit
      row    [edit → StudentEdit, delete → Students.delete confirm]
    }
  }
}

page StudentEdit {
  route "/students/{id}"
  title { zh "编辑学生" en "Edit student" }
  form Student {
    layout stack {
      name Input @required
    }
    submit → Students.update redirect "/students"
    submit create → Students.create redirect "/students"
  }
}
"#;
    let out = generate(&tmp_dir("no-rules"), fm);
    assert!(
        !out.join("lib/access.dart").exists(),
        "no rules → no access.dart (no behavioral change for existing projects)"
    );
    let list = read(&out, "lib/pages/student_list.dart");
    assert!(
        !list.contains("access.dart"),
        "no rules → no access import in pages:\n{list}"
    );
    assert!(
        !list.contains("can("),
        "no rules → no can() gating in pages:\n{list}"
    );
}
