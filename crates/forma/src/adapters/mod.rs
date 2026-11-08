pub mod bpmn;
pub mod jsonforms;
pub mod jsonschema;
pub mod openapi;
pub mod prisma_import;
pub mod sql;
pub mod sql_import;

use crate::graph::{SemanticEdge, SemanticNode};
use crate::manifest::LoadedManifest;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct AdapterContext<'a> {
    pub loaded: &'a LoadedManifest,
    /// 项目相对路径
    pub file: String,
    pub absolute_path: PathBuf,
}

/// RFC-0002 §14 S2：Lossy / Partial 导入方向的**机读损失清单**。
///
/// 逆向导入是生成视图，不保证 1:1 语义等价；所有被跳过或保守降级的构造
/// **必须**出现在报告里（同时在生成的 `.fm` 头部注释里可见），不得静默丢弃。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LossReport {
    /// 源格式：sql | prisma | openapi
    pub format: String,
    /// 源文件名（来源追踪）
    pub source: String,
    /// 生成的实体数
    pub entities: usize,
    /// 逐条损失项
    pub items: Vec<LossItem>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LossItem {
    /// 被跳过/降级的构造（如 `students.blob (BLOB)`）
    pub construct: String,
    /// 原因（如 `unsupported column type`）
    pub reason: String,
}

impl LossReport {
    pub fn is_lossy(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[derive(Default)]
pub struct AdapterResult {
    pub nodes: Vec<SemanticNode>,
    pub edges: Vec<SemanticEdge>,
}

pub trait SpecAdapter {
    fn standard(&self) -> crate::graph::Standard;
    fn parse(&self, ctx: &AdapterContext<'_>) -> Result<AdapterResult>;
}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Box<dyn SpecAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, adapter: Box<dyn SpecAdapter>) {
        self.adapters
            .insert(adapter.standard().as_str().to_string(), adapter);
    }

    pub fn get(&self, standard: &str) -> Option<&dyn SpecAdapter> {
        self.adapters.get(standard).map(|b| b.as_ref())
    }

    pub fn default_registry() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(jsonschema::JsonSchemaAdapter));
        reg.register(Box::new(openapi::OpenApiAdapter));
        reg.register(Box::new(jsonforms::JsonFormsAdapter));
        reg.register(Box::new(sql::SqlAdapter));
        reg.register(Box::new(bpmn::BpmnAdapter));
        reg
    }
}

pub fn file_display(base: &Path, full: &Path) -> String {
    full.strip_prefix(base)
        .unwrap_or(full)
        .to_string_lossy()
        .replace('\\', "/")
}
