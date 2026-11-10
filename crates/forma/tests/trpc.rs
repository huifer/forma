//! tRPC target（轨道 B 重新排期）回归：路由、procedures 裁剪、zod 输入、默认零变化。

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
        "forma-trpc-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app trpcdemo {
  version = "0.1.0"
}

entity Order {
  id       Uuid       @id @default(uuid)
  title    String(80) @required
  amount   Decimal(12,2)
  paid     Bool
  createdAt Timestamp @default(now) @readonly
}

api Orders {
  entity Order
  operations [list get create update delete]
  list { filters [paid] }
}
"#;

fn write_project(dir: &Path, framework: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: trpcdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  api:\n    framework: {framework}\n    out: generated/api\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn trpc_target_generates_typed_router() {
    let dir = tmp_dir("gen");
    write_project(&dir, "trpc");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"api-trpc"), "{names:?}");
    assert!(!names.contains(&"api-graphql"), "{names:?}");

    let router = fs::read_to_string(dir.join("generated/api/src/router.ts")).unwrap();
    assert!(router.contains("initTRPC.create()"), "{router}");
    assert!(
        router.contains("const ordersRouter = t.router({"),
        "{router}"
    );
    assert!(router.contains("appRouter = t.router({"), "{router}");
    // procedures 按 operations 裁剪 + REST 委托
    assert!(router.contains("list: publicProcedure"), "{router}");
    assert!(router.contains("create: publicProcedure"), "{router}");
    assert!(router.contains("delete: publicProcedure"), "{router}");
    assert!(router.contains("rest(`/api/orders"), "{router}");
    // zod 输入从 DSL 派生（Decimal → string、readonly 排除）
    assert!(router.contains("amount: z.string()"), "{router}");
    assert!(!router.contains("created_at:"), "{router}");
    let ctx = fs::read_to_string(dir.join("generated/api/src/context.ts")).unwrap();
    assert!(ctx.contains("REST_BASE"), "{ctx}");
    let pkg = fs::read_to_string(dir.join("generated/api/package.json")).unwrap();
    assert!(pkg.contains("@trpc/server"), "{pkg}");
    assert!(pkg.contains("zod"), "{pkg}");
}

#[test]
fn graphql_and_grpc_targets_remain_unchanged() {
    let dir = tmp_dir("graphql");
    write_project(&dir, "graphql");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"api-graphql"), "{names:?}");
    assert!(!names.contains(&"api-trpc"), "{names:?}");
}
