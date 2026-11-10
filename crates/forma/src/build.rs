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
    /// 本次构建所用 IR 版本（RFC-0002 §14 S5：版本信息在构建报告中可查）
    pub ir_version: String,
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
    // F-002 余项：构建来源规格（IR 版本 + .fm 输入指纹 + 依赖清单）
    let provenance = build_provenance(project_dir)?;
    // A-6：扩展点锚点登记（磁盘存在的锚点 → 清单 Extension 归属，永不覆盖）
    crate::artifacts::register_anchors(
        project_dir,
        &extension_anchors_for(project_dir),
        &provenance,
    )?;
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
    } else if db_engine == "mssql" || db_engine == "sqlserver" {
        push(
            "db-mssql",
            crate::emit::mssql::emit_mssql(&dir, &program, &db_out),
        )?;
        push(
            "mssql-migrate",
            crate::emit::mssql::emit_mssql_migration(&dir, &program, &db_out),
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
                targets
                    .visualization
                    .as_ref()
                    .and_then(|v| v.adapter.clone())
                    .as_deref()
                    .unwrap_or("echarts"),
            ),
        )?;
    }
    // viz runtime 面向 web-react；Next.js 暂不接图表（后续增强）
    if frontend_framework != "nextjs" && frontend_framework != "next" {
        // RFC-0010 V0.6：Adapter 选择（echarts 默认，零变化；antv → G2Plot）
        let viz_adapter = targets
            .visualization
            .as_ref()
            .and_then(|v| v.adapter.clone())
            .unwrap_or_else(|| "echarts".into());
        push(
            "viz-web",
            crate::emit::emit_visualization_with_adapter(&dir, &program, &web_out, &viz_adapter),
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
    // M11/P1：deployment 显式校验（宪法 6.2——声明了就必须有明确结果，不静默跳过）
    if let Some(dep) = loaded.manifest.deployment.as_ref() {
        if !matches!(
            dep.provider.as_str(),
            "kubernetes" | "helm" | "terraform" | "all"
        ) {
            bail!(
                "unsupported deployment provider '{}' (allowed: kubernetes, helm, terraform, all)",
                dep.provider
            );
        }
        if !backend_is_rust {
            bail!("deployment requires a Rust/axum backend target (backend.language: rust)");
        }
    }
    // M9/P1：Docker + backend engineering 只对 Rust/axum 生成。
    if backend_is_rust {
        push(
            "docker",
            crate::emit::emit_docker(&dir, &program, ".", &app_name),
        )?;
        // M11/P1：部署产物（deployment: manifest 段 opt-in，默认零产物）
        if let Some(dep) = loaded.manifest.deployment.as_ref() {
            push(
                "deployment",
                crate::emit::emit_deploy(
                    &dir,
                    &program,
                    dep,
                    &app_name,
                    &loaded.manifest.project_version,
                ),
            )?;
        }
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
        &provenance,
    )?;
    if policy.strict {
        for d in &mut artifact_diagnostics {
            d.severity = "error".into();
        }
    }

    Ok(BuildReport {
        project_name: app_name,
        ir_version: program.ir_version.clone(),
        steps,
        capabilities,
        capability_diagnostics,
        artifact_diagnostics,
    })
}

/// 构建来源规格：`.fm` 源文件清单 + 归一化内容指纹（sha256，CRLF→LF 口径）。
fn build_provenance(project_dir: &Path) -> Result<crate::artifacts::BuildProvenance> {
    let loaded = crate::manifest::load_manifest(project_dir)?;
    let patterns: Vec<String> = loaded.manifest.source.clone().unwrap_or_default();
    // source 是 glob（如 "forma/**/*.fm"）——必须展开成真实文件
    let mut files: Vec<String> = Vec::new();
    for pattern in &patterns {
        for p in crate::dsl::glob_files(project_dir, pattern) {
            let rel = p
                .strip_prefix(project_dir)
                .unwrap_or(&p)
                .to_string_lossy()
                .to_string();
            files.push(rel);
        }
    }
    files.sort();
    files.dedup();
    let mut hasher = String::new();
    for rel in &files {
        let abs = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            project_dir.join(rel)
        };
        let content = crate::util::read_text(&abs)?;
        hasher.push_str(&crate::util::sha256_hex(&content));
        hasher.push('\n');
    }
    Ok(crate::artifacts::BuildProvenance {
        ir_version: crate::dsl::IR_VERSION.to_string(),
        input_fingerprint: crate::util::sha256_hex(&hasher),
        deps: files,
    })
}

/// 扩展点锚点（ADR-0003 §6.3）：按已启用 target 声明锚点路径。
/// 锚点文件由人工创建（Forma 不凭空生成）；构建只登记、永不覆盖。
fn extension_anchors_for(project_dir: &Path) -> Vec<(String, String)> {
    let mut anchors = Vec::new();
    let loaded = crate::manifest::load_manifest(project_dir).ok();
    if let Some(loaded) = loaded {
        let targets = &loaded.manifest.targets;
        if let Some(b) = &targets.backend {
            let out = b.out.clone().unwrap_or_else(|| "generated/backend".into());
            let ext = if b.language.as_deref() == Some("typescript") {
                "ts"
            } else {
                "rs"
            };
            anchors.push((format!("{out}/src/extensions.{ext}"), "gen-rust".into()));
        }
        if let Some(f) = &targets.frontend {
            let out = f.out.clone().unwrap_or_else(|| "generated/web".into());
            match f.framework.as_deref() {
                Some("vue") | Some("vue3") => {
                    anchors.push((format!("{out}/src/overrides.vue"), "gen-vue".into()));
                }
                Some("nextjs") | Some("next") => {
                    anchors.push((format!("{out}/src/overrides.tsx"), "gen-web".into()));
                }
                _ => {
                    anchors.push((format!("{out}/src/overrides.tsx"), "gen-web".into()));
                }
            }
        }
        if let Some(m) = &targets.mobile {
            let out = m.out.clone().unwrap_or_else(|| "generated/mobile".into());
            match m.framework.as_deref() {
                Some("expo") => {
                    anchors.push((format!("{out}/src/overrides.tsx"), "gen-expo".into()));
                }
                _ => {
                    anchors.push((format!("{out}/lib/extensions.dart"), "gen-flutter".into()));
                }
            }
        }
        if let Some(db) = &targets.database {
            let out = db.out.clone().unwrap_or_else(|| "generated/db".into());
            anchors.push((format!("{out}/extensions.sql"), "gen-mysql".into()));
        }
    }
    anchors
}
