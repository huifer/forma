use forma::engine::load_project;
use forma::graph::NodeKind;
use forma::validate::run_doctor;
use std::path::PathBuf;

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/order-management")
        .canonicalize()
        .unwrap()
}

#[test]
fn load_project_builds_the_semantic_graph_from_all_standards() {
    let outcome = load_project(&example_dir()).unwrap();
    let errors: Vec<_> = outcome
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let order = outcome
        .graph
        .entities()
        .into_iter()
        .find(|e| e.name == "Order")
        .expect("Order entity must exist");

    // table bound by convention (orders ≡ Order)
    let tables: Vec<_> = outcome
        .graph
        .edges_from(&order.uri)
        .into_iter()
        .filter(|e| e.kind == forma::graph::EdgeKind::Defines)
        .filter_map(|e| outcome.graph.get(&e.to))
        .filter(|n| n.kind == NodeKind::Table)
        .collect();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].name, "orders");

    // columns bound to fields
    let field_count = outcome
        .graph
        .edges_from(&order.uri)
        .into_iter()
        .filter(|e| e.kind == forma::graph::EdgeKind::BelongsTo)
        .filter(|e| {
            outcome
                .graph
                .get(&e.to)
                .map(|n| n.kind == NodeKind::EntityField)
                .unwrap_or(false)
        })
        .count();
    assert!(field_count >= 5, "json-schema fields registered: {field_count}");

    // bound columns (binds edges)
    let binds = outcome
        .graph
        .nodes_by_kind(NodeKind::EntityField)
        .into_iter()
        .filter(|n| {
            outcome
                .graph
                .edges_from(&n.uri)
                .iter()
                .any(|e| e.kind == forma::graph::EdgeKind::Binds)
        })
        .count();
    assert!(binds >= 4, "sql columns bound to entity fields: {binds}");

    // api operations
    assert_eq!(outcome.graph.nodes_by_kind(NodeKind::ApiOperation).len(), 3);

    // workflow
    let workflows = outcome.graph.nodes_by_kind(NodeKind::Workflow);
    assert_eq!(workflows.len(), 1);
    let wf = workflows[0];
    assert_eq!(
        wf.data.get("steps").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(5)
    );
    assert_eq!(
        wf.data.get("flows").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(5)
    );

    // ui form
    assert_eq!(outcome.graph.nodes_by_kind(NodeKind::UiForm).len(), 1);
}

#[test]
fn affected_by_changing_a_field_propagates_across_standards() {
    let outcome = load_project(&example_dir()).unwrap();
    let amount_field = outcome
        .graph
        .nodes_by_kind(NodeKind::EntityField)
        .into_iter()
        .find(|n| n.name == "Order.amount")
        .expect("Order.amount field");
    let affected = outcome.graph.affected_by(&amount_field.uri);
    let standards: std::collections::HashSet<String> = affected
        .iter()
        .map(|n| n.standard.as_str().to_string())
        .collect();
    assert!(standards.contains("sql"), "SQL column affected");
    assert!(standards.contains("json-schema"), "JSON Schema type affected");
}

#[test]
fn doctor_reports_no_type_mismatches_on_the_consistent_example() {
    let outcome = load_project(&example_dir()).unwrap();
    let diagnostics = run_doctor(&outcome.graph);
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.severity == "error").collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}
