//! Forma DSL AST——解析器直接产出类型化程序结构（即 IR 的载体）。

use super::lexer::{TokKind, Token};

#[derive(Debug, Clone)]
pub struct Program {
    pub app: Option<AppDecl>,
    pub enums: Vec<EnumDecl>,
    pub entities: Vec<EntityDecl>,
    pub rules: Vec<RulesDecl>,
    pub apis: Vec<ApiDecl>,
    pub commands: Vec<CommandDecl>,
    pub pages: Vec<PageDecl>,
    /// 可视化图表（Chart IR 的 DSL 载体，RFC-0001 §3.1）
    pub charts: Vec<ChartDecl>,
    /// 认证模块（M5）；None = 纯公开应用
    pub auth: Option<AuthDecl>,
}

#[derive(Debug, Clone)]
pub struct AppDecl {
    pub name: String,
    pub version: String,
    pub locales: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    /// (locale, text)
    pub labels: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct EntityDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub indexes: Vec<Vec<String>>,
    pub uniques: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    /// 类型名（内建 / enum 名 / 实体名），args 如 String(100)、Decimal(12,2)
    pub type_name: String,
    pub type_args: Vec<String>,
    pub optional: bool,
    pub attrs: Vec<Attr>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    /// 括号参数（字符串字面量已去引号）
    pub args: Vec<String>,
    /// 块参数（如 @label { zh "…" }）：Vec<(locale, text)>
    pub block: Vec<(String, String)>,
}

/// i18n 消息：Vec<(locale, text)>
pub type Message = Vec<(String, String)>;

#[derive(Debug, Clone)]
pub struct RulesDecl {
    pub name: String,
    pub field_rules: Vec<FieldRule>,
    pub checks: Vec<CheckRule>,
    pub guards: Vec<GuardDecl>,
}

#[derive(Debug, Clone)]
pub struct FieldRule {
    pub entity: String,
    pub field: String,
    pub constraints: Vec<FieldConstraint>,
}

#[derive(Debug, Clone)]
pub enum FieldConstraint {
    Required(Message),
    Min(f64, Message),
    Max(f64, Message),
    Length(Option<f64>, Option<f64>, Message),
}

#[derive(Debug, Clone)]
pub struct CheckRule {
    pub entity: String,
    pub message: Message,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct GuardDecl {
    pub entity: String,
    pub op: String, // create | update | delete
    pub denies: Vec<DenyDecl>,
}

#[derive(Debug, Clone)]
pub struct DenyDecl {
    pub expr: Expr,
    pub reason: Message,
}

// ── 认证模块（spec M5）──────────────────────────────────

/// `auth module { provider jwt }`：用户/角色/会话 + 登录注册端点 + 中间件。
#[derive(Debug, Clone)]
pub struct AuthDecl {
    /// jwt | session（当前仅 jwt 落地；其余按宪法显式报错）
    pub provider: String,
    /// 角色枚举成员（缺省 [admin, user]）
    pub roles: Vec<String>,
    /// token 有效期（小时，缺省 24）
    pub token_hours: u64,
}

// ── 可视化（Chart）── RFC-0001 ─────────────────────────────

/// `chart` 顶层块：描述一个可视化组件的语义规格。
#[derive(Debug, Clone)]
pub struct ChartDecl {
    pub name: String,
    /// 语义图表类型：line | bar | area | pie | scatter | funnel | gauge | radar | heatmap | treemap
    pub chart_type: String,
    pub title: Message,
    /// 数据绑定：`<ApiName>.<operation>`
    pub source: Option<(String, String)>,
    pub encoding: EncodingDecl,
    pub transform: Vec<TransformDecl>,
    pub axis: AxisDecl,
    pub tooltip: Option<bool>,
    pub legend: Option<bool>,
    /// interaction.zoom
    pub zoom: Option<bool>,
    pub appearance: AppearanceDecl,
    pub line: usize,
}

#[derive(Debug, Clone, Default)]
pub struct EncodingDecl {
    pub x: Option<ChannelDecl>,
    pub y: Option<ChannelDecl>,
    pub series: Option<ChannelDecl>,
    pub color: Option<ChannelDecl>,
}

#[derive(Debug, Clone, Default)]
pub struct ChannelDecl {
    /// 实体字段路径（V0.1 只支持直接字段）
    pub field: String,
    /// temporal | quantitative | nominal | ordinal
    pub value_type: Option<String>,
    /// currency | percent | integer | decimal | datetime | date
    pub format: Option<String>,
    /// 轴标题（仅 axis 通道使用）
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TransformDecl {
    Filter {
        field: String,
        cmp: String,
        value: String,
    },
    Bucket {
        field: String,
        by: String,
    },
    Aggregate {
        op: String,
        field: Option<String>,
        group_by: Vec<String>,
    },
    Sort {
        by: String,
        desc: bool,
    },
    Limit {
        n: usize,
    },
}

#[derive(Debug, Clone, Default)]
pub struct AxisDecl {
    pub x: Option<ChannelDecl>,
    pub y: Option<ChannelDecl>,
}

#[derive(Debug, Clone, Default)]
pub struct AppearanceDecl {
    pub smooth: Option<bool>,
    pub show_symbol: Option<bool>,
}

// ── 表达式 ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    /// 字段路径：amount / order.customer.id；old = true 表示 old. 前缀
    Ref {
        path: Vec<String>,
        old: bool,
    },
    List(Vec<Expr>),
    Call(String, Vec<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    In,
}

// ── API ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ApiDecl {
    pub name: String,
    pub entity: String,
    pub operations: Vec<String>,
    pub list: Option<ListCfg>,
}

/// 命令类操作（spec §10）：状态机审批流等真实业务动作。
/// `api OrderApprove { POST "/orders/{id}/approve" input {..} pre {..} effect { set .. } audit {..} }`
#[derive(Debug, Clone)]
pub struct CommandDecl {
    pub name: String,
    pub method: String,
    pub path: String,
    /// 按钮文案（缺省用块名）
    pub label: Message,
    pub input: Vec<CommandInput>,
    /// 前置条件：对目标实体当前行求值，不成立 → 409 + message
    pub pre: Option<Expr>,
    /// pre 失败时的业务文案（zh/en）
    pub message: Option<Message>,
    pub effects: Vec<Effect>,
    /// 审计动作名（如 "order.approve"）
    pub audit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommandInput {
    pub name: String,
    pub type_name: String,
    pub optional: bool,
    pub type_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Effect {
    pub entity: String,
    pub field: String,
    pub value: EffectValue,
}

#[derive(Debug, Clone)]
pub enum EffectValue {
    /// 枚举变体（未加引号的标识符）
    Ident(String),
    Str(String),
    Num(f64),
    Bool(bool),
    /// 当前时间
    Now,
    /// 引用命令输入体字段（如 input.comment）
    InputRef(String),
}

#[derive(Debug, Clone)]
pub struct ListCfg {
    /// (字段, contains?)
    pub filters: Vec<(String, bool)>,
    pub search: Vec<String>,
    /// (字段, desc)
    pub sort: Vec<(String, bool)>,
    pub paginate: Option<String>,
}

// ── Page ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PageDecl {
    pub name: String,
    pub route: String,
    pub title: Message,
    pub body: PageBody,
}

#[derive(Debug, Clone)]
pub enum PageBody {
    Table(TableCfg),
    Form(FormCfg),
    Detail(DetailCfg),
    /// 可视化图表页面（RFC-0001 §3.3）
    Chart(ChartPageCfg),
}

/// 页面挂载的图表：引用顶层 `chart` 块名。
#[derive(Debug, Clone)]
pub struct ChartPageCfg {
    pub chart: String,
}

#[derive(Debug, Clone)]
pub struct TableCfg {
    pub query_api: String,
    pub columns: Vec<ColumnCfg>,
    pub filters: Vec<FilterCfg>,
    pub search: Vec<String>,
    pub actions: Option<ActionsCfg>,
}

#[derive(Debug, Clone)]
pub struct ColumnCfg {
    pub path: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FilterCfg {
    pub path: String,
    pub widget: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActionsCfg {
    pub create: Option<String>,
    /// (action, target) — edit → Page；delete → Api.op
    pub row: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct FormCfg {
    pub entity: String,
    pub fields: Vec<FormFieldCfg>,
    pub validation: Vec<String>,
    /// (is_create, api_op, redirect)
    pub submits: Vec<SubmitCfg>,
}

#[derive(Debug, Clone)]
pub struct FormFieldCfg {
    pub path: String,
    pub widget: String,
    pub from: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct SubmitCfg {
    pub is_create: bool,
    pub api_op: String,
    pub redirect: String,
}

pub type DetailSections = Vec<(String, Vec<(String, Option<String>)>)>;

#[derive(Debug, Clone)]
pub struct DetailCfg {
    pub entity: String,
    /// (section name, Vec<(path, hint)>)
    pub sections: DetailSections,
    pub children: Vec<(String, String)>,
}

// ── 解析诊断 ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

pub struct ParseOutcome {
    pub program: Program,
    pub errors: Vec<ParseError>,
}

/// 宽容解析：语法错误记录后跳过当前块，尽量多解析。
pub fn parse(tokens: &[Token]) -> ParseOutcome {
    let mut p = Parser {
        toks: tokens,
        pos: 0,
        errors: Vec::new(),
        pending_commands: Vec::new(),
    };
    let mut program = p.parse_program();
    program.commands = std::mem::take(&mut p.pending_commands);
    ParseOutcome {
        program,
        errors: p.errors,
    }
}

struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
    errors: Vec<ParseError>,
    /// `api` 块的命令形态经由 parse_api 分发，但 commands 与 apis 是不同的
    /// Program 槽位 —— 解析期先暂存，parse_program 结束时合并。
    pending_commands: Vec<CommandDecl>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn error_at(&mut self, tok: &Token, msg: impl Into<String>) {
        self.errors.push(ParseError {
            line: tok.line,
            col: tok.col,
            message: msg.into(),
        });
    }

    fn is_punct(&self, s: &str) -> bool {
        self.peek().kind == TokKind::Punct && self.peek().value == s
    }

    fn is_kw(&self, s: &str) -> bool {
        self.peek().kind == TokKind::Ident && self.peek().value == s
    }

    fn eat_punct(&mut self, s: &str) -> bool {
        if self.is_punct(s) {
            self.next();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, s: &str) -> bool {
        if self.is_kw(s) {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, s: &str) -> bool {
        if self.eat_punct(s) {
            true
        } else {
            let t = self.peek().clone();
            self.error_at(&t, format!("expected '{s}', found '{}'", t.value));
            false
        }
    }

    fn expect_ident(&mut self) -> Option<String> {
        if self.peek().kind == TokKind::Ident {
            Some(self.next().value)
        } else {
            let t = self.peek().clone();
            self.error_at(&t, format!("expected identifier, found '{}'", t.value));
            None
        }
    }

    fn expect_string(&mut self) -> Option<String> {
        if self.peek().kind == TokKind::Str {
            Some(self.next().value)
        } else {
            let t = self.peek().clone();
            self.error_at(&t, format!("expected string, found '{}'", t.value));
            None
        }
    }

    fn skip_to_block_end(&mut self) {
        // 跳过当前平衡块（假定光标在 '{' 之后）
        let mut depth = 1usize;
        while self.pos < self.toks.len() - 1 {
            let t = self.next();
            if t.kind == TokKind::Punct && t.value == "{" {
                depth += 1;
            } else if t.kind == TokKind::Punct && t.value == "}" {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
        }
    }

    // ── program ──

    fn parse_program(&mut self) -> Program {
        let mut program = Program {
            app: None,
            enums: Vec::new(),
            entities: Vec::new(),
            rules: Vec::new(),
            apis: Vec::new(),
            commands: Vec::new(),
            pages: Vec::new(),
            charts: Vec::new(),
            auth: None,
        };
        while self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "app" if tok.kind == TokKind::Ident => {
                    if let Some(a) = self.parse_app() {
                        program.app = Some(a);
                    }
                }
                "enum" if tok.kind == TokKind::Ident => {
                    if let Some(e) = self.parse_enum() {
                        program.enums.push(e);
                    }
                }
                "entity" if tok.kind == TokKind::Ident => {
                    if let Some(e) = self.parse_entity() {
                        program.entities.push(e);
                    }
                }
                "rules" if tok.kind == TokKind::Ident => {
                    if let Some(r) = self.parse_rules() {
                        program.rules.push(r);
                    }
                }
                "api" if tok.kind == TokKind::Ident => {
                    if let Some(a) = self.parse_api() {
                        if !a.name.is_empty() {
                            program.apis.push(a);
                        }
                    }
                }
                "page" if tok.kind == TokKind::Ident => {
                    if let Some(p) = self.parse_page() {
                        program.pages.push(p);
                    }
                }
                "auth" if tok.kind == TokKind::Ident => {
                    if let Some(a) = self.parse_auth() {
                        program.auth = Some(a);
                    }
                }
                "chart" if tok.kind == TokKind::Ident => {
                    if let Some(c) = self.parse_chart() {
                        program.charts.push(c);
                    }
                }
                _ => {
                    self.error_at(
                        &tok,
                        format!(
                            "unexpected '{}', expected app/enum/entity/rules/api/page/chart",
                            tok.value
                        ),
                    );
                    self.next();
                }
            }
        }
        program
    }

    // ── app ──

    fn parse_app(&mut self) -> Option<AppDecl> {
        self.next(); // app
        let name = self.expect_ident()?;
        let mut decl = AppDecl {
            name,
            version: "0.0.0".into(),
            locales: vec!["en".into()],
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.is_punct(";") {
                self.next();
                continue;
            }
            let key = self.peek().clone();
            match key.value.as_str() {
                "version" if key.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("=") {
                        if let Some(v) = self.expect_string() {
                            decl.version = v;
                        }
                    }
                }
                "locale" if key.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("=") {
                        decl.locales = self.parse_string_list();
                    }
                }
                other => {
                    self.error_at(&key, format!("unknown app entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        Some(decl)
    }

    fn parse_ident_list(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.eat_punct("[") {
            return out;
        }
        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(",") {
                continue;
            }
            if let Some(id) = self.expect_ident() {
                out.push(id);
            } else {
                self.next();
            }
        }
        self.expect_punct("]");
        out
    }

    fn parse_string_list(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.eat_punct("[") {
            return out;
        }
        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(",") {
                continue;
            }
            if let Some(s) = self.expect_string() {
                out.push(s);
            } else {
                self.next();
            }
        }
        self.expect_punct("]");
        out
    }

    /// 点路径：IDENT(.IDENT)* — 返回完整字符串
    fn parse_path(&mut self) -> Option<String> {
        let first = self.expect_ident()?;
        let mut out = first;
        while self.is_punct(".") {
            self.next();
            if let Some(seg) = self.expect_ident() {
                out.push('.');
                out.push_str(&seg);
            }
        }
        Some(out)
    }

    fn is_at_attr(&self, name: &str) -> bool {
        if self.peek().kind == TokKind::Punct && self.peek().value == "@" {
            let next = self.toks.get(self.pos + 1);
            return next
                .map(|t| t.kind == TokKind::Ident && t.value == name)
                .unwrap_or(false);
        }
        false
    }

    // ── enum ──

    fn parse_enum(&mut self) -> Option<EnumDecl> {
        self.next(); // enum
        let name = self.expect_ident()?;
        let mut decl = EnumDecl {
            name,
            variants: Vec::new(),
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(",") || self.eat_punct(";") {
                continue;
            }
            let Some(variant) = self.expect_ident() else {
                self.next();
                continue;
            };
            let mut labels = Vec::new();
            if self.is_at_attr("label") {
                self.next();
                self.next();
                if self.eat_punct("{") {
                    while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                        let locale = self.expect_ident();
                        let text = self.expect_string();
                        if let (Some(l), Some(t)) = (locale, text) {
                            labels.push((l, t));
                        } else {
                            self.next();
                        }
                    }
                    self.expect_punct("}");
                }
            }
            decl.variants.push(EnumVariant {
                name: variant,
                labels,
            });
        }
        self.expect_punct("}");
        Some(decl)
    }

    // ── entity ──

    fn parse_entity(&mut self) -> Option<EntityDecl> {
        self.next(); // entity
        let name = self.expect_ident()?;
        let mut decl = EntityDecl {
            name,
            fields: Vec::new(),
            indexes: Vec::new(),
            uniques: Vec::new(),
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") || self.eat_punct(",") {
                continue;
            }
            if self.is_kw("index") || self.is_kw("unique") {
                let is_unique = self.is_kw("unique");
                self.next();
                let cols = self.parse_ident_list();
                if is_unique {
                    decl.uniques.push(cols);
                } else {
                    decl.indexes.push(cols);
                }
                continue;
            }
            let Some(field) = self.parse_field() else {
                self.next();
                continue;
            };
            decl.fields.push(field);
        }
        self.expect_punct("}");
        Some(decl)
    }

    fn parse_field(&mut self) -> Option<FieldDecl> {
        let name_tok = self.peek().clone();
        let name = self.expect_ident()?;
        // 类型：IDENT [ '(' args ')' ] ['?']
        let type_name = self.expect_ident()?;
        let mut type_args = Vec::new();
        if self.eat_punct("(") {
            while !self.is_punct(")") && self.peek().kind != TokKind::Eof {
                match self.peek().kind {
                    TokKind::Num | TokKind::Ident => type_args.push(self.next().value),
                    TokKind::Punct if self.peek().value == "," => {
                        self.next();
                    }
                    _ => {
                        self.next();
                    }
                }
            }
            self.expect_punct(")");
        }
        let optional = self.eat_punct("?");
        let mut attrs = Vec::new();
        while self.peek().kind == TokKind::Punct && self.peek().value == "@" {
            self.next();
            let Some(attr_name) = self.expect_ident() else {
                break;
            };
            let mut args = Vec::new();
            let mut block = Vec::new();
            if self.eat_punct("(") {
                while !self.is_punct(")") && self.peek().kind != TokKind::Eof {
                    match self.peek().kind {
                        TokKind::Str => args.push(self.next().value),
                        TokKind::Num => args.push(self.next().value),
                        TokKind::Ident => args.push(self.next().value),
                        TokKind::Punct if self.peek().value == "," => {
                            self.next();
                        }
                        _ => {
                            self.next();
                        }
                    }
                }
                self.expect_punct(")");
            } else if self.eat_punct("{") {
                while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                    let locale = self.expect_ident();
                    let text = self.expect_string();
                    if let (Some(l), Some(t)) = (locale, text) {
                        block.push((l, t));
                    } else {
                        self.next();
                    }
                }
                self.expect_punct("}");
            }
            attrs.push(Attr {
                name: attr_name,
                args,
                block,
            });
        }
        let _ = name_tok;
        Some(FieldDecl {
            name,
            type_name,
            type_args,
            optional,
            attrs,
            line: name_tok.line,
        })
    }

    // ── rules ──

    fn parse_rules(&mut self) -> Option<RulesDecl> {
        self.next(); // rules
        let name = self.expect_ident()?;
        let mut decl = RulesDecl {
            name,
            field_rules: Vec::new(),
            checks: Vec::new(),
            guards: Vec::new(),
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") {
                continue;
            }
            if self.is_kw("field") {
                self.next();
                let Some(entity) = self.expect_ident() else {
                    break;
                };
                self.expect_punct(".");
                let Some(field) = self.expect_ident() else {
                    break;
                };
                let mut constraints = Vec::new();
                if self.eat_punct("{") {
                    while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                        let kind = self.peek().clone();
                        match kind.value.as_str() {
                            "required" if kind.kind == TokKind::Ident => {
                                self.next();
                                let msg = self.parse_message_block();
                                constraints.push(FieldConstraint::Required(msg));
                            }
                            "min" | "max" if kind.kind == TokKind::Ident => {
                                self.next();
                                let value = self.parse_number();
                                let msg = self.parse_message_block();
                                if kind.value == "min" {
                                    constraints.push(FieldConstraint::Min(value, msg));
                                } else {
                                    constraints.push(FieldConstraint::Max(value, msg));
                                }
                            }
                            "length" if kind.kind == TokKind::Ident => {
                                self.next();
                                let min = if self.is_punct("(") {
                                    self.next();
                                    let v = self.parse_number_opt();
                                    self.expect_punct(",");
                                    let max = self.parse_number_opt();
                                    self.expect_punct(")");
                                    (v, max)
                                } else {
                                    (None, None)
                                };
                                let msg = self.parse_message_block();
                                constraints.push(FieldConstraint::Length(min.0, min.1, msg));
                            }
                            other => {
                                self.error_at(&kind, format!("unknown constraint '{other}'"));
                                self.next();
                            }
                        }
                    }
                    self.expect_punct("}");
                }
                decl.field_rules.push(FieldRule {
                    entity,
                    field,
                    constraints,
                });
            } else if self.is_kw("check") {
                self.next();
                let Some(entity) = self.expect_ident() else {
                    break;
                };
                let mut message: Message = Vec::new();
                if self.peek().kind == TokKind::Str {
                    if let Some(m) = self.expect_string() {
                        message = vec![("zh".into(), m)];
                    }
                }
                let expr = if self.eat_punct("{") {
                    let e = self.parse_expr();
                    self.expect_punct("}");
                    e
                } else {
                    Expr::Bool(true)
                };
                decl.checks.push(CheckRule {
                    entity,
                    message,
                    expr,
                });
            } else if self.is_kw("guard") {
                self.next();
                let Some(entity) = self.expect_ident() else {
                    break;
                };
                self.expect_punct(".");
                let Some(op) = self.expect_ident() else { break };
                let mut denies = Vec::new();
                if self.eat_punct("{") {
                    while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                        if self.eat_kw("deny") {
                            let expr = self.parse_expr();
                            let mut reason = Vec::new();
                            if self.eat_kw("reason") {
                                reason = self.parse_message_block();
                            }
                            denies.push(DenyDecl { expr, reason });
                        } else {
                            let t = self.peek().clone();
                            self.error_at(&t, format!("expected 'deny', found '{}'", t.value));
                            self.next();
                        }
                    }
                    self.expect_punct("}");
                }
                decl.guards.push(GuardDecl { entity, op, denies });
            } else {
                let t = self.peek().clone();
                self.error_at(&t, format!("unexpected '{}' in rules", t.value));
                self.next();
            }
        }
        self.expect_punct("}");
        Some(decl)
    }

    fn parse_message_block(&mut self) -> Message {
        let mut msg = Vec::new();
        if self.eat_punct("{") {
            while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                let locale = self.expect_ident();
                let text = self.expect_string();
                if let (Some(l), Some(t)) = (locale, text) {
                    msg.push((l, t));
                } else {
                    self.next();
                }
            }
            self.expect_punct("}");
        }
        msg
    }

    fn parse_number(&mut self) -> f64 {
        if self.peek().kind == TokKind::Num {
            self.next().value.parse().unwrap_or(0.0)
        } else {
            let t = self.peek().clone();
            self.error_at(&t, format!("expected number, found '{}'", t.value));
            self.next();
            0.0
        }
    }

    fn parse_number_opt(&mut self) -> Option<f64> {
        if self.peek().kind == TokKind::Num {
            Some(self.next().value.parse().unwrap_or(0.0))
        } else {
            None
        }
    }

    // ── api ──

    fn parse_api(&mut self) -> Option<ApiDecl> {
        self.next(); // api
        let name = self.expect_ident()?;
        let mut decl = ApiDecl {
            name: name.clone(),
            entity: String::new(),
            operations: vec![
                "list".into(),
                "get".into(),
                "create".into(),
                "update".into(),
                "delete".into(),
            ],
            list: None,
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        // 命令形态：`api Name { POST "/path" ... }`（块内首个 token 是 HTTP 方法）
        if matches!(
            self.peek().value.as_str(),
            "POST" | "GET" | "PUT" | "DELETE" | "PATCH"
        ) && self.peek().kind == TokKind::Ident
        {
            let cmd = self.parse_command(name)?;
            self.pending_commands.push(cmd);
            // 空 ApiDecl 占位：commands 与 apis 是不同槽位，命令不入 apis
            return Some(ApiDecl {
                name: String::new(),
                entity: String::new(),
                operations: vec![],
                list: None,
            });
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") {
                continue;
            }
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "entity" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(e) = self.expect_ident() {
                        decl.entity = e;
                    }
                }
                "operations" if tok.kind == TokKind::Ident => {
                    self.next();
                    decl.operations = self.parse_ident_list();
                }
                "list" if tok.kind == TokKind::Ident => {
                    self.next();
                    decl.list = self.parse_list_cfg();
                }
                other => {
                    self.error_at(
                        &tok,
                        format!("unknown api entry '{other}' (commands/auth: later milestone)"),
                    );
                    self.next();
                    if self.is_punct("{") {
                        self.next();
                        self.skip_to_block_end();
                    }
                }
            }
        }
        self.expect_punct("}");
        Some(decl)
    }

    /// 认证模块（M5）：
    /// ```fm
    /// auth module {
    ///   provider jwt
    ///   roles [admin, teacher, user]
    ///   tokenHours 24
    /// }
    /// ```
    fn parse_auth(&mut self) -> Option<AuthDecl> {
        self.next(); // auth
        let kw = self.expect_ident()?;
        if kw != "module" {
            let t = self.toks[(self.pos - 1).min(self.toks.len() - 1)].clone();
            self.error_at(&t, format!("expected 'auth module', found 'auth {kw}'"));
            return None;
        }
        let mut decl = AuthDecl {
            provider: "jwt".into(),
            roles: vec!["admin".into(), "user".into()],
            token_hours: 24,
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") {
                continue;
            }
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "provider" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(p) = self.expect_ident() {
                        decl.provider = p;
                    }
                }
                "roles" if tok.kind == TokKind::Ident => {
                    self.next();
                    let roles = self.parse_ident_list();
                    if roles.is_empty() {
                        self.error_at(&tok, "auth roles must not be empty");
                    } else {
                        decl.roles = roles;
                    }
                }
                "tokenHours" if tok.kind == TokKind::Ident => {
                    self.next();
                    decl.token_hours = self.parse_number() as u64;
                }
                other => {
                    self.error_at(&tok, format!("unknown auth entry '{other}'"));
                    self.next();
                    if self.is_punct("{") {
                        self.next();
                        self.skip_to_block_end();
                    }
                }
            }
        }
        self.expect_punct("}");
        Some(decl)
    }

    /// 命令类操作（spec §10）：
    /// ```fm
    /// api OrderApprove {
    ///   POST "/orders/{id}/approve"
    ///   label { zh "审批" en "Approve" }
    ///   input { comment Text? }
    ///   pre  { status == "pending" }
    ///   message { zh "..." en "..." }
    ///   effect { set Order.status = approved
    ///            set Order.approverComment = input.comment }
    ///   audit { action "order.approve" }
    /// }
    /// ```
    fn parse_command(&mut self, name: String) -> Option<CommandDecl> {
        let method = self.next().value.to_uppercase();
        let path = self.expect_string()?;
        let mut cmd = CommandDecl {
            name,
            method,
            path,
            label: Vec::new(),
            input: Vec::new(),
            pre: None,
            message: None,
            effects: Vec::new(),
            audit: None,
        };
        // 条目直接跟在 POST "/path" 之后（label/input/pre/message/effect/audit），
        // 循环终止于 api 块的闭括号
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") {
                continue;
            }
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "label" if tok.kind == TokKind::Ident => {
                    self.next();
                    cmd.label = self.parse_message_block();
                }
                "input" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.expect_punct("{") {
                        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                            if self.eat_punct(";") {
                                continue;
                            }
                            let Some(fname) = self.expect_ident() else {
                                break;
                            };
                            let Some(type_name) = self.expect_ident() else {
                                break;
                            };
                            let mut optional = false;
                            let mut type_args = Vec::new();
                            if self.eat_punct("(") {
                                while !self.is_punct(")") && self.peek().kind != TokKind::Eof {
                                    if let Some(a) = self.expect_ident() {
                                        type_args.push(a);
                                    } else {
                                        self.next();
                                    }
                                }
                                self.expect_punct(")");
                            }
                            if self.eat_punct("?") {
                                optional = true;
                            }
                            cmd.input.push(CommandInput {
                                name: fname,
                                type_name,
                                optional,
                                type_args,
                            });
                        }
                        self.expect_punct("}");
                    }
                }
                "pre" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("{") {
                        cmd.pre = Some(self.parse_expr());
                        self.expect_punct("}");
                    }
                }
                "message" if tok.kind == TokKind::Ident => {
                    self.next();
                    cmd.message = Some(self.parse_message_block());
                }
                "effect" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.expect_punct("{") {
                        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                            if self.eat_punct(";") {
                                continue;
                            }
                            if !self.eat_kw("set") {
                                let t = self.peek().clone();
                                self.error_at(
                                    &t,
                                    format!("expected 'set' in effect, found '{}'", t.value),
                                );
                                self.next();
                                continue;
                            }
                            let Some(entity) = self.expect_ident() else {
                                break;
                            };
                            self.expect_punct(".");
                            let Some(field) = self.expect_ident() else {
                                break;
                            };
                            self.expect_punct("=");
                            let value = self.parse_effect_value();
                            cmd.effects.push(Effect {
                                entity,
                                field,
                                value,
                            });
                        }
                        self.expect_punct("}");
                    }
                }
                "audit" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("{") {
                        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                            if self.eat_kw("action") {
                                if let Some(a) = self.expect_string() {
                                    cmd.audit = Some(a);
                                }
                            } else {
                                self.next();
                            }
                        }
                        self.expect_punct("}");
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown command entry '{other}'"));
                    self.next();
                    if self.is_punct("{") {
                        self.next();
                        self.skip_to_block_end();
                    }
                }
            }
        }
        self.expect_punct("}");
        Some(cmd)
    }

    /// effect 值：枚举变体 / 字符串 / 数字 / bool / now / input.field
    fn parse_effect_value(&mut self) -> EffectValue {
        let tok = self.peek().clone();
        match tok.kind {
            TokKind::Str => EffectValue::Str(self.next().value),
            TokKind::Num => EffectValue::Num(self.next().value.parse().unwrap_or(0.0)),
            TokKind::Ident => {
                self.next();
                match tok.value.as_str() {
                    "true" => EffectValue::Bool(true),
                    "false" => EffectValue::Bool(false),
                    "now" => EffectValue::Now,
                    "input" => {
                        if self.eat_punct(".") {
                            let f = self.expect_ident().unwrap_or_default();
                            EffectValue::InputRef(f)
                        } else {
                            EffectValue::Ident("input".into())
                        }
                    }
                    _ => EffectValue::Ident(tok.value),
                }
            }
            _ => {
                self.error_at(
                    &tok,
                    "expected effect value (variant / literal / now / input.field)",
                );
                self.next();
                EffectValue::Ident(String::new())
            }
        }
    }

    fn parse_list_cfg(&mut self) -> Option<ListCfg> {
        if !self.expect_punct("{") {
            return None;
        }
        let mut cfg = ListCfg {
            filters: Vec::new(),
            search: Vec::new(),
            sort: Vec::new(),
            paginate: None,
        };
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "filters" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("[") {
                        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
                            let Some(field) = self.expect_ident() else {
                                break;
                            };
                            let contains = self.eat_kw("contains");
                            cfg.filters.push((field, contains));
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                        self.expect_punct("]");
                    }
                }
                "search" if tok.kind == TokKind::Ident => {
                    self.next();
                    cfg.search = self.parse_ident_list();
                }
                "sort" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("[") {
                        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
                            let Some(field) = self.expect_ident() else {
                                break;
                            };
                            let desc = if self.eat_kw("desc") {
                                true
                            } else {
                                self.eat_kw("asc");
                                false
                            };
                            cfg.sort.push((field, desc));
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                        self.expect_punct("]");
                    }
                }
                "paginate" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(p) = self.expect_ident() {
                        cfg.paginate = Some(p);
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown list entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        Some(cfg)
    }

    // ── chart（可视化，RFC-0001）──

    /// `chart Name { type line  title {...}  source Orders.list  encoding {...} ... }`
    fn parse_chart(&mut self) -> Option<ChartDecl> {
        let name_tok = self.peek().clone();
        self.next(); // chart
        let name = self.expect_ident()?;
        let mut decl = ChartDecl {
            name,
            chart_type: String::new(),
            title: Vec::new(),
            source: None,
            encoding: EncodingDecl::default(),
            transform: Vec::new(),
            axis: AxisDecl::default(),
            tooltip: None,
            legend: None,
            zoom: None,
            appearance: AppearanceDecl::default(),
            line: name_tok.line,
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") {
                continue;
            }
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "type" => {
                    self.next();
                    decl.chart_type = self.expect_ident().unwrap_or_default();
                }
                "title" => {
                    self.next();
                    decl.title = self.parse_title_value();
                }
                "source" => {
                    self.next();
                    if let Some(p) = self.parse_path() {
                        let (api, operation) = match p.split_once('.') {
                            Some((a, op)) => (a.to_string(), op.to_string()),
                            None => (p.clone(), "list".to_string()),
                        };
                        decl.source = Some((api, operation));
                    }
                }
                "encoding" => {
                    self.next();
                    decl.encoding = self.parse_encoding();
                }
                "transform" => {
                    self.next();
                    decl.transform = self.parse_transform();
                }
                "axis" => {
                    self.next();
                    decl.axis = self.parse_axis();
                }
                "tooltip" => {
                    self.next();
                    decl.tooltip = self.parse_enabled();
                }
                "legend" => {
                    self.next();
                    decl.legend = self.parse_enabled();
                }
                "interaction" => {
                    self.next();
                    decl.zoom = self.parse_zoom();
                }
                "appearance" => {
                    self.next();
                    decl.appearance = self.parse_appearance();
                }
                other => {
                    self.error_at(&tok, format!("unknown chart entry '{other}'"));
                    self.next();
                    if self.is_punct("{") {
                        self.next();
                        self.skip_to_block_end();
                    }
                }
            }
        }
        self.expect_punct("}");
        Some(decl)
    }

    /// title：字符串字面量（单语言）或 i18n 块。
    fn parse_title_value(&mut self) -> Message {
        if self.peek().kind == TokKind::Str {
            let t = self.next().value;
            return vec![("zh".to_string(), t)];
        }
        self.parse_message_block()
    }

    fn parse_enabled(&mut self) -> Option<bool> {
        if !self.expect_punct("{") {
            return None;
        }
        let mut enabled = None;
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "enabled" => {
                    self.next();
                    enabled = self.parse_bool();
                }
                other => {
                    self.error_at(&tok, format!("unknown entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        enabled
    }

    fn parse_zoom(&mut self) -> Option<bool> {
        if !self.expect_punct("{") {
            return None;
        }
        let mut zoom = None;
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "zoom" => {
                    self.next();
                    zoom = self.parse_bool();
                }
                other => {
                    self.error_at(&tok, format!("unknown interaction entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        zoom
    }

    fn parse_bool(&mut self) -> Option<bool> {
        if self.peek().kind == TokKind::Ident {
            let v = self.next().value;
            return match v.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
        if self.peek().kind == TokKind::Num {
            let v = self.next().value;
            return Some(v != "0");
        }
        None
    }

    fn parse_appearance(&mut self) -> AppearanceDecl {
        let mut app = AppearanceDecl::default();
        if !self.expect_punct("{") {
            return app;
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "smooth" => {
                    self.next();
                    app.smooth = self.parse_bool();
                }
                "showSymbol" => {
                    self.next();
                    app.show_symbol = self.parse_bool();
                }
                other => {
                    self.error_at(&tok, format!("unknown appearance entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        app
    }

    fn parse_encoding(&mut self) -> EncodingDecl {
        let mut enc = EncodingDecl::default();
        if !self.expect_punct("{") {
            return enc;
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "x" | "y" | "series" | "color" => {
                    self.next();
                    let ch = self.parse_channel(false);
                    match tok.value.as_str() {
                        "x" => enc.x = Some(ch),
                        "y" => enc.y = Some(ch),
                        "series" => enc.series = Some(ch),
                        _ => enc.color = Some(ch),
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown encoding entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        enc
    }

    fn parse_axis(&mut self) -> AxisDecl {
        let mut axis = AxisDecl::default();
        if !self.expect_punct("{") {
            return axis;
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "x" | "y" => {
                    self.next();
                    let ch = self.parse_channel(true);
                    if tok.value == "x" {
                        axis.x = Some(ch);
                    } else {
                        axis.y = Some(ch);
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown axis entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        axis
    }

    /// 通道声明：`{ field <path> [type <t>] [format <f>] [label "<s>"] }`
    fn parse_channel(&mut self, with_label: bool) -> ChannelDecl {
        let mut ch = ChannelDecl {
            field: String::new(),
            value_type: None,
            format: None,
            label: None,
        };
        if !self.expect_punct("{") {
            return ch;
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "field" => {
                    self.next();
                    ch.field = self.parse_path().unwrap_or_default();
                }
                "type" => {
                    self.next();
                    ch.value_type = self.expect_ident();
                }
                "format" => {
                    self.next();
                    ch.format = self.expect_ident();
                }
                "label" if with_label => {
                    self.next();
                    if self.peek().kind == TokKind::Str {
                        ch.label = Some(self.next().value);
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown channel entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        ch
    }

    fn parse_transform(&mut self) -> Vec<TransformDecl> {
        let mut ops = Vec::new();
        if !self.expect_punct("{") {
            return ops;
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "filter" => {
                    self.next();
                    ops.push(self.parse_filter());
                }
                "bucket" => {
                    self.next();
                    ops.push(self.parse_bucket());
                }
                "aggregate" => {
                    self.next();
                    ops.push(self.parse_aggregate());
                }
                "sort" => {
                    self.next();
                    ops.push(self.parse_sort());
                }
                "limit" => {
                    self.next();
                    let n = self.parse_number();
                    ops.push(TransformDecl::Limit {
                        n: n.max(0.0) as usize,
                    });
                }
                other => {
                    self.error_at(&tok, format!("unknown transform entry '{other}'"));
                    self.next();
                    if self.is_punct("{") {
                        self.next();
                        self.skip_to_block_end();
                    }
                }
            }
        }
        self.expect_punct("}");
        ops
    }

    /// `filter { field status eq "active" }`
    fn parse_filter(&mut self) -> TransformDecl {
        let mut field = String::new();
        let mut cmp = "eq".to_string();
        let mut value = String::new();
        if !self.expect_punct("{") {
            return TransformDecl::Filter { field, cmp, value };
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "field" => {
                    self.next();
                    field = self.parse_path().unwrap_or_default();
                }
                "eq" | "ne" | "gt" | "gte" | "lt" | "lte" | "in" => {
                    cmp = tok.value.clone();
                    self.next();
                    value = self.parse_literal();
                }
                "value" => {
                    self.next();
                    value = self.parse_literal();
                }
                other => {
                    self.error_at(&tok, format!("unknown filter entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        TransformDecl::Filter { field, cmp, value }
    }

    fn parse_literal(&mut self) -> String {
        match self.peek().kind {
            TokKind::Str | TokKind::Num | TokKind::Ident => self.next().value,
            _ => String::new(),
        }
    }

    /// `bucket { field createdAt by day }`
    fn parse_bucket(&mut self) -> TransformDecl {
        let mut field = String::new();
        let mut by = "day".to_string();
        if !self.expect_punct("{") {
            return TransformDecl::Bucket { field, by };
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "field" => {
                    self.next();
                    field = self.parse_path().unwrap_or_default();
                }
                "by" => {
                    self.next();
                    by = self.expect_ident().unwrap_or_else(|| "day".to_string());
                }
                other => {
                    self.error_at(&tok, format!("unknown bucket entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        TransformDecl::Bucket { field, by }
    }

    /// `aggregate { op sum field amount groupBy [createdAt] }`
    fn parse_aggregate(&mut self) -> TransformDecl {
        let mut op = "count".to_string();
        let mut field = None;
        let mut group_by = Vec::new();
        if !self.expect_punct("{") {
            return TransformDecl::Aggregate {
                op,
                field,
                group_by,
            };
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "op" => {
                    self.next();
                    op = self.expect_ident().unwrap_or_else(|| "count".to_string());
                }
                "field" => {
                    self.next();
                    field = self.parse_path();
                }
                "groupBy" | "group_by" => {
                    self.next();
                    group_by = self.parse_ident_list();
                }
                other => {
                    self.error_at(&tok, format!("unknown aggregate entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        TransformDecl::Aggregate {
            op,
            field,
            group_by,
        }
    }

    /// `sort { by createdAt desc }`
    fn parse_sort(&mut self) -> TransformDecl {
        let mut by = String::new();
        let mut desc = false;
        if !self.expect_punct("{") {
            return TransformDecl::Sort { by, desc };
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "by" => {
                    self.next();
                    by = self.parse_path().unwrap_or_default();
                }
                "desc" => {
                    self.next();
                    desc = true;
                }
                "asc" => {
                    self.next();
                    desc = false;
                }
                other => {
                    self.error_at(&tok, format!("unknown sort entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        TransformDecl::Sort { by, desc }
    }

    // ── page ──

    fn parse_page(&mut self) -> Option<PageDecl> {
        self.next(); // page
        let name = self.expect_ident()?;
        let mut decl = PageDecl {
            name,
            route: "/".into(),
            title: Vec::new(),
            body: PageBody::Table(TableCfg {
                query_api: String::new(),
                columns: Vec::new(),
                filters: Vec::new(),
                search: Vec::new(),
                actions: None,
            }),
        };
        if !self.expect_punct("{") {
            return Some(decl);
        }
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(";") {
                continue;
            }
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "route" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(r) = self.expect_string() {
                        decl.route = r;
                    }
                }
                "title" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.peek().kind == TokKind::Str {
                        if let Some(t) = self.expect_string() {
                            decl.title = vec![("zh".into(), t)];
                        }
                    } else {
                        decl.title = self.parse_message_block();
                    }
                }
                "table" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(t) = self.parse_table() {
                        decl.body = PageBody::Table(t);
                    }
                }
                "form" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(f) = self.parse_form() {
                        decl.body = PageBody::Form(f);
                    }
                }
                "detail" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(d) = self.parse_detail() {
                        decl.body = PageBody::Detail(d);
                    }
                }
                "chart" if tok.kind == TokKind::Ident => {
                    self.next();
                    if let Some(c) = self.expect_ident() {
                        decl.body = PageBody::Chart(ChartPageCfg { chart: c });
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown page entry '{other}'"));
                    self.next();
                    if self.is_punct("{") {
                        self.next();
                        self.skip_to_block_end();
                    }
                }
            }
        }
        self.expect_punct("}");
        Some(decl)
    }

    fn parse_table(&mut self) -> Option<TableCfg> {
        if !self.expect_punct("{") {
            return None;
        }
        let mut cfg = TableCfg {
            query_api: String::new(),
            columns: Vec::new(),
            filters: Vec::new(),
            search: Vec::new(),
            actions: None,
        };
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "query" if tok.kind == TokKind::Ident => {
                    self.next();
                    let Some(path) = self.parse_path() else { break };
                    cfg.query_api = path.split('.').next().unwrap_or(&path).to_string();
                }
                "columns" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("[") {
                        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
                            let Some(path) = self.expect_ident() else {
                                break;
                            };
                            let mut hint = None;
                            if self.eat_punct(":") {
                                hint = self.expect_ident();
                            }
                            cfg.columns.push(ColumnCfg { path, hint });
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                        self.expect_punct("]");
                    }
                }
                "filter" if tok.kind == TokKind::Ident => {
                    self.next();
                    let Some(path) = self.expect_ident() else {
                        break;
                    };
                    let mut widget = None;
                    if self.eat_kw("as") {
                        widget = self.expect_ident();
                    }
                    cfg.filters.push(FilterCfg { path, widget });
                }
                "search" if tok.kind == TokKind::Ident => {
                    self.next();
                    cfg.search = self.parse_ident_list();
                }
                "actions" if tok.kind == TokKind::Ident => {
                    self.next();
                    cfg.actions = self.parse_actions();
                }
                other => {
                    self.error_at(&tok, format!("unknown table entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        Some(cfg)
    }

    fn parse_actions(&mut self) -> Option<ActionsCfg> {
        if !self.expect_punct("{") {
            return None;
        }
        let mut actions = ActionsCfg {
            create: None,
            row: Vec::new(),
        };
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            if self.eat_punct(",") {
                continue;
            }
            let tok = self.peek().clone();
            if tok.value == "create" && tok.kind == TokKind::Ident {
                self.next();
                if self.peek().kind == TokKind::Arrow {
                    self.next();
                    actions.create = self.expect_ident();
                }
            } else if tok.value == "row" && tok.kind == TokKind::Ident {
                self.next();
                if self.eat_punct("[") {
                    while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
                        let Some(action) = self.expect_ident() else {
                            break;
                        };
                        if self.peek().kind == TokKind::Arrow {
                            self.next();
                        }
                        let Some(target) = self.parse_path() else {
                            break;
                        };
                        let confirm = self.eat_kw("confirm");
                        actions.row.push((
                            action,
                            if confirm {
                                format!("{target} confirm")
                            } else {
                                target
                            },
                        ));
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                    self.expect_punct("]");
                }
            } else {
                self.error_at(&tok, format!("unknown actions entry '{}'", tok.value));
                self.next();
            }
        }
        self.expect_punct("}");
        Some(actions)
    }

    fn parse_form(&mut self) -> Option<FormCfg> {
        let entity = self.expect_ident()?;
        if !self.expect_punct("{") {
            return None;
        }
        let mut cfg = FormCfg {
            entity,
            fields: Vec::new(),
            validation: Vec::new(),
            submits: Vec::new(),
        };
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "layout" if tok.kind == TokKind::Ident => {
                    self.next();
                    let _kind = self.expect_ident(); // stack | grid
                    if self.peek().kind == TokKind::Num {
                        self.next(); // grid 列数
                    }
                    if self.eat_punct("{") {
                        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                            if self.eat_punct(",") || self.eat_punct(";") {
                                continue;
                            }
                            let Some(path) = self.expect_ident() else {
                                break;
                            };
                            let mut widget = "Input".to_string();
                            if self.peek().kind == TokKind::Ident {
                                widget = self.next().value;
                                // widget 修饰词（Input email / Input money / Input datetime …）
                                const WIDGET_HINTS: [&str; 6] =
                                    ["email", "money", "datetime", "date", "number", "text"];
                                if self.peek().kind == TokKind::Ident {
                                    let t = self.peek().clone();
                                    if WIDGET_HINTS.contains(&t.value.as_str()) {
                                        widget = format!("{widget} {}", self.next().value);
                                    }
                                }
                            }
                            let mut from = None;
                            if self.eat_kw("from") {
                                from = self.expect_ident();
                            }
                            let mut required = false;
                            if self.is_at_attr("required") {
                                self.next();
                                self.next();
                                required = true;
                            }
                            cfg.fields.push(FormFieldCfg {
                                path,
                                widget,
                                from,
                                required,
                            });
                        }
                        self.expect_punct("}");
                    }
                }
                "validation" if tok.kind == TokKind::Ident => {
                    self.next();
                    cfg.validation = self.parse_ident_list();
                }
                "submit" if tok.kind == TokKind::Ident => {
                    self.next();
                    let is_create = self.eat_kw("create");
                    if self.peek().kind == TokKind::Arrow {
                        self.next();
                    }
                    let Some(api_op) = self.parse_path() else {
                        break;
                    };
                    let mut redirect = String::new();
                    if self.eat_kw("redirect") {
                        if let Some(r) = self.expect_string() {
                            redirect = r;
                        }
                    }
                    cfg.submits.push(SubmitCfg {
                        is_create,
                        api_op,
                        redirect,
                    });
                }
                other => {
                    self.error_at(&tok, format!("unknown form entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        Some(cfg)
    }

    fn parse_detail(&mut self) -> Option<DetailCfg> {
        let entity = self.expect_ident()?;
        if !self.expect_punct("{") {
            return None;
        }
        let mut cfg = DetailCfg {
            entity,
            sections: Vec::new(),
            children: Vec::new(),
        };
        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
            let tok = self.peek().clone();
            match tok.value.as_str() {
                "sections" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("{") {
                        while !self.is_punct("}") && self.peek().kind != TokKind::Eof {
                            let Some(section) = self.expect_ident() else {
                                break;
                            };
                            let fields = self.parse_hint_list();
                            cfg.sections.push((section, fields));
                        }
                        self.expect_punct("}");
                    }
                }
                "children" if tok.kind == TokKind::Ident => {
                    self.next();
                    if self.eat_punct("[") {
                        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
                            let Some(name) = self.expect_ident() else {
                                break;
                            };
                            if self.peek().kind == TokKind::Arrow {
                                self.next();
                            }
                            let Some(target) = self.expect_ident() else {
                                break;
                            };
                            cfg.children.push((name, target));
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                        self.expect_punct("]");
                    }
                }
                other => {
                    self.error_at(&tok, format!("unknown detail entry '{other}'"));
                    self.next();
                }
            }
        }
        self.expect_punct("}");
        Some(cfg)
    }

    /// `[a, b: hint, c]`
    fn parse_hint_list(&mut self) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        if !self.eat_punct("[") {
            return out;
        }
        while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
            let Some(path) = self.expect_ident() else {
                break;
            };
            let mut hint = None;
            if self.eat_punct(":") {
                hint = self.expect_ident();
            }
            out.push((path, hint));
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("]");
        out
    }

    // ── 表达式 ──

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut left = self.parse_and();
        while self.peek().kind == TokKind::Op2 && self.peek().value == "||" {
            self.next();
            let right = self.parse_and();
            left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(right));
        }
        left
    }

    fn parse_and(&mut self) -> Expr {
        let mut left = self.parse_cmp();
        while self.peek().kind == TokKind::Op2 && self.peek().value == "&&" {
            self.next();
            let right = self.parse_cmp();
            left = Expr::Bin(BinOp::And, Box::new(left), Box::new(right));
        }
        left
    }

    fn parse_cmp(&mut self) -> Expr {
        let left = self.parse_add();
        let op = match self.peek().kind {
            TokKind::Op2 => match self.peek().value.as_str() {
                "==" => Some(BinOp::Eq),
                "!=" => Some(BinOp::Ne),
                ">=" => Some(BinOp::Ge),
                "<=" => Some(BinOp::Le),
                _ => None,
            },
            TokKind::Punct => match self.peek().value.as_str() {
                ">" => Some(BinOp::Gt),
                "<" => Some(BinOp::Lt),
                _ => None,
            },
            TokKind::Ident if self.peek().value == "in" => Some(BinOp::In),
            _ => None,
        };
        if let Some(op) = op {
            self.next();
            let right = self.parse_add();
            Expr::Bin(op, Box::new(left), Box::new(right))
        } else {
            left
        }
    }

    fn parse_add(&mut self) -> Expr {
        let mut left = self.parse_mul();
        loop {
            let op = if self.is_punct("+") {
                BinOp::Add
            } else if self.is_punct("-") {
                BinOp::Sub
            } else {
                break;
            };
            self.next();
            let right = self.parse_mul();
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        left
    }

    fn parse_mul(&mut self) -> Expr {
        let mut left = self.parse_unary();
        loop {
            let op = if self.is_punct("*") {
                BinOp::Mul
            } else if self.is_punct("/") {
                BinOp::Div
            } else {
                break;
            };
            self.next();
            let right = self.parse_unary();
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        if self.is_punct("!") {
            self.next();
            return Expr::Not(Box::new(self.parse_unary()));
        }
        if self.is_punct("-") {
            self.next();
            let inner = self.parse_unary();
            return Expr::Bin(BinOp::Sub, Box::new(Expr::Num(0.0)), Box::new(inner));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Expr {
        let tok = self.peek().clone();
        match tok.kind {
            TokKind::Num => {
                self.next();
                Expr::Num(tok.value.parse().unwrap_or(0.0))
            }
            TokKind::Str => {
                self.next();
                Expr::Str(tok.value)
            }
            TokKind::Punct if tok.value == "(" => {
                self.next();
                let e = self.parse_expr();
                self.expect_punct(")");
                e
            }
            TokKind::Punct if tok.value == "[" => {
                self.next();
                let mut items = Vec::new();
                while !self.is_punct("]") && self.peek().kind != TokKind::Eof {
                    items.push(self.parse_expr());
                    if !self.eat_punct(",") {
                        break;
                    }
                }
                self.expect_punct("]");
                Expr::List(items)
            }
            TokKind::Ident => {
                match tok.value.as_str() {
                    "true" => {
                        self.next();
                        return Expr::Bool(true);
                    }
                    "false" => {
                        self.next();
                        return Expr::Bool(false);
                    }
                    "null" => {
                        self.next();
                        return Expr::Null;
                    }
                    _ => {}
                }
                // 路径 / old. 路径 / 函数调用
                let mut old = false;
                let mut parts = Vec::new();
                if tok.value == "old" {
                    self.next();
                    if self.eat_punct(".") {
                        old = true;
                        parts.push(self.expect_ident().unwrap_or_default());
                    } else {
                        return Expr::Bool(true);
                    }
                } else {
                    self.next();
                    parts.push(tok.value);
                }
                while self.is_punct(".") {
                    self.next();
                    if let Some(p) = self.expect_ident() {
                        parts.push(p);
                    }
                }
                if self.is_punct("(") {
                    self.next();
                    let mut args = Vec::new();
                    while !self.is_punct(")") && self.peek().kind != TokKind::Eof {
                        args.push(self.parse_expr());
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                    self.expect_punct(")");
                    let callee = parts.join(".");
                    return Expr::Call(callee, args);
                }
                Expr::Ref { path: parts, old }
            }
            _ => {
                self.error_at(&tok, format!("unexpected '{}' in expression", tok.value));
                self.next();
                Expr::Bool(true)
            }
        }
    }
}
