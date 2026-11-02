//! Forma — Universal Specification Compiler (Specification Federation)。
//!
//! 不发明新 DSL：把 SQL DDL / JSON Schema / OpenAPI / JSON Forms / BPMN
//! 组织成统一语义图，提供跨标准校验、语义补丁与确定性编译。

pub mod adapters;
pub mod engine;
pub mod generators;
pub mod graph;
pub mod identity;
pub mod manifest;
pub mod mcp;
pub mod patch;
pub mod scaffold;
pub mod util;
pub mod validate;

pub use engine::{build_project, load_project, BuildOutcome, LoadOutcome};
pub use graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, Standard, UnifiedSemanticGraph};
pub use manifest::{load_manifest, FormaManifest, SpecEntry, MANIFEST_FILE};
pub use patch::{apply_patch, PatchDocument, PatchOperation, PatchReport};
pub use validate::{run_doctor, Diagnostic};
