use forma::engine::load_project;
use forma::patch::{apply_patch, PatchDocument, PatchFieldSpec, PatchOperation};
use forma::scaffold::scaffold_project;
use std::fs;
use std::path::PathBuf;

fn tmp_project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forma-patch-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    scaffold_project(&dir).unwrap();
    dir
}

fn add_payment_status_patch() -> PatchDocument {
    PatchDocument {
        operations: vec![PatchOperation::AddEntityField {
            entity: "User".into(),
            field: PatchFieldSpec {
                name: "paymentStatus".into(),
                r#type: "string".into(),
                r#enum: vec!["unpaid".into(), "paid".into()],
                required: false,
                description: None,
                format: None,
                sql: None,
            },
        }],
    }
}

#[test]
fn add_entity_field_propagates_to_all_standards() {
    let dir = tmp_project();
    let report = apply_patch(&dir, &add_payment_status_patch()).unwrap();

    let op = &report.operations[0];
    assert_eq!(op.status, "applied", "{op:?}");
    let standards: std::collections::HashSet<&str> =
        op.edits.iter().map(|e| e.standard.as_str()).collect();
    assert!(standards.contains("sql"), "{op:?}");
    assert!(standards.contains("json-schema"), "{op:?}");
    assert!(standards.contains("openapi"), "{op:?}");
    assert!(standards.contains("json-forms"), "{op:?}");

    let sql = fs::read_to_string(dir.join("specs/database/schema.sql")).unwrap();
    assert!(sql.contains("payment_status TEXT"), "{sql}");

    let schema: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("specs/schemas/user.schema.json")).unwrap(),
    )
    .unwrap();
    let payment = schema.pointer("/properties/paymentStatus").unwrap();
    assert_eq!(
        payment.get("enum").unwrap(),
        &serde_json::json!(["unpaid", "paid"])
    );
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(!required.iter().any(|r| r == "paymentStatus"));

    let openapi = fs::read_to_string(dir.join("specs/api/openapi.yaml")).unwrap();
    assert!(openapi.contains("paymentStatus:"), "{openapi}");
    assert!(openapi.contains("- unpaid"));

    let ui: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("specs/ui/user-form.ui.json")).unwrap())
            .unwrap();
    let scopes: Vec<&str> = ui
        .get("elements")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.get("scope").unwrap().as_str().unwrap())
        .collect();
    assert!(scopes.contains(&"#/properties/paymentStatus"));

    assert!(report
        .build
        .as_ref()
        .map(|b| !b.files.is_empty())
        .unwrap_or(false));

    // 新字段出现在语义图
    let outcome = load_project(&dir).unwrap();
    assert!(outcome
        .graph
        .nodes_by_kind(foma_entity_field())
        .iter()
        .any(|n| n.name == "User.paymentStatus"));
}

fn foma_entity_field() -> forma::graph::NodeKind {
    forma::graph::NodeKind::EntityField
}

#[test]
fn rename_entity_field_renames_everywhere() {
    let dir = tmp_project();
    let report = apply_patch(
        &dir,
        &PatchDocument {
            operations: vec![PatchOperation::RenameEntityField {
                entity: "User".into(),
                from: "status".into(),
                to: "accountStatus".into(),
            }],
        },
    )
    .unwrap();
    assert_eq!(
        report.operations[0].status, "applied",
        "{:?}",
        report.operations[0]
    );
    let sql = fs::read_to_string(dir.join("specs/database/schema.sql")).unwrap();
    assert!(sql.contains("account_status"));
    assert!(!sql.contains("\n    status TEXT"));

    let schema: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("specs/schemas/user.schema.json")).unwrap(),
    )
    .unwrap();
    assert!(schema.pointer("/properties/accountStatus").is_some());
    assert!(schema.pointer("/properties/status").is_none());
    let required: Vec<&str> = schema
        .get("required")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert!(required.contains(&"accountStatus"), "{required:?}");

    let ui = fs::read_to_string(dir.join("specs/ui/user-form.ui.json")).unwrap();
    assert!(ui.contains("#/properties/accountStatus"));
}

#[test]
fn set_entity_field_required_toggles_not_null_and_required() {
    let dir = tmp_project();
    let report = apply_patch(
        &dir,
        &PatchDocument {
            operations: vec![PatchOperation::SetEntityFieldRequired {
                entity: "User".into(),
                field: "name".into(),
                required: false,
            }],
        },
    )
    .unwrap();
    assert_eq!(
        report.operations[0].status, "applied",
        "{:?}",
        report.operations[0]
    );
    let sql = fs::read_to_string(dir.join("specs/database/schema.sql")).unwrap();
    assert!(!sql.contains("name TEXT NOT NULL"));

    let schema: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("specs/schemas/user.schema.json")).unwrap(),
    )
    .unwrap();
    let required: Vec<&str> = schema
        .get("required")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert!(!required.contains(&"name"));
}

#[test]
fn failed_patch_rolls_back_to_original_state() {
    let dir = tmp_project();
    let before = fs::read_to_string(dir.join("specs/database/schema.sql")).unwrap();
    let report = apply_patch(
        &dir,
        &PatchDocument {
            operations: vec![PatchOperation::AddEntityField {
                entity: "NoSuchEntity".into(),
                field: PatchFieldSpec {
                    name: "x".into(),
                    r#type: "string".into(),
                    r#enum: vec![],
                    required: false,
                    description: None,
                    format: None,
                    sql: None,
                },
            }],
        },
    )
    .unwrap();
    assert_eq!(report.operations[0].status, "failed");
    let after = fs::read_to_string(dir.join("specs/database/schema.sql")).unwrap();
    assert_eq!(before, after, "files unchanged after failed patch");
}
