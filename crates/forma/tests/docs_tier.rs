//! RFC-0001 §16 / G-1：治理依赖图无 **Tier 反向边**。
//!
//! Tier 分层（RFC-0001 §7.3）：T1 = 稳定基础（IR / Type / Identity / 依赖图 / 兼容规则）；
//! T2 = 可扩展层（Capability / Adapter / Runtime / Agent Protocol）；
//! T3 = 实验层。红线：**T1 文档不得 depends_on T2 文档**（基础层不得依赖扩展层）。
//!
//! 实现：解析 `forma docs graph --json` 的边，按 Tier 映射检查 `depends_on` 方向。

use std::process::Command;

/// Tier 映射（文档 id 前缀 → 层级）
fn tier(id: &str) -> Option<u8> {
    if id.starts_with("RFC-0001") {
        // 架构总览：治理元层，不参与 Tier 约束
        None
    } else if id.starts_with("RFC-0003") {
        Some(1) // 语义核心
    } else if id.starts_with("spec:") {
        let t2_specs = ["spec:manifests", "spec:protocols"];
        if t2_specs.contains(&id) {
            Some(2)
        } else {
            // spec:ir / spec:types / spec:serialization / spec:compatibility → 基础
            Some(1)
        }
    } else if id.starts_with("RFC-0002")
        || id.starts_with("RFC-0004")
        || id.starts_with("RFC-0005")
        || id.starts_with("RFC-0006")
        || id.starts_with("RFC-0007")
        || id.starts_with("RFC-0008")
        || id.starts_with("RFC-0009")
        || id.starts_with("RFC-0010")
    {
        Some(2)
    } else {
        None
    }
}

#[test]
fn governance_graph_has_no_tier_reverse_edges() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_forma"))
        .args(["docs", "graph", "--json"])
        .current_dir(&root)
        .output()
        .expect("run forma docs graph --json");
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");

    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for e in json["edges"].as_array().unwrap() {
        if e["kind"] != "depends_on" {
            continue;
        }
        checked += 1;
        let from = e["from"].as_str().unwrap_or_default();
        let to = e["to"].as_str().unwrap_or_default();
        let (Some(ft), Some(tt)) = (tier(from), tier(to)) else {
            continue;
        };
        if ft < tt {
            violations.push(format!(
                "{from} (T{ft}) depends_on {to} (T{tt}) — Tier 反向边（基础层依赖扩展层）"
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "tier reverse edges:\n{}",
        violations.join("\n")
    );
    assert!(checked > 0, "no depends_on edges parsed");
}

#[test]
fn tier_mapping_covers_all_rfc_ids() {
    // 映射完整性：0002–0010 必须都有 Tier 归属（0001 是治理元层，豁免）
    for n in 2..=10u32 {
        let id = format!("RFC-{n:04}");
        assert!(tier(&id).is_some(), "RFC-{n:04} missing tier mapping");
    }
}
