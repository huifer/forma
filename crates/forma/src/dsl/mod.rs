//! Forma DSL：.fm 源码 → Program（类型化 IR）+ 语义检查。
//! 这是 v0.3 编译器的内核前端；各适配层 emitter 消费 `Program`。

pub mod lexer;
/// DSL `Program` → 联邦语义图桥接（M3 / spec:ir §15）。
pub mod lower;
pub mod parser;
/// DSL 主线补丁事务（RFC-0006 P2）。
pub mod patch;

pub use parser::{
    AccessDecl, ApiDecl, Attr, BinOp, ChannelDecl, ChartDecl, ChartPageCfg, CheckRule, CommandDecl,
    CommandInput, DetailCfg, Effect, EffectValue, EncodingDecl, EntityDecl, EnumDecl, Expr,
    FieldAccess, FieldConstraint, FieldDecl, FieldRule, FormCfg, FormFieldCfg, GuardDecl, ListCfg,
    Message, PageBody, PageDecl, Program, TableCfg, TransformDecl,
};

use crate::validate::Diagnostic;
use std::path::Path;

/// 内置类型词表（**单一权威清单**）——检查层与各生成层共用。
///
/// 新增类型**必须**同时：① 加进本清单；② 在各 emitter 里给出映射；
/// ③ 在 `docs/architecture/specifications/types/type-system.zh-CN.md` 记录。
/// 只改生成层而不改本清单 = 用户写了类型却被 `dsl-unknown-type` 拒绝（空承诺）。
pub const BUILTIN_TYPES: [&str; 12] = [
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
    "Attachment",
];

/// 可比较大小的类型（min / max 约束仅对这些类型有意义）。
pub const NUMERIC_TYPES: [&str; 3] = ["Int", "Float", "Decimal"];

/// 诊断来源位置（M1）：让每条诊断都能定位到规格文件与行。
///
/// 约定：`file` 来自多文件合并期的回填，`line` 来自解析期记录；
/// 字段级诊断用更精确的字段行覆盖声明行。
#[derive(Debug, Clone, Default)]
pub struct Origin {
    pub file: Option<String>,
    pub line: Option<usize>,
}

impl Origin {
    /// 声明级来源（实体 / api / 页面 / 规则 / 命令）。
    pub fn decl(file: &Option<String>, line: usize) -> Self {
        Self {
            file: file.clone(),
            line: Some(line),
        }
    }

    /// 构造 error 诊断（自动带上来源位置）。
    pub fn err(&self, code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: "error".into(),
            code: code.into(),
            message: message.into(),
            file: self.file.clone(),
            line: self.line,
            nodes: None,
        }
    }

    /// 构造 warning 诊断（自动带上来源位置）。
    pub fn warn(&self, code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: "warning".into(),
            code: code.into(),
            message: message.into(),
            file: self.file.clone(),
            line: self.line,
            nodes: None,
        }
    }
}

// ── DSL 补丁文档（RFC-0006 P2：加字段 + 加规则 + 加页面）──────────────

/// 补丁字段规格（DSL 侧：类型名直接用 DSL 词汇——内建 / 枚举 / 实体引用）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DslPatchField {
    pub name: String,
    /// DSL 类型名：`String(50)` / `Int` / `Bool` / `Timestamp` / 枚举名 / 实体名
    pub r#type: String,
}

/// DSL 主线补丁操作集。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum DslPatchOperation {
    /// 加字段：向实体块插入字段行
    AddEntityField {
        entity: String,
        field: DslPatchField,
        #[serde(default)]
        required: bool,
    },
    /// 切换字段必填（`@required`）
    SetEntityFieldRequired {
        entity: String,
        field: String,
        required: bool,
    },
    /// 加规则：追加实体级访问规则（优先并入既有 `access {Entity}` 块）
    AddAccessRule {
        entity: String,
        /// 受约束的操作：`read` / `list` / `get` / `create` / `update` / `delete`
        ///（JSON 键 `accessOp`——`op` 已被 serde 内部 tag 占用）
        #[serde(rename = "accessOp")]
        access_op: String,
        roles: Vec<String>,
    },
    /// 加页面：追加表格页（绑定某 api 的 list 操作）
    AddTablePage {
        name: String,
        title: String,
        /// api 块名（须声明 `list` 操作）
        #[serde(rename = "queryApi")]
        query_api: String,
        /// 展示列（实体字段名，至少 1 个）
        columns: Vec<String>,
    },
    /// 加实体：新增 `entity E { ... }` 声明（RFC-0006 §17：加字段 + 加校验的载体）
    AddEntity {
        entity: String,
        fields: Vec<DslPatchFieldSpec>,
    },
    /// 加字段约束：向 `rules { field E.f { ... } }` 追加 min / max / required
    AddFieldRule {
        entity: String,
        field: String,
        /// min | max | required
        constraint: String,
        /// 数值（min / max 用）
        #[serde(default)]
        value: Option<f64>,
        /// 提示文案（可选）
        #[serde(default)]
        message: Option<String>,
    },
    /// 加表单字段：向页面的 form layout 追加一行控件（RFC-0006 §17）
    AddFormField {
        page: String,
        /// 实体字段名（须已存在于表单实体上）
        field: String,
        /// 控件类型：Input | Input email | Input money | Input datetime | Textarea
        #[serde(default)]
        widget: Option<String>,
    },
    /// 改字段约束：替换已有 min / max 值（RFC-0006 §16 余项收口）
    SetFieldRule {
        entity: String,
        field: String,
        /// min | max
        constraint: String,
        /// 新数值
        value: f64,
        /// 提示文案（可选；缺省保留原值）
        #[serde(default)]
        message: Option<String>,
    },
}

/// 补丁里的字段规格（`add-entity` 用）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DslPatchFieldSpec {
    pub name: String,
    /// DSL 类型名：`String(50)` / `Int` / `Uuid` / 枚举名 / 实体名
    pub r#type: String,
    /// 属性白名单：`@id` `@default(uuid)` `@default(now)` `@required` `@readonly`
    #[serde(default)]
    pub attrs: Vec<String>,
}

/// 人工审批凭据（RFC-0006 §8.7 / §10）：
/// 高风险补丁（High/Extreme 迁移、权限规则变更）**必须**携带，否则 `forma_patch` 拒绝执行。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PatchApproval {
    /// 审批人（人名 / 工单号 / CI 门标识）
    pub by: String,
    /// 审批理由（进审计记录）
    pub reason: String,
}

/// 期望的声明块状态（RFC-0006 §16 冲突检测）：
/// `sha256` 是目标实体声明块（decl 行 → 闭括号行）经 `util::sha256_hex` 的哈希。
/// 应用时重算比对——不匹配即拒绝（`patch-conflict`），防止按过期认知改规格。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DslPatchExpectation {
    pub entity: String,
    pub sha256: String,
}

/// DSL 补丁文档。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DslPatchDocument {
    pub operations: Vec<DslPatchOperation>,
    /// 审批凭据——风险门判定需要审批时缺失即拒绝（P5）
    #[serde(default)]
    pub approval: Option<PatchApproval>,
    /// 补丁幂等键（RFC-0006 §16）：同一 patchId 重复提交不产生二次变更。
    #[serde(default, rename = "patchId")]
    pub patch_id: Option<String>,
    /// 基线版本绑定（RFC-0006 §9 / §16）：`<project>@<IR_VERSION>`，例如 `school@1.2.0`。
    /// 也接受裸 IR 版本（`1.2.0`）；应用前必须与当前 Program.ir_version 匹配。
    #[serde(default, rename = "baseRevision")]
    pub base_revision: Option<String>,
    /// 期望的声明块状态（冲突检测）：缺省 = 不检测
    #[serde(default)]
    pub expected: Option<Vec<DslPatchExpectation>>,
}

/// IR 版本（M2 / spec:ir §7）：语义契约的版本号。
///
/// 兼容性**不得**仅凭版本号判断——判定依据是契约测试 + 语义规则
/// （[spec:compatibility §2](../specifications/compatibility/compatibility-rules.zh-CN.md)）。
/// 语义变更（节点结构 / 标识规则 / 依赖边 / 类型集合）**MUST** 递增 minor 并附迁移说明。
///
/// 版本历史：1.0.0（M2 引入）→ 1.1.0 语义预留（M1–M3 落地期）→
/// 1.2.0（P3：NodeKind 增加 `chart` / `migration`，影响闭包含五类目标）。
pub const IR_VERSION: &str = "1.2.0";

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
        ir_version: IR_VERSION.to_string(),
        jobs: Vec::new(),
        lives: Vec::new(),
        exports: Vec::new(),
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
    for mut e in part.enums {
        e.file = Some(file.to_string());
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
    for mut e in part.entities {
        e.file = Some(file.to_string());
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
    for mut r in part.rules {
        r.file = Some(file.to_string());
        program.rules.push(r);
    }
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
    for mut c in part.commands {
        c.file = Some(file.to_string());
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
    for mut a in part.apis {
        a.file = Some(file.to_string());
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
    for mut p in part.pages {
        p.file = Some(file.to_string());
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
    for mut c in part.charts {
        c.file = Some(file.to_string());
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
    for mut j in part.jobs {
        j.file = Some(file.to_string());
        if program.jobs.iter().any(|x| x.name == j.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate job '{}'", j.name),
                file: Some(file.into()),
                line: Some(j.line),
                nodes: None,
            });
        } else {
            program.jobs.push(j);
        }
    }
    for mut l in part.lives {
        l.file = Some(file.to_string());
        if program.lives.iter().any(|x| x.name == l.name) {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate live '{}'", l.name),
                file: Some(file.into()),
                line: Some(l.line),
                nodes: None,
            });
        } else {
            program.lives.push(l);
        }
    }
    for mut e in part.exports {
        e.file = Some(file.to_string());
        if program
            .exports
            .iter()
            .any(|x| x.api == e.api && x.format == e.format)
        {
            diagnostics.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-duplicate".into(),
                message: format!("duplicate export of api '{}' as {}", e.api, e.format),
                file: Some(file.into()),
                line: Some(e.line),
                nodes: None,
            });
        } else {
            program.exports.push(e);
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
        let ctx = Origin::decl(&entity.file, entity.line);
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
                    file: ctx.file.clone(),
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
                &ctx,
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
                            file: ctx.file.clone(),
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
                            file: ctx.file.clone(),
                            line: Some(field.line),
                            nodes: None,
                        });
                    }
                }
            }
            // @storage（M9/P1）：仅 Attachment 字段；provider 白名单；s3/oss 必须带 bucket
            if let Some(attr) = field.attrs.iter().find(|a| a.name == "storage") {
                if field.type_name != "Attachment" {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-storage-non-attachment".into(),
                        message: format!(
                            "field {}.{}: @storage applies only to Attachment fields (found '{}')",
                            entity.name, field.name, field.type_name
                        ),
                        file: ctx.file.clone(),
                        line: Some(field.line),
                        nodes: None,
                    });
                }
                let provider = attr.args.first().map(|s| s.as_str()).unwrap_or("");
                if !matches!(provider, "local" | "s3" | "oss") {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-storage-unknown-provider".into(),
                        message: format!(
                            "field {}.{}: @storage provider '{}' not supported (allowed: local, s3, oss)",
                            entity.name, field.name, provider
                        ),
                        file: ctx.file.clone(),
                        line: Some(field.line),
                        nodes: None,
                    });
                } else if matches!(provider, "s3" | "oss")
                    && attr.args.get(1).map(|s| s.is_empty()).unwrap_or(true)
                {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-storage-missing-bucket".into(),
                        message: format!(
                            "field {}.{}: @storage {} requires a bucket argument, e.g. @storage({}, \"my-bucket\")",
                            entity.name, field.name, provider, provider
                        ),
                        file: ctx.file.clone(),
                        line: Some(field.line),
                        nodes: None,
                    });
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
                            file: ctx.file.clone(),
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
                    file: ctx.file.clone(),
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
                    file: ctx.file.clone(),
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
                        file: ctx.file.clone(),
                        line: ctx.line,
                        nodes: None,
                    });
                }
            }
        }
    }

    // 规则
    for rules in &program.rules {
        let ctx = Origin::decl(&rules.file, rules.line);
        for fr in &rules.field_rules {
            let Some(entity) = program.entities.iter().find(|e| e.name == fr.entity) else {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-entity".into(),
                    message: format!("field rule references unknown entity '{}'", fr.entity),
                    file: ctx.file.clone(),
                    line: ctx.line,
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
                    file: ctx.file.clone(),
                    line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
                        nodes: None,
                    });
                }
            }
        }
        for check in &rules.checks {
            d.extend(check_expr_paths(
                program,
                &check.entity,
                &check.expr,
                &[],
                &ctx,
            ));
        }
        for guard in &rules.guards {
            if !["create", "update", "delete"].contains(&guard.op.as_str()) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-guard-op".into(),
                    message: format!("guard op '{}' must be create|update|delete", guard.op),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
            }
            if !entity_names.contains(&guard.entity.as_str()) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-entity".into(),
                    message: format!("guard references unknown entity '{}'", guard.entity),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
                continue;
            }
            for deny in &guard.denies {
                d.extend(check_expr_paths(
                    program,
                    &guard.entity,
                    &deny.expr,
                    &[],
                    &ctx,
                ));
            }
        }
        // 实体级访问规则（access）：操作名 / 实体 / 角色 / auth module 必须自洽
        for access in &rules.access {
            if !entity_names.contains(&access.entity.as_str()) {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-unknown-entity".into(),
                    message: format!("access rule references unknown entity '{}'", access.entity),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
                continue;
            }
            // 租户维度（C4）：字段存在、必填、String、auth module 存在、单实体单租户列
            {
                let tenant_fields_here: Vec<_> = program
                    .entities
                    .iter()
                    .find(|e| e.name == access.entity)
                    .map(|e| {
                        e.fields
                            .iter()
                            .filter(|f| has_attr(f, "tenant"))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if tenant_fields_here.len() > 1 {
                    d.push(ctx.err(
                        "dsl-tenant-multiple",
                        format!(
                            "entity {} declares {} @tenant fields; at most one is allowed",
                            access.entity,
                            tenant_fields_here.len()
                        ),
                    ));
                }
                for tf in &tenant_fields_here {
                    if tf.optional {
                        d.push(ctx.err(
                            "dsl-tenant-optional",
                            format!(
                                "@tenant field {}.{} must be required (every row belongs to exactly one tenant)",
                                access.entity, tf.name
                            ),
                        ));
                    }
                    if tf.type_name != "String" && tf.type_name != "Uuid" {
                        d.push(ctx.err(
                            "dsl-tenant-type",
                            format!(
                                "@tenant field {}.{} must be String (tenant identifier is a string claim)",
                                access.entity, tf.name
                            ),
                        ));
                    }
                }
                if !tenant_fields_here.is_empty() && program.auth.is_none() {
                    d.push(ctx.err(
                        "dsl-tenant-no-auth",
                        format!(
                            "@tenant field on {} requires an `auth module` (tenant comes from the JWT claim)",
                            access.entity
                        ),
                    ));
                }
            }
            // 字段级策略（P0-2）：字段存在、非关联、角色合法、provider 存在、表达式受限
            for fp in &access.field_policies {
                let Some(entity_decl) = program.entities.iter().find(|e| e.name == access.entity)
                else {
                    continue; // dsl-unknown-entity 已由操作级检查报告
                };
                let Some(field_decl) = entity_decl.fields.iter().find(|f| f.name == fp.field)
                else {
                    d.push(ctx.err(
                        "dsl-field-policy-unknown-field",
                        format!(
                            "field policy references unknown field {}.{}",
                            access.entity, fp.field
                        ),
                    ));
                    continue;
                };
                if is_relation(program, &field_decl.type_name) {
                    d.push(ctx.err(
                        "dsl-field-policy-relation-unsupported",
                        format!(
                            "field policy on relation {}.{} is not supported (v1: scalar fields only)",
                            access.entity, fp.field
                        ),
                    ));
                    continue;
                }
                let roles = access_rule_roles(&fp.expr);
                if roles.is_empty() {
                    let is_any = matches!(&fp.expr, Expr::Ref { path, .. } if path.len() == 1 && path[0] == "any");
                    if !is_any {
                        d.push(ctx.err(
                            "dsl-field-policy-expression-unsupported",
                            format!(
                                "field policy {}.{} must be `any` or a role expression (role in [...] / role == \"...\")",
                                access.entity, fp.field
                            ),
                        ));
                    }
                    continue;
                }
                match &program.auth {
                    None => d.push(ctx.err(
                        "dsl-field-policy-no-provider",
                        format!(
                            "field policy for '{}.{}' references roles but no `auth module` is declared",
                            access.entity, fp.field
                        ),
                    )),
                    Some(auth) => {
                        for role in &roles {
                            if !auth.roles.iter().any(|r| r == role) {
                                d.push(ctx.err(
                                    "dsl-field-policy-unknown-role",
                                    format!(
                                        "field policy for '{}.{}' references unknown role '{}' (auth module roles: {})",
                                        access.entity,
                                        fp.field,
                                        role,
                                        auth.roles.join(", ")
                                    ),
                                ));
                            }
                        }
                    }
                }
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                                        file: ctx.file.clone(),
                                        line: ctx.line,
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
                    &ctx,
                ));
                // 时间 / 日期比较仅支持同类型字段对字段（不解析字符串时间字面量）。
                d.extend(check_access_temporal_comparisons(
                    program,
                    &access.entity,
                    &rule.expr,
                    &rule.op,
                    &ctx,
                ));
                d.extend(check_access_json_comparisons(
                    program,
                    &access.entity,
                    &rule.expr,
                    &rule.op,
                    &ctx,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                                file: ctx.file.clone(),
                                line: ctx.line,
                                nodes: None,
                            });
                            continue;
                        }
                        if root_field.optional {
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-access-relation-optional-unsupported".into(),
                                message: format!("access path '{}' traverses optional relation '{}'; optional relation access is not supported", path.join("."), path[0]),
                                file: ctx.file.clone(),
                                line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
                            nodes: None,
                        });
                    }
                }
            }
        }
    }

    // api
    for api in &program.apis {
        let ctx = Origin::decl(&api.file, api.line);
        if !entity_names.contains(&api.entity.as_str()) {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-unknown-entity".into(),
                message: format!(
                    "api '{}' references unknown entity '{}'",
                    api.name, api.entity
                ),
                file: ctx.file.clone(),
                line: ctx.line,
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
                    &ctx,
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
                    file: ctx.file.clone(),
                    line: ctx.line,
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
                                    file: ctx.file.clone(),
                                    line: ctx.line,
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
                                    file: ctx.file.clone(),
                                    line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                                file: ctx.file.clone(),
                                line: ctx.line,
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
                                    file: ctx.file.clone(),
                                    line: ctx.line,
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
                                    file: ctx.file.clone(),
                                    line: ctx.line,
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
        let ctx = Origin::decl(&page.file, page.line);
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                                file: ctx.file.clone(),
                                line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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

    // 全文搜索（M10/P1）：字段存在性 + 类型约束（FULLTEXT 仅 CHAR/VARCHAR/TEXT 家族）
    for entity in &program.entities {
        let ctx = Origin::decl(&entity.file, entity.line);
        for name in &entity.searchable {
            let Some(field) = entity.fields.iter().find(|f| &f.name == name) else {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-searchable-unknown-field".into(),
                    message: format!(
                        "entity {} searchable: unknown field '{}' (fields: {})",
                        entity.name,
                        name,
                        entity
                            .fields
                            .iter()
                            .map(|f| f.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
                continue;
            };
            if !matches!(field.type_name.as_str(), "String" | "Text" | "Email") {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-searchable-field-type".into(),
                    message: format!(
                        "entity {} searchable: field '{}' has type '{}' (FULLTEXT requires String/Text/Email)",
                        entity.name, name, field.type_name
                    ),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
            }
        }
    }

    // 实时推送（M10/P1）：api 引用 + 事件合法性
    for live in &program.lives {
        let ctx = Origin::decl(&live.file, live.line);
        let api_names: Vec<&str> = program.apis.iter().map(|a| a.name.as_str()).collect();
        if !api_names.contains(&live.api.as_str()) {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-live-unknown-api".into(),
                message: format!(
                    "live '{}' references unknown api '{}' (available: {})",
                    live.name,
                    live.api,
                    api_names.join(", ")
                ),
                file: ctx.file.clone(),
                line: Some(live.line),
                nodes: None,
            });
        }
        for ev in &live.events {
            if !matches!(ev.as_str(), "create" | "update" | "delete") {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-live-unknown-event".into(),
                    message: format!(
                        "live '{}': unknown event '{}' (allowed: create, update, delete)",
                        live.name, ev
                    ),
                    file: ctx.file.clone(),
                    line: Some(live.line),
                    nodes: None,
                });
            }
        }
    }

    // 数据导出（M9/P1）：api 存在性 / list 操作 / 格式白名单
    for e in &program.exports {
        let ctx = Origin::decl(&e.file, e.line);
        let api_names: Vec<&str> = program.apis.iter().map(|a| a.name.as_str()).collect();
        let Some(api) = program.apis.iter().find(|a| a.name == e.api) else {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-export-unknown-api".into(),
                message: format!(
                    "export '{}' references unknown api '{}' (available: {})",
                    e.name,
                    e.api,
                    api_names.join(", ")
                ),
                file: ctx.file.clone(),
                line: Some(e.line),
                nodes: None,
            });
            continue;
        };
        if !api.operations.iter().any(|op| op == "list") {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-export-no-list".into(),
                message: format!(
                    "export '{}' requires api '{}' to declare a list operation",
                    e.name, e.api
                ),
                file: ctx.file.clone(),
                line: Some(e.line),
                nodes: None,
            });
        }
        if !matches!(e.format.as_str(), "csv" | "xlsx") {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-export-unknown-format".into(),
                message: format!(
                    "export '{}': unknown format '{}' (allowed: csv, xlsx)",
                    e.name, e.format
                ),
                file: ctx.file.clone(),
                line: Some(e.line),
                nodes: None,
            });
        }
    }

    // 通知声明（M10/P1）：渠道合法性 / target 必填 / 事件引用
    let command_names: Vec<&str> = program.commands.iter().map(|c| c.name.as_str()).collect();
    for entity in &program.entities {
        let ctx = Origin::decl(&entity.file, entity.line);
        for n in &entity.notify {
            if !matches!(n.channel.as_str(), "email" | "sms" | "webhook" | "log") {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-notify-unknown-channel".into(),
                    message: format!(
                        "entity {}.@notify: unknown channel '{}' (allowed: email, sms, webhook, log)",
                        entity.name, n.channel
                    ),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
            }
            if n.channel == "webhook" && n.target.is_none() {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-notify-missing-target".into(),
                    message: format!(
                        "entity {}.@notify webhook: target URL is required",
                        entity.name
                    ),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
            }
            if matches!(n.channel.as_str(), "email" | "sms") && n.target.is_none() {
                d.push(Diagnostic {
                    severity: "error".into(),
                    code: "dsl-notify-missing-target".into(),
                    message: format!(
                        "entity {}.@notify {}: recipient target is required",
                        entity.name, n.channel
                    ),
                    file: ctx.file.clone(),
                    line: ctx.line,
                    nodes: None,
                });
            }
            if let Some(event) = &n.event {
                if !command_names.contains(&event.as_str()) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-notify-unknown-event".into(),
                        message: format!(
                            "entity {}.@notify: unknown event '{}' (events reference declared commands; available: {})",
                            entity.name, event, command_names.join(", ")
                        ),
                        file: ctx.file.clone(),
                        line: ctx.line,
                        nodes: None,
                    });
                }
            }
        }
    }

    // 定时任务（M10/P1）：cron 格式 + op 引用
    for job in &program.jobs {
        let ctx = Origin::decl(&job.file, job.line);
        let fields: Vec<&str> = job.cron.split(' ').filter(|f| !f.is_empty()).collect();
        if fields.len() != 5 {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-job-cron-format".into(),
                message: format!(
                    "job '{}': cron must have exactly 5 fields (minute hour day-of-month month day-of-week), got {}",
                    job.name, fields.len()
                ),
                file: ctx.file.clone(),
                line: Some(job.line),
                nodes: None,
            });
        } else {
            let ranges = [
                ("minute", 0i64..60),
                ("hour", 0i64..24),
                ("day-of-month", 1i64..32),
                ("month", 1i64..13),
                ("day-of-week", 0i64..7),
            ];
            for (i, (name, range)) in ranges.iter().enumerate() {
                let raw = fields[i];
                if raw == "*" {
                    continue;
                }
                for part in raw.split(',') {
                    if part.contains('-') {
                        let bounds: Vec<&str> = part.split('-').collect();
                        if bounds.len() != 2 {
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-job-cron-format".into(),
                                message: format!(
                                    "job '{}': invalid cron range '{}' (field {})",
                                    job.name, part, name
                                ),
                                file: ctx.file.clone(),
                                line: Some(job.line),
                                nodes: None,
                            });
                        }
                    } else if let Ok(v) = part.parse::<i64>() {
                        if !(range.contains(&v)) {
                            let allowed = format!("{}..{}", range.start, range.end);
                            d.push(Diagnostic {
                                severity: "error".into(),
                                code: "dsl-job-cron-format".into(),
                                message: format!(
                                    "job '{}': value {} out of range for field {} (allowed {})",
                                    job.name, v, name, allowed
                                ),
                                file: ctx.file.clone(),
                                line: Some(job.line),
                                nodes: None,
                            });
                        }
                    } else {
                        d.push(Diagnostic {
                            severity: "error".into(),
                            code: "dsl-job-cron-format".into(),
                            message: format!(
                                "job '{}': invalid cron value '{}' (field {})",
                                job.name, part, name
                            ),
                            file: ctx.file.clone(),
                            line: Some(job.line),
                            nodes: None,
                        });
                    }
                }
            }
        }
        // op 是 command 时：effects 不得引用 input.*（调度器无输入来源）
        if let Some(cmd) = program.commands.iter().find(|c| c.name == job.op) {
            for eff in &cmd.effects {
                if matches!(eff.value, crate::dsl::parser::EffectValue::InputRef(_)) {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-job-input-effect".into(),
                        message: format!(
                            "job '{}': target command '{}' has input-dependent effect on '{}' (scheduled jobs cannot supply input)",
                            job.name, job.op, eff.field
                        ),
                        file: ctx.file.clone(),
                        line: Some(job.line),
                        nodes: None,
                    });
                }
            }
        }
        // op 是 command 时：pre 必须是单字段比较（调度器按其生成 WHERE，静态可表达）。
        // 复杂 pre 无法安全转 SQL → 显式拒绝（fail-safe，不做全表 UPDATE 的静默降级）。
        if let Some(cmd) = program.commands.iter().find(|c| c.name == job.op) {
            if let Some(pre) = &cmd.pre {
                let simple = match pre {
                    crate::dsl::Expr::Bin(
                        crate::dsl::BinOp::Eq
                        | crate::dsl::BinOp::Ne
                        | crate::dsl::BinOp::Lt
                        | crate::dsl::BinOp::Le
                        | crate::dsl::BinOp::Gt
                        | crate::dsl::BinOp::Ge,
                        a,
                        b,
                    ) => {
                        matches!(
                            (a.as_ref(), b.as_ref()),
                            (crate::dsl::Expr::Ref { path, old: false, .. }, _)
                                if path.len() == 1
                        )
                    }
                    _ => false,
                };
                if !simple {
                    d.push(Diagnostic {
                        severity: "error".into(),
                        code: "dsl-job-pre-unsupported".into(),
                        message: format!(
                            "job '{}': target command '{}' has a complex pre condition (scheduled jobs require a single field comparison like `status == \"pending\"`)",
                            job.name, job.op
                        ),
                        file: ctx.file.clone(),
                        line: Some(job.line),
                        nodes: None,
                    });
                }
            }
        }
        // op 引用：可以是 api 名（CRUD 端点）或 command 名（POST 命令端点）
        let mut known_ops: Vec<&str> = program.apis.iter().map(|a| a.name.as_str()).collect();
        known_ops.extend(program.commands.iter().map(|c| c.name.as_str()));
        if !known_ops.contains(&job.op.as_str()) {
            d.push(Diagnostic {
                severity: "error".into(),
                code: "dsl-job-unknown-op".into(),
                message: format!(
                    "job '{}' references unknown api/command '{}' (available: {})",
                    job.name,
                    job.op,
                    known_ops.join(", ")
                ),
                file: ctx.file.clone(),
                line: Some(job.line),
                nodes: None,
            });
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
    ctx: &Origin,
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
                    file: ctx.file.clone(),
                    line: ctx.line,
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
                        file: ctx.file.clone(),
                        line: ctx.line,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
                            nodes: None,
                        });
                    }
                }
            }
            let _ = old;
        }
        Expr::Bin(_, a, b) => {
            out.extend(check_expr_paths(program, entity_name, a, vars, ctx));
            out.extend(check_expr_paths(program, entity_name, b, vars, ctx));
        }
        Expr::Not(a) => out.extend(check_expr_paths(program, entity_name, a, vars, ctx)),
        Expr::Call(_, args) => {
            for a in args {
                out.extend(check_expr_paths(program, entity_name, a, vars, ctx));
            }
        }
        Expr::List(items) => {
            for a in items {
                out.extend(check_expr_paths(program, entity_name, a, vars, ctx));
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

/// 租户维度（RFC-0008 C4 / 多租户）：
/// 返回 (实体名, 租户字段名) 列表——字段带 `@tenant` 属性即声明为租户列。
///
/// **单一来源**：Check 诊断、auth.rs（claims/CurrentUser/users 表）、repo 的
/// WHERE 注入与 create 绑定共用本函数。
pub fn tenant_fields(program: &Program) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entity in &program.entities {
        let mut seen = Vec::new();
        for f in &entity.fields {
            if has_attr(f, "tenant") {
                seen.push(f.name.clone());
            }
        }
        for f in seen {
            out.push((entity.name.clone(), f));
        }
    }
    out
}

/// 某实体的租户字段名（无租户维度 → None）。
pub fn tenant_field_of(program: &Program, entity_name: &str) -> Option<String> {
    tenant_fields(program)
        .iter()
        .find(|(e, _)| e == entity_name)
        .map(|(_, f)| f.clone())
}

/// 工程是否启用租户维度（存在 @tenant 字段且声明了 auth module）。
pub fn tenant_enabled(program: &Program) -> bool {
    program.auth.is_some() && !tenant_fields(program).is_empty()
}

/// 字段级策略的规范化视图：(实体, 字段, 允许角色)。
///
/// **单一来源**——服务端裁剪（gen-rust）、Web 显隐（gen-web）、Flutter 显隐（gen-flutter）
/// 共用本函数，避免三端口径漂移。`any`（无角色约束）不产生条目。
pub fn field_policy_roles(program: &Program) -> Vec<(String, String, Vec<String>)> {
    let mut out: Vec<(String, String, Vec<String>)> = Vec::new();
    for rules in &program.rules {
        for access in &rules.access {
            let Some(entity) = find_entity(program, &access.entity) else {
                continue;
            };
            for fp in &access.field_policies {
                let Some(field) = entity.fields.iter().find(|f| f.name == fp.field) else {
                    continue;
                };
                let roles = access_rule_roles(&fp.expr);
                if roles.is_empty() {
                    continue; // `any` → 全员可见
                }
                match out
                    .iter_mut()
                    .find(|(e, f, _)| e == &access.entity && f == &field.name)
                {
                    // 同字段多条策略 → 角色并集（任何一条允许即可见）
                    Some(entry) => {
                        for r in roles {
                            if !entry.2.contains(&r) {
                                entry.2.push(r);
                            }
                        }
                    }
                    None => out.push((access.entity.clone(), field.name.clone(), roles)),
                }
            }
        }
    }
    out
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
    ctx: &Origin,
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
        ctx: &Origin,
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
                                file: ctx.file.clone(),
                                line: ctx.line,
                                nodes: None,
                            });
                        }
                    }
                }
                walk(program, a, entity, entity_name, op_name, ctx, out);
                walk(program, b, entity, entity_name, op_name, ctx, out);
            }
            Expr::Not(a) => walk(program, a, entity, entity_name, op_name, ctx, out),
            Expr::Call(_, args) => {
                for arg in args {
                    walk(program, arg, entity, entity_name, op_name, ctx, out);
                }
            }
            Expr::List(items) => {
                for item in items {
                    walk(program, item, entity, entity_name, op_name, ctx, out);
                }
            }
            _ => {}
        }
    }
    walk(program, expr, entity, entity_name, op_name, ctx, &mut out);
    out
}

/// Json 行级条件：仅允许与 Json 字段或 `null` 做相等/不等比较（serde_json::Value PartialEq）。
/// 字符串/数字字面量不隐式转换为 JSON，大小比较无 JSON 语义。
fn check_access_json_comparisons(
    program: &Program,
    entity_name: &str,
    expr: &Expr,
    op_name: &str,
    ctx: &Origin,
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
        ctx: &Origin,
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
                            file: ctx.file.clone(),
                            line: ctx.line,
                            nodes: None,
                        });
                    }
                }
                walk(program, entity, entity_name, op_name, ctx, a, out);
                walk(program, entity, entity_name, op_name, ctx, b, out);
            }
            Expr::Not(a) => walk(program, entity, entity_name, op_name, ctx, a, out),
            Expr::Call(_, args) => {
                for arg in args {
                    walk(program, entity, entity_name, op_name, ctx, arg, out);
                }
            }
            Expr::List(items) => {
                for item in items {
                    walk(program, entity, entity_name, op_name, ctx, item, out);
                }
            }
            _ => {}
        }
    }
    walk(program, entity, entity_name, op_name, ctx, expr, &mut out);
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
    ctx: &Origin,
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
            file: ctx.file.clone(),
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
