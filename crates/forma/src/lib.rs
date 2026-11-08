//! Forma — Universal Specification Compiler (DSL 中间层)。
//!
//! 一套 DSL 描述前端、后端、数据库与规则，适配层编译为各平台完整代码。

pub mod adapters;
/// Agent 工程工具面（RFC-0006 P4）：search / get_context / plan / report。
pub mod agent;
/// 产物清单与人工修改检测（F-002 / ADR-0003）。
pub mod artifacts;
pub mod auditing;
/// Generated-project benchmark（F-008）：10×10 确定性任务矩阵。
pub mod bench;
pub mod build;
pub mod capability;
pub mod docs;
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
pub mod studio;
pub mod util;
pub mod validate;
/// 规则一致性向量（F-004）：从 DSL 规则派生跨端行为测试用例。
pub mod validation_vectors;
pub mod visualization;

pub use dsl::Program;
pub use engine::{build_project, load_project, BuildOutcome, LoadOutcome};
pub use graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, Standard, UnifiedSemanticGraph};
pub use manifest::{load_manifest, FormaManifest, SpecEntry, MANIFEST_FILE};
pub use patch::{apply_patch, PatchDocument, PatchOperation, PatchReport};
pub use validate::{run_doctor, Diagnostic};
