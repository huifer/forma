//! `forma docs` 回归测试（RFC Governance）：
//! - validate：frontmatter / ID / 状态 / 引用 / 循环；
//! - lint：模板章节 / N/A；
//! - graph / compatibility / dependencies / links；
//! - Git 基线上的规范性变更检测（P1-10）；
//! - 仓库自身治理文档是零 error 的回归防线。
//!
//! 并行安全：所有 fixture 写入唯一 `tmp_dir`。

use forma::docs::{run_docs_validate, DocsReport};
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
        "forma-docs-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_doc(root: &Path, rel: &str, content: &str) {
    let p = root.join("docs/architecture").join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn rfc(id: &str, status: &str, deps: &str) -> String {
    format!(
        r#"---
id: {id}
title: {id} 测试
status: {status}
version: 1.0
authors: [tester]
created: 2026-11-04
updated: 2026-11-04
domain: architecture
depends_on: [{deps}]
related_rfcs: []
related_adrs: []
breaking_change: false
---

# {id}

## 17. Acceptance Criteria

验收内容。

## 20. Change Log

| 版本 | 日期 | 摘要 |
|---|---|---|
| 1.0 | 2026-11-04 | init |
"#
    )
}

fn full_rfc(id: &str, deps: &str) -> String {
    let mut s = format!(
        "---\nid: {id}\ntitle: {id} 测试\nstatus: Accepted\nversion: 1.0\nauthors: [tester]\ncreated: 2026-11-04\nupdated: 2026-11-04\ndomain: architecture\ndepends_on: [{deps}]\nrelated_rfcs: []\nrelated_adrs: []\nbreaking_change: false\n---\n\n# {id}\n\n"
    );
    for (n, title) in [
        (1, "Metadata"),
        (2, "Abstract"),
        (3, "Motivation"),
        (4, "Goals"),
        (5, "Non-goals"),
        (6, "Terminology"),
        (7, "Architectural Context"),
        (8, "Design"),
        (9, "Compatibility"),
        (10, "Security Considerations"),
        (11, "Performance and Reliability"),
        (12, "Alternatives Considered"),
        (13, "Trade-offs"),
        (14, "Migration Plan"),
        (15, "Implementation Plan"),
        (16, "Conformance Requirements"),
        (17, "Acceptance Criteria"),
        (18, "Open Questions"),
        (19, "References"),
        (20, "Change Log"),
    ] {
        s.push_str(&format!("## {n}. {title}\n\n内容占位。\n\n"));
    }
    s
}

fn errors_of(root: &Path) -> Vec<String> {
    run_docs_validate(root)
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn codes_of_report(report: &DocsReport) -> Vec<String> {
    report.diagnostics.iter().map(|d| d.code.clone()).collect()
}

fn codes_of(root: &Path) -> Vec<String> {
    codes_of_report(&run_docs_validate(root))
}

// ── 1. validate ─────────────────────────────────────────────────

#[test]
fn valid_governance_tree_has_no_errors() {
    let root = tmp_dir("valid");
    write_doc(
        &root,
        "rfc/RFC-0001-alpha.zh-CN.md",
        &rfc("RFC-0001", "Accepted", ""),
    );
    write_doc(
        &root,
        "rfc/RFC-0002-beta.zh-CN.md",
        &rfc("RFC-0002", "Proposed", "RFC-0001"),
    );
    let errs = errors_of(&root);
    assert!(errs.is_empty(), "expected no errors, got: {errs:?}");
}

#[test]
fn duplicate_id_is_detected() {
    let root = tmp_dir("dup");
    write_doc(
        &root,
        "rfc/RFC-0001-alpha.zh-CN.md",
        &rfc("RFC-0001", "Accepted", ""),
    );
    write_doc(
        &root,
        "rfc/RFC-0009-dup.zh-CN.md",
        &rfc("RFC-0001", "Proposed", ""),
    );
    let codes = codes_of(&root);
    assert!(codes.iter().any(|c| c == "docs-id-duplicate"), "{codes:?}");
}

#[test]
fn invalid_status_is_detected() {
    let root = tmp_dir("status");
    write_doc(
        &root,
        "rfc/RFC-0001-x.zh-CN.md",
        &rfc("RFC-0001", "TotallyBogus", ""),
    );
    let errs = errors_of(&root);
    assert!(
        errs.iter().any(|e| e.starts_with("docs-status-invalid")),
        "{errs:?}"
    );
}

#[test]
fn missing_frontmatter_is_detected() {
    let root = tmp_dir("nofm");
    write_doc(
        &root,
        "rfc/RFC-0001-x.zh-CN.md",
        "# RFC-0001\n\n无 frontmatter。\n",
    );
    let errs = errors_of(&root);
    assert!(
        errs.iter()
            .any(|e| e.starts_with("docs-frontmatter-missing")),
        "{errs:?}"
    );
}

#[test]
fn missing_dependency_reference_is_detected() {
    let root = tmp_dir("ref");
    write_doc(
        &root,
        "rfc/RFC-0001-x.zh-CN.md",
        &rfc("RFC-0001", "Proposed", "RFC-9999"),
    );
    let errs = errors_of(&root);
    assert!(
        errs.iter().any(|e| e.starts_with("docs-ref-missing")),
        "{errs:?}"
    );
}

#[test]
fn dependency_cycle_is_detected() {
    let root = tmp_dir("cycle");
    write_doc(
        &root,
        "rfc/RFC-0001-a.zh-CN.md",
        &rfc("RFC-0001", "Proposed", "RFC-0002"),
    );
    write_doc(
        &root,
        "rfc/RFC-0002-b.zh-CN.md",
        &rfc("RFC-0002", "Proposed", "RFC-0001"),
    );
    let errs = errors_of(&root);
    assert!(errs.iter().any(|e| e.starts_with("docs-cycle")), "{errs:?}");
}

#[test]
fn repository_governance_docs_have_no_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let report = run_docs_validate(&root);
    let errs: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(errs.is_empty(), "repository docs must be valid: {errs:?}");
    assert!(report.docs_checked >= 10);
}

// ── 2. lint ─────────────────────────────────────────────────────

#[test]
fn lint_accepts_full_template_without_missing_sections() {
    let root = tmp_dir("lint-ok");
    write_doc(&root, "rfc/RFC-0001-a.zh-CN.md", &full_rfc("RFC-0001", ""));
    let report = forma::docs::run_docs_lint(&root);
    let bad: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "docs-lint-section-missing")
        .map(|d| d.message.clone())
        .collect();
    assert!(
        bad.is_empty(),
        "full template must not miss sections: {bad:?}"
    );
}

#[test]
fn lint_flags_missing_core_sections() {
    let root = tmp_dir("lint-missing");
    write_doc(
        &root,
        "rfc/RFC-0001-a.zh-CN.md",
        &rfc("RFC-0001", "Accepted", ""),
    );
    let report = forma::docs::run_docs_lint(&root);
    let missing: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "docs-lint-section-missing" && d.severity == "error")
        .map(|d| d.message.clone())
        .collect();
    assert!(
        missing.iter().any(|m| m.contains("Security")),
        "{missing:?}"
    );
    assert!(
        missing.iter().any(|m| m.contains("Migration")),
        "{missing:?}"
    );
}

#[test]
fn lint_flags_na_without_reason() {
    let root = tmp_dir("lint-na");
    let doc = full_rfc("RFC-0001", "").replace(
        "## 10. Security Considerations\n\n内容占位。",
        "## 10. Security Considerations\n\nN/A",
    );
    write_doc(&root, "rfc/RFC-0001-a.zh-CN.md", &doc);
    let codes = codes_of_report(&forma::docs::run_docs_lint(&root));
    assert!(
        codes.iter().any(|c| c == "docs-lint-na-without-reason"),
        "{codes:?}"
    );
}

// ── 3. links / dependency / compatibility / graph ────────────────

#[test]
fn check_links_detects_broken_link() {
    let root = tmp_dir("links");
    write_doc(
        &root,
        "rfc/RFC-0001-a.zh-CN.md",
        &format!(
            "{}\n## 19. References\n\n- [missing](missing.md)\n",
            rfc("RFC-0001", "Accepted", "")
        ),
    );
    let report = forma::docs::run_docs_check_links(&root);
    assert!(codes_of_report(&report)
        .iter()
        .any(|c| c == "docs-link-broken"));
}

#[test]
fn check_dependencies_detects_self_dependency() {
    let root = tmp_dir("selfdep");
    write_doc(
        &root,
        "rfc/RFC-0001-a.zh-CN.md",
        &rfc("RFC-0001", "Accepted", "RFC-0001"),
    );
    let report = forma::docs::run_docs_check_dependencies(&root);
    assert!(codes_of_report(&report)
        .iter()
        .any(|c| c == "docs-dep-self"));
}

#[test]
fn check_compatibility_requires_migration_plan_for_breaking_rfc() {
    let root = tmp_dir("compat-rfc");
    let doc =
        rfc("RFC-0001", "Accepted", "").replace("breaking_change: false", "breaking_change: true");
    write_doc(&root, "rfc/RFC-0001-a.zh-CN.md", &doc);
    let report = forma::docs::run_docs_check_compatibility(&root);
    assert!(codes_of_report(&report)
        .iter()
        .any(|c| c == "docs-compat-breaking-without-migration"));
}

#[test]
fn graph_builds_nodes_and_edges() {
    let root = tmp_dir("graph");
    write_doc(&root, "rfc/RFC-0001-a.zh-CN.md", &full_rfc("RFC-0001", ""));
    write_doc(
        &root,
        "rfc/RFC-0002-b.zh-CN.md",
        &full_rfc("RFC-0002", "RFC-0001"),
    );
    let (graph, diagnostics) = forma::docs::build_docs_graph(&root);
    assert_eq!(graph.nodes.len(), 2);
    assert!(graph
        .edges
        .iter()
        .any(|e| e.from == "RFC-0002" && e.to == "RFC-0001" && e.kind == "depends_on"));
    assert!(!diagnostics.iter().any(|d| d.code == "docs-cycle"));
}

// ── 4. P1-10：Git 基线上的破坏性变更检测 ─────────────────────────

fn init_git_repo(root: &Path) {
    use std::process::Command;
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "forma-tests@example.invalid"],
        vec!["config", "user.name", "Forma tests"],
    ] {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git command failed: {output:?}");
    }
}

fn commit_all(root: &Path, message: &str) {
    use std::process::Command;
    for args in [vec!["add", "-A"], vec!["commit", "-q", "-m", message]] {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git command failed: {output:?}");
    }
}

fn write_spec(root: &Path, version: &str, breaking: bool, sentence: &str) {
    write_doc(
        root,
        "specifications/types/type-system.md",
        &format!(
            "---\nid: spec:types\ntitle: Type System\nstatus: Accepted\nversion: {version}\nauthors: [test]\ncreated: 2026-01-01\nupdated: 2026-01-01\ndepends_on: []\nbreaking_change: {breaking}\n---\n\n# Type System\n\n## 1. Contract\n\n{sentence}\n"
        ),
    );
}

#[test]
fn compatibility_warns_when_a_normative_requirement_is_weakened() {
    let root = tmp_dir("compat-normative");
    write_spec(
        &root,
        "1.0.0",
        false,
        "Implementations MUST reject invalid input.",
    );
    init_git_repo(&root);
    commit_all(&root, "base spec");
    write_spec(
        &root,
        "1.0.1",
        false,
        "Implementations SHOULD reject invalid input.",
    );
    let report = forma::docs::run_docs_check_compatibility_against(&root, Some("HEAD"));
    assert!(
        codes_of_report(&report)
            .iter()
            .any(|c| c == "docs-compat-breaking-candidate"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn compatibility_errors_on_major_bump_without_marker() {
    let root = tmp_dir("compat-major");
    write_spec(&root, "1.0.0", false, "Contract unchanged.");
    init_git_repo(&root);
    commit_all(&root, "base spec");
    write_spec(&root, "2.0.0", false, "Contract unchanged.");
    let report = forma::docs::run_docs_check_compatibility_against(&root, Some("HEAD"));
    assert!(
        codes_of_report(&report)
            .iter()
            .any(|c| c == "docs-compat-major-without-marker"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn compatibility_accepts_declared_breaking_major_change() {
    let root = tmp_dir("compat-marked");
    write_spec(
        &root,
        "1.0.0",
        false,
        "Implementations MUST reject invalid input.",
    );
    init_git_repo(&root);
    commit_all(&root, "base spec");
    write_spec(
        &root,
        "2.0.0",
        true,
        "Implementations SHOULD reject invalid input.",
    );
    let report = forma::docs::run_docs_check_compatibility_against(&root, Some("HEAD"));
    assert!(!codes_of_report(&report)
        .iter()
        .any(|c| c == "docs-compat-major-without-marker"));
}

#[test]
fn compatibility_rejects_deleted_specification() {
    let root = tmp_dir("compat-deleted");
    write_spec(&root, "1.0.0", false, "Contract.");
    init_git_repo(&root);
    commit_all(&root, "base spec");
    fs::remove_file(root.join("docs/architecture/specifications/types/type-system.md")).unwrap();
    let report = forma::docs::run_docs_check_compatibility_against(&root, Some("HEAD"));
    assert!(codes_of_report(&report)
        .iter()
        .any(|c| c == "docs-compat-spec-deleted"));
}
