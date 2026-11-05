//! 架构治理文档校验与依赖图（`forma docs` 子命令族）。
//!
//! 校验 `docs/architecture/` 下受管文档（RFC / ADR / Spec / Capability Manifest）的
//! frontmatter 完整性、ID 唯一性、状态合法性、依赖引用存在性、模板章节合规性与内部链接有效性。
//! 复用 `crate::validate::Diagnostic` 与 `format_diagnostic` 保持输出一致。

use crate::validate::Diagnostic;
use serde::Serialize;
use serde_yaml::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const VALID_STATUS: &[&str] = &[
    "Proposed",
    "In Review",
    "Accepted",
    "Implemented",
    "Stable",
    "Deprecated",
    "Superseded",
    "Rejected",
    "Draft",
];

// ── 模板章节定义 ────────────────────────────────────────────────

/// 一个模板章节：`title` 为规范名，`aliases` 为标题匹配别名（大小写不敏感），
/// `excludes` 用于消歧（如 Goals 不能被 Non-goals 顶替），`core` 表示**不得省略**的核心章节。
struct SectionSpec {
    title: &'static str,
    aliases: &'static [&'static str],
    excludes: &'static [&'static str],
    core: bool,
}

const RFC_SECTIONS: &[SectionSpec] = &[
    SectionSpec {
        title: "Metadata",
        aliases: &["metadata", "元数据"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Abstract",
        aliases: &["abstract", "摘要"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Motivation",
        aliases: &["motivation", "动机"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Goals",
        aliases: &["goals", "目标"],
        excludes: &["non-goals", "非目标"],
        core: false,
    },
    SectionSpec {
        title: "Non-goals",
        aliases: &["non-goals", "非目标"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Terminology",
        aliases: &["terminology", "术语"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Architectural Context",
        aliases: &["architectural context", "架构上下文", "系统上下文"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Design",
        aliases: &["design", "设计"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Compatibility",
        aliases: &["compatibility", "兼容性"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Security Considerations",
        aliases: &["security", "安全"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Performance and Reliability",
        aliases: &["performance", "性能"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Alternatives Considered",
        aliases: &["alternatives", "备选"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Trade-offs",
        aliases: &["trade-offs", "取舍"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Migration Plan",
        aliases: &["migration plan", "迁移计划", "迁移"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Implementation Plan",
        aliases: &["implementation plan", "实现计划"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Conformance Requirements",
        aliases: &["conformance", "一致性", "符合性"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Acceptance Criteria",
        aliases: &["acceptance criteria", "验收标准", "验收测试"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Open Questions",
        aliases: &["open questions", "未决问题", "开放问题"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "References",
        aliases: &["references", "参考"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Change Log",
        aliases: &["change log", "变更记录"],
        excludes: &[],
        core: false,
    },
];

const ADR_SECTIONS: &[SectionSpec] = &[
    SectionSpec {
        title: "Context",
        aliases: &["context", "背景"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Decision",
        aliases: &["decision", "决策"],
        excludes: &[],
        core: true,
    },
    SectionSpec {
        title: "Consequences",
        aliases: &["consequences", "后果"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Alternatives Considered",
        aliases: &["alternatives", "备选"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "References",
        aliases: &["references", "参考"],
        excludes: &[],
        core: false,
    },
    SectionSpec {
        title: "Change Log",
        aliases: &["change log", "变更记录"],
        excludes: &[],
        core: false,
    },
];

// ── 数据结构 ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocKind {
    Rfc,
    Adr,
    Spec,
    Other,
}

impl DocKind {
    fn as_str(self) -> &'static str {
        match self {
            DocKind::Rfc => "rfc",
            DocKind::Adr => "adr",
            DocKind::Spec => "spec",
            DocKind::Other => "other",
        }
    }
}

struct GovernedDoc {
    abs: PathBuf,
    rel: String,
    kind: DocKind,
    /// 是否为「受管文档」（必须带 frontmatter、需符合模板）。
    governed: bool,
    id: Option<String>,
    status: Option<String>,
    fm: Option<Value>,
    raw: String,
}

impl GovernedDoc {
    fn list(&self, field: &str) -> Vec<String> {
        self.fm
            .as_ref()
            .map(|fm| get_list(fm, field))
            .unwrap_or_default()
    }
}

/// 一次扫描的结果：文档集合 + 基础诊断。
struct DocSet {
    docs: Vec<GovernedDoc>,
    id_set: HashSet<String>,
    /// 受管文档数（用于报告）。
    governed_count: usize,
}

pub struct DocsReport {
    pub diagnostics: Vec<Diagnostic>,
    pub docs_checked: usize,
}

/// 诊断计数汇总。
#[derive(Serialize, Debug, Clone, Default)]
pub struct DocsSummary {
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

/// `forma docs <action> --json` 的机器可读报告。
#[derive(Serialize, Debug, Clone)]
pub struct DocsJsonReport {
    pub action: String,
    pub root: String,
    pub docs_checked: usize,
    pub summary: DocsSummary,
    pub exit_code: i32,
    pub diagnostics: Vec<Diagnostic>,
}

/// 把诊断报告转成 JSON 报告（CI 可直接消费）。
pub fn json_report(action: &str, root: &Path, report: &DocsReport) -> DocsJsonReport {
    let mut summary = DocsSummary::default();
    for d in &report.diagnostics {
        match d.severity.as_str() {
            "error" => summary.error += 1,
            "warning" => summary.warning += 1,
            _ => summary.info += 1,
        }
    }
    let exit_code = i32::from(summary.error > 0);
    DocsJsonReport {
        action: action.to_string(),
        root: root.to_string_lossy().replace('\\', "/"),
        docs_checked: report.docs_checked,
        summary,
        exit_code,
        diagnostics: report.diagnostics.clone(),
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub kind: String,
    pub status: Option<String>,
    pub file: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct DocsGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

// ── 解析工具 ────────────────────────────────────────────────────

fn rel_path(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn kind_of(path: &Path, fm_id: &Option<String>) -> DocKind {
    let s = path.to_string_lossy().replace('\\', "/");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if s.contains("/rfc/") && name.starts_with("RFC-") {
        DocKind::Rfc
    } else if s.contains("/adr/") && name.starts_with("ADR-") {
        DocKind::Adr
    } else if fm_id
        .as_deref()
        .map(|i| i.starts_with("spec:"))
        .unwrap_or(false)
        || s.contains("/specifications/")
    {
        DocKind::Spec
    } else {
        DocKind::Other
    }
}

/// 抽取 `---` 包裹的 YAML frontmatter。
fn extract_frontmatter(content: &str) -> Option<Value> {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| l.trim_end() == "---")
        .map(|(i, _)| i)?;
    serde_yaml::from_str::<Value>(&lines[1..end].join("\n")).ok()
}

fn get_str(v: &Value, key: &str) -> Option<String> {
    match v.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}

fn get_list(v: &Value, key: &str) -> Vec<String> {
    match v.get(key) {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn walk_md(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_md(&p, out);
            } else if p
                .extension()
                .map(|x| x.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
}

/// 二级标题（`## x`）的 (行号, 标题)。
fn headings(raw: &str) -> Vec<(usize, String)> {
    raw.lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("## "))
        .map(|(i, l)| (i, l.trim_start_matches("## ").trim().to_string()))
        .collect()
}

/// 取某个二级标题到下一个二级标题之间的正文。
fn section_body(raw: &str, line_idx: usize) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut out = String::new();
    for l in lines.iter().skip(line_idx + 1) {
        if l.starts_with("## ") {
            break;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

fn matches_section(title: &str, spec: &SectionSpec) -> bool {
    let t = title.to_lowercase();
    if spec.excludes.iter().any(|e| t.contains(e)) {
        return false;
    }
    spec.aliases.iter().any(|a| t.contains(a))
}

/// 找到匹配该章节规范的标题行号。
fn find_section(raw: &str, spec: &SectionSpec) -> Option<usize> {
    headings(raw)
        .into_iter()
        .find(|(_, t)| matches_section(t, spec))
        .map(|(i, _)| i)
}

fn has_section(raw: &str, spec: &SectionSpec) -> bool {
    find_section(raw, spec).is_some()
}

/// 抽取 markdown 行内链接的目标（`[text](target)`）。
fn extract_links(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(pos) = raw[i..].find("](") {
        let start = i + pos + 2;
        match raw[start..].find(')') {
            Some(end) => {
                let target = raw[start..start + end].trim();
                let target = target.trim_start_matches('<').trim_end_matches('>');
                let target = target.split_whitespace().next().unwrap_or("");
                if !target.is_empty() {
                    out.push(target.to_string());
                }
                i = start + end + 1;
            }
            None => break,
        }
    }
    out
}

fn is_external_link(target: &str) -> bool {
    target.starts_with('#')
        || target.contains("://")
        || target.starts_with("mailto:")
        || target.starts_with("{{")
        || target.starts_with("data:")
}

// ── 扫描 ────────────────────────────────────────────────────────

fn scan(root: &Path) -> (DocSet, Vec<Diagnostic>) {
    let docs_dir = root.join("docs/architecture");
    let mut diags: Vec<Diagnostic> = Vec::new();

    if !docs_dir.exists() {
        diags.push(Diagnostic {
            severity: "error".into(),
            code: "docs-root-missing".into(),
            message: format!(
                "governance docs directory not found: {}",
                docs_dir.display()
            ),
            file: None,
            line: None,
            nodes: None,
        });
        return (
            DocSet {
                docs: Vec::new(),
                id_set: HashSet::new(),
                governed_count: 0,
            },
            diags,
        );
    }

    let mut files = Vec::new();
    walk_md(&docs_dir, &mut files);
    files.sort();

    let mut docs: Vec<GovernedDoc> = Vec::new();
    for p in &files {
        let raw = match std::fs::read_to_string(p) {
            Ok(r) => r,
            Err(e) => {
                diags.push(Diagnostic {
                    severity: "error".into(),
                    code: "docs-read-error".into(),
                    message: format!("cannot read {}: {e}", p.display()),
                    file: Some(rel_path(root, p)),
                    line: None,
                    nodes: None,
                });
                continue;
            }
        };
        let fm = extract_frontmatter(&raw);
        let id = fm.as_ref().and_then(|v| get_str(v, "id"));
        let kind = kind_of(p, &id);
        // 模板与治理规则只作用于受管文档（RFC / ADR / Spec / Capability Manifest）。
        let governed = matches!(kind, DocKind::Rfc | DocKind::Adr | DocKind::Spec)
            || (fm.is_some() && id.is_some());
        docs.push(GovernedDoc {
            abs: p.clone(),
            rel: rel_path(root, p),
            kind,
            governed,
            id: id.clone(),
            status: fm.as_ref().and_then(|v| get_str(v, "status")),
            fm,
            raw,
        });
    }

    let id_set: HashSet<String> = docs.iter().filter_map(|d| d.id.clone()).collect();
    let governed_count = docs.iter().filter(|d| d.governed).count();
    (
        DocSet {
            docs,
            id_set,
            governed_count,
        },
        diags,
    )
}

// ── 检查项 ──────────────────────────────────────────────────────

fn check_frontmatter(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for d in &set.docs {
        if d.governed && d.fm.is_none() {
            out.push(Diagnostic {
                severity: "error".into(),
                code: "docs-frontmatter-missing".into(),
                message: format!("{} 缺少 YAML frontmatter（id/status 等元数据）", d.rel),
                file: Some(d.rel.clone()),
                line: Some(1),
                nodes: None,
            });
        }
    }
    out
}

fn check_ids(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen: HashMap<String, Vec<String>> = HashMap::new();
    for d in &set.docs {
        if let Some(id) = &d.id {
            seen.entry(id.clone()).or_default().push(d.rel.clone());
        }
    }
    for (id, locs) in &seen {
        if locs.len() > 1 {
            out.push(Diagnostic {
                severity: "error".into(),
                code: "docs-id-duplicate".into(),
                message: format!(
                    "id `{id}` 重复出现在 {} 个文件：{}",
                    locs.len(),
                    locs.join(", ")
                ),
                file: Some(locs[0].clone()),
                line: None,
                nodes: None,
            });
        }
    }
    out
}

fn check_metadata(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for d in &set.docs {
        let Some(fm) = &d.fm else { continue };
        if !d.governed {
            continue;
        }
        // 必填元数据（按文档类型区分模板，避免 ADR 等误报）
        let required: &[&str] = match d.kind {
            DocKind::Rfc => &[
                "id", "title", "status", "version", "authors", "created", "updated",
            ],
            DocKind::Adr => &["id", "title", "status", "date", "deciders"],
            DocKind::Spec => &["id", "title", "status"],
            DocKind::Other => &["id", "status"],
        };
        for field in required {
            if get_str(fm, field).is_none() && get_list(fm, field).is_empty() {
                out.push(Diagnostic {
                    severity: "warning".into(),
                    code: "docs-metadata-incomplete".into(),
                    message: format!("{}: 缺少元数据字段 `{field}`", d.rel),
                    file: Some(d.rel.clone()),
                    line: Some(1),
                    nodes: None,
                });
            }
        }

        // 状态合法性
        if let Some(status) = &d.status {
            if !VALID_STATUS.contains(&status.as_str()) {
                out.push(Diagnostic {
                    severity: "error".into(),
                    code: "docs-status-invalid".into(),
                    message: format!("{}: status `{status}` 不在合法集合内", d.rel),
                    file: Some(d.rel.clone()),
                    line: Some(1),
                    nodes: None,
                });
            }
        }

        // breaking_change 应声明（RFC / Spec 模板含该字段；ADR 模板不含，故不强制）
        if matches!(d.kind, DocKind::Rfc | DocKind::Spec) && fm.get("breaking_change").is_none() {
            out.push(Diagnostic {
                severity: "info".into(),
                code: "docs-breakingchange-missing".into(),
                message: format!("{}: 未声明 `breaking_change` 字段", d.rel),
                file: Some(d.rel.clone()),
                line: Some(1),
                nodes: None,
            });
        }
    }
    out
}

/// 按规范名取章节定义。
fn section_by_title<'a>(specs: &'a [SectionSpec], title: &str) -> &'a SectionSpec {
    specs
        .iter()
        .find(|s| s.title == title)
        .expect("section spec exists")
}

/// validate 使用的关键章节检查（验收 / 变更记录）。
fn check_key_sections(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let acceptance = section_by_title(RFC_SECTIONS, "Acceptance Criteria");
    let changelog = section_by_title(RFC_SECTIONS, "Change Log");
    for d in &set.docs {
        if !d.governed {
            continue;
        }
        if matches!(d.kind, DocKind::Rfc) && !has_section(&d.raw, acceptance) {
            out.push(Diagnostic {
                severity: "warning".into(),
                code: "docs-acceptance-missing".into(),
                message: format!("{}: 缺少「验收标准 / Acceptance Criteria」章节", d.rel),
                file: Some(d.rel.clone()),
                line: None,
                nodes: None,
            });
        }
        if matches!(d.kind, DocKind::Rfc | DocKind::Adr) && !has_section(&d.raw, changelog) {
            out.push(Diagnostic {
                severity: "warning".into(),
                code: "docs-changelog-missing".into(),
                message: format!("{}: 缺少「变更记录 / Change Log」章节", d.rel),
                file: Some(d.rel.clone()),
                line: None,
                nodes: None,
            });
        }
    }
    out
}

/// 引用存在性检查（依赖图的核心闭合性）。
///
/// `depends_on` / `related_rfcs` / `related_adrs` 缺失 → error（核心文档图必须闭合）；
/// `related_specs` / `supersedes` / `superseded_by` 缺失 → warning（spec 属分阶段交付）。
fn check_references(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let ref_fields: &[(&str, bool)] = &[
        ("depends_on", true),
        ("related_rfcs", true),
        ("related_adrs", true),
        ("related_specs", false),
        ("supersedes", false),
        ("superseded_by", false),
    ];
    for d in &set.docs {
        if !d.governed {
            continue;
        }
        let Some(fm) = &d.fm else { continue };
        for (field, is_error) in ref_fields {
            for ref_id in get_list(fm, field) {
                if !set.id_set.contains(&ref_id) {
                    out.push(Diagnostic {
                        severity: if *is_error { "error" } else { "warning" }.into(),
                        code: "docs-ref-missing".into(),
                        message: format!("{}: `{field}` 引用 `{ref_id}` 在文档集中不存在", d.rel),
                        file: Some(d.rel.clone()),
                        line: Some(1),
                        nodes: None,
                    });
                }
            }
        }
    }
    out
}

/// 依赖图的附加检查：自依赖。
fn check_self_dependency(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for d in &set.docs {
        if !d.governed {
            continue;
        }
        let Some(id) = &d.id else { continue };
        if d.list("depends_on").iter().any(|dep| dep == id) {
            out.push(Diagnostic {
                severity: "error".into(),
                code: "docs-dep-self".into(),
                message: format!("{}: `depends_on` 引用了自身 `{id}`", d.rel),
                file: Some(d.rel.clone()),
                line: Some(1),
                nodes: None,
            });
        }
    }
    out
}

fn detect_cycles(nodes: &[(String, Vec<String>)]) -> Vec<Vec<String>> {
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (id.as_str(), i))
        .collect();
    let n = nodes.len();
    let mut color = vec![0u8; n]; // 0 unvisited, 1 in-stack, 2 done
    let mut stack: Vec<usize> = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    fn dfs(
        at: usize,
        nodes: &[(String, Vec<String>)],
        index: &HashMap<&str, usize>,
        color: &mut Vec<u8>,
        stack: &mut Vec<usize>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        color[at] = 1;
        stack.push(at);
        for dep in &nodes[at].1 {
            if let Some(&d) = index.get(dep.as_str()) {
                match color[d] {
                    0 => dfs(d, nodes, index, color, stack, cycles),
                    1 => {
                        if let Some(pos) = stack.iter().position(|&x| x == d) {
                            let cyc: Vec<String> =
                                stack[pos..].iter().map(|&i| nodes[i].0.clone()).collect();
                            cycles.push(cyc);
                        }
                    }
                    _ => {}
                }
            }
        }
        stack.pop();
        color[at] = 2;
    }

    for i in 0..n {
        if color[i] == 0 {
            dfs(i, nodes, &index, &mut color, &mut stack, &mut cycles);
        }
    }
    cycles
}

fn check_cycles(set: &DocSet) -> Vec<Diagnostic> {
    let nodes: Vec<(String, Vec<String>)> = set
        .docs
        .iter()
        .filter(|d| d.governed)
        .filter_map(|d| {
            let id = d.id.clone()?;
            Some((id, d.list("depends_on")))
        })
        .collect();
    let mut out = Vec::new();
    for cyc in detect_cycles(&nodes) {
        out.push(Diagnostic {
            severity: "error".into(),
            code: "docs-cycle".into(),
            message: format!("检测到非法循环依赖：{}", cyc.join(" → ")),
            file: None,
            line: None,
            nodes: Some(cyc),
        });
    }
    out
}

/// lint：模板章节完整性与 N/A 标注规范。
fn check_lint_sections(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for d in &set.docs {
        if !d.governed {
            continue;
        }
        let specs = match d.kind {
            DocKind::Rfc => RFC_SECTIONS,
            DocKind::Adr => ADR_SECTIONS,
            _ => continue,
        };
        for spec in specs {
            match find_section(&d.raw, spec) {
                None => out.push(Diagnostic {
                    severity: if spec.core { "error" } else { "info" }.into(),
                    code: "docs-lint-section-missing".into(),
                    message: format!(
                        "{}: 缺少章节「{}」{}",
                        d.rel,
                        spec.title,
                        if spec.core {
                            "（核心章节，不得省略）"
                        } else {
                            ""
                        }
                    ),
                    file: Some(d.rel.clone()),
                    line: None,
                    nodes: None,
                }),
                Some(line_idx) => {
                    let body = section_body(&d.raw, line_idx).trim().to_string();
                    if body.is_empty() {
                        if spec.core {
                            out.push(Diagnostic {
                                severity: "error".into(),
                                code: "docs-lint-section-empty".into(),
                                message: format!("{}: 核心章节「{}」为空", d.rel, spec.title),
                                file: Some(d.rel.clone()),
                                line: None,
                                nodes: None,
                            });
                        }
                    } else if body.eq_ignore_ascii_case("n/a") || body == "不适用" {
                        out.push(Diagnostic {
                            severity: "warning".into(),
                            code: "docs-lint-na-without-reason".into(),
                            message: format!(
                                "{}: 章节「{}」标注为 N/A 但未说明原因",
                                d.rel, spec.title
                            ),
                            file: Some(d.rel.clone()),
                            line: None,
                            nodes: None,
                        });
                    }
                }
            }
        }
    }
    out
}

/// check-links：内部相对链接是否指向存在的文件。
fn check_links(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for d in &set.docs {
        for target in extract_links(&d.raw) {
            if is_external_link(&target) {
                continue;
            }
            let path_part = target.split('#').next().unwrap_or("");
            if path_part.is_empty() {
                continue;
            }
            let resolved = d.abs.parent().unwrap_or(Path::new(".")).join(path_part);
            if !resolved.exists() {
                out.push(Diagnostic {
                    severity: "warning".into(),
                    code: "docs-link-broken".into(),
                    message: format!("{}: 链接目标不存在 → {}", d.rel, target),
                    file: Some(d.rel.clone()),
                    line: None,
                    nodes: None,
                });
            }
        }
    }
    out
}

/// check-compatibility：破坏性变更声明与废弃文档的继任关系。
fn check_compatibility(set: &DocSet) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let status_of: HashMap<&str, &str> = set
        .docs
        .iter()
        .filter_map(|d| Some((d.id.as_deref()?, d.status.as_deref()?)))
        .collect();

    for d in &set.docs {
        if !d.governed {
            continue;
        }
        let Some(fm) = &d.fm else { continue };

        // 声明破坏性变更 → 必须有迁移计划
        let breaking = matches!(fm.get("breaking_change"), Some(Value::Bool(true)));
        if breaking && matches!(d.kind, DocKind::Rfc) {
            let migration = section_by_title(RFC_SECTIONS, "Migration Plan");
            if !has_section(&d.raw, migration) {
                out.push(Diagnostic {
                    severity: "error".into(),
                    code: "docs-compat-breaking-without-migration".into(),
                    message: format!(
                        "{}: 声明 `breaking_change: true` 但缺少「Migration Plan / 迁移计划」章节",
                        d.rel
                    ),
                    file: Some(d.rel.clone()),
                    line: Some(1),
                    nodes: None,
                });
            }
        }

        // 废弃 / 被取代 → 必须指向继任者
        if matches!(d.status.as_deref(), Some("Deprecated") | Some("Superseded"))
            && d.list("superseded_by").is_empty()
        {
            out.push(Diagnostic {
                severity: "warning".into(),
                code: "docs-compat-deprecated-without-successor".into(),
                message: format!(
                    "{}: status 为 `{}` 但未通过 `superseded_by` 指向继任文档",
                    d.rel,
                    d.status.as_deref().unwrap_or("")
                ),
                file: Some(d.rel.clone()),
                line: Some(1),
                nodes: None,
            });
        }

        // 依赖了已废弃 / 被取代的文档
        for dep in d.list("depends_on") {
            if let Some(st) = status_of.get(dep.as_str()) {
                if matches!(*st, "Deprecated" | "Superseded") {
                    out.push(Diagnostic {
                        severity: "warning".into(),
                        code: "docs-compat-dep-on-deprecated".into(),
                        message: format!("{}: 依赖了状态为 `{st}` 的文档 `{dep}`", d.rel),
                        file: Some(d.rel.clone()),
                        line: Some(1),
                        nodes: None,
                    });
                }
            }
        }
    }
    out
}

// ── 对外入口 ────────────────────────────────────────────────────

pub fn run_docs_validate(root: &Path) -> DocsReport {
    let (set, mut diags) = scan(root);
    diags.extend(check_frontmatter(&set));
    diags.extend(check_ids(&set));
    diags.extend(check_metadata(&set));
    diags.extend(check_key_sections(&set));
    diags.extend(check_references(&set));
    diags.extend(check_cycles(&set));
    DocsReport {
        diagnostics: diags,
        docs_checked: set.governed_count,
    }
}

pub fn run_docs_lint(root: &Path) -> DocsReport {
    let (set, mut diags) = scan(root);
    diags.extend(check_frontmatter(&set));
    diags.extend(check_lint_sections(&set));
    DocsReport {
        diagnostics: diags,
        docs_checked: set.governed_count,
    }
}

pub fn run_docs_check_dependencies(root: &Path) -> DocsReport {
    let (set, mut diags) = scan(root);
    diags.extend(check_frontmatter(&set));
    diags.extend(check_references(&set));
    diags.extend(check_self_dependency(&set));
    diags.extend(check_cycles(&set));
    DocsReport {
        diagnostics: diags,
        docs_checked: set.governed_count,
    }
}

pub fn run_docs_check_compatibility(root: &Path) -> DocsReport {
    run_docs_check_compatibility_against(root, None)
}

fn run_docs_check_compatibility_base(root: &Path) -> DocsReport {
    let (set, mut diags) = scan(root);
    diags.extend(check_frontmatter(&set));
    diags.extend(check_compatibility(&set));
    DocsReport {
        diagnostics: diags,
        docs_checked: set.governed_count,
    }
}

/// 基于 Git 基线做规格语义变化扫描（P1-10）。
///
/// - 移除/削弱规范性 MUST 条款 → `docs-compat-breaking-candidate`（warning，需要人工确认）；
/// - 版本 major 增长但未声明 `breaking_change: true` → error；
/// - 已删除规格 → error（必须经 Deprecation/Supersedes 流程，不可直接删）。
///
/// 这是**保守检测器**：自动发现高置信破坏候选，但不试图替代人工架构审查。
pub fn run_docs_check_compatibility_against(root: &Path, base: Option<&str>) -> DocsReport {
    let mut report = run_docs_check_compatibility_base(root);
    if let Some(base_ref) = base {
        let valid = git(root, &["rev-parse", "--verify", base_ref])
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !valid {
            report.diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "docs-compat-base-invalid".into(),
                message: format!("git base `{base_ref}` does not resolve to a commit"),
                file: None,
                line: None,
                nodes: None,
            });
            return report;
        }
    }
    let base_ref = base.unwrap_or("HEAD");
    report
        .diagnostics
        .extend(detect_spec_compatibility_changes(root, base_ref));
    report
}

fn git(root: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("git").arg("-C").arg(root).args(args).output()
}

fn major_version(fm: &Value) -> Option<u64> {
    get_str(fm, "version")?
        .split('.')
        .next()?
        .trim_start_matches('v')
        .parse()
        .ok()
}

fn is_contract_line(line: &str) -> bool {
    let l = line.to_lowercase();
    let normative = ["must", "must not", "shall", "不得", "必须", "严禁"]
        .iter()
        .any(|token| l.contains(token));
    // 规格契约表中的反引号标识（类型名 / 字段名 / 错误码 / 协议 token）也是结构化语义：
    // 删除整行应自动标为兼容性候选，交由人工确认是破坏还是重排。
    let table_contract = {
        let t = line.trim();
        t.starts_with('|')
            && t.contains('`')
            && !t.contains("---")
            // 差距清单行是实现状态追踪，不是兼容契约本身（例如 spec:ir 的 G-5/G-6 关闭）。
            && !t.starts_with("| G-")
    };
    normative || table_contract
}

fn detect_spec_compatibility_changes(root: &Path, base: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let base_check = match git(root, &["rev-parse", "--verify", base]) {
        Ok(o) if o.status.success() => o,
        _ => {
            // 非 git 工程或无效 base：本地临时 fixture 不应因此失败；显式 base 由调用端校验。
            return out;
        }
    };
    let _ = base_check;
    let diff = match git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--unified=0",
            base,
            "--",
            "docs/architecture/specifications",
        ],
    ) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return out,
    };
    if diff.is_empty() {
        return out;
    }

    let name_status = match git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--name-status",
            base,
            "--",
            "docs/architecture/specifications",
        ],
    ) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return out,
    };
    for row in name_status.lines() {
        let mut cols = row.splitn(2, '\t');
        let status = cols.next().unwrap_or("");
        let path = cols.next().unwrap_or("");
        if status == "D" {
            out.push(Diagnostic {
                severity: "error".into(),
                code: "docs-compat-spec-deleted".into(),
                message: format!(
                    "{path}: specification was deleted; use Deprecated/Superseded with a migration/successor instead of direct removal"
                ),
                file: Some(path.to_string()),
                line: None,
                nodes: None,
            });
        }
    }

    let mut current_file = String::new();
    let mut removed: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    let flush = |path: &str,
                 removed: &mut Vec<String>,
                 added: &mut Vec<String>,
                 out: &mut Vec<Diagnostic>| {
        if path.is_empty() {
            removed.clear();
            added.clear();
            return;
        }
        if path.ends_with(".md") && path.contains("specifications/") {
            let abs = root.join(path);
            let current = std::fs::read_to_string(&abs).ok();
            let base_text = git(root, &["show", &format!("{base}:{path}")])
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string());
            match (base_text, current) {
                (Some(old), Some(now)) => {
                    let old_fm = extract_frontmatter(&old).unwrap_or(Value::Null);
                    let new_fm = extract_frontmatter(&now).unwrap_or(Value::Null);
                    let declared_breaking = matches!(new_fm.get("breaking_change"), Some(Value::Bool(true)));
                    let major_bump = major_version(&old_fm)
                        .zip(major_version(&new_fm))
                        .map(|(a, b)| b > a)
                        .unwrap_or(false);
                    if major_bump && !declared_breaking {
                        out.push(Diagnostic {
                            severity: "error".into(),
                            code: "docs-compat-major-without-marker".into(),
                            message: format!(
                                "{path}: major version increased but `breaking_change: true` is not declared"
                            ),
                            file: Some(path.to_string()),
                            line: Some(1),
                            nodes: None,
                        });
                    }
                    let added_set: HashSet<String> = added
                        .iter()
                        .map(|l| l.trim().to_string())
                        .collect();
                    let removed_normative: Vec<String> = removed
                        .iter()
                        .filter(|l| is_contract_line(l))
                        .filter(|l| !added_set.contains(l.trim()))
                        .cloned()
                        .collect();
                    if !removed_normative.is_empty() && !declared_breaking {
                        let evidence = removed_normative
                            .iter()
                            .take(3)
                            .map(|l| l.trim())
                            .collect::<Vec<_>>()
                            .join(" | ");
                        out.push(Diagnostic {
                            severity: "warning".into(),
                            code: "docs-compat-breaking-candidate".into(),
                            message: format!(
                                "{path}: removed/changed normative or structured contract line(s) may be breaking; review and set `breaking_change: true` if applicable. Evidence: {evidence}"
                            ),
                            file: Some(path.to_string()),
                            line: None,
                            nodes: None,
                        });
                    }
                }
                (Some(_), None) => out.push(Diagnostic {
                    severity: "error".into(),
                    code: "docs-compat-spec-deleted".into(),
                    message: format!(
                        "{path}: specification was deleted; use Deprecated/Superseded with a migration/successor instead of direct removal"
                    ),
                    file: Some(path.to_string()),
                    line: None,
                    nodes: None,
                }),
                _ => {}
            }
        }
        removed.clear();
        added.clear();
    };

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            flush(&current_file, &mut removed, &mut added, &mut out);
            current_file = path.to_string();
        } else if line.starts_with("--- ")
            || line.starts_with("diff ")
            || line.starts_with("index ")
        {
            continue;
        } else if let Some(body) = line.strip_prefix('-') {
            removed.push(body.to_string());
        } else if let Some(body) = line.strip_prefix('+') {
            added.push(body.to_string());
        }
    }
    flush(&current_file, &mut removed, &mut added, &mut out);
    out
}

pub fn run_docs_check_links(root: &Path) -> DocsReport {
    let (set, mut diags) = scan(root);
    diags.extend(check_links(&set));
    DocsReport {
        diagnostics: diags,
        docs_checked: set.governed_count,
    }
}

/// 构建依赖图（节点 = 受管文档，边 = 引用关系）。
pub fn build_docs_graph(root: &Path) -> (DocsGraph, Vec<Diagnostic>) {
    let (set, mut diags) = scan(root);
    let edge_kinds: &[(&str, &str)] = &[
        ("depends_on", "depends_on"),
        ("related_rfcs", "related_rfc"),
        ("related_adrs", "related_adr"),
        ("related_specs", "related_spec"),
        ("supersedes", "supersedes"),
        ("superseded_by", "superseded_by"),
    ];

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for d in &set.docs {
        if !d.governed {
            continue;
        }
        let Some(id) = &d.id else { continue };
        nodes.push(GraphNode {
            id: id.clone(),
            kind: d.kind.as_str().to_string(),
            status: d.status.clone(),
            file: d.rel.clone(),
        });
        for (field, kind) in edge_kinds {
            for target in d.list(field) {
                if set.id_set.contains(&target) {
                    edges.push(GraphEdge {
                        from: id.clone(),
                        to: target.clone(),
                        kind: (*kind).to_string(),
                    });
                }
            }
        }
    }
    diags.extend(check_cycles(&set));
    (DocsGraph { nodes, edges }, diags)
}

/// 把依赖图渲染为文本。
pub fn render_docs_graph(graph: &DocsGraph, diagnostics: &[Diagnostic]) -> String {
    let mut lines = vec![
        "Forma 架构依赖图".to_string(),
        "====================".to_string(),
        format!(
            "{} node(s), {} edge(s)",
            graph.nodes.len(),
            graph.edges.len()
        ),
        String::new(),
    ];
    for node in &graph.nodes {
        lines.push(format!(
            "{}  [{}] {}",
            node.id,
            node.status.as_deref().unwrap_or("-"),
            node.file
        ));
        let mut out: Vec<&GraphEdge> = graph.edges.iter().filter(|e| e.from == node.id).collect();
        out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.to.cmp(&b.to)));
        for (i, e) in out.iter().enumerate() {
            let branch = if i + 1 == out.len() {
                "└─"
            } else {
                "├─"
            };
            lines.push(format!("  {branch} {:<16} → {}", e.kind, e.to));
        }
    }
    let cycles: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.code == "docs-cycle")
        .collect();
    lines.push(String::new());
    if cycles.is_empty() {
        lines.push("循环依赖：无".to_string());
    } else {
        lines.push(format!("循环依赖：{} 处", cycles.len()));
        for c in cycles {
            lines.push(format!("  {}", c.message));
        }
    }
    lines.join("\n")
}
