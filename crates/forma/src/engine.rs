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
