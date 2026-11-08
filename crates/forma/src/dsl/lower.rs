//! DSL `Program` → 联邦语义图桥接（M3 / [spec:ir §15](../../docs/architecture/specifications/ir/ir-contract.zh-CN.md)）。
//!
//! **桥接是单向的**：`Program → UnifiedSemanticGraph`。
//! 联邦图**不得**反向成为规范表示（[ADR-0002](../../docs/architecture/adr/accepted/ADR-0002-dsl-program-as-normative-ir.zh-CN.md)）。
//!
//! 本模块**不改动** `Program` 的既有字段语义，也不改动任何 emitter 契约——
//! 它只是把 DSL 声明**投影**成联邦侧的节点/边集合，让分析类工具
//! （`forma graph` / `forma impact`）对 DSL 工程可用。
//!
//! 节点与边的取集合联邦侧**同构**（spec:ir §13 Conformance）：
//!
//! | DSL 声明 | 节点 | URI |
//! |---|---|---|
//! | `entity E` | `Entity`（Core） | `forma://project/{p}/entity/{E}` |
//! | `entity E { f }` | `EntityField`（Core） | `…/entity/{E}/field/{f}` |
//! | `enum S` | `Type`（Core） | `forma://project/{p}/type/{S}` |
//! | `api A { operations [...] }` | `ApiOperation`（Core） | `…/api/{A}.{op}` |
//! | 命令（`api X { POST … }`） | `ApiOperation`（Core） | `…/api/{X}` |
//! | `page P` | `UiForm`（Core） | `…/ui/{P}` |
//! | 表单字段 | `UiControl`（Core） | `…/ui/{P}/control/{f}` |
//!
//! 边的语义与联邦侧一致（`graph.rs::affected_by` 的方向约定）：
//! `Defines` / `Binds` 向前传播（语义变更 → 产物受影响），
//! `References` / `BelongsTo` 向后传播（产物变更 → 引用者受影响）。

use crate::dsl::{is_enum, is_relation, ApiDecl, EntityDecl, PageBody, Program};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, SourceRef, Standard};
use crate::identity::{
    api_operation_uri, entity_field_uri, entity_uri, type_uri, ui_control_uri, ui_form_uri,
};

/// 桥接产物：语义图 + 桥接元信息。
pub struct LoweredGraph {
    pub graph: crate::graph::UnifiedSemanticGraph,
    /// 参与桥接的 IR 版本（M2）。
    pub ir_version: String,
    /// 项目名（URI 命名空间）。
    pub project: String,
}

/// 把 DSL `Program` 投影为联邦语义图。
///
/// 节点 URI 使用 `forma://project/{project}/…` 命名空间，与联邦适配器
/// （`identity.rs`）**同一套** URI 构造函数——这是「同构」的可验证判据。
pub fn lower_to_graph(program: &Program) -> anyhow::Result<LoweredGraph> {
    let project = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "project".to_string());
    let mut graph = crate::graph::UnifiedSemanticGraph::new();

    // ── 枚举 → Type 节点 ──
    for e in &program.enums {
        let uri = type_uri(&project, &e.name);
        graph.add_node(SemanticNode {
            uri,
            kind: NodeKind::Type,
            name: e.name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: e.file.clone().unwrap_or_default(),
                line: Some(e.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "kind": "enum",
                "variants": e.variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
            }),
        })?;
    }

    // ── 实体 + 字段节点 ──
    for entity in &program.entities {
        let ent_uri = entity_uri(&project, &entity.name);
        graph.add_node(SemanticNode {
            uri: ent_uri.clone(),
            kind: NodeKind::Entity,
            name: entity.name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: entity.file.clone().unwrap_or_default(),
                line: Some(entity.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "table": crate::identity::to_snake_case(&crate::dsl::pluralize(&entity.name)),
                "fields": entity.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                "indexes": entity.indexes,
                "uniques": entity.uniques,
            }),
        })?;
        for field in &entity.fields {
            let f_uri = entity_field_uri(&project, &entity.name, &field.name);
            graph.add_node(SemanticNode {
                uri: f_uri.clone(),
                kind: NodeKind::EntityField,
                name: field.name.clone(),
                standard: Standard::Core,
                source: SourceRef {
                    file: entity.file.clone().unwrap_or_default(),
                    line: Some(field.line),
                },
                data: serde_json::json!({
                    "from": "dsl",
                    "entity": entity.name,
                    "type": field.type_name,
                    "optional": field.optional,
                    "relation": is_relation(program, &field.type_name),
                    "enum": is_enum(program, &field.type_name),
                }),
            })?;
            // 实体 → 字段：语义变更向前传播（字段是实体的定义产物）
            graph.add_edge(SemanticEdge {
                from: ent_uri.clone(),
                to: f_uri.clone(),
                kind: EdgeKind::Defines,
            })?;
            // 字段 → 关系目标实体 / 枚举类型：引用（目标变更 → 本字段受影响）
            if is_relation(program, &field.type_name) {
                let target = entity_uri(&project, &field.type_name);
                if graph.has(&target) {
                    graph.add_edge(SemanticEdge {
                        from: f_uri.clone(),
                        to: target,
                        kind: EdgeKind::References,
                    })?;
                }
            } else if is_enum(program, &field.type_name) {
                let target = type_uri(&project, &field.type_name);
                if graph.has(&target) {
                    graph.add_edge(SemanticEdge {
                        from: f_uri.clone(),
                        to: target,
                        kind: EdgeKind::References,
                    })?;
                }
            }
        }
    }

    // ── 物理表 + 列节点（P3：影响闭包含表列）──
    // 复用 gen-mysql 的权威表计划（plan_tables），保证与 DDL emitter 同一命名与拓扑。
    for table in crate::emit::mysql::plan_tables(program) {
        let table_uri = crate::identity::local_uri("sql", "dsl", "table", &table.table);
        graph.add_node(SemanticNode {
            uri: table_uri.clone(),
            kind: NodeKind::Table,
            name: table.table.clone(),
            standard: Standard::Sql,
            source: SourceRef {
                file: program
                    .entities
                    .iter()
                    .find(|e| e.name == table.entity)
                    .and_then(|e| e.file.clone())
                    .unwrap_or_default(),
                line: program
                    .entities
                    .iter()
                    .find(|e| e.name == table.entity)
                    .map(|e| e.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "entity": table.entity,
                "primaryKey": table.primary_key,
                "uniques": table.uniques,
                "indexes": table.indexes,
                "foreignKeys": table.fks.iter().map(|fk| serde_json::json!({
                    "column": fk.column,
                    "refTable": fk.ref_table,
                })).collect::<Vec<_>>(),
            }),
        })?;
        // 实体 → 表：语义变更向前传播（模型变更 → 表受影响）
        let ent_uri = entity_uri(&project, &table.entity);
        if graph.has(&ent_uri) {
            graph.add_edge(SemanticEdge {
                from: ent_uri.clone(),
                to: table_uri.clone(),
                kind: EdgeKind::Defines,
            })?;
        }
        for col in &table.columns {
            let col_uri = crate::identity::local_uri(
                "sql",
                "dsl",
                "column",
                &format!("{}.{}", table.table, col.name),
            );
            graph.add_node(SemanticNode {
                uri: col_uri.clone(),
                kind: NodeKind::TableColumn,
                name: format!("{}.{}", table.table, col.name),
                standard: Standard::Sql,
                source: SourceRef {
                    file: program
                        .entities
                        .iter()
                        .find(|e| e.name == table.entity)
                        .and_then(|e| e.file.clone())
                        .unwrap_or_default(),
                    line: program
                        .entities
                        .iter()
                        .find(|e| e.name == table.entity)
                        .map(|e| e.line),
                },
                data: serde_json::json!({
                    "from": "dsl",
                    "table": table.table,
                    "sqlType": col.sql_type,
                    "nullable": col.nullable,
                    "unique": col.unique,
                }),
            })?;
            // 表 → 列：BelongsTo（与联邦 SQL 适配器同构）；实体 → 列：Defines
            graph.add_edge(SemanticEdge {
                from: table_uri.clone(),
                to: col_uri.clone(),
                kind: EdgeKind::BelongsTo,
            })?;
            if graph.has(&ent_uri) {
                graph.add_edge(SemanticEdge {
                    from: ent_uri.clone(),
                    to: col_uri,
                    kind: EdgeKind::Defines,
                })?;
            }
        }
    }

    // ── API 操作节点 ──
    for api in &program.apis {
        let ent_uri = entity_uri(&project, &api.entity);
        for op in &api.operations {
            let op_id = format!("{}.{}", api.name, op);
            let op_uri = api_operation_uri(&project, &op_id);
            graph.add_node(SemanticNode {
                uri: op_uri.clone(),
                kind: NodeKind::ApiOperation,
                name: op_id.clone(),
                standard: Standard::Core,
                source: SourceRef {
                    file: api.file.clone().unwrap_or_default(),
                    line: Some(api.line),
                },
                data: serde_json::json!({
                    "from": "dsl",
                    "api": api.name,
                    "operation": op,
                    "entity": api.entity,
                }),
            })?;
            // 操作 → 实体：引用（操作引用实体；实体语义变更 → 该操作受影响）
            if graph.has(&ent_uri) {
                graph.add_edge(SemanticEdge {
                    from: op_uri.clone(),
                    to: ent_uri.clone(),
                    kind: EdgeKind::References,
                })?;
            }
        }
    }

    // ── 命令（状态机动作）→ ApiOperation 节点 ──
    for cmd in &program.commands {
        let cmd_uri = api_operation_uri(&project, &cmd.name);
        let target_entity = cmd.effects.first().map(|e| e.entity.clone()).or_else(|| {
            program
                .entities
                .iter()
                .find(|e| e.name == cmd.name)
                .map(|e| e.name.clone())
        });
        graph.add_node(SemanticNode {
            uri: cmd_uri.clone(),
            kind: NodeKind::ApiOperation,
            name: cmd.name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: cmd.file.clone().unwrap_or_default(),
                line: Some(cmd.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "kind": "command",
                "method": cmd.method,
                "path": cmd.path,
                "entity": target_entity,
                "effects": cmd.effects.iter().map(|e| format!("{}.{}", e.entity, e.field)).collect::<Vec<_>>(),
                "audit": cmd.audit,
            }),
        })?;
        if let Some(entity) = target_entity {
            let ent_uri = entity_uri(&project, &entity);
            if graph.has(&ent_uri) {
                graph.add_edge(SemanticEdge {
                    from: cmd_uri.clone(),
                    to: ent_uri,
                    kind: EdgeKind::References,
                })?;
            }
        }
    }

    // ── 图表节点（spec:ir §3.1 可视化域 `ChartNode`；RFC-0006 P3：影响闭包的图表目标）──
    for chart in &program.charts {
        let chart_uri = crate::identity::chart_uri(&project, &chart.name);
        graph.add_node(SemanticNode {
            uri: chart_uri.clone(),
            kind: NodeKind::Chart,
            name: chart.name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: chart.file.clone().unwrap_or_default(),
                line: Some(chart.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "chartType": chart.chart_type,
                "source": chart.source.as_ref().map(|(a, op)| format!("{a}.{op}")),
            }),
        })?;
        // 图表 → API 操作：引用（API 数据契约变更 → 图表受影响）
        if let Some((api_name, op)) = &chart.source {
            let op_uri = api_operation_uri(&project, &format!("{api_name}.{op}"));
            if graph.has(&op_uri) {
                graph.add_edge(SemanticEdge {
                    from: chart_uri.clone(),
                    to: op_uri,
                    kind: EdgeKind::References,
                })?;
            }
            // 编码通道字段 → 实体字段：引用（字段变更 → 图表受影响）。
            // 聚合输出键（如 count 的 `value`）不对应实体字段 → 解析不到即跳过。
            let entity = program
                .apis
                .iter()
                .find(|a| a.name == *api_name)
                .map(|a| a.entity.clone())
                .and_then(|e| program.entities.iter().find(|e2| e2.name == e).cloned());
            if let Some(entity) = entity {
                for channel in [
                    chart.encoding.x.as_ref(),
                    chart.encoding.y.as_ref(),
                    chart.encoding.series.as_ref(),
                    chart.encoding.color.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    let Some(field) = entity.fields.iter().find(|f| f.name == channel.field) else {
                        continue;
                    };
                    let field_uri = entity_field_uri(&project, &entity.name, &field.name);
                    if graph.has(&field_uri) {
                        graph.add_edge(SemanticEdge {
                            from: chart_uri.clone(),
                            to: field_uri,
                            kind: EdgeKind::References,
                        })?;
                    }
                }
            }
        }
    }

    // ── 定时任务节点（M10/P1：调度域；影响闭包的 job 目标）──
    for job in &program.jobs {
        let job_uri = crate::identity::job_uri(&project, &job.name);
        graph.add_node(SemanticNode {
            uri: job_uri.clone(),
            kind: NodeKind::Job,
            name: job.name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: job.file.clone().unwrap_or_default(),
                line: Some(job.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "cron": job.cron,
                "op": job.op,
            }),
        })?;
        // job → api/command：引用（API 语义变更 → job 受影响；references 向后传播）
        // command 节点 URI = 裸名（lower.rs:269）；api 操作节点 = {api}.{op}
        let target_uri = if program.commands.iter().any(|c| c.name == job.op) {
            api_operation_uri(&project, &job.op)
        } else {
            api_operation_uri(&project, &format!("{}.list", job.op))
        };
        if graph.has(&target_uri) {
            graph.add_edge(SemanticEdge {
                from: job_uri.clone(),
                to: target_uri,
                kind: EdgeKind::References,
            })?;
        }
    }

    // ── 迁移链节点（RFC-0006 P3：影响闭包的迁移目标）──
    // schema 变更的物化载体是版本化迁移链（spec:compatibility §6.1）：
    // 改表 ⇒ 迁移链需要新的 000N_upgrade.sql。
    let migration_uri = crate::identity::migration_uri(&project, "chain");
    graph.add_node(SemanticNode {
        uri: migration_uri.clone(),
        kind: NodeKind::Migration,
        name: "migration-chain".to_string(),
        standard: Standard::Sql,
        source: SourceRef {
            file: String::new(),
            line: None,
        },
        data: serde_json::json!({
            "from": "dsl",
            "note": "versioned migration chain (0001_init + 000N_upgrade/rollback); schema changes require a new migration",
        }),
    })?;
    let table_uris: Vec<String> = graph
        .nodes_by_kind(NodeKind::Table)
        .iter()
        .map(|n| n.uri.clone())
        .collect();
    for uri in table_uris {
        // 表 → 迁移链：Generates（向前传播）——迁移链是表的派生物，
        // 改表 ⇒ 需要新的 000N_upgrade.sql（references 不会被向前遍历，踩过）。
        graph.add_edge(SemanticEdge {
            from: uri,
            to: migration_uri.clone(),
            kind: EdgeKind::Generates,
        })?;
    }

    // ── 页面 → UiForm 节点（+ 表单控件） ──
    for page in &program.pages {
        let form_uri = ui_form_uri(&project, &page.name);
        let mut bound_entities: Vec<String> = Vec::new();
        let mut bound_apis: Vec<String> = Vec::new();
        let mut bound_charts: Vec<String> = Vec::new();
        match &page.body {
            PageBody::Table(cfg) => {
                bound_apis.push(cfg.query_api.clone());
                if let Some(api) = program.apis.iter().find(|a| a.name == cfg.query_api) {
                    bound_entities.push(api.entity.clone());
                }
            }
            PageBody::Form(cfg) => {
                bound_entities.push(cfg.entity.clone());
            }
            PageBody::Detail(cfg) => {
                bound_entities.push(cfg.entity.clone());
            }
            PageBody::Chart(cfg) => {
                bound_charts.push(cfg.chart.clone());
            }
            PageBody::Dashboard(cfg) => {
                // V0.3：看板挂载多个图表（引用边）
                for item in &cfg.items {
                    bound_charts.push(item.chart.clone());
                }
            }
        }
        graph.add_node(SemanticNode {
            uri: form_uri.clone(),
            kind: NodeKind::UiForm,
            name: page.name.clone(),
            standard: Standard::Core,
            source: SourceRef {
                file: page.file.clone().unwrap_or_default(),
                line: Some(page.line),
            },
            data: serde_json::json!({
                "from": "dsl",
                "route": page.route,
                "body": page_body_kind(&page.body),
                "boundEntities": bound_entities,
                "boundApis": bound_apis,
                "boundCharts": bound_charts,
            }),
        })?;
        // 页面 → API 操作：引用（API 变更 → 页面受影响）
        for api_name in &bound_apis {
            let Some(api) = program.apis.iter().find(|a| a.name == *api_name) else {
                continue;
            };
            for op in &api.operations {
                let op_uri = api_operation_uri(&project, &format!("{}.{}", api.name, op));
                if graph.has(&op_uri) {
                    graph.add_edge(SemanticEdge {
                        from: form_uri.clone(),
                        to: op_uri,
                        kind: EdgeKind::References,
                    })?;
                }
            }
        }
        // 表单页 → 控件（BelongsTo，与联邦 JSONForms 侧同构）
        if let PageBody::Form(cfg) = &page.body {
            for field in &cfg.fields {
                let fname = field.path.rsplit('.').next().unwrap_or(&field.path);
                let ctrl_uri = ui_control_uri(&project, &page.name, fname);
                graph.add_node(SemanticNode {
                    uri: ctrl_uri.clone(),
                    kind: NodeKind::UiControl,
                    name: fname.to_string(),
                    standard: Standard::Core,
                    source: SourceRef {
                        file: page.file.clone().unwrap_or_default(),
                        line: Some(page.line),
                    },
                    data: serde_json::json!({
                        "from": "dsl",
                        "field": fname,
                        "widget": field.widget,
                        "required": field.required,
                    }),
                })?;
                graph.add_edge(SemanticEdge {
                    from: form_uri.clone(),
                    to: ctrl_uri,
                    kind: EdgeKind::BelongsTo,
                })?;
            }
        }
        // 页面 → 图表：引用（图表变更 → 页面受影响）
        for chart_name in &bound_charts {
            let uri = crate::identity::chart_uri(&project, chart_name);
            if graph.has(&uri) {
                graph.add_edge(SemanticEdge {
                    from: form_uri.clone(),
                    to: uri,
                    kind: EdgeKind::References,
                })?;
            }
        }
        // 页面 → 实体：引用（页面引用实体；实体变更 → 页面受影响）
        for entity in &bound_entities {
            let ent_uri = entity_uri(&project, entity);
            if graph.has(&ent_uri) {
                graph.add_edge(SemanticEdge {
                    from: form_uri.clone(),
                    to: ent_uri,
                    kind: EdgeKind::References,
                })?;
            }
        }
    }

    Ok(LoweredGraph {
        graph,
        ir_version: program.ir_version.clone(),
        project,
    })
}

fn page_body_kind(body: &PageBody) -> &'static str {
    match body {
        PageBody::Table(_) => "table",
        PageBody::Form(_) => "form",
        PageBody::Detail(_) => "detail",
        PageBody::Chart(_) => "chart",
        PageBody::Dashboard(_) => "dashboard",
    }
}

/// 便捷函数：实体名 → 该实体在图中的 URI（供 CLI / MCP 解析目标）。
pub fn entity_node_uri(program: &Program, entity: &str) -> String {
    let project = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "project".to_string());
    entity_uri(&project, entity)
}

/// 便捷函数：api 名 → 操作 URI（`{Api}.{op}`）。
pub fn api_operation_node_uri(program: &Program, api: &ApiDecl, op: &str) -> String {
    let project = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "project".to_string());
    api_operation_uri(&project, &format!("{}.{}", api.name, op))
}

/// 便捷函数：字段 → 字段 URI。
pub fn field_node_uri(program: &Program, entity: &EntityDecl, field: &str) -> String {
    let project = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "project".to_string());
    entity_field_uri(&project, &entity.name, field)
}
