//! gRPC/Protobuf 导出（轨道 B 重新排期）回归：proto3 契约、路由、零变化。

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
        "forma-grpc-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app grpcdemo {
  version = "0.1.0"
}

enum OrderStatus {
  draft
  paid
  cancelled
}

entity Order {
  id        Uuid        @id @default(uuid)
  customer  String(100) @required
  amount    Decimal(12,2)
  paid      Bool
  count     Int
  status    OrderStatus @default(draft)
  createdAt Timestamp   @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
}
"#;

fn write_project(dir: &Path, api_framework: &str) {
    let out = if api_framework == "grpc" {
        "generated/grpc"
    } else {
        "generated/graphql"
    };
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: grpcdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  api:\n    framework: {api_framework}\n    out: {out}\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn grpc_target_generates_proto_contract() {
    let dir = tmp_dir("gen");
    write_project(&dir, "grpc");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"api-grpc"), "{names:?}");
    assert!(!names.contains(&"api-graphql"), "{names:?}");

    let proto = fs::read_to_string(dir.join("generated/grpc/grpcdemo.proto")).unwrap();
    // proto3 语法 + 包名
    assert!(proto.contains("syntax = \"proto3\";"), "{proto}");
    assert!(proto.contains("package grpcdemo;"), "{proto}");
    // 枚举：proto3 首成员 = 0
    assert!(proto.contains("enum OrderStatus {"), "{proto}");
    assert!(proto.contains("DRAFT = 0;"), "{proto}");
    // 实体 message：字段类型映射 + 关系外键
    assert!(proto.contains("message Order {"), "{proto}");
    assert!(proto.contains("string amount = 3;"), "{proto}");
    assert!(proto.contains("int64 count = "), "{proto}");
    assert!(proto.contains("bool paid = "), "{proto}");
    // 列表消息
    assert!(proto.contains("message OrderListRequest {"), "{proto}");
    assert!(proto.contains("repeated Order items"), "{proto}");
    // service（按 operations 裁剪）
    assert!(proto.contains("service OrdersService {"), "{proto}");
    assert!(proto.contains("rpc ListOrder"), "{proto}");
    assert!(
        proto.contains("rpc CreateOrder (CreateOrderRequest) returns (Order);"),
        "{proto}"
    );
    assert!(proto.contains("message CreateOrderRequest {"), "{proto}");
    assert!(proto.contains("rpc DeleteOrder"), "{proto}");
}

#[test]
fn graphql_framework_remains_unchanged_and_grpc_absent() {
    let dir = tmp_dir("graphql");
    write_project(&dir, "graphql");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"api-graphql"), "{names:?}");
    assert!(!names.contains(&"api-grpc"), "{names:?}");
    assert!(!dir.join("generated/grpc").exists());
}

#[test]
fn unknown_api_framework_is_rejected() {
    let dir = tmp_dir("unknown");
    write_project(&dir, "websocket");
    let err = forma::build::run_build(&dir).unwrap_err();
    assert!(
        err.to_string().contains("supported: graphql | grpc"),
        "{err}"
    );
}
