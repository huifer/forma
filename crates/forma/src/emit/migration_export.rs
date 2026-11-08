//! 迁移链导出到既有迁移体系（roadmap M9 / 轨道 A-8）。
//!
//! Forma 的规范迁移链是 `{db_out}/migrations/` 下的
//! `0001_init.sql` + `NNNN_upgrade.sql` + `NNNN_rollback.sql`。
//! 本模块把它**包装**成三种主流迁移工具的目录约定，SQL 正文逐字复制
//! （不改语义、不重排语句），仅决定文件名 / 清单结构：
//!
//! | format | 产物 |
//! |---|---|
//! | `flyway` | `V0001__init.sql`、`V0002__upgrade.sql` …（Flyway 社区版无 down） |
//! | `golang-migrate` | `000001_init.up.sql` / `.down.sql` 成对；基线的 down 由 CREATE TABLE 反推 DROP |
//! | `liquibase` | `db.changelog.xml` + `changesets/NNNN_*.sql` |
//!
//! 零副作用：只写 `--out` 目录，不修改 Forma 自身的迁移台账与链。

use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationFormat {
    Flyway,
    GolangMigrate,
    Liquibase,
}

impl MigrationFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "flyway" => Ok(Self::Flyway),
            "golang-migrate" | "golangmigrate" | "go-migrate" => Ok(Self::GolangMigrate),
            "liquibase" => Ok(Self::Liquibase),
            _ => bail!("unknown migration format '{s}' (expected flyway|golang-migrate|liquibase)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flyway => "flyway",
            Self::GolangMigrate => "golang-migrate",
            Self::Liquibase => "liquibase",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExportFile {
    pub path: String,
    pub content: String,
}

/// 基线 / 升级链的条目（按版本号升序）。
#[derive(Debug, Clone)]
struct ChainEntry {
    version: u32,
    name: &'static str, // init | upgrade
    up: String,
    down: Option<String>,
}

fn read_chain(migrations_dir: &Path) -> Result<Vec<ChainEntry>> {
    if !migrations_dir.is_dir() {
        bail!(
            "no migrations directory at {} — run `forma build` first",
            migrations_dir.display()
        );
    }
    let mut entries: Vec<ChainEntry> = Vec::new();
    // 基线
    let init = migrations_dir.join("0001_init.sql");
    if init.is_file() {
        entries.push(ChainEntry {
            version: 1,
            name: "init",
            up: fs::read_to_string(&init)?,
            down: None,
        });
    }
    // 升级链：从文件名解析版本号（避免依赖台账运行时状态）
    let mut upgrades: Vec<(u32, std::path::PathBuf)> = Vec::new();
    for e in fs::read_dir(migrations_dir)? {
        let p = e?.path();
        let Some(fname) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = fname.strip_suffix("_upgrade.sql") else {
            continue;
        };
        if let Ok(v) = stem.parse::<u32>() {
            upgrades.push((v, p));
        }
    }
    upgrades.sort_by_key(|(v, _)| *v);
    for (v, p) in upgrades {
        let rollback = migrations_dir.join(format!("{:04}_rollback.sql", v));
        entries.push(ChainEntry {
            version: v,
            name: "upgrade",
            up: fs::read_to_string(&p)?,
            down: if rollback.is_file() {
                Some(fs::read_to_string(&rollback)?)
            } else {
                None
            },
        });
    }
    if entries.is_empty() {
        bail!("no migration SQL found in {}", migrations_dir.display());
    }
    Ok(entries)
}

/// 从 CREATE TABLE 语句反推 DROP TABLE（golang-migrate 基线 down 用）。
/// 只处理 `CREATE TABLE [IF NOT EXISTS] <name>` 的常规形态，逆序输出。
fn baseline_down(init_sql: &str) -> String {
    let mut tables = Vec::new();
    for line in init_sql.lines() {
        let t = line.trim().to_uppercase();
        if !t.starts_with("CREATE TABLE") {
            continue;
        }
        let rest = line.trim();
        let mut words = rest.split_whitespace();
        words.next(); // CREATE
        words.next(); // TABLE
        let mut token = words
            .next()
            .unwrap_or("")
            .trim_matches(|c: char| c == '`' || c == '"' || c == '(' || c == ';');
        if token.eq_ignore_ascii_case("IF") {
            // IF NOT EXISTS <name>
            words.next(); // NOT
            words.next(); // EXISTS
            token = words
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| c == '`' || c == '"' || c == '(' || c == ';');
        }
        if !token.is_empty() {
            tables.push(
                token
                    .trim_matches(|c: char| c == '`' || c == '"')
                    .to_string(),
            );
        }
    }
    let mut out =
        String::from("-- Forma baseline rollback (auto-derived from CREATE TABLE order)\n");
    out.push_str("SET FOREIGN_KEY_CHECKS = 0;\n");
    for t in tables.iter().rev() {
        out.push_str(&format!("DROP TABLE IF EXISTS `{t}`;\n"));
    }
    out.push_str("SET FOREIGN_KEY_CHECKS = 1;\n");
    out
}

/// 生成导出文件清单（纯函数：调用方负责写盘）。
pub fn export_files(migrations_dir: &Path, format: MigrationFormat) -> Result<Vec<ExportFile>> {
    let chain = read_chain(migrations_dir)?;
    let mut files: Vec<ExportFile> = Vec::new();
    match format {
        MigrationFormat::Flyway => {
            for e in &chain {
                files.push(ExportFile {
                    path: format!("V{:04}__{}.sql", e.version, e.name),
                    content: e.up.clone(),
                });
            }
            files.push(ExportFile {
                path: "README.md".into(),
                content: String::from(
                    "# Flyway migration set (exported from Forma)\n\n\
                     SQL bodies are copied verbatim from Forma's versioned chain.\n\
                     Flyway Community has no `undo`; use Forma rollback SQL under\n\
                     `forma migrate down` / the source `NNNN_rollback.sql` files instead.\n",
                ),
            });
        }
        MigrationFormat::GolangMigrate => {
            for e in &chain {
                let up_name = format!("{:06}_{}.up.sql", e.version, e.name);
                let down_name = format!("{:06}_{}.down.sql", e.version, e.name);
                files.push(ExportFile {
                    path: up_name,
                    content: e.up.clone(),
                });
                let down = e.down.clone().unwrap_or_else(|| {
                    if e.name == "init" {
                        baseline_down(&e.up)
                    } else {
                        format!(
                            "-- No rollback SQL found for version {}; resolve manually.\n",
                            e.version
                        )
                    }
                });
                files.push(ExportFile {
                    path: down_name,
                    content: down,
                });
            }
        }
        MigrationFormat::Liquibase => {
            let mut xml = String::from(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <databaseChangeLog\n  xmlns=\"http://www.liquibase.org/xml/ns/dbchangelog\"\n  \
                 xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n  \
                 xsi:schemaLocation=\"http://www.liquibase.org/xml/ns/dbchangelog \
                 http://www.liquibase.org/xml/ns/dbchangelog/dbchangelog-4.1.xsd\">\n\n",
            );
            for e in &chain {
                let rel = format!("changesets/{:04}_{}.sql", e.version, e.name);
                files.push(ExportFile {
                    path: rel.clone(),
                    content: e.up.clone(),
                });
                xml.push_str(&format!(
                    "  <changeSet id=\"forma-{:04}-{}\" author=\"forma\">\n    \
                     <sqlFile path=\"{rel}\" relativeToChangelogFile=\"true\"/>\n  \
                     </changeSet>\n",
                    e.version, e.name
                ));
            }
            xml.push_str("\n</databaseChangeLog>\n");
            files.push(ExportFile {
                path: "db.changelog.xml".into(),
                content: xml,
            });
        }
    }
    Ok(files)
}

/// 写盘（目录不存在则创建；同名覆盖为确定性重建）。
pub fn export_to(
    migrations_dir: &Path,
    out_dir: &Path,
    format: MigrationFormat,
) -> Result<Vec<ExportFile>> {
    let files = export_files(migrations_dir, format)?;
    fs::create_dir_all(out_dir)?;
    for f in &files {
        let dest = out_dir.join(&f.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &f.content)?;
    }
    Ok(files)
}
