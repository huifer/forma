//! DSL 构建编排：`forma build` 与 MCP `forma_build` 共用的同一条管线。
//!
//! 抽成库函数的原因：CLI 与 MCP 必须产出完全一致的产物集合——
//! AI 通过 MCP 看到的构建结果与开发者本地跑 `forma build` 不会漂移。

use crate::dsl::{load_program, Program};
use crate::generators::GeneratedFile;
use crate::manifest::load_manifest;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// 单个 emitter 的产物报告。
pub struct StepReport {
    pub name: &'static str,
    pub files: Vec<GeneratedFile>,
}

/// 一次 DSL 构建的完整报告。
pub struct BuildReport {
    pub project_name: String,
    pub steps: Vec<StepReport>,
}

impl BuildReport {
    pub fn total_files(&self) -> usize {
        self.steps.iter().map(|s| s.files.len()).sum()
    }

    pub fn step_names(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.name).collect()
    }
}

/// 加载 DSL 工程（manifest 带 `source`）；联邦工程返回明确错误。
pub fn load_dsl_program(project_dir: &Path) -> Result<(PathBuf, Program)> {
    let loaded = load_manifest(project_dir)?;
    let source = loaded
        .manifest
        .source
        .clone()
        .context("not a DSL project (forma.yaml has no `source`)")?;
    let (program, mut diagnostics) = load_program(&loaded.dir, &source)?;
    // 语义检查是编译管线第 3 阶段（architecture §4）：类型/引用/规则/可视化
    // 的硬错误必须在 Emit 之前阻断，不能只靠 `forma validate` 事后发现。
    diagnostics.extend(crate::dsl::check_program(&program));
    let hard_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    if !hard_errors.is_empty() {
        let mut msg = format!("✗ build failed: {} DSL error(s)\n", hard_errors.len());
        for d in &hard_errors {
            msg.push_str(&crate::validate::format_diagnostic(d));
            msg.push('\n');
        }
        bail!("{}", msg.trim_end());
    }
    Ok((loaded.dir, program))
}

/// 完整构建：全部已配置 target 的产物写入磁盘并返回报告。
pub fn run_build(project_dir: &Path) -> Result<BuildReport> {
    let (dir, program) = load_dsl_program(project_dir)?;
    let app_name = crate::manifest::load_manifest(project_dir)?
        .manifest
        .project_name;
    let targets = crate::manifest::load_manifest(project_dir)?
        .manifest
        .targets;

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
    let seed_out = format!("{db_out}/seed");
    let mobile_out = targets
        .mobile
        .as_ref()
        .map(|m| m.out.clone().unwrap_or_else(|| "generated/mobile".into()))
        .unwrap_or_default();
    let api_out = "generated/api-docs";

    let mut steps: Vec<StepReport> = Vec::new();
    let mut push = |name: &'static str, r: anyhow::Result<Vec<GeneratedFile>>| -> Result<()> {
        steps.push(StepReport { name, files: r? });
        Ok(())
    };

    push("db-mysql", crate::emit::emit_mysql(&dir, &program, &db_out))?;
    push(
        "backend-rust",
        crate::emit::emit_rust_backend(&dir, &program, &backend_out, &app_name),
    )?;
    push(
        "web-react",
        crate::emit::emit_web_react(&dir, &program, &web_out, &app_name),
    )?;
    push(
        "viz-web",
        crate::emit::emit_visualization(&dir, &program, &web_out),
    )?;
    push(
        "openapi",
        crate::emit::emit_openapi(&dir, &program, api_out),
    )?;
    push(
        "diagrams",
        crate::emit::emit_diagrams(&dir, &program, "generated/diagrams"),
    )?;
    push(
        "migrate",
        crate::emit::emit_migration(&dir, &program, &db_out)
            .map(|f| f.map(|f| vec![f]).unwrap_or_default()),
    )?;
    push("seed", crate::emit::emit_seed(&dir, &program, &seed_out))?;
    if targets.mobile.is_some() {
        push(
            "mobile-flutter",
            crate::emit::emit_flutter(&dir, &program, &mobile_out, &app_name),
        )?;
    }

    Ok(BuildReport {
        project_name: app_name,
        steps,
    })
}
