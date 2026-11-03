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
    assert_eq!(program.commands.len(), 2, "报名批准/驳回命令");
    assert_eq!(program.pages.len(), 14); // 12 管理 + 2 图表占位页

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

#[test]
fn auth_module_parses() {
    let dsl = r#"
app demo { version = "0.1.0" }
auth module {
  provider jwt
  roles [admin, teacher, student]
  tokenHours 12
}
"#;
    let program = forma::dsl::parser::parse(&forma::dsl::lexer::lex(dsl)).program;
    let auth = program.auth.expect("auth module");
    assert_eq!(auth.provider, "jwt");
    assert_eq!(auth.roles, vec!["admin", "teacher", "student"]);
    assert_eq!(auth.token_hours, 12);
}

#[test]
fn command_operations_parse() {
    let dsl = r#"
app demo { version = "0.1.0" }
enum St { pending approved }
entity Order {
  id Uuid @id @default(uuid)
  status St @default(pending)
  note Text?
}
api OrderApprove {
  POST "/orders/{id}/approve"
  label { zh "审批" en "Approve" }
  input { comment Text? }
  pre { status == "pending" }
  message { zh "只有待审批的订单可以审批" en "Only pending orders can be approved" }
  effect {
    set Order.status = approved
    set Order.note = input.comment
  }
  audit { action "order.approve" }
}
"#;
    let outcome = forma::dsl::parser::parse(&forma::dsl::lexer::lex(dsl));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let program = outcome.program;
    assert_eq!(program.commands.len(), 1);
    let cmd = &program.commands[0];
    assert_eq!(cmd.method, "POST");
    assert_eq!(cmd.path, "/orders/{id}/approve");
    assert_eq!(cmd.input.len(), 1);
    assert_eq!(cmd.input[0].name, "comment");
    assert!(cmd.input[0].optional);
    assert!(cmd.pre.is_some());
    assert_eq!(cmd.effects.len(), 2);
    assert_eq!(cmd.audit.as_deref(), Some("order.approve"));
    assert_eq!(cmd.label.len(), 2);
}
