//! forma CLI — Specification Federation 的命令行入口。

use anyhow::Result;
use std::path::PathBuf;
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
            } else if ["export", "out", "json", "to", "steps"].contains(&name) {
                if let Some(v) = argv.get(i + 1) {
                    flags.insert(name.to_string(), v.clone());
                    i += 1;
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
                flags.get("out").map(PathBuf::from),
                flags.get("json").map(PathBuf::from),
            ))
        }
        "init" => {
            forma::scaffold::scaffold_project(&project_dir(0))?;
            println!("✓ Forma project scaffolded at {}", project_dir(0).display());
            println!("  next: forma validate && forma build");
            Ok(0)
        }
        "validate" => {
            let outcome = forma::engine::load_project(&project_dir(0))?;
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
            Ok(if errors == 0 { 0 } else { 1 })
        }
        "doctor" => {
            let outcome = forma::engine::load_project(&project_dir(0))?;
            let mut all = outcome.diagnostics.clone();
            all.extend(forma::validate::run_doctor(&outcome.graph));
            if all.is_empty() {
                println!("✓ no cross-standard issues found");
                return Ok(0);
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
            Ok(if count("error") > 0 { 1 } else { 0 })
        }
        "graph" => {
            let dir = project_dir(0);
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
            let ops = match forma::emit::pending_migration_ops(project_dir, &program) {
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
