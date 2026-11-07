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
#[derive(Debug)]
pub struct StepReport {
    pub name: &'static str,
    pub files: Vec<GeneratedFile>,
}

/// 一次 DSL 构建的完整报告。
#[derive(Debug)]
pub struct BuildReport {
    pub project_name: String,
    pub steps: Vec<StepReport>,
    /// 本次构建解析出的能力集合（RFC-0004 Capability Registry）。
    pub capabilities: Vec<ResolvedCapability>,
    /// 能力解析产生的非阻断诊断（warning / info）。
    pub capability_diagnostics: Vec<crate::validate::Diagnostic>,
    /// 产物归属裁决诊断（artifact-modified / adopted / overwritten，F-002）
    pub artifact_diagnostics: Vec<crate::validate::Diagnostic>,
}

/// 产物归属裁决旗标（ADR-0003）：默认保留人工版本 + 警告。
#[derive(Debug, Clone, Copy, Default)]
pub struct OwnershipPolicy {
    /// `--adopt`：冲突文件登记为 Adopted（人工版本胜出）
    pub adopt: bool,
    /// `--overwrite`：显式覆盖（生成版本胜出 + 审计）
    pub overwrite: bool,
    /// `--strict`：冲突提升为 error
    pub strict: bool,
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
    run_build_with(project_dir, OwnershipPolicy::default())
}

pub fn run_build_with(project_dir: &Path, policy: OwnershipPolicy) -> Result<BuildReport> {
    // F-002：构建前快照受保护产物（清单哈希 ≠ 磁盘内容 = 人工改过）
    let protected = crate::artifacts::snapshot_protected(project_dir)?;
    let (dir, program) = load_dsl_program(project_dir)?;
    let loaded = crate::manifest::load_manifest(project_dir)?;
    let app_name = loaded.manifest.project_name.clone();
    let targets = loaded.manifest.targets.clone();
    let (capabilities, capability_diagnostics) = resolve_capabilities(&dir, &loaded.manifest)?;

    // 游标响应是 `{items,nextCursor}` envelope：Web 已支持 useInfiniteQuery；
    // V0.2 数据绑定完整化：可视化运行时支持 cursor 分页源（解包 {items, nextCursor}
    // 信封取第 0 页）。Flutter 端游标已支持（CursorPage 信封 + 加载更多）。

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
    let graphql_out = targets
        .api
        .as_ref()
        .and_then(|t| t.out.clone())
        .unwrap_or_else(|| "generated/graphql".into());

    let mut steps: Vec<StepReport> = Vec::new();
    let mut push = |name: &'static str, r: anyhow::Result<Vec<GeneratedFile>>| -> Result<()> {
        steps.push(StepReport { name, files: r? });
        Ok(())
    };

    // M10：db target 的 engine 字段决定 DDL 方言（mysql 默认 / postgres）
    let db_engine = targets
        .database
        .as_ref()
        .and_then(|t| t.engine.clone())
        .unwrap_or_else(|| "mysql".into());
    if db_engine.starts_with("postgres") || db_engine == "postgresql" {
        push(
            "db-postgres",
            crate::emit::postgres::emit_postgres(&dir, &program, &db_out),
        )?;
        push(
            "postgres-migrate",
            crate::emit::postgres::emit_postgres_migration(&dir, &program, &db_out),
        )?;
    } else if db_engine == "sqlite" || db_engine == "sqlite3" {
        push(
            "db-sqlite",
            crate::emit::sqlite::emit_sqlite(&dir, &program, &db_out),
        )?;
        push(
            "sqlite-migrate",
            crate::emit::sqlite::emit_sqlite_migration(&dir, &program, &db_out),
        )?;
    } else if db_engine == "redis" {
        push(
            "db-redis",
            crate::emit::redis::emit_redis(&dir, &program, &db_out),
        )?;
    } else {
        push("db-mysql", crate::emit::emit_mysql(&dir, &program, &db_out))?;
    }
    // Backend target before web/API emitters (preserve deterministic step order).
    let backend_is_rust = targets
        .backend
        .as_ref()
        .map(|b| b.language.as_deref().unwrap_or("rust") == "rust")
        .unwrap_or(false);
    if let Some(backend) = targets.backend.as_ref() {
        let backend_language = backend.language.as_deref().unwrap_or("rust");
        let backend_framework = backend.framework.as_deref().unwrap_or("axum");
        match (backend_language, backend_framework) {
            ("typescript" | "ts", "nestjs" | "nest") => {
                push(
                    "backend-nestjs",
                    crate::emit::nest::emit_nestjs(&dir, &program, &backend_out, &app_name),
                )?;
            }
            ("typescript" | "ts", "hono") => {
                push(
                    "backend-hono",
                    crate::emit::hono::emit_hono(&dir, &program, &backend_out, &app_name),
                )?;
            }
            ("python" | "python3", "fastapi") => {
                push(
                    "backend-fastapi",
                    crate::emit::fastapi::emit_fastapi(&dir, &program, &backend_out, &app_name),
                )?;
            }
            ("rust", _) | (_, "axum") => {
                push(
                    "backend-rust",
                    crate::emit::emit_rust_backend(&dir, &program, &backend_out, &app_name),
                )?;
            }
            (language, framework) => anyhow::bail!(
                "unsupported backend target: language={language}, framework={framework}; supported: rust/axum, typescript/nestjs, typescript/hono, python/fastapi"
            ),
        }
    }
    let frontend_framework = targets
        .frontend
        .as_ref()
        .and_then(|t| t.framework.clone())
        .unwrap_or_else(|| "react".into());
    if frontend_framework == "vue" || frontend_framework == "vue3" {
        push(
            "frontend-vue",
            crate::emit::vue::emit_vue(&dir, &program, &web_out, &app_name),
        )?;
    } else if frontend_framework == "nextjs" || frontend_framework == "next" {
        push(
            "frontend-nextjs",
            crate::emit::nextjs::emit_nextjs(&dir, &program, &web_out, &app_name),
        )?;
    } else {
        push(
            "web-react",
            crate::emit::emit_web_react(
                &dir,
                &program,
                &web_out,
                &app_name,
                targets
                    .frontend
                    .as_ref()
                    .map(|t| t.uses_rhf)
                    .unwrap_or(false),
                targets
                    .frontend
                    .as_ref()
                    .map(|t| t.uses_zustand)
                    .unwrap_or(false),
            ),
        )?;
    }
    // viz runtime 面向 web-react（echarts 依赖 + client 组件）；Next.js 暂不接图表（后续增强）
    if frontend_framework != "nextjs" && frontend_framework != "next" {
        push(
            "viz-web",
            crate::emit::emit_visualization(&dir, &program, &web_out),
        )?;
    }
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
    // M9/P1：Docker + backend engineering 只对 Rust/axum 生成。
    if backend_is_rust {
        push(
            "docker",
            crate::emit::emit_docker(&dir, &program, ".", &app_name),
        )?;
        push(
            "backend-engineering",
            crate::emit::engineering::emit_backend_engineering(
                &dir,
                &program,
                &backend_out,
                &app_name,
            ),
        )?;
    }
    // web-engineering（CI/Makefile 模板面向 vite+vitest）只对 React/Vue 前端生成；
    // Next.js 由 nextjs emitter 自带 CI（next build 语义不同）。
    if targets.frontend.is_some()
        && !program.pages.is_empty()
        && frontend_framework != "nextjs"
        && frontend_framework != "next"
    {
        push(
            "web-engineering",
            crate::emit::engineering::emit_web_engineering(&dir, &program, &web_out),
        )?;
    }
    if let Some(api_target) = targets.api.as_ref() {
        let api_framework = api_target.framework.as_deref().unwrap_or("graphql");
        if api_framework == "graphql" {
            push(
                "api-graphql",
                crate::emit::graphql::emit_graphql(&dir, &program, &graphql_out, &app_name),
            )?;
        } else {
            anyhow::bail!("unsupported api target style: {api_framework}; supported: graphql");
        }
    }
    if let Some(mobile) = targets.mobile.as_ref() {
        let mobile_framework = mobile.framework.as_deref().unwrap_or("flutter");
        if mobile_framework == "expo" {
            push(
                "mobile-expo",
                crate::emit::expo::emit_expo(&dir, &program, &mobile_out, &app_name),
            )?;
        } else {
            push(
                "mobile-flutter",
                crate::emit::emit_flutter(&dir, &program, &mobile_out, &app_name),
            )?;
        }
    }

    // F-002：构建后恢复受保护产物 + 生成归属诊断 + 登记清单
    let all_files: Vec<GeneratedFile> =
        steps.iter().flat_map(|s| s.files.iter().cloned()).collect();
    let (mut artifact_diagnostics, _resolved) = crate::artifacts::restore_protected(
        project_dir,
        &protected,
        policy.adopt,
        policy.overwrite,
        policy.strict,
        &all_files,
    )?;
    if policy.strict {
        for d in &mut artifact_diagnostics {
            d.severity = "error".into();
        }
    }

    Ok(BuildReport {
        project_name: app_name,
        steps,
        capabilities,
        capability_diagnostics,
        artifact_diagnostics,
    })
}
