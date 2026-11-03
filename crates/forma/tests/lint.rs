use forma::dsl::lexer::lex;
use forma::dsl::parser::parse;
use forma::lint::run_lint;

fn program(src: &str) -> forma::dsl::Program {
    parse(&lex(src)).program
}

#[test]
fn lint_reports_missing_id() {
    let p = program("app demo { version = \"0.1.0\" }\nentity Bad { name String(50) @required }");
    let diags = run_lint(&p, None);
    assert!(
        diags.iter().any(|d| d.rule == "completeness/require-id"),
        "{diags:?}"
    );
}

#[test]
fn lint_reports_missing_timestamps() {
    let p = program("app demo { version = \"0.1.0\" }\nentity Bad { id Uuid @id @default(uuid) }");
    let diags = run_lint(&p, None);
    assert!(
        diags
            .iter()
            .any(|d| d.rule == "completeness/require-timestamps"),
        "{diags:?}"
    );
}

#[test]
fn lint_reports_non_pascal_entity() {
    let p = program(
        "app demo { version = \"0.1.0\" }\nentity bad_entity { id Uuid @id @default(uuid) }",
    );
    let diags = run_lint(&p, None);
    assert!(
        diags.iter().any(|d| d.rule == "naming/entity-pascal-case"),
        "{diags:?}"
    );
}

#[test]
fn lint_reports_non_camel_field() {
    let p = program("app demo { version = \"0.1.0\" }\nentity Good { id Uuid @id @default(uuid) my_field String(50) }");
    let diags = run_lint(&p, None);
    assert!(
        diags.iter().any(|d| d.rule == "naming/field-camel-case"),
        "{diags:?}"
    );
}

#[test]
fn lint_clean_entity_passes() {
    let p = program(
        r#"
app demo { version = "0.1.0" }
entity User {
  id Uuid @id @default(uuid)
  name String(100) @required
  createdAt Timestamp @default(now) @readonly
}
"#,
    );
    let diags = run_lint(&p, None);
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn lint_reports_missing_index_on_relation() {
    let p = program(
        r#"
app demo { version = "0.1.0" }
entity School { id Uuid @id @default(uuid) name String(100) @required }
entity Student {
  id Uuid @id @default(uuid)
  school School @required
  name String(50) @required
}
"#,
    );
    let diags = run_lint(&p, None);
    assert!(
        diags.iter().any(|d| d.rule == "performance/missing-index"),
        "{diags:?}"
    );
}

#[test]
fn lint_reports_email_not_unique() {
    let p = program("app demo { version = \"0.1.0\" }\nentity User { id Uuid @id @default(uuid) email Email @required }");
    let diags = run_lint(&p, None);
    assert!(
        diags.iter().any(|d| d.rule == "security/email-unique"),
        "{diags:?}"
    );
}

#[test]
fn lint_reports_page_route_not_kebab() {
    let src = r#"
app demo { version = "0.1.0" }
entity User { id Uuid @id @default(uuid) name String(50) @required }
api Users { entity User operations [list] }
page UserList {
  route "/user_list"
  title "Users"
  table { query Users.list columns [name] }
}
"#;
    let p = program(src);
    let diags = run_lint(&p, None);
    assert!(
        diags
            .iter()
            .any(|d| d.rule == "convention/page-route-kebab"),
        "{diags:?}"
    );
}
