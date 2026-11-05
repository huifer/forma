//! forma CLI — Specification Federation 的命令行入口。

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::exit;

const HELP: &str = "forma — Universal Specification Compiler

Usage:
  forma init [dir]           Scaffold a new Forma project
  forma validate [dir]       Validate manifest + specifications
  forma doctor [dir]         Full cross-standard diagnostics
  forma graph [dir]          Print semantic graph (--json | --export <file>)
  forma build [dir]          Incremental build (--force)
  forma seed [dir]           生成确定性案例数据（db/seed/*.sql + fixtures/*.json）
  forma migrate [dir]        数据库迁移：status/history/verify/up/down/risk [--sql] [--apply] [--to v] [--steps N]
  forma patch [dir] <file>   Apply a semantic patch (transactional)
  forma docs validate [dir]   Governance docs: validate | lint | graph | check-compatibility [--base <git-ref>] | check-dependencies | check-links
  forma capability <action>   Capability registry: list | check | show <id> [dir]
  forma mcp [dir]            Start MCP stdio server (for Codex / Claude Code / Pi)
  forma --help | --version
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = run(&args).unwrap_or_else(|err| {
        eprintln!("fatal: {err:#}");
        1
    });
    exit(code);
}

fn run(argv: &[String]) -> Result<i32> {
    let command = argv.first().map(String::as_str).unwrap_or("help");
    let mut positional: Vec<String> = Vec::new();
    let mut flags: std::collections::HashMap<String, String> = Default::default();
    let mut i = 1;
    while i < argv.len() {
        let arg = &argv[i];
        if let Some(name) = arg.strip_prefix("--") {
            if let Some(eq) = name.find('=') {
                flags.insert(name[..eq].to_string(), name[eq + 1..].to_string());
            } else if ["export", "out", "json-out", "to", "steps", "base"].contains(&name) {
                // 只有当下一个 token 存在且不是另一个 flag 时，才把它当作本 flag 的参数；
                // 否则按布尔开关处理（如 `forma graph --json`）。
                match argv.get(i + 1) {
                    Some(v) if !v.starts_with("--") => {
                        flags.insert(name.to_string(), v.clone());
                        i += 1;
                    }
                    _ => {
                        flags.insert(name.to_string(), String::new());
                    }
                }
            } else {
                flags.insert(name.to_string(), String::new());
            }
        } else {
            positional.push(arg.clone());
        }
        i += 1;
    }

    let project_dir = |idx: usize| -> PathBuf {
        PathBuf::from(positional.get(idx).map(String::as_str).unwrap_or("."))
    };

    match command {
        "check" => Ok(cmd_check(project_dir(0))),
        "lint" => {
            let dir = project_dir(0);
            let manifest = forma::manifest::load_manifest(&dir)?;
            let source = manifest.manifest.source.clone().unwrap_or_default();
            let (program, diagnostics) = forma::dsl::load_program(&dir, &source)?;
            let parse_errors = diagnostics.iter().filter(|d| d.severity == "error").count();
            if parse_errors > 0 {
                for d in diagnostics.iter().filter(|d| d.severity == "error") {
                    eprintln!("{}", forma::validate::format_diagnostic(d));
                }
                eprintln!("✗ fix DSL errors before linting");
                return Ok(1);
            }
            let lint_diags = forma::lint::run_lint(&program, None);
            let by_sev = |s: &str| lint_diags.iter().filter(|d| d.severity == s).count();
            for d in &lint_diags {
                let entity = d.entity.as_deref().unwrap_or("");
                let field = d
                    .field
                    .as_deref()
                    .map(|f| format!(" .{f}"))
                    .unwrap_or_default();
                println!(
                    "[{: >7}] {: <44} {}{}  ── {}",
                    d.severity, d.rule, entity, field, d.message
                );
                if let Some(h) = &d.fix_hint {
                    println!("           └─ 💡 {h}");
                }
            }
            println!(
                "\n{} errors, {} warnings, {} info — {} rule(s) checked",
                by_sev("error"),
                by_sev("warning"),
                by_sev("info"),
                forma::lint::builtin_rules().len()
            );
            Ok(if by_sev("error") > 0 { 1 } else { 0 })
        }
        "seed" => {
            let dir = project_dir(0);
            let manifest = forma::manifest::load_manifest(&dir)?;
            let source = manifest.manifest.source.clone().unwrap_or_default();
            let (program, diagnostics) = forma::dsl::load_program(&dir, &source)?;
            let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
            if errors > 0 {
                for d in diagnostics.iter().filter(|d| d.severity == "error") {
                    eprintln!("{}", forma::validate::format_diagnostic(d));
                }
                return Ok(1);
            }
            let db_out = manifest
                .manifest
                .targets
                .database
                .as_ref()
                .and_then(|t| t.out.clone())
                .unwrap_or_else(|| "generated/db".into());
            // --verify：确定性校验——磁盘种子必须与 DSL 纯函数推导 byte 级一致
            if flags.contains_key("verify") {
                let report = forma::emit::verify_seed(&dir, &program, &format!("{db_out}/seed"))?;
                if report.ok {
                    println!(
                        "✓ seed verify ok — v{} · SHA-256 {}…",
                        report.version,
                        &report.sql_hash[..16]
                    );
                    return Ok(0);
                }
                for issue in &report.issues {
                    eprintln!("  ✗ {issue}");
                }
                eprintln!(
                    "✗ seed verify failed — {} issue(s)；重建方式：forma seed {}",
                    report.issues.len(),
                    dir.display()
                );
                return Ok(1);
            }
            let files = forma::emit::emit_seed(&dir, &program, &format!("{db_out}/seed"))?;
            for f in &files {
                println!("  + {} ({})", f.path, f.generator);
            }
            println!(
                "✓ seed ok — 应用方式：mysql ... < db/seed/seed_*.sql（INSERT IGNORE，幂等可重放）"
            );
            Ok(0)
        }
        "diagrams" => {
            let dir = project_dir(0);
            let manifest = forma::manifest::load_manifest(&dir)?;
            let source = manifest.manifest.source.clone().unwrap_or_default();
            let (program, diagnostics) = forma::dsl::load_program(&dir, &source)?;
            let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
            if errors > 0 {
                for d in diagnostics.iter().filter(|d| d.severity == "error") {
                    eprintln!("{}", forma::validate::format_diagnostic(d));
                }
                return Ok(1);
            }
            let files = forma::emit::emit_diagrams(&dir, &program, "generated/diagrams")?;
            for f in &files {
                println!("  + {} ({})", f.path, f.generator);
            }
            println!(
                "✓ diagrams ok — 打开 generated/diagrams/diagrams.html 查看 ER/数据流/业务流程图"
            );
            Ok(0)
        }
        "diff" => {
            if positional.len() < 2 {
                eprintln!("usage: forma diff <dirA> <dirB> [--out report.html] [--json d.json]");
                return Ok(2);
            }
            Ok(cmd_diff(
                &project_dir(0),
                &PathBuf::from(positional[1].clone()),
                flags
                    .get("out")
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from),
                flags
                    .get("json-out")
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    // `--json` 是全局布尔开关；diff 保持旧语法 `--json d.json`——
                    // 其后的路径落到 positional[2]。
                    .or_else(|| positional.get(2).map(PathBuf::from)),
            ))
        }
        "init" => {
            forma::scaffold::scaffold_project(&project_dir(0))?;
            println!("✓ Forma project scaffolded at {}", project_dir(0).display());
            println!("  next: forma validate && forma build");
            Ok(0)
        }
        "validate" => Ok(cmd_validate(&project_dir(0))),
        "doctor" => Ok(cmd_doctor(&project_dir(0))),
        "graph" => {
            let dir = project_dir(0);
            // DSL 工程走模型摘要（与 MCP forma_graph 同源），联邦工程走语义图
            if forma::engine::is_dsl_project(&dir) {
                let check = forma::engine::dsl_check(&dir)?;
                let summary = forma::engine::dsl_model_summary(&check.program);
                if flags.contains_key("json") {
                    println!("{}", serde_json::to_string_pretty(&summary)?);
                } else {
                    println!("{}", render_dsl_model_text(&summary));
                }
                return Ok(if check.error_count() > 0 { 1 } else { 0 });
            }
            let outcome = forma::engine::load_project(&dir)?;
            let summary = outcome.graph.to_summary();
            if flags.contains_key("json") {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", render_graph_text(&summary));
            }
            if let Some(export) = flags.get("export") {
                let out = PathBuf::from(export);
                forma::util::write_text(&out, &serde_json::to_string_pretty(&summary)?)?;
                println!("\n✓ graph exported to {}", out.display());
            }
            Ok(0)
        }
        "build" => {
            // DSL 工程（manifest 带 source）→ 新编译管线；否则走联邦原型
            let dir = project_dir(0);
            let manifest = forma::manifest::load_manifest(&dir)?;
            if manifest.manifest.source.is_some() {
                return Ok(cmd_build_dsl(dir.clone(), flags.contains_key("force")));
            }
            let outcome = forma::engine::build_project(&forma::engine::BuildOptions {
                project_dir: dir,
                force: flags.contains_key("force"),
                entities: None,
                dry_run: false,
            })?;
            for file in &outcome.files {
                println!("  + {} ({})", file.path, file.generator);
            }
            if outcome.skipped > 0 {
                println!("  = {} file(s) up-to-date, skipped", outcome.skipped);
            }
            let errors = outcome
                .diagnostics
                .iter()
                .filter(|d| d.severity == "error")
                .count();
            for d in &outcome.diagnostics {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            if errors > 0 {
                eprintln!("✗ build failed with {errors} error(s)");
                return Ok(1);
            }
            println!(
                "✓ build ok — {} file(s) written, {} skipped",
                outcome.files.len(),
                outcome.skipped
            );
            Ok(0)
        }
        "migrate" => {
            // forma migrate [status|history|verify|up|down] [dir] [--sql] [--apply] [--to v] [--steps N]
            // 子命令与项目目录顺序无关：已知子命令取一个，其余第一个当作项目目录
            let subs = ["status", "history", "verify", "up", "down"];
            let mut sub = "status".to_string();
            let mut dir_arg: Option<String> = None;
            let mut sub_seen = false;
            for arg in &positional {
                if !sub_seen && subs.contains(&arg.as_str()) {
                    sub = arg.clone();
                    sub_seen = true;
                } else if dir_arg.is_none() {
                    dir_arg = Some(arg.clone());
                }
            }
            let dir = PathBuf::from(dir_arg.as_deref().unwrap_or("."));
            Ok(cmd_migrate(
                &dir,
                &sub,
                flags.contains_key("sql"),
                flags.contains_key("apply"),
                flags.get("to").map(String::as_str),
                flags
                    .get("steps")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1),
            ))
        }
        "docs" => Ok(cmd_docs(&positional, &flags)),
        "capability" => Ok(cmd_capability(&positional, &flags)),
        "patch" => {
            let Some(patch_file) = positional.get(1) else {
                eprintln!("usage: forma patch [dir] <patch.json>");
                return Ok(2);
            };
            let text = forma::util::read_text(&PathBuf::from(patch_file))?;
            let patch: forma::patch::PatchDocument = serde_json::from_str(&text)
                .map_err(|e| anyhow::anyhow!("cannot parse patch file: {e}"))?;
            let report = forma::patch::apply_patch(&project_dir(0), &patch)?;
            for op in &report.operations {
                println!("\n[{}] {} → {}", op.status, op.op, op.entity);
                for edit in &op.edits {
                    println!(
                        "  • {}: {} — {} ({})",
                        edit.standard, edit.action, edit.detail, edit.file
                    );
                }
                if let Some(m) = &op.message {
                    println!("  ! {m}");
                }
            }
            if let Some(build) = &report.build {
                println!("\nbuild: {} file(s) regenerated", build.files.len());
            }
            for d in &report.diagnostics {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            let failed = report.operations.iter().any(|o| o.status == "failed");
            println!(
                "{}",
                if failed {
                    "\n✗ patch completed with failures"
                } else {
                    "\n✓ patch applied"
                }
            );
            Ok(if failed { 1 } else { 0 })
        }
        "mcp" => {
            forma::mcp::start_mcp_server(project_dir(0))?;
            Ok(0)
        }
        "--version" | "version" => {
            println!("forma 0.2.0 (Specification Federation, Rust)");
            Ok(0)
        }
        "--help" | "help" => {
            print!("{HELP}");
            Ok(0)
        }
        other => {
            eprintln!("unknown command: {other}\n\n{HELP}");
            Ok(2)
        }
    }
}

fn cmd_check(project_dir: PathBuf) -> i32 {
    let manifest = match forma::manifest::load_manifest(&project_dir) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("✗ {err:#}");
            return 1;
        }
    };
    let Some(source) = manifest.manifest.source.clone() else {
        eprintln!("✗ not a DSL project (forma.yaml has no `source`)");
        return 1;
    };
    let (program, parse_diags) = match forma::dsl::load_program(&project_dir, &source) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("✗ {err:#}");
            return 1;
        }
    };
    let mut diagnostics = parse_diags;
    diagnostics.extend(forma::dsl::check_program(&program));
    let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
    for d in &diagnostics {
        eprintln!("{}", forma::validate::format_diagnostic(d));
    }
    println!(
        "{} {} entities, {} enums, {} apis, {} pages — {} diagnostics ({} errors)",
        if errors == 0 { "✓" } else { "✗" },
        program.entities.len(),
        program.enums.len(),
        program.apis.len(),
        program.pages.len(),
        diagnostics.len(),
        errors
    );
    if errors == 0 {
        0
    } else {
        1
    }
}

fn cmd_build_dsl(project_dir: PathBuf, _force: bool) -> i32 {
    match forma::build::run_build(&project_dir) {
        Ok(report) => {
            for step in &report.steps {
                for f in &step.files {
                    println!("  + {} ({})", f.path, f.generator);
                }
            }
            for d in &report.capability_diagnostics {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            if !report.capabilities.is_empty() {
                let caps: Vec<String> = report
                    .capabilities
                    .iter()
                    .map(|c| {
                        if c.explicit {
                            format!("{}@{}", c.id, c.version)
                        } else {
                            format!("{}@{} (dep)", c.id, c.version)
                        }
                    })
                    .collect();
                println!("capabilities: {}", caps.join(", "));
            }
            println!(
                "✓ build ok — {} file(s) written ({})",
                report.total_files(),
                report
                    .steps
                    .iter()
                    .map(|s| format!("{}: {}", s.name, s.files.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            0
        }
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    }
}

/// `forma capability <list|check|show> [dir]` —— Capability Registry 的命令行入口。
fn cmd_capability(positional: &[String], flags: &std::collections::HashMap<String, String>) -> i32 {
    let action = positional.first().cloned().unwrap_or_else(|| "list".into());
    // `show <id> [dir]`；其余动作为 `[dir]`。
    let (dir, id) = if action == "show" {
        (
            positional
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            positional.get(1).cloned(),
        )
    } else {
        (
            positional
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
            None,
        )
    };

    let mut registry = forma::capability::CapabilityRegistry::with_builtins();
    let mut diags = registry.load_project_dir(&dir);

    match action.as_str() {
        "list" => {
            for m in registry.manifests() {
                let src = registry
                    .source_of(&m.metadata.id)
                    .cloned()
                    .unwrap_or_default();
                println!(
                    "{:<26} {:<8} {:<16} {}",
                    m.metadata.id, m.metadata.version, m.spec.category, src
                );
            }
            println!("\n{} capability(s) registered", registry.ids().len());
            0
        }
        "show" => {
            let Some(id) = id else {
                eprintln!("usage: forma capability show <capability-id> [dir]");
                return 2;
            };
            match registry.get(&id) {
                Some(m) => {
                    match serde_json::to_string_pretty(m) {
                        Ok(t) => println!("{t}"),
                        Err(e) => {
                            eprintln!("cannot serialize manifest: {e}");
                            return 1;
                        }
                    }
                    0
                }
                None => {
                    eprintln!("✗ unknown capability `{id}`");
                    1
                }
            }
        }
        "check" => {
            let manifest = match forma::manifest::load_manifest(&dir) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("✗ {e:#}");
                    return 1;
                }
            };
            let resolution = registry.resolve(
                &manifest.manifest.capabilities.enabled,
                &manifest.manifest.capabilities.pins,
            );
            diags.extend(resolution.diagnostics.iter().cloned());
            for d in &diags {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            let errors = diags.iter().filter(|d| d.severity == "error").count();
            let warnings = diags.iter().filter(|d| d.severity == "warning").count();
            if flags.contains_key("json") {
                let out = serde_json::json!({
                    "capabilities": resolution.active,
                    "permissions": resolution.permissions,
                    "diagnostics": diags,
                    "errors": errors,
                    "warnings": warnings,
                });
                match serde_json::to_string_pretty(&out) {
                    Ok(t) => println!("{t}"),
                    Err(e) => {
                        eprintln!("cannot serialize report: {e}");
                        return 1;
                    }
                }
            } else {
                for c in &resolution.active {
                    println!(
                        "{:<26} {:<8} {}",
                        c.id,
                        c.version,
                        if c.explicit { "explicit" } else { "dependency" }
                    );
                }
                if !resolution.permissions.is_empty() {
                    println!("\npermissions: {}", resolution.permissions.join(", "));
                }
            }
            if errors > 0 {
                println!("✗ capability check — {errors} error(s), {warnings} warning(s)");
                1
            } else {
                println!(
                    "✓ capability check — {} active, {warnings} warning(s)",
                    resolution.active.len()
                );
                0
            }
        }
        other => {
            eprintln!("forma capability: unknown action `{other}` (available: list, check, show)");
            2
        }
    }
}

fn cmd_migrate(
    project_dir: &std::path::Path,
    sub: &str,
    sql_only: bool,
    apply: bool,
    target: Option<&str>,
    steps: usize,
) -> i32 {
    let manifest = match forma::manifest::load_manifest(project_dir) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("✗ {err:#}");
            return 1;
        }
    };
    let out_dir = manifest
        .manifest
        .targets
        .database
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/db".into());

    match sub {
        "status" => {
            println!("Migrations ({}):", out_dir);
            let lines = forma::emit::migration_status(project_dir).unwrap_or_default();
            for l in &lines {
                println!("{l}");
            }
            let ledger = read_ledger_lenient(project_dir);
            let applied = ledger.entries.iter().filter(|e| e.applied).count();
            println!(
                "\n{} migration(s): {} applied, {} pending",
                ledger.entries.len(),
                applied,
                ledger.entries.len() - applied
            );
            println!("  full SQL trail: {out_dir}/migrations/MIGRATIONS.md");
            0
        }
        "history" => {
            println!("Migration history:");
            for l in forma::emit::migration_status(project_dir).unwrap_or_default() {
                println!("{l}");
            }
            0
        }
        "verify" => {
            let source = manifest.manifest.source.clone().unwrap_or_default();
            let (program, diagnostics) = match forma::dsl::load_program(project_dir, &source) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("✗ {err:#}");
                    return 1;
                }
            };
            let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
            if errors > 0 {
                eprintln!("✗ DSL 有 {errors} 个错误，先修复再 verify");
                return 1;
            }
            let issues = match forma::emit::migration_verify(project_dir, &program, &out_dir) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("✗ {err:#}");
                    return 1;
                }
            };
            if issues.is_empty() {
                println!(
                    "✓ 迁移链完整且与当前 DSL schema 一致（编号连续 · 升级/回滚配对 · 哈希一致 · 前向重放无漂移）"
                );
                0
            } else {
                println!("✗ 迁移链校验失败（{} 项）：", issues.len());
                for i in &issues {
                    println!("  - {i}");
                }
                1
            }
        }
        "risk" => {
            let source = manifest.manifest.source.clone().unwrap_or_default();
            let (program, diagnostics) = match forma::dsl::load_program(project_dir, &source) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("✗ {err:#}");
                    return 1;
                }
            };
            let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
            if errors > 0 {
                eprintln!("✗ DSL 有 {errors} 个错误，先修复再评估迁移风险");
                return 1;
            }
            let ops = match forma::emit::pending_migration_ops(project_dir, &out_dir, &program) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("✗ {err:#}");
                    return 1;
                }
            };
            for l in forma::emit::migration_risk_lines(&ops) {
                println!("{l}");
            }
            0
        }
        "up" => {
            let pending = match forma::emit::migration_up(project_dir, &out_dir, apply, target) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("✗ {err:#}");
                    return 1;
                }
            };
            if pending.is_empty() {
                if !sql_only {
                    println!("✓ database is up to date — no pending migrations");
                }
                return 0;
            }
            for (file, sql) in &pending {
                if !sql_only {
                    println!("── {} ──", file.rsplit('/').next().unwrap_or(file));
                }
                println!("{sql}");
            }
            if sql_only {
                return 0;
            }
            if apply {
                println!("✓ applied {} migration(s)", pending.len());
            } else {
                println!(
                    "⏳ {} pending migration(s) (dry run) — use --apply to mark applied",
                    pending.len()
                );
            }
            0
        }
        "down" => match forma::emit::migration_down(project_dir, &out_dir, apply, steps, target) {
            Ok(rolled) if rolled.is_empty() => {
                println!("✓ nothing to roll back");
                0
            }
            Ok(rolled) => {
                for (file, sql) in &rolled {
                    if !sql_only {
                        println!("── {} ──", file.rsplit('/').next().unwrap_or(file));
                    }
                    println!("{sql}");
                }
                if sql_only {
                    return 0;
                }
                if apply {
                    println!("✓ rolled back {} migration(s)", rolled.len());
                } else {
                    println!(
                        "⏳ rollback plan for {} migration(s) (dry run) — use --apply",
                        rolled.len()
                    );
                }
                0
            }
            Err(err) => {
                eprintln!("✗ {err:#}");
                1
            }
        },
        _ => {
            eprintln!("unknown migrate subcommand: {sub}");
            2
        }
    }
}

fn read_ledger_lenient(project_dir: &std::path::Path) -> forma::emit::MigrationLedger {
    let p = project_dir.join(".forma").join("migrations.json");
    forma::util::read_text(&p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn cmd_diff(
    dir_a: &std::path::Path,
    dir_b: &std::path::Path,
    out_html: Option<PathBuf>,
    out_json: Option<PathBuf>,
) -> i32 {
    match forma::emit::emit_diff(dir_a, dir_b, out_html.as_deref(), out_json.as_deref()) {
        Ok(report) => {
            if let Some(h) = out_html {
                println!("✓ diff report written to {}", h.display());
            }
            if let Some(j) = out_json {
                println!("✓ diff json written to {}", j.display());
            }
            if report.summary.unchanged {
                println!("  (no differences)");
            } else {
                println!(
                    "  +{} ~{} -{}",
                    report.summary.added, report.summary.modified, report.summary.removed
                );
            }
            0
        }
        Err(err) => {
            eprintln!("✗ {err:#}");
            1
        }
    }
}

fn render_graph_text(summary: &forma::graph::GraphSummary) -> String {
    let mut lines = vec![
        "Unified Semantic Graph".to_string(),
        "======================".to_string(),
    ];
    for entity in summary.nodes.iter().filter(|n| n.kind == "entity") {
        lines.push(String::new());
        lines.push(format!("▣ {}  ({})", entity.name, entity.uri));
        for edge in summary.edges.iter().filter(|e| e.from == entity.uri) {
            if let Some(target) = summary.nodes.iter().find(|n| n.uri == edge.to) {
                lines.push(format!(
                    "   ├─ {} → [{}] {} \"{}\" ({})",
                    edge.kind, target.standard, target.kind, target.name, target.source.file
                ));
            }
        }
    }
    lines.join("\n")
}

fn cmd_docs(positional: &[String], flags: &std::collections::HashMap<String, String>) -> i32 {
    let known = [
        "validate",
        "lint",
        "graph",
        "check-compatibility",
        "check-dependencies",
        "check-links",
    ];
    let (action, dir) = if positional
        .first()
        .map(|s| known.contains(&s.as_str()))
        .unwrap_or(false)
    {
        (
            positional[0].clone(),
            positional
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
        )
    } else {
        (
            "validate".to_string(),
            positional
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
        )
    };

    if action == "graph" {
        return cmd_docs_graph(&dir, flags);
    }

    let report = match action.as_str() {
        "validate" => forma::docs::run_docs_validate(&dir),
        "lint" => forma::docs::run_docs_lint(&dir),
        "check-dependencies" => forma::docs::run_docs_check_dependencies(&dir),
        "check-compatibility" => forma::docs::run_docs_check_compatibility_against(
            &dir,
            flags
                .get("base")
                .filter(|s| !s.is_empty())
                .map(String::as_str),
        ),
        "check-links" => forma::docs::run_docs_check_links(&dir),
        other => {
            eprintln!(
                "forma docs: unknown action '{other}' (available: {})",
                known.join(", ")
            );
            return 2;
        }
    };

    // `--json`：输出机器可读报告（诊断不重复写到 stderr）。
    if flags.contains_key("json") {
        let json = forma::docs::json_report(&action, &dir, &report);
        match serde_json::to_string_pretty(&json) {
            Ok(t) => println!("{t}"),
            Err(e) => {
                eprintln!("forma docs {action}: cannot serialize report: {e}");
                return 1;
            }
        }
        return json.exit_code;
    }

    for d in &report.diagnostics {
        eprintln!("{}", forma::validate::format_diagnostic(d));
    }
    let errors = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .count();
    let warnings = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == "warning")
        .count();
    let infos = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == "info")
        .count();

    if errors > 0 {
        println!(
            "✗ docs {action} — {} governed doc(s) checked, {errors} error(s), {warnings} warning(s)",
            report.docs_checked
        );
        1
    } else {
        println!(
            "✓ docs {action} — {} governed doc(s) checked, {warnings} warning(s), {infos} info",
            report.docs_checked
        );
        0
    }
}

/// `forma docs graph` —— 渲染 RFC/ADR/Spec 依赖图（`--json` 输出机器可读格式）。
fn cmd_docs_graph(dir: &Path, flags: &std::collections::HashMap<String, String>) -> i32 {
    let (graph, diagnostics) = forma::docs::build_docs_graph(dir);
    for d in &diagnostics {
        eprintln!("{}", forma::validate::format_diagnostic(d));
    }
    if flags.contains_key("json") {
        match serde_json::to_string_pretty(&graph) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("forma docs graph: cannot serialize graph: {e}");
                return 1;
            }
        }
    } else {
        println!("{}", forma::docs::render_docs_graph(&graph, &diagnostics));
    }
    let errors = diagnostics.iter().filter(|d| d.severity == "error").count();
    if errors > 0 {
        1
    } else {
        0
    }
}

/// `forma validate` —— DSL 工程走 `check_program` 管线，联邦工程走语义图管线。
///
/// **关键**：DSL 工程**必须**跑 `check_program`——否则带语义错误的工程会「假绿」
/// （退出码 0、0 diagnostics）。这是 RFC-0003 M4 的落地点。
fn cmd_validate(project_dir: &Path) -> i32 {
    match forma::engine::check_project(project_dir) {
        Ok(forma::engine::ProjectCheck::Dsl(check)) => {
            for d in &check.diagnostics {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            let errors = check.error_count();
            let warnings = check
                .diagnostics
                .iter()
                .filter(|d| d.severity == "warning")
                .count();
            println!(
                "{} {} — {} entities, {} enums, {} apis, {} pages ({} errors, {} warnings)",
                if errors == 0 {
                    "✓ valid"
                } else {
                    "✗ invalid"
                },
                check.project_name,
                check.program.entities.len(),
                check.program.enums.len(),
                check.program.apis.len(),
                check.program.pages.len(),
                errors,
                warnings
            );
            if errors > 0 {
                1
            } else {
                0
            }
        }
        Ok(forma::engine::ProjectCheck::Federal(outcome)) => {
            for d in &outcome.diagnostics {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            let errors = outcome
                .diagnostics
                .iter()
                .filter(|d| d.severity == "error")
                .count();
            println!(
                "{} {} nodes, {} diagnostics ({errors} errors)",
                if errors == 0 {
                    "✓ valid —"
                } else {
                    "✗ invalid —"
                },
                outcome.graph.all_nodes().len(),
                outcome.diagnostics.len()
            );
            if errors > 0 {
                1
            } else {
                0
            }
        }
        Err(err) => {
            eprintln!("✗ {err:#}");
            1
        }
    }
}

/// `forma doctor` —— DSL 工程跑语义检查，联邦工程跑跨标准诊断。
fn cmd_doctor(project_dir: &Path) -> i32 {
    match forma::engine::check_project(project_dir) {
        Ok(forma::engine::ProjectCheck::Dsl(check)) => {
            for d in &check.diagnostics {
                eprintln!("{}", forma::validate::format_diagnostic(d));
            }
            for d in &check.lint {
                eprintln!("[{}] {} — {}", d.severity.to_uppercase(), d.rule, d.message);
            }
            let errors = check.error_count();
            let warnings = check
                .diagnostics
                .iter()
                .filter(|d| d.severity == "warning")
                .count()
                + check
                    .lint
                    .iter()
                    .filter(|d| d.severity == "warning")
                    .count();
            println!(
                "\n{} errors, {} warnings, {} lint finding(s)",
                errors,
                warnings,
                check.lint.len()
            );
            if errors > 0 {
                1
            } else {
                0
            }
        }
        Ok(forma::engine::ProjectCheck::Federal(outcome)) => {
            let mut all = outcome.diagnostics.clone();
            all.extend(forma::validate::run_doctor(&outcome.graph));
            if all.is_empty() {
                println!("✓ no cross-standard issues found");
                return 0;
            }
            for severity in ["error", "warning", "info"] {
                for d in all.iter().filter(|d| d.severity == severity) {
                    eprintln!("{}", forma::validate::format_diagnostic(d));
                }
            }
            let count = |s: &str| all.iter().filter(|d| d.severity == s).count();
            println!(
                "\n{} errors, {} warnings, {} info",
                count("error"),
                count("warning"),
                count("info")
            );
            if count("error") > 0 {
                1
            } else {
                0
            }
        }
        Err(err) => {
            eprintln!("✗ {err:#}");
            1
        }
    }
}

/// DSL 模型摘要的文本渲染。
fn render_dsl_model_text(summary: &serde_json::Value) -> String {
    let mut lines = vec!["Forma DSL 模型".to_string(), "==============".to_string()];
    if let Some(entities) = summary.get("entities").and_then(|v| v.as_array()) {
        for e in entities {
            let name = e.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let table = e.get("table").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!("▣ {name}  ({table})"));
            if let Some(fields) = e.get("fields").and_then(|v| v.as_array()) {
                for f in fields {
                    let fname = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let ftype = f.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                    let opt = if f.get("optional").and_then(|v| v.as_bool()).unwrap_or(false) {
                        "?"
                    } else {
                        ""
                    };
                    lines.push(format!("    ├─ {fname}: {ftype}{opt}"));
                }
            }
        }
    }
    for (label, key) in [("枚举", "enums"), ("API", "apis"), ("页面", "pages")] {
        if let Some(items) = summary.get(key).and_then(|v| v.as_array()) {
            if items.is_empty() {
                continue;
            }
            lines.push(String::new());
            lines.push(format!("{label}（{}）", items.len()));
            for i in items {
                let n = i.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                lines.push(format!("  • {n}"));
            }
        }
    }
    lines.join("\n")
}
