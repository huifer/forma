//! 适配层（Emitter）集合：消费 DSL Program，生成各目标产物。

pub mod diagram;
pub mod diff;
pub mod flutter;
pub mod migrate;
pub mod mysql;
pub mod openapi;
pub mod replay;
pub mod rust_backend;
pub mod seed;
pub mod web_react;

pub use diagram::emit_diagrams;
pub use diff::{diff_programs, emit_diff, DiffReport};
pub use flutter::emit_flutter;
pub use migrate::{
    diff_down_sql, diff_ops, emit_migration, migration_down, migration_status, migration_up,
    migration_verify, MigrationEntry, MigrationLedger, MigrationOp,
};
pub use mysql::emit_mysql;
pub use openapi::emit_openapi;
pub use replay::{replay_sql, schema_diff, schema_from_plans, SimSchema, SimTable};
pub use rust_backend::emit_rust_backend;
pub use seed::emit_seed;
pub use web_react::emit_web_react;

pub use super::identity::to_snake_case as snake;
pub use crate::dsl::{
    field_attr, find_entity, has_attr, is_enum, is_relation, load_program, pick_message, pluralize,
    BinOp, EntityDecl, Expr, FieldDecl, Program,
};
