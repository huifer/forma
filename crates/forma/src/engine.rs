//! 引擎：load（多适配器合并 + 约定匹配）与 build（增量编译编排）。

use crate::adapters::{AdapterRegistry, AdapterResult};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, Standard, UnifiedSemanticGraph};
use crate::manifest::{load_manifest, resolve_spec_path, LoadedManifest, GROUP_ORDER};
use crate::validate::{run_doctor, Diagnostic};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct LoadOutcome {
    pub loaded: LoadedManifest,
    pub graph: UnifiedSemanticGraph,
    pub diagnostics: Vec<Diagnostic>,
    /// 同一实体 URI 的多源定义（patch / 诊断用）
    pub entity_sources: HashMap<String, Vec<(String, String)>>,
}

pub fn load_project(project_dir: &Path) -> Result<LoadOutcome> {
    load_project_with(project_dir, &AdapterRegistry::default_registry())
}

pub fn load_project_with(project_dir: &Path, registry: &AdapterRegistry) -> Result<LoadOutcome> {
    let loaded = load_manifest(project_dir)?;
    let mut diagnostics: Vec<Diagnostic> = loaded
        .issues
        .iter()
        .map(|i| Diagnostic {
            severity: i.severity.clone(),
            code: "manifest-issue".into(),
            message: i.message.clone(),
            file: Some("forma.yaml".into()),
            line: None,
            nodes: None,
        })
        .collect();

    let mut graph = UnifiedSemanticGraph::new();
    let mut entity_sources: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut seen_edges: HashSet<String> = HashSet::new();

    // 固定顺序：schemas → api → ui → database → workflows
    for group in GROUP_ORDER {
        let (_, format, _) = crate::manifest::spec_group(group).unwrap();
        let Some(adapter) = registry.get(format.as_str()) else {
            continue;
        };
        for entry in loaded.manifest.group(group) {
            if entry.format != format {
                continue;
            }
            let absolute_path = resolve_spec_path(&loaded, &entry.path);
            let ctx = crate::adapters::AdapterContext {
                loaded: &loaded,
                file: entry.path.clone(),
                absolute_path,
            };
            match adapter.parse(&ctx) {
                Ok(result) => {
                    merge_into_graph(&mut graph, result, &mut entity_sources, &mut seen_edges)
                }
                Err(err) => diagnostics.push(Diagnostic {
                    severity: "error".into(),
                    code: "parse-failed".into(),
                    message: format!("{}: {err}", entry.path),
                    file: Some(entry.path.clone()),
                    line: None,
                    nodes: None,
                }),
            }
        }
    }

    // 实体约定匹配：未绑定的 SQL 表 ↔ 实体（orders ≡ Order）
    match_entities_by_convention(&mut graph, &mut diagnostics);
    // UI 表单绑定：boundSchemaId ↔ JSON Schema $id → 实体
    bind_forms_to_entities(&mut graph);

    Ok(LoadOutcome {
        loaded,
        graph,
        diagnostics,
        entity_sources,
    })
}

fn merge_into_graph(
    graph: &mut UnifiedSemanticGraph,
    result: AdapterResult,
    entity_sources: &mut HashMap<String, Vec<(String, String)>>,
    seen_edges: &mut HashSet<String>,
) {
    for node in result.nodes {
        if graph.has(&node.uri) {
            let existing = graph.get(&node.uri).unwrap();
            if existing.kind != node.kind || existing.standard == node.standard {
                // 同标准重复定义 → 保留首个，记录冲突供诊断
                entity_sources
                    .entry(node.uri.clone())
                    .or_default()
                    .push((node.source.file.clone(), node.standard.as_str().to_string()));
                continue;
            }
            entity_sources
                .entry(node.uri.clone())
                .or_default()
                .push((node.source.file.clone(), node.standard.as_str().to_string()));
            continue;
        }
        if node.kind == NodeKind::Entity {
            entity_sources.insert(
                node.uri.clone(),
                vec![(node.source.file.clone(), node.standard.as_str().to_string())],
            );
        }
        let _ = graph.add_node(node);
    }
    for edge in result.edges {
        let key = format!("{}|{}|{}", edge.from, edge.to, edge.kind.as_str());
        if seen_edges.contains(&key) {
            continue;
        }
        if !graph.has(&edge.from) || !graph.has(&edge.to) {
            continue;
        }
        seen_edges.insert(key);
        let _ = graph.add_edge(edge);
    }
}

fn match_entities_by_convention(
    graph: &mut UnifiedSemanticGraph,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let entity_uris: Vec<(String, String)> = graph
        .entities()
        .into_iter()
        .map(|e| (e.name.clone(), e.uri.clone()))
        .collect();
    let by_normalized: HashMap<String, String> = entity_uris
        .into_iter()
        .map(|(name, uri)| {
            (
                crate::identity::normalize_name(&crate::identity::singularize(&name)),
                uri,
            )
        })
        .collect();

    let table_uris: Vec<String> = graph
        .nodes_by_kind(NodeKind::Table)
        .into_iter()
        .map(|t| t.uri.clone())
        .collect();

    for table_uri in table_uris {
        let table = graph.get(&table_uri).unwrap();
        let bound = table
            .data
            .get("entityUri")
            .and_then(|v| v.as_str())
            .map(String::from);
        if bound.is_some() {
            // 已有 binding — 引擎补列绑定
            if let Some(e) = bound {
                if graph.has(&e) {
                    let _ = graph.add_edge(SemanticEdge {
                        from: e.clone(),
                        to: table_uri.clone(),
                        kind: EdgeKind::Defines,
                    });
                    match_columns_by_convention(graph, &e, &table_uri);
                }
            }
            continue;
        }
        let Some(matched) = by_normalized.get(&crate::identity::normalize_name(
            &crate::identity::singularize(&table.name),
        )) else {
            diagnostics.push(Diagnostic {
                severity: "warning".into(),
                code: "table-entity-unbound".into(),
                message: format!(
                    "SQL 表 \"{}\" 未绑定到任何实体（缺 x-forma-entity / bindings / 约定匹配）",
                    table.name
                ),
                file: Some(table.source.file.clone()),
                line: table.source.line,
                nodes: Some(vec![table_uri.clone()]),
            });
            continue;
        };
        let entity = matched.clone();
        let _ = graph.add_edge(SemanticEdge {
            from: entity.clone(),
            to: table_uri.clone(),
            kind: EdgeKind::Defines,
        });
        for edge in graph
            .edges_from(&table_uri)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
        {
            if edge.kind == EdgeKind::BelongsTo {
                let _ = graph.add_edge(SemanticEdge {
                    from: entity.clone(),
                    to: edge.to.clone(),
                    kind: EdgeKind::Defines,
                });
            }
        }
        match_columns_by_convention(graph, &entity, &table_uri);
    }
}

fn match_columns_by_convention(
    graph: &mut UnifiedSemanticGraph,
    entity_uri: &str,
    table_uri: &str,
) {
    // 实体字段（json-schema 来源）
    let field_uris: Vec<String> = graph
        .edges_from(entity_uri)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::BelongsTo)
        .map(|e| e.to.clone())
        .filter(|uri| graph.get(uri).map(|n| n.kind) == Some(NodeKind::EntityField))
        .collect();
    let field_by_name: HashMap<String, String> = field_uris
        .iter()
        .map(|uri| {
            let field = graph.get(uri).unwrap();
            (
                crate::identity::normalize_name(field.name.rsplit('.').next().unwrap_or("")),
                uri.clone(),
            )
        })
        .collect();

    let column_uris: Vec<String> = graph
        .edges_from(table_uri)
        .into_iter()
        .filter(|e| e.kind == EdgeKind::BelongsTo)
        .map(|e| e.to.clone())
        .collect();

    for col_uri in column_uris {
        let column = graph.get(&col_uri).unwrap();
        let col_name = column.name.rsplit('.').next().unwrap_or("");
        if let Some(field_uri) = field_by_name.get(&crate::identity::normalize_name(col_name)) {
            let _ = graph.add_edge(SemanticEdge {
                from: field_uri.clone(),
                to: col_uri.clone(),
                kind: EdgeKind::Binds,
            });
        }
    }
}

fn bind_forms_to_entities(graph: &mut UnifiedSemanticGraph) {
    let form_uris: Vec<String> = graph
        .nodes_by_kind(NodeKind::UiForm)
        .into_iter()
        .map(|f| f.uri.clone())
        .collect();
    for form_uri in form_uris {
        let form = graph.get(&form_uri).unwrap();
        let Some(bound) = form.data.get("boundSchemaId").and_then(|v| v.as_str()) else {
            continue;
        };
        if graph
            .edges_to(&form_uri)
            .iter()
            .any(|e| e.kind == EdgeKind::References)
        {
            continue;
        }
        let schema_type = graph.nodes_by_kind(NodeKind::Type).into_iter().find(|n| {
            n.standard == Standard::JsonSchema
                && crate::identity::normalize_name(
                    n.data.get("$id").and_then(|v| v.as_str()).unwrap_or(""),
                ) == crate::identity::normalize_name(bound)
        });
        let Some(schema_type) = schema_type else {
            continue;
        };
        if let Some(entity) = schema_type.data.get("entityUri").and_then(|v| v.as_str()) {
            if graph.has(entity) {
                let _ = graph.add_edge(SemanticEdge {
                    from: entity.to_string(),
                    to: form_uri.clone(),
                    kind: EdgeKind::References,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct BuildCache {
    pub version: u8,
    pub entries: HashMap<String, CacheEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CacheEntry {
    pub output_hash: String,
    pub inputs: HashMap<String, String>,
}

pub struct GenerateContext<'a> {
    pub loaded: &'a LoadedManifest,
    pub graph: &'a UnifiedSemanticGraph,
    /// 限定只生成与这些实体相关的产物；None = 全部
    pub entities: Option<Vec<String>>,
    /// 强制重建（忽略增量缓存）
    pub force: bool,
}

pub struct BuildOutcome {
    pub load: LoadOutcome,
    pub files: Vec<crate::generators::GeneratedFile>,
    pub skipped: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub cache_path: std::path::PathBuf,
}

pub fn build_project(options: &BuildOptions) -> Result<BuildOutcome> {
    let load = load_project(&options.project_dir)?;
    let mut hard_errors: Vec<Diagnostic> = load
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .cloned()
        .collect();
    hard_errors.extend(
        run_doctor(&load.graph)
            .into_iter()
            .filter(|d| d.severity == "error"),
    );

    let cache_path = options.project_dir.join(".forma").join("build.json");
    let mut cache = read_cache(&cache_path);

    let mut files: Vec<crate::generators::GeneratedFile> = Vec::new();
    let mut skipped = 0usize;
    // F-002：构建前快照受保护产物
    let protected = crate::artifacts::snapshot_protected(&options.project_dir)?;

    if hard_errors.is_empty() {
        let ctx = GenerateContext {
            loaded: &load.loaded,
            graph: &load.graph,
            entities: options.entities.clone(),
            force: options.force,
        };
        for generator in crate::generators::default_generators() {
            for file in generator.generate(&ctx)? {
                let abs = options.project_dir.join(&file.path);
                let output_hash = crate::util::sha256_hex(&file.content);
                let current_on_disk = if abs.is_file() {
                    crate::util::sha256_hex(&crate::util::read_text(&abs)?)
                } else {
                    String::new()
                };
                let up_to_date = cache
                    .entries
                    .get(&file.path)
                    .map(|e| e.output_hash == current_on_disk)
                    .unwrap_or(false);
                if !options.force && !options.dry_run && up_to_date {
                    skipped += 1;
                    continue;
                }
                if !options.dry_run {
                    crate::util::write_text(&abs, &file.content)?;
                    cache.entries.insert(
                        file.path.clone(),
                        CacheEntry {
                            output_hash,
                            inputs: HashMap::new(),
                        },
                    );
                }
                files.push(file);
            }
        }
        if !options.dry_run {
            crate::util::write_text(&cache_path, &serde_json::to_string_pretty(&cache)?)?;
        }
    }

    let mut diagnostics = load.diagnostics.clone();
    diagnostics.extend(run_doctor(&load.graph));
    // F-002：构建后恢复受保护产物 + 归属诊断
    if !options.dry_run {
        // 联邦工程无 DSL 源文件，来源规格按空值登记（保留哈希/归属语义）
        let prov = crate::artifacts::BuildProvenance::default();
        let (mut artifact_diags, _) = crate::artifacts::restore_protected(
            &options.project_dir,
            &protected,
            options.ownership.adopt,
            options.ownership.overwrite,
            options.ownership.strict,
            &files,
            &prov,
        )?;
        if options.ownership.strict {
            for d in &mut artifact_diags {
                d.severity = "error".into();
            }
        }
        diagnostics.extend(artifact_diags);
    }
    Ok(BuildOutcome {
        load,
        files,
        skipped,
        diagnostics,
        cache_path,
    })
}

pub struct BuildOptions {
    pub project_dir: std::path::PathBuf,
    pub force: bool,
    pub entities: Option<Vec<String>>,
    pub dry_run: bool,
    /// F-002 产物归属裁决（ADR-0003）：默认保留人工版本 + 警告
    pub ownership: crate::build::OwnershipPolicy,
}

fn read_cache(path: &Path) -> BuildCache {
    if path.is_file() {
        if let Ok(text) = crate::util::read_text(path) {
            if let Ok(v) = serde_json::from_str::<BuildCache>(&text) {
                if v.version == 1 {
                    return v;
                }
            }
        }
    }
    BuildCache {
        version: 1,
        entries: HashMap::new(),
    }
}

// ── DSL 工程检查（CLI `forma validate` / `doctor` 与 MCP `forma_check` 共用）────
//
// 治理要求（memory: forma-mcp-init-dsl / RFC-0003 M4）：CLI 与 MCP **不允许漂移**。
// 任何"检查 DSL 工程"的入口都必须走本节函数，否则会出现
// 「带语义错误的 DSL 工程 `forma validate` 仍退出 0」的假绿。

/// 是否为 DSL 工程（`forma.yaml` 带 `source`）。
pub fn is_dsl_project(project_dir: &Path) -> bool {
    load_manifest(project_dir)
        .map(|m| m.manifest.source.is_some())
        .unwrap_or(false)
}

/// DSL 工程的完整检查结果。
pub struct DslCheck {
    pub program: crate::dsl::Program,
    /// 解析 + 语义检查诊断（`dsl-*` / `viz-*`）
    pub diagnostics: Vec<Diagnostic>,
    /// 风格 lint（`lint::LintDiagnostic`，rule 级）
    pub lint: Vec<crate::lint::LintDiagnostic>,
    pub project_name: String,
    pub version: String,
}

impl DslCheck {
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .count()
    }
}

/// 加载并检查一个 DSL 工程。
///
/// **MUST NOT** 在未运行 `check_program` 的情况下报告"valid"——那正是被修掉的假绿。
pub fn dsl_check(project_dir: &Path) -> Result<DslCheck> {
    let loaded = load_manifest(project_dir)?;
    let Some(source) = loaded.manifest.source.clone() else {
        anyhow::bail!("not a DSL project (forma.yaml has no `source`)");
    };
    let (program, diagnostics) = crate::dsl::load_program(&loaded.dir, &source)?;
    let mut diagnostics = diagnostics;
    // 语义检查是编译管线第 3 阶段：类型/引用/规则/路由/权限/可视化的硬错误。
    diagnostics.extend(crate::dsl::check_program(&program));
    let lint = crate::lint::run_lint(&program, None);
    let version = program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| loaded.manifest.project_version.clone());
    Ok(DslCheck {
        program,
        diagnostics,
        lint,
        project_name: loaded.manifest.project_name,
        version,
    })
}

/// 统一的工程检查入口：DSL 工程走 `check_program` 管线，联邦工程走语义图管线。
///
/// CLI `forma validate` / `doctor` **必须**经由本函数分发——
/// 这是修掉「DSL 工程假绿」的那个分发点（RFC-0003 M4）。
pub enum ProjectCheck {
    Dsl(Box<DslCheck>),
    Federal(Box<LoadOutcome>),
}

pub fn check_project(project_dir: &Path) -> Result<ProjectCheck> {
    if is_dsl_project(project_dir) {
        Ok(ProjectCheck::Dsl(Box::new(dsl_check(project_dir)?)))
    } else {
        Ok(ProjectCheck::Federal(Box::new(load_project(project_dir)?)))
    }
}

/// DSL 工程的语义图（M3 桥接）：`Program → UnifiedSemanticGraph`。
///
/// 桥接是**单向**的（spec:ir §15 / ADR-0002）；联邦图不得反向成为规范表示。
/// CLI `forma graph` 与 MCP `forma_graph` 共用本函数，避免两份实现漂移。
pub fn dsl_graph(program: &crate::dsl::Program) -> crate::graph::GraphSummary {
    match crate::dsl::lower::lower_to_graph(program) {
        Ok(lowered) => lowered.graph.to_summary(),
        // 桥接失败不应让「查看模型」不可用——退化为空图并由调用方报告错误。
        Err(_) => crate::graph::UnifiedSemanticGraph::new().to_summary(),
    }
}

/// DSL 工程的语义图（带错误信息）——返回图对象本身（影响闭包需要边集合）。
pub fn dsl_graph_result(
    program: &crate::dsl::Program,
) -> anyhow::Result<(crate::graph::UnifiedSemanticGraph, String)> {
    let lowered = crate::dsl::lower::lower_to_graph(program)?;
    Ok((lowered.graph, lowered.ir_version))
}

/// DSL 工程的模型摘要（实体/枚举/API/页面/规则/图表）。
///
/// 权威实现在此；MCP `forma_graph` 与 CLI `forma graph` 共用，避免两份实现漂移。
pub fn dsl_model_summary(program: &crate::dsl::Program) -> serde_json::Value {
    let entities: Vec<serde_json::Value> = program
        .entities
        .iter()
        .map(|e| {
            let fields: Vec<serde_json::Value> = e
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "type": f.type_name,
                        "optional": f.optional,
                        "relation": crate::dsl::is_relation(program, &f.type_name),
                        "enum": crate::dsl::is_enum(program, &f.type_name),
                    })
                })
                .collect();
            serde_json::json!({
                "name": e.name,
                "table": crate::identity::to_snake_case(&crate::dsl::pluralize(&e.name)),
                "fields": fields,
                "indexes": e.indexes,
                "uniques": e.uniques,
            })
        })
        .collect();
    let enums: Vec<serde_json::Value> = program
        .enums
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "variants": e.variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let apis: Vec<serde_json::Value> = program
        .apis
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name,
                "entity": a.entity,
                "operations": a.operations,
            })
        })
        .collect();
    let pages: Vec<serde_json::Value> = program
        .pages
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "route": p.route,
                "kind": match p.body {
                    crate::dsl::PageBody::Table(_) => "table",
                    crate::dsl::PageBody::Form(_) => "form",
                    crate::dsl::PageBody::Detail(_) => "detail",
                    crate::dsl::PageBody::Chart(_) => "chart",
                    crate::dsl::PageBody::Dashboard(_) => "dashboard",
                },
            })
        })
        .collect();
    serde_json::json!({
        "entityCount": entities.len(),
        "entities": entities,
        "enums": enums,
        "apis": apis,
        "pages": pages,
        "ruleBlocks": program.rules.len(),
        "chartCount": program.charts.len(),
        "charts": program
            .charts
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "type": c.chart_type,
                    "source": c.source.as_ref().map(|(a, op)| format!("{a}.{op}")),
                })
            })
            .collect::<Vec<_>>(),
    })
}
