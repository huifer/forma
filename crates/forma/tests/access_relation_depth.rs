//! A-2：行级访问条件**任意深度关系遍历**回归。
//!
//! 验收：
//! - 检查层：多层路径通过；非关系 / 可选关系 / 未知字段仍显式诊断（fail-closed）；
//! - 生成层：逐跳 `repo::find_{E}` 取数（关系行缺失 → 403）、无未绑定别名；
//!   即使目标实体没有 get/update/delete API 也生成 find 函数；
//! - 无租户维度工程不引用 `user.tenant`（存量编译错误回归）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-a2-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const HEAD: &str = r#"
app a2 {
  version = "0.1.0"
}

auth module {
  provider jwt
  roles [admin]
  tokenHours 12
}

entity District {
  id   Uuid       @id @default(uuid)
  name String(50) @required
  tier Int
}

entity School {
  id       Uuid       @id @default(uuid)
  district District   @required
  name     String(50) @required
}

entity Student {
  id     Uuid       @id @default(uuid)
  school School     @required
  name   String(50) @required
  amount Decimal(10,2)
}

api Students {
  entity Student
  operations [get update delete]
}
"#;

fn project(dir: &Path, rule: &str) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: a2\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        format!("{HEAD}\nrules R {{\n  access Student {{\n    update → {rule}\n  }}\n}}\n"),
    )
    .unwrap();
}

fn check(dir: &Path) -> Vec<String> {
    let (p, diags) = forma::dsl::load_program(dir, &["forma/**/*.fm".into()]).unwrap();
    let mut codes: Vec<String> = diags
        .iter()
        .filter(|d| d.severity == "error")
        .map(|d| d.code.clone())
        .collect();
    codes.extend(
        forma::dsl::check_program(&p)
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| d.code.clone()),
    );
    codes
}

#[test]
fn multi_hop_relation_paths_are_accepted() {
    for rule in [
        "role == \"admin\" && school.name == \"X\"",
        "role == \"admin\" && school.district.name == \"X\"",
        "role == \"admin\" && school.district.tier > 2",
    ] {
        let dir = tmp("accept");
        project(&dir, rule);
        let codes = check(&dir);
        assert!(
            codes.is_empty(),
            "rule `{rule}` should be accepted, got {codes:?}\n{}",
            fs::read_to_string(dir.join("forma/app.fm")).unwrap()
        );
    }
}

#[test]
fn invalid_paths_still_fail_closed() {
    // 非关系遍历
    let dir = tmp("nonrelation");
    project(&dir, "role == \"admin\" && name.length == \"X\"");
    let codes = check(&dir);
    assert!(codes.iter().any(|c| c == "dsl-path"), "{codes:?}");
    assert!(
        codes.iter().any(|c| c == "dsl-access-relation-unsupported"),
        "{codes:?}"
    );

    // 可选关系遍历
    let dir = tmp("optional");
    fs::write(
        dir.join("forma/app.fm"),
        format!(
            "{}\nrules R {{\n  access Student {{\n    update → school.name == \"X\"\n  }}\n}}\n",
            HEAD.replace("school School     @required", "school School?")
        ),
    )
    .unwrap();
    let codes = check(&dir);
    assert!(
        codes
            .iter()
            .any(|c| c == "dsl-access-relation-optional-unsupported"),
        "{codes:?}"
    );

    // 深层未知字段
    let dir = tmp("unknown");
    project(&dir, "role == \"admin\" && school.district.nope == \"X\"");
    let codes = check(&dir);
    assert!(codes.iter().any(|c| c == "dsl-unknown-field"), "{codes:?}");
}

#[test]
fn generated_backend_fetches_relation_rows_per_hop_and_fails_closed() {
    let dir = tmp("emit");
    project(
        &dir,
        "role == \"admin\" && school.district.name == \"X\" && amount > 100",
    );
    forma::build::run_build(&dir).unwrap();

    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    // 逐跳取数
    assert!(
        routes.contains("repo::find_school(&db, &row.school_id"),
        "{routes}"
    );
    assert!(
        routes.contains("repo::find_district(&db, &__row1.district_id"),
        "{routes}"
    );
    // 关系行缺失 → 403（不是兜底值放行）
    assert!(routes.contains("access relation row missing"), "{routes}");
    assert!(routes.contains("AppError::Forbidden"), "{routes}");
    // 最终字段绑定，无未绑定的 SQL 别名残留
    assert!(routes.contains("__row2.name.clone()"), "{routes}");
    assert!(!routes.contains("t1.name"), "{routes}");
    // 无租户维度 → 不引用 user.tenant
    assert!(!routes.contains("user.tenant"), "{routes}");
    // Decimal 字面量按最终字段类型渲染
    assert!(
        routes.contains("rust_decimal::Decimal::try_from(100i64)"),
        "{routes}"
    );

    // 目标实体没有 get/update/delete API 也必须生成 find 函数
    let repo = fs::read_to_string(dir.join("generated/backend/src/repo.rs")).unwrap();
    assert!(repo.contains("pub async fn find_school("), "{repo}");
    assert!(repo.contains("pub async fn find_district("), "{repo}");
}

#[test]
fn one_hop_traversal_still_generates_valid_guard() {
    let dir = tmp("onehop");
    project(&dir, "role == \"admin\" && school.name == \"X\"");
    assert!(check(&dir).is_empty());
    forma::build::run_build(&dir).unwrap();
    let routes = fs::read_to_string(dir.join("generated/backend/src/routes.rs")).unwrap();
    assert!(
        routes.contains("repo::find_school(&db, &row.school_id"),
        "{routes}"
    );
    assert!(routes.contains("__row1.name.clone()"), "{routes}");
}
