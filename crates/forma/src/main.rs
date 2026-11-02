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
            } else if name == "export" {
                if let Some(v) = argv.get(i + 1) {
                    flags.insert("export".into(), v.clone());
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
            let errors = outcome.diagnostics.iter().filter(|d| d.severity == "error").count();
            println!(
                "{} {} nodes, {} diagnostics ({errors} errors)",
                if errors == 0 { "✓ valid —" } else { "✗ invalid —" },
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
            let outcome = forma::engine::build_project(&forma::engine::BuildOptions {
                project_dir: project_dir(0),
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
            let errors = outcome.diagnostics.iter().filter(|d| d.severity == "error").count();
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
            println!("{}", if failed { "\n✗ patch completed with failures" } else { "\n✓ patch applied" });
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

fn render_graph_text(
    summary: &forma::graph::GraphSummary,
) -> String {
    let mut lines = vec!["Unified Semantic Graph".to_string(), "======================".to_string()];
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
