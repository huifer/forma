//! DSL 构建编排：`forma build` 与 MCP `forma_build` 共用的同一条管线。
//!
//! 抽成库函数的原因：CLI 与 MCP 必须产出完全一致的产物集合——
//! AI 通过 MCP 看到的构建结果与开发者本地跑 `forma build` 不会漂移。

use crate::capability::{CapabilityRegistry, ResolvedCapability};
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
    /// 本次构建解析出的能力集合（RFC-0004 Capability Registry）。
    pub capabilities: Vec<ResolvedCapability>,
    /// 能力解析产生的非阻断诊断（warning / info）。
    pub capability_diagnostics: Vec<crate::validate::Diagnostic>,
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

/// 解析工程启用的能力（RFC-0004）。
///
/// 能力错误（未知能力 / 依赖缺失 / 版本不兼容 / pin 不匹配）会**阻断构建**——
/// 这正是「能力必须自洽」的执行点。
pub fn resolve_capabilities(
    project_dir: &Path,
    manifest: &crate::manifest::FormaManifest,
) -> Result<(Vec<ResolvedCapability>, Vec<crate::validate::Diagnostic>)> {
    let mut registry = CapabilityRegistry::with_builtins();
    let mut diags = registry.load_project_dir(project_dir);
    let resolution = registry.resolve(&manifest.capabilities.enabled, &manifest.capabilities.pins);
    diags.extend(resolution.diagnostics.iter().cloned());

    let errors: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
    if !errors.is_empty() {
        let mut msg = format!("✗ build failed: {} capability error(s)\n", errors.len());
        for d in &errors {
            msg.push_str(&crate::validate::format_diagnostic(d));
            msg.push('\n');
        }
        bail!("{}", msg.trim_end());
    }
    let non_blocking: Vec<_> = diags
        .into_iter()
        .filter(|d| d.severity != "error")
        .collect();
    Ok((resolution.active, non_blocking))
}

/// 完整构建：全部已配置 target 的产物写入磁盘并返回报告。
pub fn run_build(project_dir: &Path) -> Result<BuildReport> {
    let (dir, program) = load_dsl_program(project_dir)?;
    let loaded = crate::manifest::load_manifest(project_dir)?;
    let app_name = loaded.manifest.project_name.clone();
    let targets = loaded.manifest.targets.clone();
    let (capabilities, capability_diagnostics) = resolve_capabilities(&dir, &loaded.manifest)?;

    // 游标响应是 `{items,nextCursor}` envelope：Web 已支持 useInfiniteQuery；
    // Flutter / visualization runtime 仍消费裸列表，必须在任何 emitter 写盘前显式拒绝，
    // 避免同一工程的多目标产物语义不一致。
    let cursor_apis: Vec<&str> = program
        .apis
        .iter()
        .filter(|api| api.list.as_ref().and_then(|l| l.paginate.as_deref()) == Some("cursor"))
        .map(|api| api.name.as_str())
        .collect();
    if !cursor_apis.is_empty() && targets.mobile.is_some() {
        bail!(
            "cursor pagination is not yet supported by the configured Flutter target (api: {}); remove the mobile target or use `paginate offset`",
            cursor_apis.join(", ")
        );
    }
    for chart in &program.charts {
        if let Some((source_api, _)) = &chart.source {
            if cursor_apis.iter().any(|name| name == &source_api.as_str()) {
                bail!(
                    "chart `{}` consumes cursor-paginated api `{}` but the visualization runtime currently requires a list response; use offset pagination",
                    chart.name, source_api
                );
            }
        }
    }

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
        capabilities,
        capability_diagnostics,
    })
}
