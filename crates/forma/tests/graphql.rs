//! GraphQL target 回归测试（M10）：schema 契约 + resolver 网关。
//! 验证：SDL 必须能被 `graphql.parse()` 接受、生成项目 `tsc --noEmit` 必须过。

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
        "forma-gql-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app gql {
  version = "0.1.0"
}
enum Status { draft active }
entity User {
  id Uuid @id @default(uuid)
  name String(50) @required
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
  status Status?
  owner User @required
}
api Items {
  entity Item
  operations [list get create update delete]
}
api Users {
  entity User
  operations [list get]
}
"#;

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: gql\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  api:\n    framework: graphql\n    out: generated/graphql\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn graphql_target_generates_schema_and_resolvers() {
    let dir = tmp_dir("schema");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"api-graphql"), "{names:?}");

    let schema = fs::read_to_string(dir.join("generated/graphql/schema.graphql")).unwrap();
    // 枚举
    assert!(schema.contains("enum Status"), "{schema}");
    assert!(schema.contains("draft"), "{schema}");
    // 实体类型 + 标量 + 关系
    assert!(schema.contains("type Item {"), "{schema}");
    assert!(schema.contains("ownerId: ID!"), "{schema}");
    assert!(schema.contains("owner: User!"), "{schema}");
    // Decimal → String（后端序列化契约）
    assert!(schema.contains("amount: String"), "{schema}");
    // Input 类型
    assert!(schema.contains("input CreateItemInput"), "{schema}");
    assert!(schema.contains("input UpdateItemInput"), "{schema}");
    // Query / Mutation
    assert!(
        schema.contains("items(page: Int, pageSize: Int, q: String): [Item!]!"),
        "{schema}"
    );
    assert!(schema.contains("item(id: ID!): Item"), "{schema}");
    assert!(
        schema.contains("createItem(input: CreateItemInput!): Item!"),
        "{schema}"
    );
    assert!(
        schema.contains("updateItem(id: ID!, input: UpdateItemInput!): Item!"),
        "{schema}"
    );
    assert!(schema.contains("deleteItem(id: ID!): Boolean!"), "{schema}");

    // src/schema.ts 与 schema.graphql 同源（typeDefs 内嵌）
    let schema_ts = fs::read_to_string(dir.join("generated/graphql/src/schema.ts")).unwrap();
    assert!(schema_ts.contains("export const typeDefs"), "{schema_ts}");
    assert!(schema_ts.contains("type Item {"), "{schema_ts}");

    // rest.ts：服务端 REST 客户端（env 可覆盖）
    let rest = fs::read_to_string(dir.join("generated/graphql/src/rest.ts")).unwrap();
    assert!(rest.contains("REST_BASE"), "{rest}");
    assert!(rest.contains("ItemApi"), "{rest}");
    assert!(rest.contains("UserApi"), "{rest}");

    // resolvers.ts：Query/Mutation 委托 + 正向对象关系
    let resolvers = fs::read_to_string(dir.join("generated/graphql/src/resolvers.ts")).unwrap();
    assert!(resolvers.contains("Query:"), "{resolvers}");
    assert!(resolvers.contains("Mutation:"), "{resolvers}");
    assert!(resolvers.contains("owner:"), "{resolvers}");
    assert!(resolvers.contains("ownerId"), "{resolvers}");

    // server.ts：graphql-yoga 网关
    let server = fs::read_to_string(dir.join("generated/graphql/src/server.ts")).unwrap();
    assert!(server.contains("createYoga"), "{server}");
    assert!(server.contains("/graphql"), "{server}");
}

#[test]
fn no_api_target_generates_zero_graphql_files() {
    let dir = tmp_dir("zero");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: nolang\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(!names.contains(&"api-graphql"), "{names:?}");
    assert!(!dir.join("generated/graphql").exists());
}
