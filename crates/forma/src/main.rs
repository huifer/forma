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
            } else if ["export", "out", "json"].contains(&name) {
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
    let manifest = match forma::manifest::load_manifest(&project_dir) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("✗ {err:#}");
            return 1;
        }
    };
    let source = manifest.manifest.source.clone().unwrap_or_default();
    let (program, diagnostics) = match forma::dsl::load_program(&project_dir, &source) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("✗ {err:#}");
            return 1;
        }
    };
    let hard_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    if !hard_errors.is_empty() {
        for d in &hard_errors {
            eprintln!("{}", forma::validate::format_diagnostic(d));
        }
        eprintln!("✗ build failed: {} DSL error(s)", hard_errors.len());
        return 1;
    }
    let app_name = manifest.manifest.project_name.clone();
    let targets = &manifest.manifest.targets;
    let db_out = targets
        .database
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/db".into());
    let backend_out = targets
        .backend
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/backend".into());
    let web_out = targets
        .frontend
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/web".into());
    let api_out = "generated/api-docs";

    let mut all: Vec<forma::generators::GeneratedFile> = Vec::new();
    let steps: Vec<EmitStep> = vec![
        (
            "db-mysql",
            Box::new(|| forma::emit::emit_mysql(&project_dir, &program, &db_out)),
        ),
        (
            "backend-rust",
            Box::new(|| {
                forma::emit::emit_rust_backend(&project_dir, &program, &backend_out, &app_name)
            }),
        ),
        (
            "web-react",
            Box::new(|| forma::emit::emit_web_react(&project_dir, &program, &web_out, &app_name)),
        ),
        (
            "openapi",
            Box::new(|| forma::emit::emit_openapi(&project_dir, &program, api_out)),
        ),
        (
            "diagrams",
            Box::new(|| forma::emit::emit_diagrams(&project_dir, &program, "generated/diagrams")),
        ),
        (
            "migrate",
            Box::new(|| {
                Ok(
                    forma::emit::emit_migration(&project_dir, &program, &db_out)?
                        .map(|f| vec![f])
                        .unwrap_or_default(),
                )
            }),
        ),
    ];
    let mut report: Vec<String> = Vec::new();
    for (name, step) in steps {
        match step() {
            Ok(files) => {
                for f in &files {
                    println!("  + {} ({})", f.path, f.generator);
                }
                report.push(format!("{}: {} file(s)", name, files.len()));
                all.extend(files);
            }
            Err(err) => {
                eprintln!("✗ emitter {name} failed: {err:#}");
                return 1;
            }
        }
    }
    println!(
        "✓ build ok — {} file(s) written ({})",
        all.len(),
        report.join(", ")
    );
    0
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

type EmitStep<'a> = (
    &'a str,
    Box<dyn Fn() -> anyhow::Result<Vec<forma::generators::GeneratedFile>> + 'a>,
);

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
