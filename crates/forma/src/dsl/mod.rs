//! Forma DSL：.fm 源码 → Program（类型化 IR）+ 语义检查。
//! 这是 v0.3 编译器的内核前端；各适配层 emitter 消费 `Program`。

pub mod lexer;
pub mod parser;

pub use parser::{
    ApiDecl, Attr, BinOp, ChannelDecl, ChartDecl, ChartPageCfg, CheckRule, CommandDecl,
    CommandInput, DetailCfg, Effect, EffectValue, EncodingDecl, EntityDecl, EnumDecl, Expr,
    FieldConstraint, FieldDecl, FieldRule, FormCfg, FormFieldCfg, GuardDecl, ListCfg, Message,
    PageBody, PageDecl, Program, TableCfg, TransformDecl,
};

use crate::validate::Diagnostic;
use std::path::Path;

pub const BUILTIN_TYPES: [&str; 10] = [
    "Uuid",
    "String",
    "Email",
    "Text",
    "Int",
    "Float",
    "Decimal",
    "Bool",
    "Timestamp",
    "Date",
];

// ── 加载 ────────────────────────────────────────────────

/// 收集 `source` glob 下的全部 .fm 并解析合并为一个 Program。
pub fn load_program(
    project_dir: &Path,
    patterns: &[String],
) -> Result<(Program, Vec<Diagnostic>), anyhow::Error> {
    let mut files = Vec::new();
    for pattern in patterns {
        files.extend(glob_files(project_dir, pattern));
    }
    files.sort();
    files.dedup();

    let mut program = Program {
        app: None,
        enums: Vec::new(),
        entities: Vec::new(),
        rules: Vec::new(),
        apis: Vec::new(),
        commands: Vec::new(),
        charts: Vec::new(),
        auth: None,
        pages: Vec::new(),
    };
    let mut diagnostics = Vec::new();

    if files.is_empty() {
        diagnostics.push(Diagnostic {
            severity: "error".into(),
            code: "dsl-no-source".into(),
            message: "no .fm source files matched `source` globs".into(),
            file: None,
            line: None,
            nodes: None,
        });
        return Ok((program, diagnostics));
    }

    for file in &files {
        let rel = crate::util::rel_path(project_dir, file);
        let src = crate::util::read_text(file)?;
        let tokens = lexer::lex(&src);
        let outcome = parser::parse(&tokens);
        for e in &outcome.errors {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-parse".into(),
                message: e.message.clone(),
                file: Some(rel.clone()),
                line: Some(e.line),
                nodes: None,
            });
        }
        merge_program(&mut program, outcome.program, &rel, &mut diagnostics);
    }
    Ok((program, diagnostics))
}

fn merge_program(
    program: &mut Program,
    part: Program,
    file: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if part.app.is_some() {
        if program.app.is_some() {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate-app".into(),
                message: "`app` declared more than once".into(),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.app = part.app;
        }
    }
    for e in part.enums {
        if program.enums.iter().any(|x| x.name == e.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate enum '{}'", e.name),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.enums.push(e);
        }
    }
    for e in part.entities {
        if program.entities.iter().any(|x| x.name == e.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate entity '{}'", e.name),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.entities.push(e);
        }
    }
    program.rules.extend(part.rules);
    // 认证模块（单例）
    if let Some(auth) = part.auth {
        if program.auth.is_some() {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: "duplicate auth module".into(),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.auth = Some(auth);
        }
    }
    for c in part.commands {
        if program.commands.iter().any(|x| x.name == c.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate api command '{}'", c.name),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.commands.push(c);
        }
    }
    for a in part.apis {
        if program.apis.iter().any(|x| x.name == a.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate api '{}'", a.name),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.apis.push(a);
        }
    }
    for p in part.pages {
        if program.pages.iter().any(|x| x.name == p.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate page '{}'", p.name),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.pages.push(p);
        }
    }
    for c in part.charts {
        if program.charts.iter().any(|x| x.name == c.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "viz-duplicate".into(),
                message: format!("duplicate chart '{}'", c.name),
                file: Some(file.into()),
                line: None,
                nodes: None,
            });
        } else {
            program.charts.push(c);
        }
    }
}

/// 极简 glob：支持 `dir/*.fm`、`dir/**/*.fm`、`*.fm`、精确相对路径。
pub fn glob_files(project_dir: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
    let pattern = pattern.trim_start_matches("./");
    let abs = project_dir.join(pattern);
    if !pattern.contains('*') {
        if abs.is_file() {
            return vec![abs];
        }
        return Vec::new();
    }
    // 取 pattern 中第一个含通配符之前的目录作为扫描根
    let mut root = project_dir.to_path_buf();
    let segs: Vec<&str> = pattern.split('/').collect();
    for seg in &segs {
        if seg.contains('*') {
            break;
        }
        root = root.join(seg);
    }
    let recursive = pattern.contains("**");
    let mut result = Vec::new();
    collect_fm(&root, recursive, &mut result);
    result.sort();
    result
}

fn collect_fm(dir: &Path, recursive: bool, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive
                && path
                    .file_name()
                    .map(|n| n != "node_modules" && n != "target")
                    .unwrap_or(false)
            {
                collect_fm(&path, recursive, out);
            }
        } else if path.extension().map(|e| e == "fm").unwrap_or(false) {
            out.push(path);
        }
    }
}

// ── 语义检查 ────────────────────────────────────────────

pub fn check_program(program: &Program) -> Vec<Diagnostic> {
    let mut d = Vec::new();
    let enum_names: Vec<&str> = program.enums.iter().map(|e| e.name.as_str()).collect();
    let entity_names: Vec<&str> = program.entities.iter().map(|e| e.name.as_str()).collect();

    // 实体字段类型
    for entity in &program.entities {
        for field in &entity.fields {
            let known = BUILTIN_TYPES.contains(&field.type_name.as_str())
                || enum_names.contains(&field.type_name.as_str())
                || entity_names.contains(&field.type_name.as_str());
            if !known {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-type".into(),
                    message: format!(
                        "field {}.{} has unknown type '{}' (expected builtin, enum or entity)",
                        entity.name, field.name, field.type_name
                    ),
                    file: None,
                    line: Some(field.line),
                    nodes: None,
                });
            }
            // 实体引用目标必须有 @id
            if entity_names.contains(&field.type_name.as_str()) {
                let target = program.entities.iter().find(|e| e.name == field.type_name);
                if let Some(target) = target {
                    if !target
                        .fields
                        .iter()
                        .any(|f| f.attrs.iter().any(|a| a.name == "id"))
                    {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-relation-target".into(),
                            message: format!(
                                "relation {}.{} → '{}' has no @id primary key",
                                entity.name, field.name, field.type_name
                            ),
                            file: None,
                            line: Some(field.line),
                            nodes: None,
                        });
                    }
                }
            }
            // @readonly 必须有 @default
            if field.attrs.iter().any(|a| a.name == "readonly")
                && !field.attrs.iter().any(|a| a.name == "default")
            {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-readonly-default".into(),
                    message: format!(
                        "field {}.{} is @readonly but has no @default",
                        entity.name, field.name
                    ),
                    file: None,
                    line: Some(field.line),
                    nodes: None,
                });
            }
        }
        // 字段重名
        for (i, f) in entity.fields.iter().enumerate() {
            if entity.fields[..i].iter().any(|x| x.name == f.name) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-duplicate".into(),
                    message: format!("duplicate field {}.{}", entity.name, f.name),
                    file: None,
                    line: Some(f.line),
                    nodes: None,
                });
            }
        }
        // index/unique 列存在
        for cols in entity.indexes.iter().chain(entity.uniques.iter()) {
            for c in cols {
                if !entity.fields.iter().any(|f| &f.name == c) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-column".into(),
                        message: format!(
                            "index/unique references unknown field {}.{}",
                            entity.name, c
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
        }
    }

    // 规则
    for rules in &program.rules {
        for fr in &rules.field_rules {
            let Some(entity) = program.entities.iter().find(|e| e.name == fr.entity) else {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-entity".into(),
                    message: format!("field rule references unknown entity '{}'", fr.entity),
                    file: None,
                    line: None,
                    nodes: None,
                });
                continue;
            };
            if !entity.fields.iter().any(|f| f.name == fr.field) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-field".into(),
                    message: format!(
                        "field rule references unknown field {}.{}",
                        fr.entity, fr.field
                    ),
                    file: None,
                    line: None,
                    nodes: None,
                });
            }
        }
        for check in &rules.checks {
            d.extend(check_expr_paths(program, &check.entity, &check.expr));
        }
        for guard in &rules.guards {
            if !["create", "update", "delete"].contains(&guard.op.as_str()) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-guard-op".into(),
                    message: format!("guard op '{}' must be create|update|delete", guard.op),
                    file: None,
                    line: None,
                    nodes: None,
                });
            }
            if !entity_names.contains(&guard.entity.as_str()) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-entity".into(),
                    message: format!("guard references unknown entity '{}'", guard.entity),
                    file: None,
                    line: None,
                    nodes: None,
                });
                continue;
            }
            for deny in &guard.denies {
                d.extend(check_expr_paths(program, &guard.entity, &deny.expr));
            }
        }
    }

    // api
    for api in &program.apis {
        if !entity_names.contains(&api.entity.as_str()) {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-unknown-entity".into(),
                message: format!(
                    "api '{}' references unknown entity '{}'",
                    api.name, api.entity
                ),
                file: None,
                line: None,
                nodes: None,
            });
            continue;
        }
        let entity = program
            .entities
            .iter()
            .find(|e| e.name == api.entity)
            .unwrap();
        if let Some(list) = &api.list {
            for (f, _) in &list.filters {
                let root = f.split('.').next().unwrap_or(f);
                if !entity.fields.iter().any(|x| x.name == root) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-field".into(),
                        message: format!(
                            "api '{}'.list filter '{}' not on entity '{}'",
                            api.name, f, api.entity
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
            for s in &list.search {
                if !entity.fields.iter().any(|x| &x.name == s) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-field".into(),
                        message: format!(
                            "api '{}'.list search '{}' not on entity '{}'",
                            api.name, s, api.entity
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
            for (s, _) in &list.sort {
                if !entity.fields.iter().any(|x| &x.name == s) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-field".into(),
                        message: format!(
                            "api '{}'.sort '{}' not on entity '{}'",
                            api.name, s, api.entity
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
        }
    }

    // page
    for page in &program.pages {
        let api_names: Vec<&str> = program.apis.iter().map(|a| a.name.as_str()).collect();
        let rule_names: Vec<&str> = program.rules.iter().map(|r| r.name.as_str()).collect();
        match &page.body {
            PageBody::Table(t) => {
                if !api_names.contains(&t.query_api.as_str()) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-api".into(),
                        message: format!(
                            "page '{}'.table query '{}' is not a known api",
                            page.name, t.query_api
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
            PageBody::Form(f) => {
                if !entity_names.contains(&f.entity.as_str()) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-entity".into(),
                        message: format!(
                            "page '{}'.form references unknown entity '{}'",
                            page.name, f.entity
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                    continue;
                }
                let entity = program
                    .entities
                    .iter()
                    .find(|e| e.name == f.entity)
                    .unwrap();
                for ff in &f.fields {
                    let root = ff.path.split('.').next().unwrap_or(&ff.path);
                    if !entity.fields.iter().any(|x| x.name == root) {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-unknown-field".into(),
                            message: format!(
                                "page '{}'.form field '{}' not on entity '{}'",
                                page.name, ff.path, f.entity
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                    if let Some(from) = &ff.from {
                        if !entity_names.contains(&from.as_str())
                            && !enum_names.contains(&from.as_str())
                        {
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-unknown-source".into(),
                                message: format!(
                                    "page '{}'.form select 'from {}' is not an entity or enum",
                                    page.name, from
                                ),
                                file: None,
                                line: None,
                                nodes: None,
                            });
                        }
                    }
                }
                for v in &f.validation {
                    if !rule_names.contains(&v.as_str()) {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-unknown-rules".into(),
                            message: format!(
                                "page '{}'.validation '{}' is not a known rules block",
                                page.name, v
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                }
                for s in &f.submits {
                    let Some((api_name, op)) = s.api_op.split_once('.') else {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-unknown-api".into(),
                            message: format!(
                                "page '{}'.submit '{}' must be Api.operation",
                                page.name, s.api_op
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                        continue;
                    };
                    let Some(api) = program.apis.iter().find(|a| a.name == api_name) else {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-unknown-api".into(),
                            message: format!(
                                "page '{}'.submit references unknown api '{}'",
                                page.name, api_name
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                        continue;
                    };
                    if !api.operations.contains(&op.to_string()) {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-unknown-operation".into(),
                            message: format!(
                                "page '{}'.submit references '{}.{}' which is not in operations",
                                page.name, api_name, op
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                }
            }
            PageBody::Detail(det) => {
                if !entity_names.contains(&det.entity.as_str()) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-unknown-entity".into(),
                        message: format!(
                            "page '{}'.detail references unknown entity '{}'",
                            page.name, det.entity
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
            PageBody::Chart(c) => {
                if !program.charts.iter().any(|ch| ch.name == c.chart) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "viz-unknown-chart".into(),
                        message: format!(
                            "page '{}' references unknown chart '{}'",
                            page.name, c.chart
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
        }
    }

    // 可视化图表（RFC-0001）
    for chart in &program.charts {
        if let Err(errs) = crate::visualization::lower_chart(program, chart) {
            d.extend(errs);
        }
    }

    d
}

/// 校验表达式中引用的字段路径（首段必须是实体的字段；二段允许跨关系）。
fn check_expr_paths(program: &Program, entity_name: &str, expr: &Expr) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(entity) = program.entities.iter().find(|e| e.name == entity_name) else {
        return out;
    };
    match expr {
        Expr::Ref { path, old } => {
            if path.is_empty() {
                return out;
            }
            let Some(field) = entity.fields.iter().find(|f| f.name == path[0]) else {
                out.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-field".into(),
                    message: format!("rule references unknown field {}.{}", entity.name, path[0]),
                    file: None,
                    line: None,
                    nodes: None,
                });
                return out;
            };
            if path.len() > 1 {
                // 跨关系：path[0] 必须是关系字段
                let is_relation = program.entities.iter().any(|e| e.name == field.type_name);
                if !is_relation {
                    out.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-path".into(),
                        message: format!(
                            "'{}.{}' is not a relation, cannot traverse '{}'",
                            entity.name, path[0], path[1]
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                } else if let Some(target) =
                    program.entities.iter().find(|e| e.name == field.type_name)
                {
                    if !target.fields.iter().any(|f| f.name == path[1]) {
                        out.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-unknown-field".into(),
                            message: format!("'{}' has no field '{}'", field.type_name, path[1]),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                }
            }
            let _ = old;
        }
        Expr::Bin(_, a, b) => {
            out.extend(check_expr_paths(program, entity_name, a));
            out.extend(check_expr_paths(program, entity_name, b));
        }
        Expr::Not(a) => out.extend(check_expr_paths(program, entity_name, a)),
        Expr::Call(_, args) => {
            for a in args {
                out.extend(check_expr_paths(program, entity_name, a));
            }
        }
        Expr::List(items) => {
            for a in items {
                out.extend(check_expr_paths(program, entity_name, a));
            }
        }
        _ => {}
    }
    out
}

// ── 公共辅助（emitter 共用） ──────────────────────────────

pub fn find_entity<'a>(program: &'a Program, name: &str) -> Option<&'a EntityDecl> {
    program.entities.iter().find(|e| e.name == name)
}

pub fn is_relation(program: &Program, type_name: &str) -> bool {
    program.entities.iter().any(|e| e.name == type_name)
}

pub fn is_enum(program: &Program, type_name: &str) -> bool {
    program.enums.iter().any(|e| e.name == type_name)
}

/// 消息选择：zh → en → 第一项 → 兜底。
pub fn pick_message(msg: &Message, fallback: &str) -> String {
    if let Some((_, t)) = msg.iter().find(|(l, _)| l == "zh") {
        return t.clone();
    }
    if let Some((_, t)) = msg.iter().find(|(l, _)| l == "en") {
        return t.clone();
    }
    msg.first()
        .map(|(_, t)| t.clone())
        .unwrap_or_else(|| fallback.to_string())
}

pub fn field_attr<'a>(field: &'a FieldDecl, name: &str) -> Option<&'a Attr> {
    field.attrs.iter().find(|a| a.name == name)
}

pub fn has_attr(field: &FieldDecl, name: &str) -> bool {
    field.attrs.iter().any(|a| a.name == name)
}

pub fn pluralize(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with("s") && !lower.ends_with("ss") {
        name.to_string()
    } else if lower.ends_with('y')
        && lower.len() > 1
        && !['a', 'e', 'i', 'o', 'u'].contains(&lower.chars().nth(lower.len() - 2).unwrap())
    {
        format!("{}ies", &name[..name.len() - 1])
    } else if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|s| lower.ends_with(s))
    {
        format!("{name}es")
    } else {
        format!("{name}s")
    }
}
