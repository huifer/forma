//! Forma DSL 词法器：.fm 源码 → Token 流。

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Ident, // 标识符（关键字上下文判定）
    Str,   // 字符串（已解码）
    Num,   // 数字字面量
    Punct, // 单字符标点 = , : ; ( ) [ ] { } < > + - * / ! . @
    Arrow, // →
    Op2,   // 双字符运算符 == != >= <= && ||
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokKind,
    pub value: String,
    pub line: usize,
    pub col: usize,
}

pub fn lex(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;

    macro_rules! bump {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }

    while i < n {
        let c = chars[i];
        // whitespace
        if c.is_whitespace() {
            bump!();
            continue;
        }
        // line comment
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                bump!();
            }
            continue;
        }
        // block comment
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            bump!();
            bump!();
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                bump!();
            }
            if i < n {
                bump!();
            }
            if i < n {
                bump!();
            }
            continue;
        }
        let (start_line, start_col) = (line, col);
        // string
        if c == '"' {
            bump!();
            let mut value = String::new();
            while i < n && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < n {
                    bump!();
                    match chars[i] {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        other => value.push(other),
                    }
                    bump!();
                } else {
                    value.push(chars[i]);
                    bump!();
                }
            }
            if i < n {
                bump!(); // closing quote
            }
            tokens.push(Token {
                kind: TokKind::Str,
                value,
                line: start_line,
                col: start_col,
            });
            continue;
        }
        // number
        if c.is_ascii_digit() {
            let mut value = String::new();
            while i < n && (chars[i].is_ascii_digit() || chars[i] == '.') {
                value.push(chars[i]);
                bump!();
            }
            tokens.push(Token {
                kind: TokKind::Num,
                value,
                line: start_line,
                col: start_col,
            });
            continue;
        }
        // identifier
        if c.is_alphanumeric() && !c.is_ascii_digit() || c == '_' {
            let mut value = String::new();
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                value.push(chars[i]);
                bump!();
            }
            tokens.push(Token {
                kind: TokKind::Ident,
                value,
                line: start_line,
                col: start_col,
            });
            continue;
        }
        // arrow →
        if c == '→' {
            bump!();
            tokens.push(Token {
                kind: TokKind::Arrow,
                value: "→".into(),
                line: start_line,
                col: start_col,
            });
            continue;
        }
        // two-char operators
        if i + 1 < n {
            let two: String = chars[i..i + 2].iter().collect();
            if matches!(two.as_str(), "==" | "!=" | ">=" | "<=" | "&&" | "||") {
                bump!();
                bump!();
                tokens.push(Token {
                    kind: TokKind::Op2,
                    value: two,
                    line: start_line,
                    col: start_col,
                });
                continue;
            }
        }
        // single punct
        if "=,:;()[]{}<>+-*/!.@?".contains(c) {
            bump!();
            tokens.push(Token {
                kind: TokKind::Punct,
                value: c.to_string(),
                line: start_line,
                col: start_col,
            });
            continue;
        }
        // unknown char → skip with placeholder (diagnostics handled by parser)
        bump!();
    }

    tokens.push(Token {
        kind: TokKind::Eof,
        value: String::new(),
        line,
        col,
    });
    tokens
}
