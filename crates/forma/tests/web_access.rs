//! 前端角色感知回归测试（P0-3）：
//! - DSL 的 `api { auth }` / `rules { access }` → 生成的 Web 按当前用户角色隐藏操作；
//! - **仅体验层**：服务端才是安全边界（ADR-0004）——前端只在「已知角色不足」时隐藏；
//! - 无授权规则的工程**不受影响**（不生成 access.ts，不引入 hook）。
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
        "forma-webaccess-{tag}-{}-{seq}-{nanos}",
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
        "project:\n  name: webaccess\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const WITH_RULES_FM: &str = r#"
app webaccess {
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
  form {
    query  Students.get
    submit Students.update
    fields [name]
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
  form {
    query  Schools.get
    submit Schools.update
    fields [name]
  }
}
"#;

fn generate_web(dir: &Path) -> String {
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(&loaded.dir, &program, &out.to_string_lossy(), "webaccess")
        .unwrap();
    fs::read_to_string(out.join("src/pages/StudentList.tsx")).unwrap()
}

// ── 1. 有授权规则 → 生成 access.ts + 门控 ────────────────────────

#[test]
fn access_rules_produce_access_ts() {
    let dir = tmp_dir("access-ts");
    write_project(&dir, WITH_RULES_FM);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(&loaded.dir, &program, &out.to_string_lossy(), "webaccess")
        .unwrap();

    let access = fs::read_to_string(out.join("src/access.ts")).unwrap();
    // 规则内容：实体 → 受约束操作 + 允许角色
    assert!(
        access.contains("\"Student\""),
        "access.ts must contain the Student rule:\n{access}"
    );
    assert!(
        access.contains("\"update\"") && access.contains("\"admin\""),
        "access.ts must contain ops and roles:\n{access}"
    );
    // 角色 hook
    assert!(
        access.contains("useCurrentRole") && access.contains("useCan"),
        "access.ts must expose the role hooks:\n{access}"
    );
    // 关键语义：角色未知时**不隐藏**（宁可让服务端判定，也不误伤）
    assert!(
        access.contains("if (!role) return true;"),
        "unknown role must not hide actions:\n{access}"
    );
}

#[test]
fn gated_buttons_are_wrapped_in_can_checks() {
    let dir = tmp_dir("gating");
    write_project(&dir, WITH_RULES_FM);
    let page = generate_web(&dir);

    // import + hook
    assert!(
        page.contains("import { useCan } from \"../access\";"),
        "page must import useCan:\n{page}"
    );
    assert!(
        page.contains("const canUpdate = useCan(\"Student\", \"update\");"),
        "page must compute canUpdate:\n{page}"
    );
    assert!(
        page.contains("const canDelete = useCan(\"Student\", \"delete\");"),
        "page must compute canDelete:\n{page}"
    );
    // 受约束操作的按钮被包裹
    assert!(
        page.contains("{canUpdate && <a"),
        "edit button must be gated:\n{page}"
    );
    assert!(
        page.contains("{canDelete && <Button"),
        "delete button must be gated:\n{page}"
    );
}

#[test]
fn unrestricted_entity_is_untouched() {
    let dir = tmp_dir("untouched");
    write_project(&dir, WITH_RULES_FM);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(&loaded.dir, &program, &out.to_string_lossy(), "webaccess")
        .unwrap();
    let school = fs::read_to_string(out.join("src/pages/SchoolList.tsx")).unwrap();

    // 无规则的实体：不得引入 useCan / 不得包裹按钮
    assert!(
        !school.contains("useCan"),
        "unrestricted page must not import useCan:\n{school}"
    );
    assert!(
        !school.contains("{canUpdate &&"),
        "unrestricted page must not gate buttons:\n{school}"
    );
    assert!(
        school.contains("<a className=\"btn btn-ghost\""),
        "unrestricted page keeps the plain edit link:\n{school}"
    );
}

// ── 2. 无授权规则 → 完全不生成 access.ts ─────────────────────────

#[test]
fn project_without_rules_has_no_access_ts() {
    let dir = tmp_dir("no-rules");
    write_project(
        &dir,
        r#"
app webaccess {
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
  form {
    query  Students.get
    submit Students.update
    fields [name]
  }
}
"#,
    );
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(&loaded.dir, &program, &out.to_string_lossy(), "webaccess")
        .unwrap();

    assert!(
        !out.join("src/access.ts").exists(),
        "no rules → no access.ts (no behavioral change for existing projects)"
    );
    let page = fs::read_to_string(out.join("src/pages/StudentList.tsx")).unwrap();
    assert!(
        !page.contains("useCan"),
        "no rules → no useCan in pages:\n{page}"
    );
}

// ── 3. api 级与实体级规则合并（角色取交集） ───────────────────────

#[test]
fn api_level_and_entity_level_rules_are_merged() {
    let dir = tmp_dir("merged");
    write_project(
        &dir,
        r#"
app webaccess {
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
  auth {
    update → role in ["admin", "teacher"]
  }
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
  form {
    query  Students.get
    submit Students.update
    fields [name]
  }
}

rules AccessRules {
  access Student {
    update → role in ["teacher", "student"]
  }
}

auth module {
  provider jwt
  roles [admin, teacher, student]
}
"#,
    );
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(&loaded.dir, &program, &out.to_string_lossy(), "webaccess")
        .unwrap();
    let access = fs::read_to_string(out.join("src/access.ts")).unwrap();

    // 交集：{admin, teacher} ∩ {teacher, student} = {teacher}
    assert!(
        access.contains("\"teacher\""),
        "merged rule must contain the intersection role:\n{access}"
    );
    assert!(
        !access.contains("\"admin\""),
        "merged rule must drop non-intersection roles:\n{access}"
    );
    assert!(
        !access.contains("\"student\""),
        "merged rule must drop non-intersection roles:\n{access}"
    );
}
