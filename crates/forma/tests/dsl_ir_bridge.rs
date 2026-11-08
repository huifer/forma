//! IR 契约桥接回归测试（M1 / M2 / M3，spec:ir §15）：
//!
//! - **M1**：诊断携带精确 file/line（声明级 + 字段级）；
//! - **M2**：`IR_VERSION` + `Program.ir_version` + 内建类型单一权威清单（四个目标映射齐备）；
//! - **M3**：`Program → UnifiedSemanticGraph` 桥接——稳定标识（`forma://` URI）、
//!   来源追踪（SourceRef）与依赖边，影响闭包覆盖 API 与页面。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use forma::dsl::{BUILTIN_TYPES, IR_VERSION};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "forma-irbridge-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: irbridge\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  db:\n    engine: mysql\n    out: db\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

fn load(dir: &Path) -> (forma::dsl::Program, Vec<forma::validate::Diagnostic>) {
    forma::dsl::load_program(dir, &["forma/**/*.fm".to_string()]).unwrap()
}

// ── M1：诊断携带精确 file/line ────────────────────────────────

const BROKEN_FM: &str = r#"
app irbridge {
  version = "0.1.0"
}

entity Student {
  id   Uuid   @id @default(uuid)
  name String(50) @required
  bad  NoSuchType
}
"#;

#[test]
fn diagnostics_carry_file_and_line() {
    let dir = tmp_dir("m1-loc");
    write_project(&dir, BROKEN_FM);
    let (program, _) = load(&dir);
    let diags = forma::dsl::check_program(&program);
    let d = diags
        .iter()
        .find(|d| d.code == "dsl-unknown-type")
        .expect("unknown type must be reported");
    assert_eq!(d.file.as_deref(), Some("forma/app.fm"), "{d:?}");
    let line = d.line.expect("diagnostic must carry a line");
    // bad 字段在源码里的行号（1-based）
    let src = fs::read_to_string(dir.join("forma/app.fm")).unwrap();
    let expected = src
        .lines()
        .position(|l| l.contains("NoSuchType"))
        .map(|i| i + 1)
        .unwrap();
    assert_eq!(line, expected, "{d:?}");
}

#[test]
fn every_dsl_diagnostic_has_a_location() {
    // 多种错误类型（类型 / 引用 / 规则 / 权限）都必须能定位到文件与行
    let dir = tmp_dir("m1-all");
    write_project(
        &dir,
        r#"
app irbridge {
  version = "0.1.0"
}

entity Student {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

api Students {
  entity Student
  operations [list get]
  auth {
    update → role in ["admin"]
  }
}

rules R {
  field Student.name { required "必填" }
  access Student {
    update → role in ["ghost"]
  }
}
"#,
    );
    let (program, _) = load(&dir);
    let diags = forma::dsl::check_program(&program);
    assert!(!diags.is_empty(), "fixture must produce diagnostics");
    for d in &diags {
        assert!(
            d.file.is_some() && d.line.is_some(),
            "diagnostic must carry file+line: {d:?}"
        );
    }
}

// ── M2：IR 版本 + 内建类型单一权威清单 ──────────────────────────

#[test]
fn program_carries_ir_version() {
    let dir = tmp_dir("m2-version");
    write_project(&dir, BROKEN_FM);
    let (program, _) = load(&dir);
    assert_eq!(program.ir_version, IR_VERSION);
    assert_eq!(IR_VERSION, "1.2.0");
}

#[test]
fn builtin_types_have_mappings_in_all_targets() {
    // 「生成层支持的类型必须检查层也可用」——四个目标必须覆盖全部内建类型
    for ty in BUILTIN_TYPES {
        let dir = tmp_dir(&format!("m2-{}", ty.to_lowercase()));
        write_project(
            &dir,
            &format!(
                "app irbridge {{ version = \"0.1.0\" }}\nentity Item {{\n  id  Uuid @id @default(uuid)\n  v   {ty}\n}}\n"
            ),
        );
        let (program, _) = load(&dir);
        let diags = forma::dsl::check_program(&program);
        assert!(
            diags.iter().all(|d| d.code != "dsl-unknown-type"),
            "{ty} must be a known builtin type: {diags:?}"
        );
        let entity = forma::dsl::find_entity(&program, "Item").unwrap();
        let field = &entity.fields[1];
        assert_eq!(field.type_name, ty);

        let mysql = forma::emit::mysql_type(&program, field);
        let rust = forma::emit::rust_type(&program, field);
        let ts = forma::emit::ts_type(&program, &field.type_name, field.optional);
        let dart = forma::emit::dart_type(&program, field);
        for (target, mapped) in [
            ("mysql", &mysql),
            ("rust", &rust),
            ("ts", &ts),
            ("dart", &dart),
        ] {
            assert!(!mapped.is_empty(), "{ty} has no {target} mapping");
        }
        // MySQL 侧必须是**该类型的具体列类型**（不是关系兜底的 CHAR(36)）
        let expected_mysql = match ty {
            "Uuid" => "CHAR(36)",
            "String" => "VARCHAR(255)",
            "Email" => "VARCHAR(320)",
            "Text" => "TEXT",
            "Int" => "BIGINT",
            "Float" => "DOUBLE",
            "Decimal" => "DECIMAL(12,2)",
            "Bool" => "TINYINT(1)",
            "Timestamp" => "DATETIME(3)",
            "Date" => "DATE",
            "Json" => "JSON",
            "Attachment" => "VARCHAR(500)",
            other => panic!("unhandled builtin type {other}"),
        };
        assert_eq!(
            mysql, expected_mysql,
            "{ty} must map to its own column type"
        );
    }
}

// ── M3：Program → 语义图桥接 ───────────────────────────────────

const BRIDGE_FM: &str = r#"
app irbridge {
  version = "0.1.0"
}

enum Status {
  pending
  approved
}

entity Student {
  id     Uuid    @id @default(uuid)
  name   String(50) @required
  status Status
}

entity School {
  id   Uuid   @id @default(uuid)
  name String(50) @required
}

api Students {
  entity Student
  operations [list get create update delete]
  list { filters [name] }
}

api Schools {
  entity School
  operations [list get]
}

rules R {
  access Student {
    update → role in ["admin"]
  }
}

auth module {
  provider jwt
  roles [admin, student]
}

page StudentList {
  route "/students"
  title { zh "学生" en "Students" }
  table {
    query   Students.list
    columns [name]
  }
}

page StudentEdit {
  route "/students/{id}"
  title { zh "编辑" en "Edit" }
  form Student {
    layout stack {
      name Input @required
    }
    submit → Students.update redirect "/students"
  }
}
"#;

fn lowered(dir: &Path) -> (forma::dsl::Program, forma::graph::UnifiedSemanticGraph) {
    let (program, _) = load(dir);
    let lowered = forma::dsl::lower::lower_to_graph(&program).unwrap();
    (program, lowered.graph)
}

#[test]
fn bridge_produces_stable_ids_and_source_refs() {
    let dir = tmp_dir("m3-ids");
    write_project(&dir, BRIDGE_FM);
    let (_program, graph) = lowered(&dir);

    // 实体 / 字段：稳定标识与联邦侧同一套 URI 构造函数
    let student = graph
        .entities()
        .into_iter()
        .find(|n| n.name == "Student")
        .expect("Student entity node");
    assert_eq!(student.uri, "forma://project/irbridge/entity/Student");
    assert_eq!(student.standard.as_str(), "core");
    // 来源追踪：file + line 均可定位
    assert_eq!(student.source.file, "forma/app.fm");
    assert!(student.source.line.is_some_and(|l| l > 0), "{student:?}");

    let field = graph
        .all_nodes()
        .into_iter()
        .find(|n| n.uri == "forma://project/irbridge/entity/Student/field/status")
        .expect("field node");
    assert_eq!(field.kind.as_str(), "entity-field");

    // 枚举 → Type 节点；API 操作 → ApiOperation 节点
    assert!(graph.has("forma://project/irbridge/type/Status"));
    assert!(graph.has("forma://project/irbridge/api/Students.list"));
    // 页面 → UiForm 节点
    assert!(graph.has("forma://project/irbridge/ui/StudentList"));
}

#[test]
fn bridge_produces_dependency_edges() {
    let dir = tmp_dir("m3-edges");
    write_project(&dir, BRIDGE_FM);
    let (_program, graph) = lowered(&dir);
    let summary = graph.to_summary();

    let has = |from: &str, to: &str, kind: &str| {
        summary
            .edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == kind)
    };
    // 实体 → 字段：Defines
    assert!(
        has(
            "forma://project/irbridge/entity/Student",
            "forma://project/irbridge/entity/Student/field/name",
            "defines"
        ),
        "{summary:?}"
    );
    // 操作 → 实体：References（操作引用实体）
    assert!(
        has(
            "forma://project/irbridge/api/Students.list",
            "forma://project/irbridge/entity/Student",
            "references"
        ),
        "{summary:?}"
    );
    // 页面 → 操作：References（API 变更 → 页面受影响）
    assert!(
        has(
            "forma://project/irbridge/ui/StudentList",
            "forma://project/irbridge/api/Students.list",
            "references"
        ),
        "{summary:?}"
    );
    // 字段 → 枚举：References
    assert!(
        has(
            "forma://project/irbridge/entity/Student/field/status",
            "forma://project/irbridge/type/Status",
            "references"
        ),
        "{summary:?}"
    );
}

#[test]
fn impact_closure_covers_api_and_pages() {
    let dir = tmp_dir("m3-impact");
    write_project(&dir, BRIDGE_FM);
    let (_program, graph) = lowered(&dir);
    let affected = graph.affected_by("forma://project/irbridge/entity/Student");
    let uris: Vec<&str> = affected.iter().map(|n| n.uri.as_str()).collect();
    // 实体变更 → 其字段、绑定该实体的 API 操作
    assert!(
        uris.contains(&"forma://project/irbridge/entity/Student/field/name"),
        "{uris:?}"
    );
    assert!(
        uris.contains(&"forma://project/irbridge/api/Students.list"),
        "{uris:?}"
    );
    // API 变更 → 引用该 API 的页面（反向传播）
    let affected = graph.affected_by("forma://project/irbridge/api/Students.list");
    let uris: Vec<&str> = affected.iter().map(|n| n.uri.as_str()).collect();
    assert!(
        uris.contains(&"forma://project/irbridge/ui/StudentList"),
        "{uris:?}"
    );
}

// ── P3 收尾：影响闭包含五类目标（表列 / 模型 / 表单 / 图表 / 迁移）──────

const P3_FM: &str = r#"
app irbridge {
  version = "0.1.0"
}

entity Student {
  id         Uuid       @id @default(uuid)
  name       String(50) @required
  enrolledAt Timestamp  @default(now) @readonly
  status     String(20)
}

api Students {
  entity Student
  operations [list get create update delete]
  list { filters [name] }
}

chart EnrollmentTrend {
  type  line
  title { zh "报名趋势" en "Enrollment trend" }
  source Students.list
  encoding {
    x      { field enrolledAt type temporal }
    y      { field value      type quantitative }
    series { field status     type nominal }
  }
}

page Dashboard {
  route "/dashboard"
  title { zh "看板" en "Dashboard" }
  chart EnrollmentTrend
}

page StudentList {
  route "/students"
  title { zh "学生" en "Students" }
  table {
    query   Students.list
    columns [name]
  }
}
"#;

#[test]
fn chart_and_migration_nodes_are_in_the_graph() {
    let dir = tmp_dir("p3-chart");
    write_project(&dir, P3_FM);
    let (_program, graph) = lowered(&dir);
    let summary = graph.to_summary();

    // 图表节点 + 来源位置
    let chart = summary
        .nodes
        .iter()
        .find(|n| n.uri == "forma://project/irbridge/chart/EnrollmentTrend")
        .expect("chart node");
    assert_eq!(chart.kind, "chart");
    assert_eq!(chart.source.file, "forma/app.fm");
    assert!(chart.source.line.unwrap_or(0) > 0);

    // 迁移链节点
    assert!(
        summary
            .nodes
            .iter()
            .any(|n| n.uri == "forma://project/irbridge/migration/chain" && n.kind == "migration"),
        "{summary:?}"
    );

    let has = |from: &str, to: &str, kind: &str| {
        summary
            .edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == kind)
    };
    // 图表 → API 操作：引用
    assert!(
        has(
            "forma://project/irbridge/chart/EnrollmentTrend",
            "forma://project/irbridge/api/Students.list",
            "references"
        ),
        "{summary:?}"
    );
    // 图表 → 编码通道字段：引用（聚合输出 `value` 不对应实体字段 → 不产生边）
    assert!(
        has(
            "forma://project/irbridge/chart/EnrollmentTrend",
            "forma://project/irbridge/entity/Student/field/enrolledAt",
            "references"
        ),
        "{summary:?}"
    );
    assert!(
        !has(
            "forma://project/irbridge/chart/EnrollmentTrend",
            "forma://project/irbridge/entity/Student/field/value",
            "references"
        ),
        "aggregate output must not fake an entity-field edge"
    );
    // 页面 → 图表：引用
    assert!(
        has(
            "forma://project/irbridge/ui/Dashboard",
            "forma://project/irbridge/chart/EnrollmentTrend",
            "references"
        ),
        "{summary:?}"
    );
    // 表 → 迁移链：Generates（迁移链是表的派生物，改表 ⇒ 需要新迁移）
    assert!(
        has(
            "forma://local/sql/dsl/table/students",
            "forma://project/irbridge/migration/chain",
            "generates"
        ),
        "{summary:?}"
    );
}

#[test]
fn impact_closure_covers_all_five_target_kinds() {
    // RFC-0006 P3 验收：改实体 ⇒ 表列、模型、表单、图表、迁移五类目标全部入闭包
    let dir = tmp_dir("p3-closure");
    write_project(&dir, P3_FM);
    let (_program, graph) = lowered(&dir);
    let affected = graph.affected_by("forma://project/irbridge/entity/Student");
    let kinds: Vec<&str> = affected.iter().map(|n| n.kind.as_str()).collect();
    for expected in [
        "entity-field",
        "table",
        "table-column",
        "api-operation",
        "ui-form",
        "chart",
        "migration",
    ] {
        assert!(
            kinds.contains(&expected),
            "impact closure must contain {expected} targets: {kinds:?}"
        );
    }
    // 图表经由 api-operation / 字段两条路径进入闭包
    assert!(
        affected
            .iter()
            .any(|n| n.uri == "forma://project/irbridge/chart/EnrollmentTrend"),
        "{kinds:?}"
    );
    assert!(
        affected
            .iter()
            .any(|n| n.uri == "forma://project/irbridge/migration/chain"),
        "{kinds:?}"
    );
}

#[test]
fn mcp_graph_and_impact_work_on_dsl_projects() {
    let dir = tmp_dir("m3-mcp");
    write_project(&dir, BRIDGE_FM);
    let mut args = serde_json::Map::new();
    let graph = forma::mcp::call_tool(&dir, "forma_graph", &args).unwrap();
    assert_eq!(graph["irVersion"], IR_VERSION, "{graph}");
    assert!(graph["nodeCount"].as_u64().unwrap() > 0, "{graph}");
    assert!(graph["edgeCount"].as_u64().unwrap() > 0, "{graph}");

    // 影响分析不再被拒（DSL 工程走桥接图）
    args.insert(
        "target".to_string(),
        serde_json::Value::String("Student".to_string()),
    );
    let impact = forma::mcp::call_tool(&dir, "forma_impact", &args).unwrap();
    assert_eq!(
        impact["target"], "forma://project/irbridge/entity/Student",
        "{impact}"
    );
    assert!(impact["affectedCount"].as_u64().unwrap() > 0, "{impact}");
}

#[test]
fn mcp_check_reports_located_diagnostics() {
    // MCP 的诊断必须与 CLI 同形携带 file/line（不得漂移）
    let dir = tmp_dir("mcp-loc");
    write_project(&dir, BROKEN_FM);
    let args = serde_json::Map::new();
    let out = forma::mcp::call_tool(&dir, "forma_check", &args).unwrap();
    let diags = out["diagnostics"].as_array().expect("diagnostics array");
    let d = diags
        .iter()
        .find(|d| d["code"] == "dsl-unknown-type")
        .expect("unknown type diagnostic");
    assert_eq!(d["file"], "forma/app.fm", "{out}");
    assert!(d["line"].as_u64().unwrap_or(0) > 0, "{out}");
    assert_eq!(out["errors"], 1, "{out}");
}

#[test]
fn lowering_output_is_byte_identical_across_runs() {
    // RFC-0003 §16：同一 Program 两次 Lowering / 检查结果 byte 级一致
    let dir = tmp_dir("m3-determinism");
    write_project(&dir, BRIDGE_FM);
    let call = || {
        let args = serde_json::json!({});
        forma::mcp::call_tool(&dir, "forma_graph", args.as_object().unwrap()).unwrap()
    };
    let a = serde_json::to_string(&call()).unwrap();
    let b = serde_json::to_string(&call()).unwrap();
    assert_eq!(a, b, "lower_to_graph must be deterministic");
    assert!(a.contains("entity"), "{a}");
}
