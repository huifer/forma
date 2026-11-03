//! Forma Lint — DSL 质量检查器（类似 ESLint）。
//!
//! 内置规则分类：naming / completeness / security / performance / convention
//! 用法：`forma lint [dir]`

use crate::dsl::Program;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Off,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LintDiagnostic {
    pub rule: String,
    pub severity: String,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

impl LintDiagnostic {
    pub fn new(rule: &str, category: &str, sev: Severity, msg: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            severity: sev.as_str().into(),
            category: category.into(),
            message: msg.into(),
            entity: None,
            field: None,
            line: None,
            fix_hint: None,
        }
    }
    pub fn entity(mut self, e: &str) -> Self {
        self.entity = Some(e.into());
        self
    }
    pub fn field(mut self, f: &str) -> Self {
        self.field = Some(f.into());
        self
    }
    pub fn line(mut self, l: usize) -> Self {
        self.line = Some(l);
        self
    }
    pub fn hint(mut self, h: &str) -> Self {
        self.fix_hint = Some(h.into());
        self
    }
}

pub type LintFn = fn(&Program) -> Vec<LintDiagnostic>;

pub struct LintRule {
    pub id: &'static str,
    pub category: &'static str,
    pub severity: Severity,
    pub check: LintFn,
}

fn has_attr(f: &crate::dsl::FieldDecl, name: &str) -> bool {
    f.attrs.iter().any(|a| a.name == name)
}

fn is_pascal(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_camel(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn to_pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn to_camel(s: &str) -> String {
    let p = to_pascal(s);
    let mut c = p.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ── naming ──

fn lint_entity_pascal(p: &Program) -> Vec<LintDiagnostic> {
    p.entities
        .iter()
        .filter(|e| !is_pascal(&e.name))
        .map(|e| {
            LintDiagnostic::new(
                "naming/entity-pascal-case",
                "naming",
                Severity::Warning,
                format!("实体名 '{}' 应使用 PascalCase", e.name),
            )
            .entity(&e.name)
            .hint(&format!("改为 '{}'", to_pascal(&e.name)))
        })
        .collect()
}

fn lint_field_camel(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.entities {
        for f in &e.fields {
            if !is_camel(&f.name) {
                out.push(
                    LintDiagnostic::new(
                        "naming/field-camel-case",
                        "naming",
                        Severity::Warning,
                        format!("字段名 '{}' 应使用 camelCase", f.name),
                    )
                    .entity(&e.name)
                    .field(&f.name)
                    .line(f.line)
                    .hint(&format!("改为 '{}'", to_camel(&f.name))),
                );
            }
        }
    }
    out
}

// ── completeness ──

fn lint_require_id(p: &Program) -> Vec<LintDiagnostic> {
    p.entities
        .iter()
        .filter(|e| !e.fields.iter().any(|f| has_attr(f, "id")))
        .map(|e| {
            LintDiagnostic::new(
                "completeness/require-id",
                "completeness",
                Severity::Error,
                format!("实体 '{}' 缺少 @id 主键", e.name),
            )
            .entity(&e.name)
            .hint("添加 `id Uuid @id @default(uuid)`")
        })
        .collect()
}

fn lint_require_timestamps(p: &Program) -> Vec<LintDiagnostic> {
    p.entities
        .iter()
        .filter(|e| {
            !e.fields
                .iter()
                .any(|f| f.name == "createdAt" || f.name == "created_at")
        })
        .map(|e| {
            LintDiagnostic::new(
                "completeness/require-timestamps",
                "completeness",
                Severity::Warning,
                format!("实体 '{}' 缺少 createdAt", e.name),
            )
            .entity(&e.name)
            .hint("添加 `createdAt Timestamp @default(now) @readonly`")
        })
        .collect()
}

fn lint_i18n_labels(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.enums {
        for v in &e.variants {
            if v.labels.is_empty() {
                out.push(
                    LintDiagnostic::new(
                        "completeness/i18n-labels",
                        "completeness",
                        Severity::Info,
                        format!("枚举 {} 变体 '{}' 缺少 @label", e.name, v.name),
                    )
                    .entity(&e.name)
                    .field(&v.name),
                );
            }
        }
    }
    for pg in &p.pages {
        if pg.title.is_empty() {
            out.push(
                LintDiagnostic::new(
                    "completeness/i18n-labels",
                    "completeness",
                    Severity::Warning,
                    format!("页面 '{}' 缺少 title", pg.name),
                )
                .entity(&pg.name),
            );
        }
    }
    out
}

// ── security ──

fn lint_email_unique(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.entities {
        for f in &e.fields {
            if f.type_name == "Email" && !has_attr(f, "unique") {
                out.push(
                    LintDiagnostic::new(
                        "security/email-unique",
                        "security",
                        Severity::Warning,
                        format!("Email 字段 {}.{} 建议添加 @unique", e.name, f.name),
                    )
                    .entity(&e.name)
                    .field(&f.name),
                );
            }
        }
    }
    out
}

fn lint_sensitive_hidden(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.entities {
        for f in &e.fields {
            let lower = f.name.to_lowercase();
            if (lower.contains("password") || lower.contains("secret") || lower.contains("token"))
                && !has_attr(f, "hidden")
            {
                out.push(
                    LintDiagnostic::new(
                        "security/sensitive-hidden",
                        "security",
                        Severity::Warning,
                        format!("敏感字段 {}.{} 建议添加 @hidden", e.name, f.name),
                    )
                    .entity(&e.name)
                    .field(&f.name),
                );
            }
        }
    }
    out
}

fn lint_entity_has_access(p: &Program) -> Vec<LintDiagnostic> {
    let mut covered = std::collections::HashSet::new();
    for r in &p.rules {
        for g in &r.guards {
            covered.insert(g.entity.clone());
        }
    }
    let mut out = Vec::new();
    for e in &p.entities {
        if e.fields.iter().any(|f| f.type_name == "Email") && !covered.contains(&e.name) {
            out.push(
                LintDiagnostic::new(
                    "security/entity-has-access",
                    "security",
                    Severity::Info,
                    format!("实体 '{}' 包含敏感字段但没有 access/guard 规则", e.name),
                )
                .entity(&e.name)
                .hint("添加 `rules` 块的 `guard` 或 `access` 声明"),
            );
        }
    }
    out
}

fn lint_string_max_length(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.entities {
        for f in &e.fields {
            if f.type_name == "String" && f.type_args.is_empty() && !f.optional {
                out.push(
                    LintDiagnostic::new(
                        "security/string-max-length",
                        "security",
                        Severity::Warning,
                        format!(
                            "必填字段 {}.{} 的 String 建议指定长度 String(n)",
                            e.name, f.name
                        ),
                    )
                    .entity(&e.name)
                    .field(&f.name),
                );
            }
        }
    }
    out
}

// ── performance ──

fn lint_missing_index(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.entities {
        for f in &e.fields {
            if crate::dsl::is_relation(p, &f.type_name) {
                let col = crate::identity::to_snake_case(&f.name);
                let has = e.indexes.iter().any(|cols| cols.contains(&col))
                    || e.uniques.iter().any(|cols| cols.contains(&col))
                    || has_attr(f, "indexed");
                if !has {
                    out.push(
                        LintDiagnostic::new(
                            "performance/missing-index",
                            "performance",
                            Severity::Warning,
                            format!("关系字段 {}.{} 建议添加索引", e.name, f.name),
                        )
                        .entity(&e.name)
                        .field(&f.name)
                        .hint("添加到 `index [...]`"),
                    );
                }
            }
        }
    }
    out
}

fn lint_enum_not_indexed(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.entities {
        for f in &e.fields {
            if p.enums.iter().any(|x| x.name == f.type_name)
                && (f.name == "status" || f.name == "state")
                && !e.indexes.iter().any(|cols| cols.contains(&f.name))
            {
                out.push(
                    LintDiagnostic::new(
                        "performance/enum-not-indexed",
                        "performance",
                        Severity::Info,
                        format!("状态字段 {}.{} 建议添加索引", e.name, f.name),
                    )
                    .entity(&e.name)
                    .field(&f.name),
                );
            }
        }
    }
    out
}

// ── convention ──

fn lint_page_route_kebab(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for pg in &p.pages {
        for seg in pg
            .route
            .split('/')
            .filter(|s| !s.is_empty() && !s.starts_with('{'))
        {
            if seg.contains('_') || seg.chars().any(|c| c.is_ascii_uppercase()) {
                out.push(
                    LintDiagnostic::new(
                        "convention/page-route-kebab",
                        "convention",
                        Severity::Warning,
                        format!("路由 '{}' 建议使用 kebab-case", pg.route),
                    )
                    .entity(&pg.name),
                );
            }
        }
    }
    out
}

fn lint_api_name_plural(p: &Program) -> Vec<LintDiagnostic> {
    p.apis
        .iter()
        .filter(|a| !a.name.ends_with('s') && !a.name.ends_with("list"))
        .map(|a| {
            LintDiagnostic::new(
                "convention/api-name-plural",
                "convention",
                Severity::Info,
                format!("API '{}' 建议使用复数形式", a.name),
            )
            .entity(&a.name)
        })
        .collect()
}

// ── 注册表 ──

pub fn builtin_rules() -> Vec<LintRule> {
    vec![
        LintRule {
            id: "naming/entity-pascal-case",
            category: "naming",
            severity: Severity::Warning,
            check: lint_entity_pascal,
        },
        LintRule {
            id: "naming/field-camel-case",
            category: "naming",
            severity: Severity::Warning,
            check: lint_field_camel,
        },
        LintRule {
            id: "completeness/require-id",
            category: "completeness",
            severity: Severity::Error,
            check: lint_require_id,
        },
        LintRule {
            id: "completeness/require-timestamps",
            category: "completeness",
            severity: Severity::Warning,
            check: lint_require_timestamps,
        },
        LintRule {
            id: "completeness/i18n-labels",
            category: "completeness",
            severity: Severity::Info,
            check: lint_i18n_labels,
        },
        LintRule {
            id: "security/email-unique",
            category: "security",
            severity: Severity::Warning,
            check: lint_email_unique,
        },
        LintRule {
            id: "security/sensitive-hidden",
            category: "security",
            severity: Severity::Warning,
            check: lint_sensitive_hidden,
        },
        LintRule {
            id: "security/entity-has-access",
            category: "security",
            severity: Severity::Info,
            check: lint_entity_has_access,
        },
        LintRule {
            id: "security/string-max-length",
            category: "security",
            severity: Severity::Warning,
            check: lint_string_max_length,
        },
        LintRule {
            id: "performance/missing-index",
            category: "performance",
            severity: Severity::Warning,
            check: lint_missing_index,
        },
        LintRule {
            id: "performance/enum-not-indexed",
            category: "performance",
            severity: Severity::Info,
            check: lint_enum_not_indexed,
        },
        LintRule {
            id: "convention/page-route-kebab",
            category: "convention",
            severity: Severity::Warning,
            check: lint_page_route_kebab,
        },
        LintRule {
            id: "convention/api-name-plural",
            category: "convention",
            severity: Severity::Info,
            check: lint_api_name_plural,
        },
    ]
}

pub fn run_lint(
    program: &Program,
    config: Option<&std::collections::HashMap<String, Severity>>,
) -> Vec<LintDiagnostic> {
    let mut all = Vec::new();
    for rule in builtin_rules() {
        let sev = config
            .and_then(|c| c.get(rule.id))
            .copied()
            .unwrap_or(rule.severity);
        if sev == Severity::Off {
            continue;
        }
        let diags = (rule.check)(program);
        for mut d in diags {
            d.severity = sev.as_str().into();
            d.rule = rule.id.into();
            all.push(d);
        }
    }
    all.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then(a.rule.cmp(&b.rule))
            .then(a.entity.cmp(&b.entity))
    });
    all
}
