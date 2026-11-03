use forma::dsl::{check_program, load_program};
use std::path::PathBuf;

fn school_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/school")
        .canonicalize()
        .unwrap()
}

#[test]
fn parse_school_use_case() {
    let dir = school_dir();
    let m = forma::manifest::load_manifest(&dir).unwrap();
    let (program, diags) = load_program(&dir, &m.manifest.source.clone().unwrap()).unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(program.entities.len(), 7);
    assert_eq!(program.enums.len(), 2);
    assert_eq!(program.apis.len(), 7);
    assert_eq!(program.pages.len(), 12);

    // 关系字段
    let student = program
        .entities
        .iter()
        .find(|e| e.name == "Student")
        .unwrap();
    assert_eq!(
        student
            .fields
            .iter()
            .find(|f| f.name == "school")
            .unwrap()
            .type_name,
        "School"
    );

    // 规则
    let rules = &program.rules[0];
    assert!(!rules.checks.is_empty());
    assert!(!rules.guards.is_empty());
}

#[test]
fn check_reports_unknown_field_in_rule() {
    let src = r#"
entity E {
  id Uuid @id @default(uuid)
  t Timestamp @default(now)
}
rules R {
  check E "msg" {
    t >= nonexistent
  }
}
"#;
    let toks = forma::dsl::lexer::lex(src);
    let out = forma::dsl::parser::parse(&toks);
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let diags = check_program(&out.program);
    assert!(
        diags.iter().any(|d| d.code == "dsl-unknown-field"),
        "{diags:?}"
    );
}

#[test]
fn check_reports_unknown_relation_target() {
    let src = r#"
entity A {
  id Uuid @id @default(uuid)
  b Nope @required
}
"#;
    let toks = forma::dsl::lexer::lex(src);
    let out = forma::dsl::parser::parse(&toks);
    let diags = check_program(&out.program);
    assert!(
        diags.iter().any(|d| d.code == "dsl-unknown-type"),
        "{diags:?}"
    );
}

#[test]
fn optional_marker_works() {
    let src = r#"
entity A {
  id Uuid @id @default(uuid)
  note Text?
}
"#;
    let toks = forma::dsl::lexer::lex(src);
    let out = forma::dsl::parser::parse(&toks);
    let note = out.program.entities[0]
        .fields
        .iter()
        .find(|f| f.name == "note")
        .unwrap();
    assert!(note.optional);
}
