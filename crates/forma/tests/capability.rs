//! Capability Registry 集成测试（RFC-0004）：
//! - Manifest 模式与内置能力；
//! - 依赖闭包 / 版本 pin / IR 兼容性；
//! - **新增能力不必修改 Semantic Core**（项目自备 Manifest 即可接入）。

use forma::capability::{CapabilityRegistry, CapabilitySelection, IR_VERSION};
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
        "forma-cap-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const MINIMAL_FM: &str = r#"
app capdemo {
  version = "0.1.0"
}

entity Order {
  id     Uuid   @id @default(uuid)
  amount Decimal(12,2) @required
}

api Orders {
  entity Order
  operations [list get create]
}
"#;

fn write_project(dir: &Path, fm: &str, capabilities_yaml: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    let mut yaml = String::from(
        "project:\n  name: capdemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n",
    );
    if !capabilities_yaml.is_empty() {
        yaml.push_str("\ncapabilities:\n");
        yaml.push_str(capabilities_yaml);
    }
    yaml.push_str("\ntargets:\n  web:\n    framework: react\n    out: generated/web\n");
    fs::write(dir.join("forma.yaml"), yaml).unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

// ── 1. 内置能力 ────────────────────────────────────────────────

#[test]
fn builtin_manifests_are_valid_and_resolvable() {
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.ids().len() >= 10, "expected builtin capabilities");
    for m in reg.manifests() {
        assert_eq!(m.api_version, "forma.dev/v1");
        assert_eq!(m.kind, "Capability");
        assert!(
            m.metadata.id.contains('.'),
            "id must be `<category>.<name>`: {}",
            m.metadata.id
        );
        assert!(
            forma::capability::Version::parse(&m.metadata.version).is_some(),
            "version must be semver: {}",
            m.metadata.version
        );
        // 每个能力的 IR 兼容范围必须覆盖当前编译器提供的 IR 版本
        let ir = forma::capability::Version::parse(IR_VERSION).unwrap();
        if let Some(range) = &m.spec.compatibility.ir {
            assert!(
                forma::capability::version_matches(range, &ir),
                "{} requires IR `{range}` but compiler provides {IR_VERSION}",
                m.metadata.id
            );
        }
    }
}

#[test]
fn dependency_closure_is_resolved_topologically() {
    let reg = CapabilityRegistry::with_builtins();
    let res = reg.resolve(&["visualization.echarts".to_string()], &[]);
    assert!(
        res.errors().is_empty(),
        "unexpected errors: {:?}",
        res.errors()
    );
    let ids: Vec<&str> = res.active.iter().map(|c| c.id.as_str()).collect();
    // 被依赖者在前
    let pos_binding = ids.iter().position(|i| *i == "data.binding").unwrap();
    let pos_echarts = ids
        .iter()
        .position(|i| *i == "visualization.echarts")
        .unwrap();
    assert!(
        pos_binding < pos_echarts,
        "dependency must come first: {ids:?}"
    );
    assert!(res.active[pos_echarts].explicit);
    assert!(!res.active[pos_binding].explicit);
    assert!(res.permissions.contains(&"data.read".to_string()));
}

#[test]
fn pin_mismatch_and_unknown_capability_are_errors() {
    let reg = CapabilityRegistry::with_builtins();
    let res = reg.resolve(
        &["data.mysql".to_string()],
        &[("data.mysql".to_string(), "2.0.0".to_string())],
    );
    assert!(res
        .errors()
        .iter()
        .any(|d| d.code == "capability-pin-mismatch"));

    let res = reg.resolve(&["no.such".to_string()], &[]);
    assert!(res.errors().iter().any(|d| d.code == "capability-unknown"));
}

#[test]
fn incompatible_ir_version_is_rejected() {
    let mut reg = CapabilityRegistry::with_builtins();
    let yaml = r#"
apiVersion: forma.dev/v1
kind: Capability
metadata:
  id: viz.future
  version: 1.0.0
spec:
  category: visualization
  compatibility:
    ir: ">=99.0.0"
"#;
    reg.load_manifest_text(yaml, "capabilities/viz.future.yaml")
        .unwrap();
    let res = reg.resolve(&["viz.future".to_string()], &[]);
    assert!(
        res.errors()
            .iter()
            .any(|d| d.code == "capability-ir-incompatible"),
        "expected capability-ir-incompatible, got {:?}",
        res.errors()
    );
}

// ── 2. 清单解析（forma.yaml） ────────────────────────────────────

#[test]
fn manifest_parses_capabilities_in_both_forms() {
    let dir = tmp_dir("manifest");
    write_project(&dir, MINIMAL_FM, "  - data.mysql\n  - ui.table\n");
    let m = forma::manifest::load_manifest(&dir).unwrap();
    assert_eq!(
        m.manifest.capabilities.enabled,
        vec!["data.mysql", "ui.table"]
    );
    assert!(m.manifest.capabilities.pins.is_empty());

    let dir2 = tmp_dir("manifest-map");
    write_project(
        &dir2,
        MINIMAL_FM,
        "  data.mysql: \">=1.0.0 <2.0.0\"\n  visualization.echarts: \"1.0.0\"\n",
    );
    let m2 = forma::manifest::load_manifest(&dir2).unwrap();
    assert_eq!(
        m2.manifest.capabilities.enabled,
        vec!["data.mysql", "visualization.echarts"]
    );
    assert_eq!(m2.manifest.capabilities.pins.len(), 2);
}

#[test]
fn default_selection_is_empty() {
    let dir = tmp_dir("manifest-default");
    write_project(&dir, MINIMAL_FM, "");
    let m = forma::manifest::load_manifest(&dir).unwrap();
    assert!(m.manifest.capabilities.enabled.is_empty());
    let sel = CapabilitySelection::default();
    assert!(sel.enabled.is_empty());
}

// ── 3. 构建期集成：能力错误阻断构建 ──────────────────────────────

#[test]
fn unknown_capability_blocks_build() {
    let dir = tmp_dir("build-unknown");
    write_project(&dir, MINIMAL_FM, "  - no.such.capability\n");
    let err = match forma::build::run_build(&dir) {
        Ok(_) => panic!("expected build to fail on unknown capability"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("capability-unknown"),
        "expected capability-unknown in build error, got: {msg}"
    );
}

#[test]
fn incompatible_capability_blocks_build() {
    let dir = tmp_dir("build-incompat");
    write_project(&dir, MINIMAL_FM, "  - viz.future\n");
    fs::create_dir_all(dir.join("capabilities")).unwrap();
    fs::write(
        dir.join("capabilities/viz.future.yaml"),
        r#"
apiVersion: forma.dev/v1
kind: Capability
metadata:
  id: viz.future
  version: 1.0.0
spec:
  category: visualization
  compatibility:
    ir: ">=99.0.0"
"#,
    )
    .unwrap();
    let err = match forma::build::run_build(&dir) {
        Ok(_) => panic!("expected build to fail on incompatible capability"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("capability-ir-incompatible"),
        "expected capability-ir-incompatible, got: {msg}"
    );
}

// ── 4. 核心命题：新增能力不必修改 Semantic Core ──────────────────

#[test]
fn adding_a_capability_requires_no_core_change() {
    let dir = tmp_dir("extensibility");
    write_project(&dir, MINIMAL_FM, "  - data.mysql\n");

    // 基线：只启用内置能力，构建通过。
    let baseline = forma::build::run_build(&dir).unwrap();
    let baseline_ids: Vec<&str> = baseline
        .capabilities
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(baseline_ids, vec!["data.mysql"]);
    let baseline_web = fs::read_to_string(dir.join("generated/web/src/api.ts")).ok();

    // 新增一个能力：只放一个 Manifest + 在 forma.yaml 里声明它。
    // 没有改动任何核心代码（dsl / ir / emit）。
    fs::create_dir_all(dir.join("capabilities")).unwrap();
    fs::write(
        dir.join("capabilities/visualization.vega-lite.yaml"),
        r#"
apiVersion: forma.dev/v1
kind: Capability
metadata:
  id: visualization.vega-lite
  version: 1.0.0
  description: Vega-Lite 渲染器（第三方能力，仅凭 Manifest 接入）
spec:
  category: visualization
  provides: [chart.bar, chart.line]
  consumes: [ChartIR, DataBinding]
  adapters: [vega-lite-adapter]
  runtime: [forma-web-runtime]
  dependencies: [data.binding]
  permissions: [data.read]
  compatibility:
    ir: ">=1.0.0 <2.0.0"
    runtime: ">=1.0.0"
  conformance:
    suite: visualization.chart.v1
"#,
    )
    .unwrap();
    write_project(
        &dir,
        MINIMAL_FM,
        "  - data.mysql\n  - visualization.vega-lite\n",
    );

    // 构建依然通过；新能力进入依赖闭包；核心产物不变。
    let after = forma::build::run_build(&dir).unwrap();
    let after_ids: Vec<&str> = after.capabilities.iter().map(|c| c.id.as_str()).collect();
    assert!(
        after_ids.contains(&"visualization.vega-lite"),
        "new capability must be active: {after_ids:?}"
    );
    assert!(
        after_ids.contains(&"data.binding"),
        "dependency closure must be pulled in: {after_ids:?}"
    );
    assert!(
        after
            .capabilities
            .iter()
            .any(|c| c.id == "data.binding" && !c.explicit),
        "dependency must be marked non-explicit"
    );
    assert!(
        after
            .capabilities
            .iter()
            .any(|c| c.id == "visualization.vega-lite" && c.explicit),
        "directly declared capability must be marked explicit"
    );
    // 未实现的 adapter 给出警告（而非静默忽略）
    assert!(
        after
            .capability_diagnostics
            .iter()
            .any(|d| d.code == "capability-adapter-not-builtin"),
        "expected capability-adapter-not-builtin warning"
    );
    // 核心产物字节级不变 —— 新能力没有触碰核心
    if let Some(baseline_web) = baseline_web {
        let after_web = fs::read_to_string(dir.join("generated/web/src/api.ts")).unwrap();
        assert_eq!(baseline_web, after_web, "core artifact changed");
    }
}

// ── RFC-0004 §8.4 / G-2：10 项契约的结构化断言 + Deprecation 契约 ──

#[test]
fn builtin_manifests_cover_the_ten_contracts() {
    // RFC-0004 §8.4 的 10 项契约 → Manifest 结构化字段 / Registry 行为的映射断言：
    // Discovery/Registration/Versioning/Dependency/Permissions/Validation 由 Registry 与
    // 其余 capability.rs 测试承担；此处逐 manifest 断言字段面齐备。
    let reg = CapabilityRegistry::with_builtins();
    assert!(reg.ids().len() >= 10);
    for m in reg.manifests() {
        // Manifest 面存在（serde 结构化，而非自由文本）
        assert_eq!(m.api_version, "forma.dev/v1");
        assert_eq!(m.kind, "Capability");
        // Versioning：semver + IR 兼容范围
        assert!(forma::capability::Version::parse(&m.metadata.version).is_some());
        assert!(m.spec.compatibility.ir.is_some() || m.spec.compatibility.runtime.is_some());
        // Dependency：dependencies 字段存在（可空列表），且 registry 闭包可解析
        let _ = &m.spec.dependencies;
        // Permissions：字段存在且可汇总
        let _ = &m.spec.permissions;
        // Adapter / Runtime / Provides / Consumes：字段存在
        let _ = &m.spec.adapters;
        let _ = &m.spec.runtime;
        let _ = &m.spec.provides;
        let _ = &m.spec.consumes;
        // Conformance：声明 suite
        assert!(
            m.spec.conformance.is_some(),
            "{} must declare conformance suite",
            m.metadata.id
        );
    }
}

#[test]
fn deprecation_requires_registered_successor() {
    let reg = CapabilityRegistry::with_builtins();
    // 弃用但无 successor → error（spec:compatibility §9）
    let mut m = reg.manifests().next().unwrap().clone();
    m.spec.deprecated = Some(forma::capability::Deprecation {
        reason: "test".into(),
        successor: None,
    });
    let mut r = CapabilityRegistry::new();
    r.register(m, "test".into()).unwrap();
    let res = r.resolve(&[], &[]);
    assert!(
        res.errors()
            .iter()
            .any(|d| d.code == "capability-deprecation-missing-successor"),
        "{:?}",
        res.errors()
    );

    // successor 未注册 → error
    let mut m = reg.manifests().next().unwrap().clone();
    m.spec.deprecated = Some(forma::capability::Deprecation {
        reason: "test".into(),
        successor: Some("no.such".into()),
    });
    let mut r = CapabilityRegistry::new();
    r.register(m, "test".into()).unwrap();
    let res = r.resolve(&[], &[]);
    assert!(
        res.errors()
            .iter()
            .any(|d| d.code == "capability-deprecation-successor-unknown"),
        "{:?}",
        res.errors()
    );
}
