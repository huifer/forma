//! SQLite DDL 子集解析器。
//! 覆盖：CREATE TABLE（列、类型、PRIMARY KEY / NOT NULL / UNIQUE [KEY name] / DEFAULT / CHECK /
//! KEY·INDEX / FULLTEXT KEY [WITH PARSER] /
//! REFERENCES、表级约束）、CREATE [UNIQUE] INDEX。

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Ident,
    Quoted,
    Str,
    Number,
    Punct,
    Keyword,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokKind,
    pub value: String,
    pub offset: usize,
}

const KEYWORDS: [&str; 56] = [
    "CREATE",
    "TABLE",
    "INDEX",
    "UNIQUE",
    "FULLTEXT",
    "IF",
    "NOT",
    "EXISTS",
    "ON",
    "PRIMARY",
    "KEY",
    "FOREIGN",
    "REFERENCES",
    "DEFAULT",
    "CHECK",
    "CONSTRAINT",
    "NULL",
    "AUTOINCREMENT",
    "ASC",
    "DESC",
    "COLLATE",
    "WHERE",
    "WITHOUT",
    "ROWID",
    "STRICT",
    "AND",
    "OR",
    "IN",
    "IS",
    "LIKE",
    "BETWEEN",
    "SELECT",
    "FROM",
    "AS",
    "CONFLICT",
    "ROLLBACK",
    "ABORT",
    "FAIL",
    "IGNORE",
    "REPLACE",
    "DEFERRABLE",
    "INITIALLY",
    "DEFERRED",
    "IMMEDIATE",
    "MATCH",
    "NO",
    "ACTION",
    "SET",
    "NULLS",
    "FIRST",
    "LAST",
    "GENERATED",
    "ALWAYS",
    "STORED",
    "VIRTUAL",
    "TO",
];

pub fn tokenize(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let n = bytes.len();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < n {
        let c = bytes[i] as char;
        // whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // line comment
        if c == '-' && i + 1 < n && bytes[i + 1] == b'-' {
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // block comment
        if c == '/' && i + 1 < n && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }
        // string literal
        if c == '\'' {
            let start = i;
            i += 1;
            let mut value = String::new();
            while i < n {
                if bytes[i] == b'\'' && i + 1 < n && bytes[i + 1] == b'\'' {
                    value.push('\'');
                    i += 2;
                } else if bytes[i] == b'\'' {
                    i += 1;
                    break;
                } else {
                    let ch_len = utf8_len(bytes[i]);
                    value.push_str(&sql[i..i + ch_len]);
                    i += ch_len;
                }
            }
            tokens.push(Token {
                kind: TokKind::Str,
                value,
                offset: start,
            });
            continue;
        }
        // quoted identifier
        if c == '"' || c == '`' || c == '[' {
            let start = i;
            let close = if c == '[' { ']' } else { c };
            i += 1;
            let mut value = String::new();
            while i < n && bytes[i] as char != close {
                let ch_len = utf8_len(bytes[i]);
                value.push_str(&sql[i..i + ch_len]);
                i += ch_len;
            }
            i = (i + 1).min(n);
            tokens.push(Token {
                kind: TokKind::Quoted,
                value,
                offset: start,
            });
            continue;
        }
        // number
        if c.is_ascii_digit()
            || (c == '.'
                && bytes
                    .get(i + 1)
                    .is_some_and(|b| (*b as char).is_ascii_digit()))
        {
            let start = i;
            while i < n {
                let ch = bytes[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '.' {
                    i += 1;
                } else if (ch == '+' || ch == '-') && i > start {
                    let prev = bytes[i - 1] as char;
                    if prev == 'e' || prev == 'E' {
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: TokKind::Number,
                value: sql[start..i].to_string(),
                offset: start,
            });
            continue;
        }
        // identifier / keyword
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < n {
                let ch = bytes[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &sql[start..i];
            let kind = if KEYWORDS.contains(&word.to_uppercase().as_str()) {
                TokKind::Keyword
            } else {
                TokKind::Ident
            };
            tokens.push(Token {
                kind,
                value: word.to_string(),
                offset: start,
            });
            continue;
        }
        // punctuation & operators（DEFAULT 表达式内的运算符按单字符处理）
        tokens.push(Token {
            kind: TokKind::Punct,
            value: c.to_string(),
            offset: i,
        });
        i += 1;
    }
    tokens
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[derive(Debug, Clone)]
pub struct SqlColumn {
    pub name: String,
    /// 归一化大写基础类型，如 TEXT / INTEGER / DECIMAL
    pub base_type: String,
    /// 原样类型文本，如 DECIMAL(12, 2)
    pub type_text: String,
    pub not_null: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub auto_increment: bool,
    pub default_value: Option<String>,
    pub references: Option<(String, Option<String>)>,
    /// 列名 token 在源码中的偏移
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct SqlTableConstraint {
    pub kind: &'static str, // primary-key | unique | foreign-key | check
    pub columns: Vec<String>,
    pub references: Option<(String, Vec<String>)>,
}

#[derive(Debug, Clone)]
pub struct SqlTable {
    pub name: String,
    pub columns: Vec<SqlColumn>,
    pub constraints: Vec<SqlTableConstraint>,
    /// 列清单右括号在源码中的偏移（patch 插列定位用）
    pub close_paren_offset: usize,
    pub statement_start: usize,
}

#[derive(Debug, Clone)]
pub struct SqlIndex {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub statement_start: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedSql {
    pub tables: Vec<SqlTable>,
    pub indexes: Vec<SqlIndex>,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    tables: Vec<SqlTable>,
    indexes: Vec<SqlIndex>,
    pending_close: Option<usize>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn is_kw(&self, kw: &str) -> bool {
        self.peek()
            .is_some_and(|t| t.kind == TokKind::Keyword && t.value.to_uppercase() == kw)
    }

    fn is_punct(&self, p: &str) -> bool {
        self.peek()
            .is_some_and(|t| t.kind == TokKind::Punct && t.value == p)
    }

    fn expect_kw(&mut self, kw: &str) -> anyhow::Result<()> {
        let t = self.next();
        match t {
            Some(ref t) if t.kind == TokKind::Keyword && t.value.to_uppercase() == kw => Ok(()),
            other => Err(anyhow::anyhow!("expected {kw}, got {other:?}")),
        }
    }

    fn expect_punct(&mut self, p: &str) -> anyhow::Result<()> {
        let t = self.next();
        match t {
            Some(ref t) if t.kind == TokKind::Punct && t.value == p => Ok(()),
            other => Err(anyhow::anyhow!("expected \"{p}\", got {other:?}")),
        }
    }

    fn read_name(&mut self) -> anyhow::Result<(String, usize)> {
        let t = self.next();
        match t {
            Some(ref t) if matches!(t.kind, TokKind::Ident | TokKind::Quoted | TokKind::Str) => {
                Ok((t.value.clone(), t.offset))
            }
            other => Err(anyhow::anyhow!("expected identifier, got {other:?}")),
        }
    }

    fn skip_if_not_exists(&mut self) -> anyhow::Result<()> {
        if self.is_kw("IF") {
            self.next();
            self.expect_kw("NOT")?;
            self.expect_kw("EXISTS")?;
        }
        Ok(())
    }

    fn skip_balanced_parens(&mut self) -> anyhow::Result<()> {
        self.expect_punct("(")?;
        let mut depth = 1usize;
        while depth > 0 {
            let t = self.next();
            match t {
                None => anyhow::bail!("unexpected end of input inside parens"),
                Some(t) if t.kind == TokKind::Punct && t.value == "(" => depth += 1,
                Some(t) if t.kind == TokKind::Punct && t.value == ")" => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    fn skip_foreign_key_actions(&mut self) {
        while self.is_kw("ON") {
            self.next();
            if self.is_kw("DELETE") || self.is_kw("UPDATE") {
                self.next();
            }
            while self.is_kw("SET")
                || self.is_kw("NULL")
                || self.is_kw("CASCADE")
                || self.is_kw("RESTRICT")
                || self.is_kw("NO")
                || self.is_kw("ACTION")
                || self.is_kw("DEFAULT")
            {
                self.next();
            }
            if self.is_kw("MATCH") {
                self.next();
                let _ = self.read_name();
            }
            while self.is_kw("DEFERRABLE")
                || self.is_kw("NOT")
                || self.is_kw("INITIALLY")
                || self.is_kw("DEFERRED")
                || self.is_kw("IMMEDIATE")
            {
                self.next();
            }
        }
    }

    fn is_constraint_start(&self) -> bool {
        self.peek().is_some_and(|t| {
            t.kind == TokKind::Keyword
                && [
                    "PRIMARY",
                    "NOT",
                    "NULL",
                    "UNIQUE",
                    "DEFAULT",
                    "CHECK",
                    "REFERENCES",
                    "COLLATE",
                    "CONSTRAINT",
                ]
                .contains(&t.value.to_uppercase().as_str())
        })
    }

    /// 消费到约束关键字 / ',' / ')' 之前的类型 token，并返回规范类型文本。
    fn read_type_parts(&mut self) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        while let Some(t) = self.peek() {
            if (t.kind == TokKind::Punct && (t.value == "," || t.value == ")"))
                || self.is_constraint_start()
            {
                break;
            }
            let t = self.next().unwrap();
            parts.push(t.value.clone());
            if self.is_punct("(") {
                let mut depth = 0usize;
                while let Some(p) = self.next() {
                    parts.push(p.value.clone());
                    if p.kind == TokKind::Punct && p.value == "(" {
                        depth += 1;
                    } else if p.kind == TokKind::Punct && p.value == ")" {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }
        }
        parts
    }

    fn parse_column(
        &mut self,
        table_constraints: &mut Vec<SqlTableConstraint>,
    ) -> anyhow::Result<SqlColumn> {
        let (name, offset) = self.read_name()?;
        let type_parts = self.read_type_parts();
        let type_text = canonical_type_text(&type_parts);
        let base_type = type_parts
            .first()
            .map(|s| s.to_uppercase())
            .unwrap_or_else(|| "TEXT".to_string());

        let mut col = SqlColumn {
            name,
            base_type,
            type_text,
            not_null: false,
            primary_key: false,
            unique: false,
            auto_increment: false,
            default_value: None,
            references: None,
            offset,
        };

        loop {
            if self.peek().is_none() || self.is_punct(",") || self.is_punct(")") {
                break;
            }
            if self.is_kw("PRIMARY") {
                self.next();
                self.expect_kw("KEY")?;
                col.primary_key = true;
                if self.is_kw("ASC") || self.is_kw("DESC") {
                    self.next();
                }
                if self.is_kw("AUTOINCREMENT") {
                    self.next();
                    col.auto_increment = true;
                }
            } else if self.is_kw("NOT") {
                self.next();
                self.expect_kw("NULL")?;
                col.not_null = true;
            } else if self.is_kw("NULL") {
                self.next();
            } else if self.is_kw("UNIQUE") {
                self.next();
                col.unique = true;
            } else if self.is_kw("DEFAULT") {
                self.next();
                let mut parts: Vec<String> = Vec::new();
                while let Some(t) = self.peek() {
                    if (t.kind == TokKind::Punct && (t.value == "," || t.value == ")"))
                        || self.is_constraint_start()
                    {
                        break;
                    }
                    let d = self.next().unwrap();
                    parts.push(d.value.clone());
                    if self.is_punct("(") {
                        let mut depth = 0usize;
                        while let Some(p) = self.next() {
                            parts.push(p.value.clone());
                            if p.kind == TokKind::Punct && p.value == "(" {
                                depth += 1;
                            } else if p.kind == TokKind::Punct && p.value == ")" {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }
                col.default_value = Some(parts.join(" "));
            } else if self.is_kw("REFERENCES") {
                self.next();
                let (table, _) = self.read_name()?;
                let mut column = None;
                if self.is_punct("(") {
                    self.next();
                    column = Some(self.read_name()?.0);
                    self.expect_punct(")")?;
                }
                col.references = Some((table, column));
                self.skip_foreign_key_actions();
            } else if self.is_kw("CHECK") {
                self.next();
                self.skip_balanced_parens()?;
                table_constraints.push(SqlTableConstraint {
                    kind: "check",
                    columns: vec![col.name.clone()],
                    references: None,
                });
            } else if self.is_kw("COLLATE") {
                self.next();
                let _ = self.read_name();
            } else {
                // 未知尾部 token — 防御性跳过
                self.next();
            }
        }
        Ok(col)
    }

    fn parse_create(&mut self) -> anyhow::Result<()> {
        let statement_start = self.peek().unwrap().offset;
        self.next(); // CREATE

        let mut unique_index = false;
        if self.is_kw("UNIQUE") {
            self.next();
            unique_index = true;
        }

        if self.is_kw("TABLE") {
            self.next();
            self.skip_if_not_exists()?;
            let (mut table_name, _) = self.read_name()?;
            if self.is_punct(".") {
                self.next();
                table_name = self.read_name()?.0;
            }
            self.expect_punct("(")?;
            let mut columns: Vec<SqlColumn> = Vec::new();
            let mut constraints: Vec<SqlTableConstraint> = Vec::new();

            loop {
                if self.is_punct(")") {
                    let close_paren_offset = self.next().unwrap().offset;
                    // 右括号位置随 columns/constraints 一起保存
                    self.pending_close = Some(close_paren_offset);
                    break;
                }
                if self.is_kw("CONSTRAINT") {
                    self.next();
                    let _ = self.read_name()?;
                    continue;
                }
                if self.is_kw("PRIMARY") {
                    self.next();
                    self.expect_kw("KEY")?;
                    self.expect_punct("(")?;
                    let mut cols = Vec::new();
                    while !self.is_punct(")") {
                        cols.push(self.read_name()?.0);
                        if self.is_kw("ASC") || self.is_kw("DESC") {
                            self.next();
                        }
                        if self.is_punct(",") {
                            self.next();
                        }
                    }
                    self.next(); // )
                    for c in columns.iter_mut() {
                        if cols.contains(&c.name) {
                            c.primary_key = true;
                        }
                    }
                    constraints.push(SqlTableConstraint {
                        kind: "primary-key",
                        columns: cols,
                        references: None,
                    });
                } else if self.is_kw("UNIQUE") {
                    self.next();
                    // MySQL: `UNIQUE KEY name (cols)`；标准/SQLite: `UNIQUE (cols)`
                    if self.is_kw("KEY") {
                        self.next();
                        let _ = self.read_name()?;
                    }
                    self.expect_punct("(")?;
                    let mut cols = Vec::new();
                    while !self.is_punct(")") {
                        cols.push(self.read_name()?.0);
                        if self.is_punct(",") {
                            self.next();
                        }
                    }
                    self.next();
                    constraints.push(SqlTableConstraint {
                        kind: "unique",
                        columns: cols,
                        references: None,
                    });
                } else if self.is_kw("FOREIGN") {
                    self.next();
                    self.expect_kw("KEY")?;
                    self.expect_punct("(")?;
                    let mut cols = Vec::new();
                    while !self.is_punct(")") {
                        cols.push(self.read_name()?.0);
                        if self.is_punct(",") {
                            self.next();
                        }
                    }
                    self.next();
                    self.expect_kw("REFERENCES")?;
                    let (ref_table, _) = self.read_name()?;
                    let mut ref_cols = Vec::new();
                    if self.is_punct("(") {
                        self.next();
                        while !self.is_punct(")") {
                            ref_cols.push(self.read_name()?.0);
                            if self.is_punct(",") {
                                self.next();
                            }
                        }
                        self.next();
                    }
                    self.skip_foreign_key_actions();
                    constraints.push(SqlTableConstraint {
                        kind: "foreign-key",
                        columns: cols,
                        references: Some((ref_table, ref_cols)),
                    });
                } else if self.is_kw("KEY") || self.is_kw("INDEX") {
                    // MySQL 表内二级索引：`KEY name (cols)` / `INDEX name (cols)`
                    //（Forma 自产 DDL 每表都有 `KEY idx_*`；曾导致 import sql 硬错误）
                    self.next();
                    let _ = self.read_name()?;
                    self.expect_punct("(")?;
                    let mut cols = Vec::new();
                    while !self.is_punct(")") {
                        cols.push(self.read_name()?.0);
                        if self.is_punct(",") {
                            self.next();
                        }
                    }
                    self.next(); // )
                    constraints.push(SqlTableConstraint {
                        kind: "index",
                        columns: cols,
                        references: None,
                    });
                } else if self.is_kw("FULLTEXT") {
                    // MySQL: `FULLTEXT KEY name (cols) [WITH PARSER parser]` → searchable
                    self.next();
                    self.expect_kw("KEY")?;
                    let _ = self.read_name()?;
                    self.expect_punct("(")?;
                    let mut cols = Vec::new();
                    while !self.is_punct(")") {
                        cols.push(self.read_name()?.0);
                        if self.is_punct(",") {
                            self.next();
                        }
                    }
                    self.next(); // )
                                 // 消费 WITH PARSER ngram 等尾部选项，直到逗号 / 右括号
                    while let Some(t) = self.peek() {
                        if t.kind == TokKind::Punct && (t.value == "," || t.value == ")") {
                            break;
                        }
                        self.next();
                    }
                    constraints.push(SqlTableConstraint {
                        kind: "fulltext",
                        columns: cols,
                        references: None,
                    });
                } else if self.is_kw("CHECK") {
                    self.next();
                    self.skip_balanced_parens()?;
                    constraints.push(SqlTableConstraint {
                        kind: "check",
                        columns: vec![],
                        references: None,
                    });
                } else {
                    columns.push(self.parse_column(&mut constraints)?);
                }
                if self.is_punct(",") {
                    self.next();
                    continue;
                }
            }

            self.tables.push(SqlTable {
                name: table_name,
                columns,
                constraints,
                close_paren_offset: self.pending_close.take().unwrap_or(statement_start),
                statement_start,
            });
            return Ok(());
        }

        if self.is_kw("INDEX") {
            self.next();
            self.skip_if_not_exists()?;
            let (index_name, _) = self.read_name()?;
            self.expect_kw("ON")?;
            let (table_name, _) = self.read_name()?;
            self.expect_punct("(")?;
            let mut cols = Vec::new();
            while !self.is_punct(")") {
                cols.push(self.read_name()?.0);
                if self.is_kw("ASC") || self.is_kw("DESC") {
                    self.next();
                }
                if self.is_kw("COLLATE") {
                    self.next();
                    let _ = self.read_name()?;
                }
                if self.is_punct(",") {
                    self.next();
                }
            }
            self.next(); // )
            while self
                .peek()
                .is_some_and(|t| !(t.kind == TokKind::Punct && t.value == ";"))
            {
                self.next();
            }
            self.indexes.push(SqlIndex {
                name: index_name,
                table: table_name,
                columns: cols,
                unique: unique_index,
                statement_start,
            });
            return Ok(());
        }

        // 不支持的 CREATE（VIEW/TRIGGER/…）— 跳到 ';'
        while self
            .peek()
            .is_some_and(|t| !(t.kind == TokKind::Punct && t.value == ";"))
        {
            self.next();
        }
        Ok(())
    }
}

pub fn parse_sql_ddl(sql: &str) -> anyhow::Result<ParsedSql> {
    let mut parser = Parser {
        tokens: tokenize(sql),
        pos: 0,
        tables: Vec::new(),
        indexes: Vec::new(),
        pending_close: None,
    };
    while parser.pos < parser.tokens.len() {
        if parser.is_punct(";") {
            parser.next();
            continue;
        }
        if !parser.is_kw("CREATE") {
            // 容忍语句之间的游离 token
            parser.next();
            continue;
        }
        parser.parse_create()?;
    }
    Ok(ParsedSql {
        tables: parser.tables,
        indexes: parser.indexes,
    })
}

fn canonical_type_text(parts: &[String]) -> String {
    let joined = parts.join(" ");
    // 1) 去掉括号/逗号前的空格
    let mut s = String::new();
    for c in joined.chars() {
        if c == '(' || c == ')' || c == ',' {
            while s.ends_with(' ') {
                s.pop();
            }
            s.push(c);
        } else if c == ' ' {
            if !s.is_empty() && !s.ends_with(' ') && !s.ends_with('(') && !s.ends_with(',') {
                s.push(' ');
            }
        } else {
            s.push(c);
        }
    }
    // 2) 逗号后规范化为恰好一个空格
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == ',' {
            while chars.peek() == Some(&' ') {
                chars.next();
            }
            if chars.peek().is_some() && chars.peek() != Some(&')') {
                out.push(' ');
            }
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

use crate::adapters::{AdapterContext, AdapterResult, SpecAdapter};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, SourceRef, Standard};
use crate::identity::{local_uri, normalize_name};

pub struct SqlAdapter;

/// SQL 类型 ↔ JSON Schema 类型兼容表（校验共用）。
pub fn sql_type_compatible(sql_type: &str, json_type: &str) -> bool {
    let compatible: &[&str] = match sql_type {
        "TEXT" | "VARCHAR" | "CHAR" | "CLOB" => &["string"],
        "INTEGER" | "INT" | "BIGINT" | "SMALLINT" => &["integer", "number"],
        "REAL" | "FLOAT" | "DOUBLE" | "DECIMAL" | "NUMERIC" => &["number"],
        "BOOLEAN" | "BOOL" => &["boolean"],
        "TIMESTAMP" | "DATETIME" | "DATE" | "TIME" => &["string"],
        "BLOB" => &["string"],
        "JSON" => &["object", "string", "array"],
        _ => return true,
    };
    compatible.contains(&json_type)
}

fn column_json(c: &SqlColumn, line: usize) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "name": c.name,
        "type": c.base_type,
        "typeText": c.type_text,
        "notNull": c.not_null,
        "primaryKey": c.primary_key,
        "unique": c.unique,
        "line": line,
    });
    let map = obj.as_object_mut().unwrap();
    if let Some(d) = &c.default_value {
        map.insert("defaultValue".into(), serde_json::json!(d));
    }
    if let Some((t, col)) = &c.references {
        map.insert(
            "references".into(),
            serde_json::json!({"table": t, "column": col}),
        );
    }
    obj
}

fn table_json(t: &SqlTable, entity_uri: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "name": t.name,
        "entityUri": entity_uri,
        "columns": [], // 由列节点承载；表格节点保存概要
        "constraintKinds": t.constraints.iter().map(|c| c.kind).collect::<Vec<_>>(),
    })
}

impl SpecAdapter for SqlAdapter {
    fn standard(&self) -> Standard {
        Standard::Sql
    }

    fn parse(&self, ctx: &AdapterContext<'_>) -> anyhow::Result<AdapterResult> {
        let sql = crate::util::read_text(&ctx.absolute_path)?;
        let parsed = parse_sql_ddl(&sql)?;
        let mut result = AdapterResult::default();

        // binding 表（manifest.bindings）
        let mut binding_by_table: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for b in &ctx.loaded.manifest.bindings {
            binding_by_table.insert(normalize_name(&b.table), b.uri.clone());
        }

        for table in &parsed.tables {
            let table_uri = local_uri("sql", &ctx.file, "table", &table.name);
            let entity = binding_by_table.get(&normalize_name(&table.name)).cloned();

            result.nodes.push(SemanticNode {
                uri: table_uri.clone(),
                kind: NodeKind::Table,
                name: table.name.clone(),
                standard: Standard::Sql,
                source: SourceRef {
                    file: ctx.file.clone(),
                    line: Some(crate::util::line_of(&sql, table.statement_start)),
                },
                data: {
                    let mut v = table_json(table, entity.as_deref());
                    // 表内列概要（doctor 用）
                    if let Some(map) = v.as_object_mut() {
                        map.insert(
                            "columns".into(),
                            serde_json::Value::Array(
                                table
                                    .columns
                                    .iter()
                                    .map(|c| column_json(c, crate::util::line_of(&sql, c.offset)))
                                    .collect(),
                            ),
                        );
                    }
                    v
                },
            });

            for col in &table.columns {
                let col_uri = local_uri(
                    "sql",
                    &ctx.file,
                    "column",
                    &format!("{}.{}", table.name, col.name),
                );
                result.nodes.push(SemanticNode {
                    uri: col_uri.clone(),
                    kind: NodeKind::TableColumn,
                    name: format!("{}.{}", table.name, col.name),
                    standard: Standard::Sql,
                    source: SourceRef {
                        file: ctx.file.clone(),
                        line: Some(crate::util::line_of(&sql, col.offset)),
                    },
                    data: column_json(col, crate::util::line_of(&sql, col.offset)),
                });
                result.edges.push(SemanticEdge {
                    from: table_uri.clone(),
                    to: col_uri.clone(),
                    kind: EdgeKind::BelongsTo,
                });
                if let Some(e) = &entity {
                    result.edges.push(SemanticEdge {
                        from: e.clone(),
                        to: col_uri,
                        kind: EdgeKind::Defines,
                    });
                }
            }

            if let Some(e) = &entity {
                result.edges.push(SemanticEdge {
                    from: e.clone(),
                    to: table_uri.clone(),
                    kind: EdgeKind::Defines,
                });
            }

            for col in &table.columns {
                if let Some((ref_table, _)) = &col.references {
                    if let Some(target) = binding_by_table.get(&normalize_name(ref_table)) {
                        result.edges.push(SemanticEdge {
                            from: table_uri.clone(),
                            to: target.clone(),
                            kind: EdgeKind::References,
                        });
                    }
                }
            }
        }

        for index in &parsed.indexes {
            let idx_uri = local_uri("sql", &ctx.file, "index", &index.name);
            result.nodes.push(SemanticNode {
                uri: idx_uri.clone(),
                kind: NodeKind::Index,
                name: index.name.clone(),
                standard: Standard::Sql,
                source: SourceRef {
                    file: ctx.file.clone(),
                    line: Some(crate::util::line_of(&sql, index.statement_start)),
                },
                data: serde_json::json!({
                    "name": index.name,
                    "table": index.table,
                    "columns": index.columns,
                    "unique": index.unique,
                }),
            });
            let table_uri = local_uri("sql", &ctx.file, "table", &index.table);
            if result.nodes.iter().any(|n| n.uri == table_uri) {
                result.edges.push(SemanticEdge {
                    from: table_uri,
                    to: idx_uri,
                    kind: EdgeKind::BelongsTo,
                });
            }
        }

        Ok(result)
    }
}
