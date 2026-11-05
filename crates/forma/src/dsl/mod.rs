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

/// 内置类型词表（**单一权威清单**）——检查层与各生成层共用。
///
/// 新增类型**必须**同时：① 加进本清单；② 在各 emitter 里给出映射；
/// ③ 在 `docs/architecture/specifications/types/type-system.zh-CN.md` 记录。
/// 只改生成层而不改本清单 = 用户写了类型却被 `dsl-unknown-type` 拒绝（空承诺）。
pub const BUILTIN_TYPES: [&str; 11] = [
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
    "Json",
];

/// 可比较大小的类型（min / max 约束仅对这些类型有意义）。
pub const NUMERIC_TYPES: [&str; 3] = ["Int", "Float", "Decimal"];

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
            // 类型参数边界（P1-2）：String(n) / Decimal(p,s) 的取值必须合法，
            // 否则生成端会产出无法执行的目标（VARCHAR(0) / DECIMAL(2,5) / 精度越界）
            d.extend(check_type_args(
                &entity.name,
                &field.name,
                &field.type_name,
                &field.type_args,
                field.line,
            ));
            // 属性形式的 min/max（P2-5）：仅数值类型，且必须有且仅有 1 个数值参数
            for attr in &field.attrs {
                if matches!(attr.name.as_str(), "min" | "max") {
                    if !NUMERIC_TYPES.contains(&field.type_name.as_str()) {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-invalid-constraint".into(),
                            message: format!(
                                "field {}.{}: @{} applies to non-numeric type '{}' (allowed: {})",
                                entity.name,
                                field.name,
                                attr.name,
                                field.type_name,
                                NUMERIC_TYPES.join(", ")
                            ),
                            file: None,
                            line: Some(field.line),
                            nodes: None,
                        });
                    }
                    if attr.args.len() != 1 || attr.args[0].parse::<f64>().is_err() {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-invalid-constraint".into(),
                            message: format!(
                                "field {}.{}: @{} requires exactly 1 numeric argument",
                                entity.name, field.name, attr.name
                            ),
                            file: None,
                            line: Some(field.line),
                            nodes: None,
                        });
                    }
                }
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
                continue;
            }
            // min / max 只对数值类型有意义——否则生成端会产出无法编译的比较
            let field_type = entity
                .fields
                .iter()
                .find(|f| f.name == fr.field)
                .map(|f| f.type_name.clone())
                .unwrap_or_default();
            for c in &fr.constraints {
                let is_range = matches!(c, FieldConstraint::Min(..) | FieldConstraint::Max(..));
                if is_range && !NUMERIC_TYPES.contains(&field_type.as_str()) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-invalid-constraint".into(),
                        message: format!(
                            "field rule {}.{} applies min/max to non-numeric type '{}' (allowed: {})",
                            fr.entity,
                            fr.field,
                            field_type,
                            NUMERIC_TYPES.join(", ")
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
            }
        }
        for check in &rules.checks {
            d.extend(check_expr_paths(program, &check.entity, &check.expr, &[]));
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
                d.extend(check_expr_paths(program, &guard.entity, &deny.expr, &[]));
            }
        }
        // 实体级访问规则（access）：操作名 / 实体 / 角色 / auth module 必须自洽
        for access in &rules.access {
            if !entity_names.contains(&access.entity.as_str()) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-entity".into(),
                    message: format!("access rule references unknown entity '{}'", access.entity),
                    file: None,
                    line: None,
                    nodes: None,
                });
                continue;
            }
            for rule in &access.rules {
                if !ACCESS_OPS.contains(&rule.op.as_str()) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-access-unknown-operation".into(),
                        message: format!(
                            "access op '{}' must be one of {}",
                            rule.op,
                            ACCESS_OPS.join("|")
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
                // 角色必须在 auth module 的 roles 内
                let roles = access_rule_roles(&rule.expr);
                if !roles.is_empty() {
                    match &program.auth {
                        None => d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-access-no-provider".into(),
                            message: format!(
                                "access rule for '{}' references roles but no `auth module` is declared (permissions cannot be enforced)",
                                access.entity
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        }),
                        Some(auth) => {
                            for role in &roles {
                                if !auth.roles.iter().any(|r| r == role) {
                                    d.push(Diagnostic {
                                        severity: "error".into(),
                                        code: "dsl-access-unknown-role".into(),
                                        message: format!(
                                            "access rule for '{}' references unknown role '{}' (auth module roles: {})",
                                            access.entity,
                                            role,
                                            auth.roles.join(", ")
                                        ),
                                        file: None,
                                        line: None,
                                        nodes: None,
                                    });
                                }
                            }
                        }
                    }
                }
                // 表达式引用的字段必须存在
                d.extend(check_expr_paths(
                    program,
                    &access.entity,
                    &rule.expr,
                    ACCESS_VARS,
                ));
                // 时间 / 日期比较仅支持同类型字段对字段（不解析字符串时间字面量）。
                d.extend(check_access_temporal_comparisons(
                    program,
                    &access.entity,
                    &rule.expr,
                    &rule.op,
                ));
                d.extend(check_access_json_comparisons(
                    program,
                    &access.entity,
                    &rule.expr,
                    &rule.op,
                ));
                // 行级条件：只有**能取到实体行**的操作（get / update / delete）才可能成立
                let row_capable = matches!(rule.op.as_str(), "get" | "update" | "delete");
                let entity = program.entities.iter().find(|e| e.name == access.entity);
                for path in expr_refs(&rule.expr) {
                    if path.len() == 1 && ACCESS_VARS.contains(&path[0].as_str()) {
                        continue;
                    }
                    if !row_capable {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-access-row-unsupported".into(),
                            message: format!(
                                "access rule for '{}' op '{}' references field '{}' — operation '{}' has no entity row (row-level conditions apply to get/update/delete)",
                                access.entity, rule.op, path[0], rule.op
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                        continue;
                    }
                    if path.len() > 2 {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-access-relation-depth-unsupported".into(),
                            message: format!(
                                "access rule for '{}' op '{}' traverses more than one relation (`{}`); maximum supported path is relation.field",
                                access.entity, rule.op, path.join(".")
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                        continue;
                    }
                    let Some(root_field) =
                        entity.and_then(|e| e.fields.iter().find(|f| f.name == path[0]))
                    else {
                        continue; // check_expr_paths already reported the unknown field
                    };
                    let field = if path.len() == 1 {
                        Some(root_field)
                    } else {
                        let target = program
                            .entities
                            .iter()
                            .find(|e| e.name == root_field.type_name);
                        if target.is_none() {
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-access-relation-unsupported".into(),
                                message: format!(
                                    "access path '{}' traverses non-relation field '{}'",
                                    path.join("."),
                                    path[0]
                                ),
                                file: None,
                                line: None,
                                nodes: None,
                            });
                            continue;
                        }
                        if root_field.optional {
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-access-relation-optional-unsupported".into(),
                                message: format!("access path '{}' traverses optional relation '{}'; optional relation access is not supported", path.join("."), path[0]),
                                file: None,
                                line: None,
                                nodes: None,
                            });
                            continue;
                        }
                        target.and_then(|e| e.fields.iter().find(|f| f.name == path[1]))
                    };
                    let Some(field) = field else {
                        continue;
                    };
                    let type_name = field.type_name.clone();
                    if !access_comparable(program, &type_name) {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-access-field-type-unsupported".into(),
                            message: format!(
                                "access rule for '{}' op '{}' compares field '{}' of type '{}' — comparable types are string/enum/number/JSON or same-type Timestamp/Date fields",
                                access.entity, rule.op, path.join("."), type_name
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                }
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
        // 命令类操作：输入字段的类型参数校验（commands 其余语义检查待补）
        for cmd in &program.commands {
            for input in &cmd.input {
                d.extend(check_type_args(
                    &cmd.name,
                    &input.name,
                    &input.type_name,
                    &input.type_args,
                    0,
                ));
            }
        }

        // api 级授权规则：
        //  - 操作名必须在该 api 的 operations 内（否则规则永远不会生效）；
        //  - 角色必须在 auth module 的 roles 内（否则没有人能通过）；
        //  - 没有 auth module 时无法执行 → 报错，避免「声明了但不执行」的空承诺。
        if !api.auth.is_empty() {
            match &program.auth {
                None => d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-auth-no-provider".into(),
                    message: format!(
                        "api '{}' declares auth rules but no `auth module` is declared (permissions cannot be enforced)",
                        api.name
                    ),
                    file: None,
                    line: None,
                    nodes: None,
                }),
                Some(auth) => {
                    for rule in &api.auth {
                        for op in &rule.operations {
                            if !api.operations.iter().any(|o| o == op) {
                                d.push(Diagnostic {
                                    severity: "error".into(),
                                    code: "dsl-auth-unknown-operation".into(),
                                    message: format!(
                                        "api '{}' auth rule references unknown operation '{}' (operations: {})",
                                        api.name,
                                        op,
                                        api.operations.join(", ")
                                    ),
                                    file: None,
                                    line: None,
                                    nodes: None,
                                });
                            }
                        }
                        for role in &rule.roles {
                            if !auth.roles.iter().any(|r| r == role) {
                                d.push(Diagnostic {
                                    severity: "error".into(),
                                    code: "dsl-auth-unknown-role".into(),
                                    message: format!(
                                        "api '{}' auth rule references unknown role '{}' (auth module roles: {})",
                                        api.name,
                                        role,
                                        auth.roles.join(", ")
                                    ),
                                    file: None,
                                    line: None,
                                    nodes: None,
                                });
                            }
                        }
                    }
                }
            }
        }
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
            if let Some(mode) = &list.paginate {
                if !matches!(mode.as_str(), "offset" | "cursor") {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-invalid-pagination".into(),
                        message: format!(
                            "api '{}'.list paginate mode '{}' must be offset|cursor",
                            api.name, mode
                        ),
                        file: None,
                        line: None,
                        nodes: None,
                    });
                }
                if mode == "cursor" {
                    if list.sort.is_empty() {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-cursor-sort-required".into(),
                            message: format!(
                                "api '{}'.list paginate cursor requires at least one sort field (a unique id tie-breaker is appended automatically)",
                                api.name
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                    let mut seen_sort = std::collections::HashSet::new();
                    for (sort_name, _) in &list.sort {
                        if !seen_sort.insert(sort_name) {
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-cursor-sort-duplicate".into(),
                                message: format!(
                                    "api '{}'.list cursor sort field '{}' is duplicated",
                                    api.name, sort_name
                                ),
                                file: None,
                                line: None,
                                nodes: None,
                            });
                        }
                        if let Some(field) = entity.fields.iter().find(|f| f.name == *sort_name) {
                            if field.optional {
                                d.push(Diagnostic {
                                    severity: "error".into(),
                                    code: "dsl-cursor-sort-nullable".into(),
                                    message: format!(
                                        "api '{}'.list cursor sort field '{}' is nullable; keyset pagination requires non-null sort keys",
                                        api.name, field.name
                                    ),
                                    file: None,
                                    line: None,
                                    nodes: None,
                                });
                            }
                            let supported = matches!(
                                field.type_name.as_str(),
                                "Uuid"
                                    | "String"
                                    | "Text"
                                    | "Email"
                                    | "Int"
                                    | "Float"
                                    | "Decimal"
                                    | "Timestamp"
                                    | "Date"
                            ) || enum_names.contains(&field.type_name.as_str());
                            if !supported {
                                d.push(Diagnostic {
                                    severity: "error".into(),
                                    code: "dsl-cursor-sort-type-unsupported".into(),
                                    message: format!(
                                        "api '{}'.list cursor sort field '{}' has unsupported type '{}'",
                                        api.name, field.name, field.type_name
                                    ),
                                    file: None,
                                    line: None,
                                    nodes: None,
                                });
                            }
                        }
                    }
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
fn check_expr_paths(
    program: &Program,
    entity_name: &str,
    expr: &Expr,
    vars: &[&str],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(entity) = program.entities.iter().find(|e| e.name == entity_name) else {
        return out;
    };
    match expr {
        Expr::Ref { path, old } => {
            if path.is_empty() {
                return out;
            }
            // 绑定变量（如访问规则里的 `role` / `any`）不是实体字段
            if path.len() == 1 && vars.contains(&path[0].as_str()) {
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
            out.extend(check_expr_paths(program, entity_name, a, vars));
            out.extend(check_expr_paths(program, entity_name, b, vars));
        }
        Expr::Not(a) => out.extend(check_expr_paths(program, entity_name, a, vars)),
        Expr::Call(_, args) => {
            for a in args {
                out.extend(check_expr_paths(program, entity_name, a, vars));
            }
        }
        Expr::List(items) => {
            for a in items {
                out.extend(check_expr_paths(program, entity_name, a, vars));
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

/// 访问规则支持的操作名。
pub const ACCESS_OPS: &[&str] = &["read", "list", "get", "create", "update", "delete"];

/// 访问规则里的绑定变量（不是实体字段）。
pub const ACCESS_VARS: &[&str] = &["role", "any"];

/// 从访问规则表达式中提取被引用的**角色字面量**：
/// - `role in ["admin", "ops"]` → admin, ops
/// - `role == "admin"` → admin
pub fn access_rule_roles(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_access_roles(expr, &mut out);
    out
}

fn collect_access_roles(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Bin(BinOp::In, a, b) => {
            if is_role_ref(a) {
                if let Expr::List(items) = &**b {
                    for it in items {
                        if let Expr::Str(s) = it {
                            if !out.contains(s) {
                                out.push(s.clone());
                            }
                        }
                    }
                }
            }
            collect_access_roles(a, out);
            collect_access_roles(b, out);
        }
        Expr::Bin(BinOp::Eq, a, b) => {
            if is_role_ref(a) {
                if let Expr::Str(s) = &**b {
                    if !out.contains(s) {
                        out.push(s.clone());
                    }
                }
            }
            collect_access_roles(a, out);
            collect_access_roles(b, out);
        }
        Expr::Bin(_, a, b) => {
            collect_access_roles(a, out);
            collect_access_roles(b, out);
        }
        Expr::Not(a) => collect_access_roles(a, out),
        _ => {}
    }
}

fn is_role_ref(expr: &Expr) -> bool {
    matches!(expr, Expr::Ref { path, .. } if path.len() == 1 && path[0] == "role")
}

/// 收集表达式里引用的字段路径。
pub fn expr_refs(expr: &Expr) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    match expr {
        Expr::Ref { path, .. } => out.push(path.clone()),
        Expr::Bin(_, a, b) => {
            out.extend(expr_refs(a));
            out.extend(expr_refs(b));
        }
        Expr::Not(a) => out.extend(expr_refs(a)),
        Expr::Call(_, args) => {
            for a in args {
                out.extend(expr_refs(a));
            }
        }
        Expr::List(items) => {
            for i in items {
                out.extend(expr_refs(i));
            }
        }
        _ => {}
    }
    out
}

/// 访问条件中**可比较**的字段类型（字符串族 / 枚举 / 数值 / 时间 / 日期）。
fn access_comparable(program: &Program, type_name: &str) -> bool {
    if is_enum(program, type_name) {
        return true;
    }
    matches!(
        type_name,
        "String" | "Text" | "Email" | "Int" | "Float" | "Decimal" | "Timestamp" | "Date" | "Json"
    )
}

fn is_order_comparison(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

fn access_ref_type(program: &Program, entity: &EntityDecl, path: &[String]) -> Option<String> {
    if path.len() == 1 {
        return entity
            .fields
            .iter()
            .find(|f| f.name == path[0])
            .map(|f| f.type_name.clone());
    }
    if path.len() == 2 {
        let relation = entity.fields.iter().find(|f| f.name == path[0])?;
        let target = program
            .entities
            .iter()
            .find(|e| e.name == relation.type_name)?;
        return target
            .fields
            .iter()
            .find(|f| f.name == path[1])
            .map(|f| f.type_name.clone());
    }
    None
}

fn access_expr_type(program: &Program, entity: &EntityDecl, expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ref { path, .. } => access_ref_type(program, entity, path),
        Expr::Num(_) => Some("<number-literal>".into()),
        Expr::Str(_) => Some("<string-literal>".into()),
        Expr::Bool(_) => Some("Bool".into()),
        Expr::Null => Some("<null-literal>".into()),
        _ => None,
    }
}

/// Timestamp / Date 行级比较必须是同类型字段对字段；不支持隐式解析字符串/数值字面量。
fn check_access_temporal_comparisons(
    program: &Program,
    entity_name: &str,
    expr: &Expr,
    op_name: &str,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(entity) = program.entities.iter().find(|e| e.name == entity_name) else {
        return out;
    };
    fn walk(
        program: &Program,
        expr: &Expr,
        entity: &EntityDecl,
        entity_name: &str,
        op_name: &str,
        out: &mut Vec<Diagnostic>,
    ) {
        match expr {
            Expr::Bin(op, a, b) => {
                if is_order_comparison(*op) {
                    let left = access_expr_type(program, entity, a);
                    let right = access_expr_type(program, entity, b);
                    let temporal = |t: &str| matches!(t, "Timestamp" | "Date");
                    if let (Some(l), Some(r)) = (&left, &right) {
                        if (temporal(l) || temporal(r)) && l != r {
                            out.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-access-temporal-type-mismatch".into(),
                                message: format!(
                                    "access rule for '{}' op '{}' compares incompatible temporal types '{}' and '{}'; only same-type Timestamp/Date field comparisons are supported",
                                    entity_name, op_name, l, r
                                ),
                                file: None,
                                line: None,
                                nodes: None,
                            });
                        }
                    }
                }
                walk(program, a, entity, entity_name, op_name, out);
                walk(program, b, entity, entity_name, op_name, out);
            }
            Expr::Not(a) => walk(program, a, entity, entity_name, op_name, out),
            Expr::Call(_, args) => {
                for arg in args {
                    walk(program, arg, entity, entity_name, op_name, out);
                }
            }
            Expr::List(items) => {
                for item in items {
                    walk(program, item, entity, entity_name, op_name, out);
                }
            }
            _ => {}
        }
    }
    walk(program, expr, entity, entity_name, op_name, &mut out);
    out
}

/// Json 行级条件：仅允许与 Json 字段或 `null` 做相等/不等比较（serde_json::Value PartialEq）。
/// 字符串/数字字面量不隐式转换为 JSON，大小比较无 JSON 语义。
fn check_access_json_comparisons(
    program: &Program,
    entity_name: &str,
    expr: &Expr,
    op_name: &str,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let Some(entity) = program.entities.iter().find(|e| e.name == entity_name) else {
        return out;
    };
    fn walk(
        program: &Program,
        entity: &EntityDecl,
        entity_name: &str,
        op_name: &str,
        expr: &Expr,
        out: &mut Vec<Diagnostic>,
    ) {
        match expr {
            Expr::Bin(op, a, b) => {
                let lt = access_expr_type(program, entity, a);
                let rt = access_expr_type(program, entity, b);
                if lt.as_deref() == Some("Json") || rt.as_deref() == Some("Json") {
                    let allowed_op = matches!(op, BinOp::Eq | BinOp::Ne);
                    let counterpart_ok = match (&lt, &rt) {
                        (Some(l), Some(r)) if l == "Json" && r == "Json" => true,
                        (Some(l), Some(r)) if l == "Json" && r == "<null-literal>" => true,
                        (Some(l), Some(r)) if r == "Json" && l == "<null-literal>" => true,
                        _ => false,
                    };
                    if !allowed_op || !counterpart_ok {
                        out.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-access-json-comparison-unsupported".into(),
                            message: format!("access rule for '{}' op '{}' uses unsupported Json comparison; Json supports only ==/!= with another Json field or null", entity_name, op_name),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                }
                walk(program, entity, entity_name, op_name, a, out);
                walk(program, entity, entity_name, op_name, b, out);
            }
            Expr::Not(a) => walk(program, entity, entity_name, op_name, a, out),
            Expr::Call(_, args) => {
                for arg in args {
                    walk(program, entity, entity_name, op_name, arg, out);
                }
            }
            Expr::List(items) => {
                for item in items {
                    walk(program, entity, entity_name, op_name, item, out);
                }
            }
            _ => {}
        }
    }
    walk(program, entity, entity_name, op_name, expr, &mut out);
    out
}

/// 类型参数边界校验（P1-2）。
///
/// `String(n)`：0 < n ≤ 65535（VARCHAR 上限）；
/// `Decimal(p,s)`：恰好 2 个参数，0 < p ≤ 65（MySQL DECIMAL 上限），s ≤ p；
/// 其余类型**不接受**参数（`Int(5)` / `Bool(1)` 一律报错——生成端无从映射）。
pub fn check_type_args(
    owner: &str,
    field_name: &str,
    type_name: &str,
    type_args: &[String],
    line: usize,
) -> Vec<Diagnostic> {
    let mut d = Vec::new();
    let mut bad = |msg: String| {
        d.push(Diagnostic {
            severity: "error".into(),
            code: "dsl-invalid-type-arg".into(),
            message: format!("{}{}: {msg}", owner, {
                if field_name.is_empty() {
                    String::new()
                } else {
                    format!(".{field_name}")
                }
            }),
            file: None,
            line: Some(line),
            nodes: None,
        });
    };
    match type_name {
        "String" => {
            if type_args.len() > 1 {
                bad(format!(
                    "String takes at most 1 argument (length), got {}",
                    type_args.len()
                ));
                return d;
            }
            if let Some(arg) = type_args.first() {
                match arg.parse::<u64>() {
                    Err(_) => bad(format!("String length must be a number, got '{arg}'")),
                    Ok(0) => bad("String length must be > 0 (VARCHAR(0) is invalid)".to_string()),
                    Ok(n) if n > 65535 => bad(format!(
                        "String length must be ≤ 65535 (VARCHAR limit), got {n}"
                    )),
                    Ok(_) => {}
                }
            }
        }
        "Decimal" => {
            if type_args.len() != 2 {
                bad(format!(
                    "Decimal requires 2 arguments: Decimal(precision, scale), got {}",
                    type_args.len()
                ));
                return d;
            }
            let (Some(p), Some(sc)) = (
                type_args[0].parse::<u64>().ok(),
                type_args[1].parse::<u64>().ok(),
            ) else {
                bad("Decimal precision/scale must be numbers".to_string());
                return d;
            };
            if p == 0 {
                bad("Decimal precision must be > 0".to_string());
            } else if p > 65 {
                bad(format!(
                    "Decimal precision must be ≤ 65 (MySQL DECIMAL limit), got {p}"
                ));
            } else if sc > p {
                bad(format!("Decimal scale must be ≤ precision, got {p},{sc}"));
            }
        }
        other => {
            if !type_args.is_empty() {
                bad(format!("type '{other}' does not take arguments"));
            }
        }
    }
    d
}
