//! MongoDB target（轨道 B 重新排期）回归：集合设计派生、engine 路由、默认零变化。

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
        "forma-mongo-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app mongodemo {
  version = "0.1.0"
}

entity School {
  id   Uuid       @id @default(uuid)
  name String(80) @required
}

entity Student {
  id     Uuid       @id @default(uuid)
  name   String(80) @required
  age    Int
  school School     @required
  active Bool
  note   Text
  createdAt Timestamp @default(now) @readonly
  index [school]
  unique [name, school]
}

api Students {
  entity Student
  operations [list get create update delete]
}
"#;

fn write_project(dir: &Path, engine: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: mongodemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: {engine}\n    out: generated/db\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn mongodb_emits_collection_design_and_shell_script() {
    let dir = tmp_dir("gen");
    write_project(&dir, "mongodb");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"db-mongodb"), "{names:?}");
    assert!(!names.contains(&"db-mysql"), "{names:?}");

    let spec: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("generated/db/collections.json")).unwrap(),
    )
    .unwrap();
    let cols = spec["collections"].as_array().unwrap();
    assert_eq!(cols.len(), 2, "{spec}");
    let students = cols.iter().find(|c| c["name"] == "students").unwrap();
    let schema = &students["validator"]["$jsonSchema"];
    assert_eq!(schema["bsonType"], "object");
    assert!(schema["required"].as_array().unwrap().contains(&serde_json::json!("name")));
    // 关系字段 → `{rel}_id`，BSON string
    assert_eq!(schema["properties"]["school_id"]["bsonType"], "string");
    assert_eq!(schema["properties"]["age"]["bsonType"], "int");
    assert_eq!(schema["properties"]["active"]["bsonType"], "bool");
    // 索引与唯一约束派生
    let indexes = students["indexes"].as_array().unwrap();
    assert!(indexes.iter().any(|i| i["unique"] == true), "{indexes:?}");

    let init = fs::read_to_string(dir.join("generated/db/0001_init.js")).unwrap();
    assert!(init.contains("db.createCollection('students'"), "{init}");
    assert!(init.contains("createIndex"), "{init}");
    let up = fs::read_to_string(dir.join("generated/db/migrations/0001_upgrade.js")).unwrap();
    assert!(up.contains("db.createCollection('students'"), "{up}");
    let readme = fs::read_to_string(dir.join("generated/db/README.md")).unwrap();
    assert!(readme.contains("students"), "{readme}");
}

#[test]
fn default_engine_has_no_mongodb_artifacts() {
    let dir = tmp_dir("default");
    write_project(&dir, "mysql");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"db-mysql"), "{names:?}");
    assert!(!names.contains(&"db-mongodb"), "{names:?}");
    assert!(!dir.join("generated/db/collections.json").exists());
}
