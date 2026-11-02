//! 适配层（Emitter）集合：消费 DSL Program，生成各目标产物。

pub mod diff;
pub mod mysql;
pub mod openapi;
pub mod rust_backend;
pub mod web_react;

pub use diff::{diff_programs, emit_diff, DiffReport};
pub use mysql::emit_mysql;
pub use openapi::emit_openapi;
pub use rust_backend::emit_rust_backend;
pub use web_react::emit_web_react;

pub use super::identity::to_snake_case as snake;
pub use crate::dsl::{
    field_attr, find_entity, has_attr, is_enum, is_relation, load_program, pick_message, pluralize,
    BinOp, EntityDecl, Expr, FieldDecl, Program,
};
