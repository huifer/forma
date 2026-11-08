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
