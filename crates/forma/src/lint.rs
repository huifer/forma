//! Forma Lint — DSL 质量检查器（类似 ESLint）。
//!
//! 内置规则分类：
//! - naming:       命名规范（PascalCase/camelCase/snake_case）
//! - completeness: 完备性（@id/时间戳/i18n/索引）
//! - security:     安全（@unique/@hidden/密码字段/权限声明）
//! - performance:  性能（索引/避免 N+1）
//! - convention:   约定（路由风格/API 命名/关系命名）
//!
//! 用法：`forma lint [dir]`，输出与 Diagnostic 兼容。

use crate::dsl::{EntityDecl, FieldDecl, PageBody, Program};
use crate::validate::Diagnostic;
use serde::{Deserialize, Serialize};

/// 规则严重级
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
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
            Severity::Off => "off",
        }
    }
}

/// 单条 lint 规则的配置（用于 .forma-lint.toml 覆盖默认级别）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintRuleConfig {
    pub severity: Severity,
}

/// lint 规则上下文——一条规则检查整个 Program 并返回诊断
pub type LintFn = fn(&Program) -> Vec<LintDiagnostic>;

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
    pub fn new(rule: &str, category: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            severity: severity.as_str().into(),
            category: category.into(),
            message: message.into(),
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

    pub fn format(&self) -> String {
        let mut s = format!("[{: >7}] {: <40} {}", self.severity, self.rule, self.message);
        if let Some(e) = &self.entity {
            s.push_str(&format!("  (entity: {e})"));
        }
        if let Some(f) = &self.field {
            s.push_str(&format!("  (field: {f})"));
        }
        s
    }
}

/// 内置规则注册表——(id, category, 默认严重级, 检查函数)
pub fn builtin_rules() -> Vec<LintRule> {
    vec![
        // ── naming ──
        LintRule { id: "naming/entity-pascal-case", category: "naming", default_severity: Severity::Warning, check: lint_entity_pascal_case },
        LintRule { id: "naming/field-camel-case", category: "naming", default_severity: Severity::Warning, check: lint_field_camel_case },
        // ── completeness ──
        LintRule { id: "completeness/require-id", category: "completeness", default_severity: Severity::Error, check: lint_require_id },
        LintRule { id: "completeness/require-timestamps", category: "completeness", default_severity: Severity::Warning, check: lint_require_timestamps },
        LintRule { id: "completeness/i18n-labels", category: "completeness", default_severity: Severity::Info, check: lint_i18n_labels },
        LintRule { id: "completeness/no-empty-description", category: "completeness", default_severity: Severity::Info, check: lint_no_empty_description },
        // ── security ──
        LintRule { id: "security/email-unique", category: "security", default_severity: Severity::Warning, check: lint_email_unique },
        LintRule { id: "security/PASSWORD-hidden", category: "security", default_severity: Severity::Warning, check: lint_PASSWORD_hidden },
        LintRule { id: "security/entity-has-access", category: "security", default_severity: Severity::Info, check: lint_entity_has_access },
        LintRule { id: "security/string-max-length", category: "security", default_severity: Severity::Warning, check: lint_string_max_length },
        // ── performance ──
        LintRule { id: "performance/missing-index", category: "performance", default_severity: Severity::Warning, check: lint_missing_index },
        LintRule { id: "performance/enum-not-indexed", category: "performance", default_severity: Severity::Info, check: lint_enum_not_indexed },
        // ── convention ──
        LintRule { id: "convention/relation-naming", category: "convention", default_severity: Severity::Info, check: lint_relation_naming },
        LintRule { id: "convention/page-route-kebab", category: "convention", default_severity: Severity::Warning, check: lint_page_route_kebab },
        LintRule { id: "convention/api-name-plural", category: "convention", default_severity: Severity::Info, check: lint_api_name_plural },
    ]
}

pub struct LintRule {
    pub id: &'static str,
    pub category: &'static str,
    pub default_severity: Severity,
    pub check: LintFn,
}

/// 运行全部 lint 规则
pub fn run_lint(program: &Program, config: Option<&std::collections::HashMap<String, Severity>>) -> Vec<LintDiagnostic> {
    let mut all = Vec::new();
    for rule in builtin_rules() {
        let severity = config
            .and_then(|c| c.get(rule.id))
            .copied()
            .unwrap_or(rule.default_severity);
        if severity == Severity::Off {
            continue;
        }
        let diags = (rule.check)(program);
        for mut d in diags {
            d.severity = severity.as_str().into();
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

// ── 辅助 ────────────────────────────────────────────────

fn is_pascal_case(s: &str) -> bool {
    s.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric())
        && !s.contains('_')
        && !s.contains('-')
}

fn is_camel_case(s: &str) -> bool {
    s.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric())
        && !s.contains('_')
        && !s.contains('-')
}

fn is_snake_case(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.starts_with('_')
        && !s.ends_with('_')
        && !s.contains("__")
}

// ── naming ──────────────────────────────────────────────

fn lint_entity_pascal_case(p: &Program) -> Vec<LintDiagnostic> {
    p.entities
        .iter()
        .filter(|e| !is_pascal_case(&e.name))
        .map(|e| {
            LintDiagnostic::new(
                "naming/entity-pascal-case", "naming", Severity::Warning,
                format!("实体名 '{}' 应使用 PascalCase", e.name),
            )
            .entity(&e.name)
            .hint(&format!("改为 '{}'", to_pascal(&e.name)))
        })
        .collect()
}

fn lint_field_camel_case(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if !is_camel_case(&field.name) {
                out.push(
                    LintDiagnostic::new(
                        "naming/field-camel-case", "naming", Severity::Warning,
                        format!("字段名 '{}' 应使用 camelCase", field.name),
                    )
                    .entity(&entity.name)
                    .field(&field.name)
                    .line(field.line)
                    .hint(&format!("改为 '{}'", to_camel(&field.name))),
                );
            }
        }
    }
    out
}

fn to_pascal(s: &str) -> String {
    s.split('_').map(|w| {
        let mut c = w.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }).collect()
}

fn to_camel(s: &str) -> String {
    let pascal = to_pascal(s);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ── completeness ────────────────────────────────────────

fn lint_require_id(p: &Program) -> Vec<LintDiagnostic> {
    p.entities
        .iter()
        .filter(|e| !e.fields.iter().any(|f| has_attr(f, "id")))
        .map(|e| {
            LintDiagnostic::new(
                "completeness/require-id", "completeness", Severity::Error,
                format!("实体 '{}' 缺少 @id 主键字段", e.name),
            )
            .entity(&e.name)
            .hint("添加 `id Uuid @id @default(uuid)`")
        })
        .collect()
}

fn lint_require_timestamps(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        let has_created = entity.fields.iter().any(|f| f.name == "createdAt" || f.name == "created_at");
        if !has_created {
            out.push(
                LintDiagnostic::new(
                    "completeness/require-timestamps", "completeness", Severity::Warning,
                    format!("实体 '{}' 缺少 createdAt 时间戳字段", entity.name),
                )
                .entity(&entity.name)
                .hint("添加 `createdAt Timestamp @default(now) @readonly`"),
            );
        }
    }
    out
}

fn lint_i18n_labels(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for e in &p.enums {
        for v in &e.variants {
            if v.labels.is_empty() {
                out.push(
                    LintDiagnostic::new(
                        "completeness/i18n-labels", "completeness", Severity::Info,
                        format!("枚举 {} 的变体 '{}' 缺少 @label", e.name, v.name),
                    )
                    .entity(&e.name)
                    .field(&v.name)
                    .hint("添加 `@label { zh \"…\" en \"…\" }`"),
                );
            }
        }
    }
    for page in &p.pages {
        if page.title.is_empty() {
            out.push(
                LintDiagnostic::new(
                    "completeness/i18n-labels", "completeness", Severity::Warning,
                    format!("页面 '{}' 缺少 title", page.name),
                )
                .entity(&page.name),
            );
        }
    }
    out
}

fn lint_no_empty_description(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if field.type_name == "Text" || field.type_name == "String" {
                let has_description = field.attrs.iter().any(|a| a.name == "description");
                if !has_description {
                    out.push(
                        LintDiagnostic::new(
                            "completeness/no-empty-description", "completeness", Severity::Info,
                            format!("字段 {}.{} 缺少 description", entity.name, field.name),
                        )
                        .entity(&entity.name)
                        .field(&field.name),
                    );
                }
            }
        }
    }
    out
}

// ── security ────────────────────────────────────────────

fn lint_email_unique(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if field.type_name == "Email" && !has_attr(field, "unique") {
                out.push(
                    LintDiagnostic::new(
                        "security/email-unique", "security", Severity::Warning,
                        format!("Email 字段 {}.{} 建议添加 @unique", entity.name, field.name),
                    )
                    .entity(&entity.name)
                    .field(&field.name),
                );
            }
        }
    }
    out
}

fn lint_PASSWORD_hidden(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            let lower = field.name.to_lowercase();
            let sensitive = lower.contains("PASSWORD") || lower.contains("secret") || lower.contains("token");
            if sensitive && !has_attr(field, "hidden") {
                out.push(
                    LintDiagnostic::new(
                        "security/PASSWORD-hidden", "security", Severity::Warning,
                        format!("敏感字段 {}.{} 建议添加 @hidden", entity.name, field.name),
                    )
                    .entity(&entity.name)
                    .field(&field.name)
                    .hint("添加 `@hidden` 防止序列化泄露"),
                );
            }
        }
    }
    out
}

fn lint_entity_has_access(p: &Program) -> Vec<LintDiagnostic> {
    // 检查实体是否被 access/guard 规则覆盖（信息级）
    let mut covered_entities = std::collections::HashSet::new();
    for rules in &p.rules {
        for guard in &rules.guards {
            covered_entities.insert(guard.entity.clone());
        }
    }
    // MVP：只检查有 Email 字段的实体（通常是核心业务实体）
    let mut out = Vec::new();
    for entity in &p.entities {
        if entity.fields.iter().any(|f| f.type_name == "Email")
            && !covered_entities.contains(&entity.name)
        {
            out.push(
                LintDiagnostic::new(
                    "security/entity-has-access", "security", Severity::Info,
                    format!("实体 '{}' 包含敏感字段但没有 access/guard 规则", entity.name),
                )
                .entity(&entity.name)
                .hint("添加 `rules` 块的 `guard` 或 `access` 声明"),
            );
        }
    }
    out
}

fn lint_string_max_length(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if field.type_name == "String" && field.type_args.is_empty() && !field.optional {
                out.push(
                    LintDiagnostic::new(
                        "security/string-max-length", "security", Severity::Warning,
                        format!("必填字段 {}.{} 的 String 建议指定长度限制 String(n)", entity.name, field.name),
                    )
                    .entity(&entity.name)
                    .field(&field.name),
                );
            }
        }
    }
    out
}

// ── performance ─────────────────────────────────────────

fn lint_missing_index(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if crate::dsl::is_relation(p, &field.type_name) {
                // 关系字段应有索引
                let col = crate::identity::to_snake_case(&field.name);
                let has_index = entity.indexes.iter().any(|cols| cols.contains(&col))
                    || entity.uniques.iter().any(|cols| cols.contains(&col))
                    || has_attr(field, "indexed");
                if !has_index {
                    out.push(
                        LintDiagnostic::new(
                            "performance/missing-index", "performance", Severity::Warning,
                            format!("关系字段 {}.{} 建议添加索引", entity.name, field.name),
                        )
                        .entity(&entity.name)
                        .field(&field.name)
                        .hint("添加到 `index [...]` 或使用 `@indexed`"),
                    );
                }
            }
        }
    }
    out
}

fn lint_enum_not_indexed(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if p.enums.iter().any(|e| e.name == field.type_name)
                && entity.fields.iter().any(|f| f.name == "status" || f.name == "state")
            {
                let has_index = entity.indexes.iter().any(|cols| cols.contains(&field.name));
                if !has_index {
                    out.push(
                        LintDiagnostic::new(
                            "performance/enum-not-indexed", "performance", Severity::Info,
                            format!("状态字段 {}.{} 建议添加索引以加速过滤查询", entity.name, field.name),
                        )
                        .entity(&entity.name)
                        .field(&field.name),
                    );
                }
            }
        }
    }
    out
}

// ── convention ──────────────────────────────────────────

fn lint_relation_naming(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for entity in &p.entities {
        for field in &entity.fields {
            if crate::dsl::is_relation(p, &field.type_name) {
                // 关系字段名应为小写实体名或带语义前缀
                let lower = field.type_name.to_lowercase();
                if field.name.to_lowercase() != lower
                    && !field.name.to_lowercase().starts_with(&lower)
                    && !field.name.ends_with("Id")
                {
                    out.push(
                        LintDiagnostic::new(
                            "convention/relation-naming", "convention", Severity::Info,
                            format!("关系字段 {}.{} 建议与目标实体名 '{}{}' 保持一致",
                                entity.name, field.name, lower, "…"),
                        )
                        .entity(&entity.name)
                        .field(&field.name),
                    );
                }
            }
        }
    }
    out
}

fn lint_page_route_kebab(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for page in &p.pages {
        let segs: Vec<&str> = page.route.split('/').filter(|s| !s.is_empty() && !s.starts_with('{')).collect();
        for seg in segs {
            if seg.contains('_') || seg.chars().any(|c| c.is_ascii_uppercase()) {
                out.push(
                    LintDiagnostic::new(
                        "convention/page-route-kebab", "convention", Severity::Warning,
                        format!("路由 '{}' 建议使用 kebab-case（如 '/{}'）", page.route, seg.to_lowercase().replace('_', "-")),
                    )
                    .entity(&page.name),
                );
            }
        }
    }
    out
}

fn lint_api_name_plural(p: &Program) -> Vec<LintDiagnostic> {
    let mut out = Vec::new();
    for api in &p.apis {
        let last = api.name.chars().last().unwrap_or(' ');
        if last != 's' && last != 'y' && !api.name.ends_with("list") {
            // 检查是否以 s 结尾——不是则提示
            if !api.name.ends_with('s') {
                out.push(
                    LintDiagnostic::new(
                        "convention/api-name-plural", "convention", Severity::Info,
                        format!("API '{}' 建议使用复数形式（如 '{}s'）", api.name, api.name),
                    )
                    .entity(&api.name),
                );
            }
        }
    }
    out
}

// ── helpers ─────────────────────────────────────────────

fn has_attr(field: &FieldDecl, name: &str) -> bool {
    field.attrs.iter().any(|a| a.name == name)
}

