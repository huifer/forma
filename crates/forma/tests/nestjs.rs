//! NestJS target 回归测试（M10）：TypeScript/NestJS + TypeORM CRUD 子集。
//! 不支持的安全语义必须显式拒绝，不得静默降级。

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
        "forma-nest-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app nest {
  version = "0.1.0"
}
entity Item {
  id Uuid @id @default(uuid)
  name String(50) @required
  amount Decimal(10,2)
}
api Items {
  entity Item
  operations [list get create update delete]
}
page ItemList {
  route "/items"
  title { zh "条目" en "Items" }
  table { query Items.list columns [name, amount] }
}
"#;

fn setup(dir: &Path) {
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: nest\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: generated/db\n  backend:\n    language: typescript\n    framework: nestjs\n    out: generated/backend\n",
    ).unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn nestjs_target_generates_crud_project() {
    let dir = tmp_dir("crud");
    setup(&dir);
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name).collect();
    assert!(names.contains(&"backend-nestjs"), "{names:?}");
    assert!(!names.contains(&"backend-rust"), "{names:?}");
    let entity = fs::read_to_string(dir.join("generated/backend/src/item/item.entity.ts")).unwrap();
    assert!(entity.contains("@Entity(\"items\")"), "{entity}");
    assert!(entity.contains("@PrimaryColumn"), "{entity}");
    assert!(entity.contains("type: \"decimal\""), "{entity}");
    let controller =
        fs::read_to_string(dir.join("generated/backend/src/item/item.controller.ts")).unwrap();
    assert!(controller.contains("@Get()"), "{controller}");
    assert!(controller.contains("@Post()"), "{controller}");
    assert!(controller.contains("@Delete(\":id\")"), "{controller}");
    let dto = fs::read_to_string(dir.join("generated/backend/src/item/item.dto.ts")).unwrap();
    assert!(dto.contains("@IsString()"), "{dto}");
    assert!(dto.contains("class CreateItemDto"), "{dto}");
}

#[test]
fn nestjs_target_rejects_unimplemented_security_semantics() {
    let dir = tmp_dir("reject");
    setup(&dir);
    fs::write(
        dir.join("forma/app.fm"),
        FM.replace(
            "api Items {",
            "auth module { provider jwt roles [admin] }\n\napi Items {",
        ),
    )
    .unwrap();
    let err = forma::build::run_build(&dir).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("unsupported DSL features"), "{msg}");
    assert!(msg.contains("auth module"), "{msg}");
}

/// 回归（2026-11-11 验证实跑抓出）：NestJS 产物在含枚举 / 可选字段时编译不过——
/// ① 枚举 TS 类型未声明（TS2304 `Cannot find name 'OrderStatus'`）；
/// ② 可选性写成了 `status?: OrderStatus?`（`T?` 是非法 TS 语法 → TS17019）。
#[test]
fn nestjs_declares_enum_types_and_keeps_optional_marker_on_property() {
    let dir = tmp_dir("enum");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: nest\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: typescript\n    framework: nestjs\n    out: generated/backend\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app nest { version = "0.1.0" }
enum OrderStatus { draft paid }
entity Order {
  id     Uuid        @id @default(uuid)
  title  String(80)  @required
  status OrderStatus @default(draft)
  note   String(50)?
}
api Orders { entity Order operations [list get create update delete] }
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    // 枚举类型声明文件
    let enums = fs::read_to_string(dir.join("generated/backend/src/enums.ts")).unwrap();
    assert!(
        enums.contains(r#"export type OrderStatus = "draft" | "paid";"#),
        "{enums}"
    );
    let entity =
        fs::read_to_string(dir.join("generated/backend/src/order/order.entity.ts")).unwrap();
    assert!(
        entity.contains(r#"import { OrderStatus } from "../enums";"#),
        "{entity}"
    );
    // 可选性只能落在属性名上：类型不得带 `?`
    assert!(entity.contains("status?: OrderStatus;"), "{entity}");
    assert!(entity.contains("note?: string;"), "{entity}");
    assert!(!entity.contains("OrderStatus?"), "{entity}");
    assert!(!entity.contains("string?"), "{entity}");
    let dto = fs::read_to_string(dir.join("generated/backend/src/order/order.dto.ts")).unwrap();
    assert!(
        dto.contains(r#"import { OrderStatus } from "../enums";"#),
        "{dto}"
    );
    assert!(dto.contains("status?: OrderStatus;"), "{dto}");
    assert!(!dto.contains("OrderStatus?"), "{dto}");
}
