//! Forma — Universal Specification Compiler (DSL 中间层)。
//!
//! 一套 DSL 描述前端、后端、数据库与规则，适配层编译为各平台完整代码。

pub mod adapters;
pub mod dsl;
pub mod emit;
pub mod engine;
pub mod generators;
pub mod graph;
pub mod identity;
pub mod lint;
pub mod manifest;
pub mod mcp;
pub mod patch;
pub mod scaffold;
pub mod util;
pub mod validate;

pub use dsl::Program;
pub use engine::{build_project, load_project, BuildOutcome, LoadOutcome};
pub use graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, Standard, UnifiedSemanticGraph};
pub use manifest::{load_manifest, FormaManifest, SpecEntry, MANIFEST_FILE};
pub use patch::{apply_patch, PatchDocument, PatchOperation, PatchReport};
pub use validate::{run_doctor, Diagnostic};
