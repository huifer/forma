//! Unified Semantic Graph — Forma 的内部统一模型。
//! 用户不手写它；Forma 从各标准规格文件解析归一得到。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Standard {
    Sql,
    JsonSchema,
    Openapi,
    JsonForms,
    Bpmn,
    Core,
}

impl Standard {
    pub fn as_str(&self) -> &'static str {
        match self {
            Standard::Sql => "sql",
            Standard::JsonSchema => "json-schema",
            Standard::Openapi => "openapi",
            Standard::JsonForms => "json-forms",
            Standard::Bpmn => "bpmn",
            Standard::Core => "core",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Entity,
    EntityField,
    Type,
    Table,
    TableColumn,
    Index,
    ApiOperation,
    UiForm,
    UiControl,
    Workflow,
    WorkflowStep,
    /// 可视化图表（spec:ir §3.1 可视化域 `ChartNode`；DSL `chart` 块）
    Chart,
    /// 版本化迁移链（schema 变更的物化载体；spec:compatibility §6.1）
    Migration,
    /// 定时任务（M10/P1：DSL `job` 块；调度域）
    Job,
    /// 规格规则（RFC-0003 §18：access 条件 / guards / field rules / checks）——
    /// 表达式级依赖的载体（规则 → 引用字段）
    Rule,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Entity => "entity",
            NodeKind::EntityField => "entity-field",
            NodeKind::Type => "type",
            NodeKind::Table => "table",
            NodeKind::TableColumn => "table-column",
            NodeKind::Index => "index",
            NodeKind::ApiOperation => "api-operation",
            NodeKind::UiForm => "ui-form",
            NodeKind::UiControl => "ui-control",
            NodeKind::Workflow => "workflow",
            NodeKind::WorkflowStep => "workflow-step",
            NodeKind::Chart => "chart",
            NodeKind::Migration => "migration",
            NodeKind::Job => "job",
            NodeKind::Rule => "rule",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    Defines,
    BelongsTo,
    Binds,
    References,
    Generates,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Defines => "defines",
            EdgeKind::BelongsTo => "belongs-to",
            EdgeKind::Binds => "binds",
            EdgeKind::References => "references",
            EdgeKind::Generates => "generates",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRef {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SemanticNode {
    pub uri: String,
    pub kind: NodeKind,
    pub name: String,
    pub standard: Standard,
    pub source: SourceRef,
    /// 各适配器的领域数据（JSON）
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SemanticEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

#[derive(Default)]
pub struct UnifiedSemanticGraph {
    nodes: BTreeMap<String, SemanticNode>,
    out_edges: HashMap<String, Vec<SemanticEdge>>,
    in_edges: HashMap<String, Vec<SemanticEdge>>,
}

impl UnifiedSemanticGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: SemanticNode) -> anyhow::Result<()> {
        if self.nodes.contains_key(&node.uri) {
            anyhow::bail!("duplicate node uri: {}", node.uri);
        }
        self.out_edges.entry(node.uri.clone()).or_default();
        self.in_edges.entry(node.uri.clone()).or_default();
        self.nodes.insert(node.uri.clone(), node);
        Ok(())
    }

    pub fn has(&self, uri: &str) -> bool {
        self.nodes.contains_key(uri)
    }

    pub fn get(&self, uri: &str) -> Option<&SemanticNode> {
        self.nodes.get(uri)
    }

    pub fn require(&self, uri: &str) -> anyhow::Result<&SemanticNode> {
        self.nodes
            .get(uri)
            .ok_or_else(|| anyhow::anyhow!("unknown node: {uri}"))
    }

    pub fn all_nodes(&self) -> Vec<&SemanticNode> {
        self.nodes.values().collect()
    }

    pub fn nodes_by_kind(&self, kind: NodeKind) -> Vec<&SemanticNode> {
        self.nodes.values().filter(|n| n.kind == kind).collect()
    }

    pub fn entities(&self) -> Vec<&SemanticNode> {
        self.nodes_by_kind(NodeKind::Entity)
    }

    pub fn add_edge(&mut self, edge: SemanticEdge) -> anyhow::Result<()> {
        if !self.nodes.contains_key(&edge.from) {
            anyhow::bail!("edge.from unknown node: {}", edge.from);
        }
        if !self.nodes.contains_key(&edge.to) {
            anyhow::bail!("edge.to unknown node: {}", edge.to);
        }
        self.out_edges
            .entry(edge.from.clone())
            .or_default()
            .push(edge.clone());
        self.in_edges.entry(edge.to.clone()).or_default().push(edge);
        Ok(())
    }

    pub fn edges_from(&self, uri: &str) -> Vec<&SemanticEdge> {
        self.out_edges
            .get(uri)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn edges_to(&self, uri: &str) -> Vec<&SemanticEdge> {
        self.in_edges
            .get(uri)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// 反向影响闭包：跨标准语义变更传播的引擎。
    /// 正向：defines/binds/generates（改语义 → 产物受影响）；
    /// 反向：references/belongs-to（改产物 → 引用者/父节点受影响）。
    pub fn affected_by(&self, uri: &str) -> Vec<&SemanticNode> {
        let mut visited = std::collections::HashSet::new();
        visited.insert(uri.to_string());
        let mut queue = vec![uri.to_string()];
        let mut result: Vec<&SemanticNode> = Vec::new();

        while let Some(current) = queue.pop() {
            if let Some(out) = self.out_edges.get(&current) {
                for edge in out {
                    if matches!(
                        edge.kind,
                        EdgeKind::Defines | EdgeKind::Binds | EdgeKind::Generates
                    ) && visited.insert(edge.to.clone())
                    {
                        queue.push(edge.to.clone());
                        if let Some(n) = self.nodes.get(&edge.to) {
                            result.push(n);
                        }
                    }
                }
            }
            if let Some(inp) = self.in_edges.get(&current) {
                for edge in inp {
                    if matches!(edge.kind, EdgeKind::References | EdgeKind::BelongsTo)
                        && visited.insert(edge.from.clone())
                    {
                        queue.push(edge.from.clone());
                        if let Some(n) = self.nodes.get(&edge.from) {
                            result.push(n);
                        }
                    }
                }
            }
        }
        result
    }

    /// 序列化摘要（CLI / Studio / MCP 消费）。
    pub fn to_summary(&self) -> GraphSummary {
        let mut nodes: Vec<SummaryNode> = self
            .nodes
            .values()
            .map(|n| SummaryNode {
                uri: n.uri.clone(),
                kind: n.kind.as_str().to_string(),
                name: n.name.clone(),
                standard: n.standard.as_str().to_string(),
                source: n.source.clone(),
            })
            .collect();
        nodes.sort_by(|a, b| a.uri.cmp(&b.uri));

        let mut edges: Vec<SummaryEdge> = Vec::new();
        for list in self.out_edges.values() {
            for e in list {
                edges.push(SummaryEdge {
                    from: e.from.clone(),
                    to: e.to.clone(),
                    kind: e.kind.as_str().to_string(),
                });
            }
        }
        edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
        GraphSummary { nodes, edges }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryNode {
    pub uri: String,
    pub kind: String,
    pub name: String,
    pub standard: String,
    pub source: SourceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSummary {
    pub nodes: Vec<SummaryNode>,
    pub edges: Vec<SummaryEdge>,
}
