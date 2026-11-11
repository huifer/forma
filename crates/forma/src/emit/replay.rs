//! 迁移链重放模拟器：以真实 MySQL 语义模拟执行 Forma 生成的 SQL 子集，
//! 用于验证多版本迁移链的前向重放与当前 DSL schema 一致（无需真实数据库）。
//!
//! 支持 Forma 发出的全部语句形态：
//! - CREATE TABLE（列/PRIMARY KEY/UNIQUE KEY/KEY/CONSTRAINT FOREIGN KEY）
//! - DROP TABLE IF EXISTS
//! - ALTER TABLE ADD COLUMN / DROP COLUMN / MODIFY COLUMN
//! - ALTER TABLE ADD {UNIQUE KEY|INDEX} / DROP INDEX / ADD CONSTRAINT FOREIGN KEY / DROP FOREIGN KEY

use super::mysql::{key_name, ColumnPlan, FkPlan, TablePlan};
use anyhow::bail;

#[derive(Debug, Clone, PartialEq)]
pub struct SimColumn {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimTable {
    pub name: String,
    pub columns: Vec<SimColumn>,
    pub uniques: Vec<(String, Vec<String>)>,
    pub indexes: Vec<(String, Vec<String>)>,
    pub fks: Vec<(String, String, String)>, // (constraint, column, ref_table)
    pub primary_key: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimSchema {
    pub tables: Vec<SimTable>,
}

impl SimSchema {
    pub fn table(&self, name: &str) -> Option<&SimTable> {
        self.tables.iter().find(|t| t.name == name)
    }

    fn table_mut(&mut self, name: &str) -> anyhow::Result<&mut SimTable> {
        self.tables
            .iter_mut()
            .find(|t| t.name == name)
            .ok_or_else(|| anyhow::anyhow!("表 {name} 不存在（重放顺序错误或基线缺失）"))
    }

    fn upsert_table(&mut self, t: SimTable) {
        match self.tables.iter_mut().find(|x| x.name == t.name) {
            Some(slot) => *slot = t,
            None => self.tables.push(t),
        }
    }

    fn drop_table(&mut self, name: &str) {
        self.tables.retain(|t| t.name != name);
    }
}

/// 从 gen-mysql 的 TablePlan 派生等价的模拟 schema（用于与重放结果比对）。
pub fn schema_from_plans(plans: &[TablePlan]) -> SimSchema {
    SimSchema {
        tables: plans
            .iter()
            .map(|p| SimTable {
                name: p.table.clone(),
                columns: p
                    .columns
                    .iter()
                    .map(|c| SimColumn {
                        name: c.name.clone(),
                        sql_type: c.sql_type.clone(),
                        nullable: c.nullable,
                        // 列级 @unique 统一建模为命名唯一键（见 render_create_table），
                        // 与 ALTER 路径产出对齐
                        unique: false,
                    })
                    .collect(),
                uniques: p
                    .uniques
                    .iter()
                    .map(|cols| (key_name("uq", &p.table, cols), cols.clone()))
                    .chain(p.columns.iter().filter(|c| c.unique).map(|c| {
                        let cols = vec![c.name.clone()];
                        (key_name("uq", &p.table, &cols), cols)
                    }))
                    .collect(),
                indexes: {
                    let mut idx: Vec<(String, Vec<String>)> = p
                        .indexes
                        .iter()
                        .map(|cols| (key_name("idx", &p.table, cols), cols.clone()))
                        .collect();
                    // FULLTEXT（M10/P1）：ft_ 前缀名字进同一通道——diff 按名字对比，
                    // SQL 生成时按前缀改写为 FULLTEXT（见 migrate.rs）
                    idx.extend(
                        p.fulltext
                            .iter()
                            .map(|cols| (key_name("ft", &p.table, cols), cols.clone())),
                    );
                    idx
                },
                fks: p
                    .fks
                    .iter()
                    .map(|f: &FkPlan| {
                        (
                            format!("fk_{}_{}", p.table, f.column),
                            f.column.clone(),
                            f.ref_table.clone(),
                        )
                    })
                    .collect(),
                primary_key: p.primary_key.clone(),
            })
            .collect(),
    }
}

fn strip_comments(line: &str) -> String {
    let no_block = if let Some(pos) = line.find("/*") {
        line[..pos].to_string()
    } else {
        line.to_string()
    };
    // 去除 -- 注释（SQL 行注释）
    match no_block.find("--") {
        Some(pos) => no_block[..pos].to_string(),
        None => no_block,
    }
}

fn split_statements(sql: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut cur = String::new();
    for line in sql.lines() {
        let clean = strip_comments(line);
        cur.push_str(&clean);
        cur.push('\n');
        if clean.trim_end().ends_with(';') {
            stmts.push(cur.trim().to_string());
            cur.clear();
        }
    }
    let tail = cur.trim().to_string();
    if !tail.is_empty() {
        stmts.push(tail);
    }
    stmts.retain(|s| !s.trim().is_empty());
    stmts
}

/// 重放一段 SQL（多条语句）到模拟 schema；任何不支持的语句/结构冲突都会报错。
pub fn replay_sql(schema: &mut SimSchema, sql: &str) -> anyhow::Result<()> {
    for stmt in split_statements(sql) {
        let s = stmt.replace(';', "");
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(rest) = s.strip_prefix("CREATE TABLE ") {
            apply_create_table(schema, rest.trim())?;
        } else if let Some(rest) = s.strip_prefix("DROP TABLE IF EXISTS ") {
            let name = rest.trim();
            if schema.table(name).is_none() {
                bail!("DROP TABLE {name}: 表不存在");
            }
            schema.drop_table(name);
        } else if let Some(rest) = s.strip_prefix("ALTER TABLE ") {
            apply_alter(schema, rest.trim())?;
        } else {
            bail!("不支持的语句: {s}");
        }
    }
    Ok(())
}

fn apply_create_table(schema: &mut SimSchema, body: &str) -> anyhow::Result<()> {
    // "schools ( ... ) ENGINE=..." → 表名 + 列/约束行
    let Some(paren) = body.find('(') else {
        bail!("CREATE TABLE 语法不完整: {body}");
    };
    let name = body[..paren].trim().to_string();
    let inner = body[paren + 1..]
        .rsplit_once(')')
        .map(|(inner, _)| inner)
        .ok_or_else(|| anyhow::anyhow!("CREATE TABLE 括号不配对: {body}"))?;

    let mut t = SimTable {
        name: name.clone(),
        ..Default::default()
    };
    for raw in inner.lines() {
        let line = raw.trim().trim_end_matches(',').trim();
        if line.is_empty() {
            continue;
        }
        if let Some(cols) = line.strip_prefix("PRIMARY KEY (") {
            t.primary_key = cols.trim_end_matches(')').trim().to_string();
        } else if let Some(rest) = line.strip_prefix("UNIQUE KEY ") {
            let (n, cols) = parse_key_def(rest)?;
            t.uniques.push((n, cols));
        } else if let Some(rest) = line.strip_prefix("KEY ") {
            let (n, cols) = parse_key_def(rest)?;
            t.indexes.push((n, cols));
        } else if let Some(rest) = line.strip_prefix("FULLTEXT KEY ") {
            // FULLTEXT KEY ft_t_cols (cols) [WITH PARSER ngram] → 归入 ft_ 前缀索引通道
            //（与 plan→sim 转换一致；此前未识别，被当成名为 FULLTEXT 的幽灵列，
            //  导致 0002 生成 DROP COLUMN FULLTEXT + 重复 ADD FULLTEXT——真实迁移执行必炸）
            let rest = match rest.split_once(" WITH PARSER ") {
                Some((head, _)) => head,
                None => rest,
            };
            let (n, cols) = parse_key_def(rest)?;
            t.indexes.push((n, cols));
        } else if let Some(rest) = line.strip_prefix("CONSTRAINT ") {
            let rest = rest.strip_prefix("fk_").map(|_| rest).unwrap_or(rest);
            // CONSTRAINT fk_{t}_{col} FOREIGN KEY (col) REFERENCES ref (id)
            let parts: Vec<&str> = rest.splitn(3, char::is_whitespace).collect();
            let cname = parts.first().copied().unwrap_or_default().to_string();
            let Some(fkpart) = rest.split("FOREIGN KEY ").nth(1) else {
                bail!("无法解析约束: {line}");
            };
            let (col, refpart) = fkpart
                .split_once("REFERENCES ")
                .ok_or_else(|| anyhow::anyhow!("FOREIGN KEY 缺 REFERENCES: {line}"))?;
            let col = col.trim().trim_start_matches('(').trim_end_matches(')');
            let reftable = refpart.split_whitespace().next().unwrap_or_default();
            t.fks.push((cname, col.to_string(), reftable.to_string()));
        } else {
            // 列定义: name TYPE [NULL|NOT NULL] [UNIQUE]
            let mut it = line.split_whitespace();
            let cname = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("空列定义: {line}"))?;
            let sql_type = it
                .next()
                .ok_or_else(|| anyhow::anyhow!("列缺少类型: {line}"))?
                .to_string();
            let tail: Vec<&str> = it.collect();
            let nullable = !(tail.contains(&"NOT") && tail.contains(&"NULL"));
            let unique = tail.contains(&"UNIQUE");
            t.columns.push(SimColumn {
                name: cname.to_string(),
                sql_type,
                nullable,
                unique,
            });
        }
    }
    schema.upsert_table(t);
    Ok(())
}

/// "uq_t_a_b (a, b)" 或 "uq_t_a_b (a,b)" → (名字, [列])
fn parse_key_def(rest: &str) -> anyhow::Result<(String, Vec<String>)> {
    let Some(paren) = rest.find('(') else {
        bail!("索引定义缺少列清单: {rest}");
    };
    let name = rest[..paren].trim().to_string();
    let cols: Vec<String> = rest[paren + 1..]
        .trim_end_matches(')')
        .split(',')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    Ok((name, cols))
}

fn parse_column_def(rest: &str) -> anyhow::Result<(String, String, bool, bool)> {
    let mut it = rest.split_whitespace();
    let name = it
        .next()
        .ok_or_else(|| anyhow::anyhow!("ADD COLUMN 缺列名: {rest}"))?;
    let sql_type = it
        .next()
        .ok_or_else(|| anyhow::anyhow!("ADD COLUMN 缺类型: {rest}"))?
        .to_string();
    let tail: Vec<&str> = it.collect();
    let nullable = !tail.contains(&"NOT");
    let unique = tail.contains(&"UNIQUE");
    Ok((name.to_string(), sql_type, nullable, unique))
}

fn apply_alter(schema: &mut SimSchema, rest: &str) -> anyhow::Result<()> {
    let mut it = rest.splitn(2, char::is_whitespace);
    let table = it
        .next()
        .ok_or_else(|| anyhow::anyhow!("ALTER TABLE 缺表名: {rest}"))?
        .to_string();
    let action = it.next().unwrap_or_default().trim();

    if let Some(col) = action.strip_prefix("ADD COLUMN ") {
        let (name, sql_type, nullable, unique) = parse_column_def(col)?;
        let t = schema.table_mut(&table)?;
        if t.columns.iter().any(|c| c.name == name) {
            bail!("ALTER TABLE {table} ADD COLUMN {name}: 列已存在");
        }
        t.columns.push(SimColumn {
            name,
            sql_type,
            nullable,
            unique,
        });
    } else if let Some(col) = action.strip_prefix("DROP COLUMN ") {
        let name = col.trim();
        let t = schema.table_mut(&table)?;
        let before = t.columns.len();
        t.columns.retain(|c| c.name != name);
        if t.columns.len() == before {
            bail!("ALTER TABLE {table} DROP COLUMN {name}: 列不存在");
        }
    } else if let Some(col) = action.strip_prefix("MODIFY COLUMN ") {
        let (name, sql_type, nullable, unique) = parse_column_def(col)?;
        let t = schema.table_mut(&table)?;
        let Some(c) = t.columns.iter_mut().find(|c| c.name == name) else {
            bail!("ALTER TABLE {table} MODIFY COLUMN {name}: 列不存在");
        };
        c.sql_type = sql_type;
        c.nullable = nullable;
        c.unique = unique;
    } else if let Some(def) = action.strip_prefix("ADD UNIQUE KEY ") {
        let (name, cols) = parse_key_def(def)?;
        schema.table_mut(&table)?.uniques.push((name, cols));
    } else if let Some(def) = action.strip_prefix("ADD FULLTEXT INDEX ") {
        // FULLTEXT 索引：模拟器按普通索引登记（只关心名字去重）
        let def = def.strip_suffix(" WITH PARSER ngram").unwrap_or(def);
        let (name, cols) = parse_key_def(def)?;
        let t = schema.table_mut(&table)?;
        if t.uniques.iter().any(|(n, _)| *n == name) || t.indexes.iter().any(|(n, _)| *n == name) {
            bail!("ALTER TABLE {table} ADD FULLTEXT INDEX {name}: 索引重名（Duplicate key name）");
        }
        t.indexes.push((name, cols));
    } else if let Some(def) = action.strip_prefix("ADD INDEX ") {
        let (name, cols) = parse_key_def(def)?;
        let t = schema.table_mut(&table)?;
        if t.uniques.iter().any(|(n, _)| *n == name) || t.indexes.iter().any(|(n, _)| *n == name) {
            bail!("ALTER TABLE {table} ADD INDEX {name}: 索引重名（Duplicate key name）");
        }
        t.indexes.push((name, cols));
    } else if let Some(name) = action.strip_prefix("DROP INDEX ") {
        let name = name.trim();
        let t = schema.table_mut(&table)?;
        let before = t.indexes.len() + t.uniques.len();
        t.indexes.retain(|(n, _)| n != name);
        t.uniques.retain(|(n, _)| n != name);
        if t.indexes.len() + t.uniques.len() == before {
            bail!("ALTER TABLE {table} DROP INDEX {name}: 索引不存在");
        }
    } else if let Some(def) = action.strip_prefix("ADD CONSTRAINT ") {
        // fk_{t}_{col} FOREIGN KEY (col) REFERENCES ref (id)
        let mut parts = def.splitn(2, char::is_whitespace);
        let cname = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("ADD CONSTRAINT 缺名: {def}"))?
            .to_string();
        let body = parts.next().unwrap_or_default();
        let Some(fkpart) = body.split("FOREIGN KEY ").nth(1) else {
            bail!("ADD CONSTRAINT 语法不完整: {def}");
        };
        let (col, refpart) = fkpart
            .split_once("REFERENCES ")
            .ok_or_else(|| anyhow::anyhow!("FOREIGN KEY 缺 REFERENCES: {def}"))?;
        let col = col.trim().trim_start_matches('(').trim_end_matches(')');
        let reftable = refpart.split_whitespace().next().unwrap_or_default();
        schema
            .table_mut(&table)?
            .fks
            .push((cname, col.to_string(), reftable.to_string()));
    } else if let Some(name) = action.strip_prefix("DROP FOREIGN KEY ") {
        let name = name.trim();
        let t = schema.table_mut(&table)?;
        let before = t.fks.len();
        t.fks.retain(|(n, _, _)| n != name);
        if t.fks.len() == before {
            bail!("ALTER TABLE {table} DROP FOREIGN KEY {name}: 约束不存在");
        }
    } else {
        bail!("不支持的 ALTER TABLE 操作: {action}");
    }
    Ok(())
}

// ── schema 等价性比较（与 TablePlan 派生结果逐项对齐） ─────

fn sort_cols(v: &mut [(String, Vec<String>)]) {
    v.sort_by(|a, b| a.0.cmp(&b.0));
}

/// 重放结果与期望 schema 是否一致；不一致时返回差异列表。
pub fn schema_diff(actual: &SimSchema, expected: &SimSchema) -> Vec<String> {
    let mut diffs = Vec::new();
    for t in &expected.tables {
        let Some(a) = actual.table(&t.name) else {
            diffs.push(format!("缺少表 {}", t.name));
            continue;
        };
        for col in &t.columns {
            match a.columns.iter().find(|c| c.name == col.name) {
                None => diffs.push(format!("{}.{} 缺少列", t.name, col.name)),
                Some(ac) => {
                    if ac.sql_type != col.sql_type || ac.nullable != col.nullable {
                        diffs.push(format!(
                            "{}.{} 类型/可空不符: 期望 {}{}，实际 {}{}",
                            t.name,
                            col.name,
                            col.sql_type,
                            if col.nullable { " NULL" } else { " NOT NULL" },
                            ac.sql_type,
                            if ac.nullable { " NULL" } else { " NOT NULL" }
                        ));
                    }
                    if ac.unique != col.unique {
                        diffs.push(format!(
                            "{}.{} 内联 unique 不符: 期望 {}，实际 {}",
                            t.name, col.name, col.unique, ac.unique
                        ));
                    }
                }
            }
        }
        for c in &a.columns {
            if !t.columns.iter().any(|x| x.name == c.name) {
                diffs.push(format!("{}.{} 多余列", t.name, c.name));
            }
        }
        let mut exp_uq = t.uniques.clone();
        let mut act_uq = a.uniques.clone();
        sort_cols(&mut exp_uq);
        sort_cols(&mut act_uq);
        if exp_uq != act_uq {
            diffs.push(format!(
                "{} 唯一约束不符: 期望 {exp_uq:?}，实际 {act_uq:?}",
                t.name
            ));
        }
        let mut exp_idx = t.indexes.clone();
        let mut act_idx = a.indexes.clone();
        sort_cols(&mut exp_idx);
        sort_cols(&mut act_idx);
        if exp_idx != act_idx {
            diffs.push(format!(
                "{} 索引不符: 期望 {exp_idx:?}，实际 {act_idx:?}",
                t.name
            ));
        }
        let mut exp_fk = t.fks.clone();
        let mut act_fk = a.fks.clone();
        exp_fk.sort();
        act_fk.sort();
        if exp_fk != act_fk {
            diffs.push(format!(
                "{} 外键不符: 期望 {exp_fk:?}，实际 {act_fk:?}",
                t.name
            ));
        }
        if a.primary_key != t.primary_key {
            diffs.push(format!(
                "{} 主键不符: 期望 {}，实际 {}",
                t.name, t.primary_key, a.primary_key
            ));
        }
    }
    for t in &actual.tables {
        if expected.table(&t.name).is_none() {
            diffs.push(format!("多余表 {}", t.name));
        }
    }
    diffs
}

/// TablePlan 列 → SimColumn（供测试构造期望差异）。
pub fn plan_columns(p: &TablePlan) -> Vec<ColumnPlan> {
    p.columns.clone()
}
