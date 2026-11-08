//! RFC-0003 §15 阶段 3 / G-10：`spec:ir` 的机器可读 Schema 与结构校验。
//!
//! 契约文件：`docs/architecture/specifications/ir/program.schema.json`（JSON Schema draft-07）。
//! 本测试实现一个**最小校验器**（type / required / properties / items / $ref / enum 子集，
//! 无新增依赖），用真实 `Program` 序列化结果验证 schema 与实现一致——schema 漂移会被测试抓住。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("forma-irschema-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

// 测试 CWD = crates/forma
const SCHEMA_PATH: &str = "../../docs/architecture/specifications/ir/program.schema.json";

fn schema() -> serde_json::Value {
    let text = fs::read_to_string(SCHEMA_PATH).expect("schema file exists");
    serde_json::from_str(&text).expect("schema is valid JSON")
}

/// 最小 JSON Schema 校验器（仅本契约使用的子集）
fn validate(
    value: &serde_json::Value,
    schema: &serde_json::Value,
    root: &serde_json::Value,
    path: &str,
) -> Vec<String> {
    let mut errs = Vec::new();
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let target = r
            .strip_prefix("#/$defs/")
            .and_then(|name| root["$defs"].get(name))
            .unwrap_or(&serde_json::Value::Null);
        errs.extend(validate(value, target, root, path));
        return errs;
    }
    if let Some(tv) = schema.get("type") {
        let types: Vec<&str> = tv.as_str().map(|t| vec![t]).unwrap_or_else(|| {
            tv.as_array()
                .unwrap()
                .iter()
                .filter_map(|v| v.as_str())
                .collect()
        });
        let ok = types.iter().any(|t| match *t {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => true,
        });
        if !ok {
            errs.push(format!("{path}: expected {types:?}, got {value}"));
            return errs;
        }
    }
    if let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) {
        if !enum_vals.iter().any(|e| e == value) {
            errs.push(format!("{path}: {value} not in enum {enum_vals:?}"));
        }
    }
    if value.is_object() {
        for req in schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[])
        {
            let key = req.as_str().unwrap_or_default();
            if !value.as_object().unwrap().contains_key(key) {
                errs.push(format!("{path}: missing required `{key}`"));
            }
        }
        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for (k, sub) in props {
                if let Some(v) = value.get(k) {
                    errs.extend(validate(v, sub, root, &format!("{path}.{k}")));
                }
            }
        }
    }
    if value.is_array() {
        if let Some(item_schema) = schema.get("items") {
            for (i, v) in value.as_array().unwrap().iter().enumerate() {
                errs.extend(validate(v, item_schema, root, &format!("{path}[{i}]")));
            }
        }
    }
    errs
}

#[test]
fn program_json_conforms_to_ir_schema() {
    let dir = tmp("program");
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: irschema\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n",
    )
    .unwrap();
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app irschema { version = "0.1.0" }
enum Status { pending active }
entity Student {
  id Uuid @id @default(uuid)
  name String(50) @required
  status Status @default(pending)
}
api Students { entity Student operations [list get create update delete] }
page StudentList {
  route "/students"
  title { en "Students" }
  table {
    query   Students.list
    columns [name]
    actions { row [delete → Students.delete confirm] }
  }
}
"#,
    )
    .unwrap();
    let (program, _) = forma::dsl::load_program(&dir, &["forma/**/*.fm".into()]).unwrap();

    // IR 契约视图：从 Program 提取 schema 描述的关键字段面。
    // （Program 未启用 serde 直出——避免把内部声明结构暴露为公开契约；
    //   机器可读契约覆盖 IR 的稳定字段面，见 spec:ir 的引用。）
    let app = program.app.as_ref();
    let view = serde_json::json!({
        "ir_version": program.ir_version,
        "app": app.map(|a| serde_json::json!({
            "name": a.name,
            "version": a.version,
            "locales": a.locales,
        })),
        "entities": program.entities.iter().map(|e| serde_json::json!({
            "name": e.name,
            "line": e.line,
            "file": e.file,
            "fields": e.fields.iter().map(|f| serde_json::json!({
                "name": f.name,
                "type_name": f.type_name,
                "optional": f.optional,
                "line": f.line,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "enums": program.enums.iter().map(|e| serde_json::json!({
            "name": e.name,
            "line": e.line,
            "file": e.file,
            "variants": e.variants.iter().map(|v| &v.name).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "rules": program.rules.iter().map(|r| serde_json::json!({
            "access": r.access.iter().map(|a| serde_json::json!({
                "entity": a.entity,
                "op_count": a.rules.len(),
            })).collect::<Vec<_>>(),
            "guards": r.guards.iter().map(|g| serde_json::json!({
                "entity": g.entity,
                "op": g.op,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "apis": program.apis.iter().map(|a| serde_json::json!({
            "entity": a.entity,
            "operations": a.operations,
            "auth": a.auth.iter().map(|r| serde_json::json!({
                "operations": r.operations,
                "roles": r.roles,
            })).collect::<Vec<_>>(),
            "line": a.line,
            "file": a.file,
        })).collect::<Vec<_>>(),
        "pages": program.pages.iter().map(|p| serde_json::json!({
            "name": p.name,
            "route": p.route,
            "line": p.line,
            "file": p.file,
        })).collect::<Vec<_>>(),
        "jobs": program.jobs.iter().map(|j| serde_json::json!({
            "name": j.name,
            "cron": j.cron,
            "op": j.op,
        })).collect::<Vec<_>>(),
        "lives": program.lives.iter().map(|l| serde_json::json!({
            "name": l.name,
            "api": l.api,
            "events": l.events,
        })).collect::<Vec<_>>(),
        "exports": program.exports.iter().map(|e| serde_json::json!({
            "name": e.name,
            "format": e.format,
            "api": e.api,
        })).collect::<Vec<_>>(),
        "charts": program.charts.iter().map(|c| serde_json::json!({
            "name": c.name,
            "chart_type": c.chart_type,
        })).collect::<Vec<_>>(),
        "commands": program.commands.iter().map(|c| serde_json::json!({
            "name": c.name,
        })).collect::<Vec<_>>(),
        "auth": program.auth.as_ref().map(|a| serde_json::json!({
            "provider": a.provider,
            "roles": a.roles,
            "token_hours": a.token_hours,
        })),
    });
    let s = schema();
    let errs = validate(&view, &s, &s, "$");
    assert!(errs.is_empty(), "schema violations:\n{}", errs.join("\n"));
    // 契约字段与实现一致
    assert_eq!(view["ir_version"], forma::dsl::IR_VERSION);
    assert_eq!(view["entities"][0]["name"], "Student");
    assert_eq!(view["enums"][0]["name"], "Status");
    assert_eq!(view["apis"][0]["entity"], "Student");
}

#[test]
fn schema_declares_the_builtin_type_vocabulary() {
    // 内建类型词表（BUILTIN_TYPES）必须是 schema 的可机读子集
    let s = schema();
    let enum_vals: Vec<&str> = s["$defs"]["BuiltinType"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let expected: Vec<&str> = forma::dsl::BUILTIN_TYPES.to_vec();
    assert_eq!(
        enum_vals, expected,
        "schema builtin types must match BUILTIN_TYPES"
    );
}

#[test]
fn schema_is_a_valid_json_document() {
    let s = schema();
    assert_eq!(s["$schema"], "http://json-schema.org/draft-07/schema#");
    assert!(s["$defs"].is_object());
}

#[test]
fn schema_file_is_referenced_from_spec_ir() {
    // 治理双向一致：spec:ir 必须引用机器可读契约，否则机器消费者找不到入口
    let spec = fs::read_to_string(Path::new(
        "../../docs/architecture/specifications/ir/ir-contract.zh-CN.md",
    ))
    .unwrap();
    assert!(
        spec.contains("program.schema.json"),
        "spec:ir must reference the machine-readable schema"
    );
}
