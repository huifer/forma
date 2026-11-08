//! RFC-0008 §18 / RFC-0005 §18：策略即测试——权限向量从 DSL 规则派生，并与前端语义一致。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-accvec-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app accvec { version = "0.1.0" }
auth module { provider jwt roles [admin, teacher, student] }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
}
api Students { entity Student operations [list get create update delete] }
rules AccessRules {
  access Student {
    update → role in ["admin", "teacher"]
    delete → role == "admin"
  }
}
page StudentList {
  route "/students"
  title { en "Students" }
  table {
    query   Students.list
    columns [name]
    actions { row [delete → Students.delete confirm] }
  }
}
"#;

fn vectors(dir: &Path) -> Vec<forma::validation_vectors::AccessVector> {
    let (p, _) = forma::dsl::load_program(dir, &["forma/**/*.fm".into()]).unwrap();
    forma::validation_vectors::access_vectors(&p)
}

#[test]
fn access_vectors_derive_from_dsl_rules() {
    let dir = tmp("derive");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: accvec\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let v = vectors(&dir);

    // update：admin/teacher 允许，student 拒绝
    for role in ["admin", "teacher"] {
        assert!(
            v.iter()
                .any(|x| x.entity == "Student" && x.op == "update" && x.role == role && x.allowed),
            "{role} should be allowed on update: {v:?}"
        );
    }
    assert!(
        v.iter().any(|x| x.entity == "Student"
            && x.op == "update"
            && x.role == "student"
            && !x.allowed),
        "student must be denied on update: {v:?}"
    );
    // delete：仅 admin
    assert!(v
        .iter()
        .any(|x| x.op == "delete" && x.role == "admin" && x.allowed));
    assert!(v
        .iter()
        .any(|x| x.op == "delete" && x.role == "teacher" && !x.allowed));
    // 无规则的操作（list/get/create）不产生向量（未声明即不约束）
    assert!(!v.iter().any(|x| x.op == "list"));
}

#[test]
fn read_op_expands_to_list_and_get() {
    let dir = tmp("read");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: accvec\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app accvec { version = "0.1.0" }
auth module { provider jwt roles [admin] }
entity Student { id Uuid @id @default(uuid) name String(50) @required }
api Students { entity Student operations [list get] }
rules R { access Student { read → role == "admin" } }
"#,
    )
    .unwrap();
    let v = vectors(&dir);
    assert!(v.iter().any(|x| x.op == "list" && x.allowed));
    assert!(v.iter().any(|x| x.op == "get" && x.allowed));
}

#[test]
fn generated_web_and_dart_tests_carry_the_same_vectors() {
    // 跨端一致性：Web 与 Dart 测试文件由同一派生结果渲染（结构断言）
    let dir = tmp("generated");
    fs::write(dir.join("forma.yaml"), "project:\n  name: accvec\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\ntargets:\n  web:\n    framework: react\n    out: generated/web\n  mobile:\n    out: app_mobile\n").unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    let web_test = report
        .steps
        .iter()
        .flat_map(|s| s.files.iter())
        .find(|f| f.path.ends_with("src/access.test.ts"))
        .expect("web access test generated");
    let dart_test = report
        .steps
        .iter()
        .flat_map(|s| s.files.iter())
        .find(|f| f.path.ends_with("test/access_vector_test.dart"))
        .expect("dart access test generated");
    let v = vectors(&dir);
    for x in &v {
        assert!(
            web_test.content.contains(&format!(
                "{{ entity: \"{}\", op: \"{}\", role: \"{}\", allowed: {} }}",
                x.entity, x.op, x.role, x.allowed
            )),
            "web test missing vector {x:?}"
        );
        assert!(
            dart_test.content.contains(&format!(
                "canForRole('{}', '{}', '{}'), {}",
                x.entity, x.op, x.role, x.allowed
            )),
            "dart test missing vector {x:?}"
        );
    }
}
