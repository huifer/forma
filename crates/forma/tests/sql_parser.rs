use forma::adapters::sql::parse_sql_ddl;

const DDL: &str = r#"
-- customers for reference
CREATE TABLE customers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE orders (
    id TEXT PRIMARY KEY,
    amount DECIMAL(12, 2) NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    customer_id TEXT,
    created_at TIMESTAMP NOT NULL,
    FOREIGN KEY (customer_id) REFERENCES customers(id)
);

CREATE UNIQUE INDEX idx_orders_code ON orders(code);
"#;

#[test]
fn parse_tables_and_columns() {
    let parsed = parse_sql_ddl(DDL).unwrap();
    assert_eq!(parsed.tables.len(), 2);
    let orders = parsed.tables.iter().find(|t| t.name == "orders").unwrap();
    assert_eq!(orders.columns.len(), 5);
    assert_eq!(orders.columns[0].name, "id");
    assert!(orders.columns[0].primary_key);
    assert_eq!(orders.columns[1].base_type, "DECIMAL");
    assert_eq!(orders.columns[1].type_text, "DECIMAL(12, 2)");
    assert!(orders.columns[1].not_null);
    assert_eq!(orders.columns[2].default_value.as_deref(), Some("pending"));
    assert!(!orders.columns[3].not_null);
}

#[test]
fn parse_table_constraints() {
    let parsed = parse_sql_ddl(DDL).unwrap();
    let orders = parsed.tables.iter().find(|t| t.name == "orders").unwrap();
    let fk = orders
        .constraints
        .iter()
        .find(|c| c.kind == "foreign-key")
        .unwrap();
    assert_eq!(
        fk.references.as_ref().unwrap(),
        &("customers".to_string(), vec!["id".to_string()])
    );
}

#[test]
fn parse_indexes() {
    let parsed = parse_sql_ddl(DDL).unwrap();
    assert_eq!(parsed.indexes.len(), 1);
    let idx = &parsed.indexes[0];
    assert_eq!(idx.name, "idx_orders_code");
    assert_eq!(idx.table, "orders");
    assert!(idx.unique);
    assert_eq!(idx.columns, vec!["code"]);
}

#[test]
fn parse_quoted_identifiers_and_block_comments() {
    let parsed = parse_sql_ddl(
        r#"
    /* block
       comment */
    CREATE TABLE "order items" (
        "id" TEXT PRIMARY KEY,
        qty INTEGER NOT NULL
    );
  "#,
    )
    .unwrap();
    assert_eq!(parsed.tables.len(), 1);
    assert_eq!(parsed.tables[0].name, "order items");
    assert_eq!(parsed.tables[0].columns[0].name, "id");
    assert_eq!(parsed.tables[0].columns[1].name, "qty");
}

#[test]
fn parse_composite_primary_key_and_check() {
    let parsed = parse_sql_ddl(
        r#"
    CREATE TABLE t (
        a TEXT,
        b TEXT,
        c INTEGER CHECK (c > 0),
        PRIMARY KEY (a, b)
    );
  "#,
    )
    .unwrap();
    let t = &parsed.tables[0];
    assert!(t.columns[0].primary_key);
    assert!(t.columns[1].primary_key);
    assert!(t.constraints.iter().any(|c| c.kind == "primary-key"));
    assert!(t.constraints.iter().any(|c| c.kind == "check" && c.columns.contains(&"c".to_string())));
}
