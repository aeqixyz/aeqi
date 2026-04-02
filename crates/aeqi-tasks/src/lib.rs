//! Git-native task management with JSONL persistence and hierarchical IDs.
//!
//! Tasks are organized as a DAG with prefix-based IDs (e.g., `ALG-1`, `ALG-1.1`),
//! support priorities, dependencies, assignees, and checkpoints. Parent tasks
//! with children provide natural grouping.
//!
//! Key types: [`Task`], [`TaskBoard`], [`TaskQuery`].

pub mod dependency_inference;
pub mod query;
pub mod store;
pub mod task;

pub use dependency_inference::{InferredDependency, infer_dependencies};
pub use query::TaskQuery;
pub use store::TaskBoard;
pub use task::{
    Checkpoint, Priority, Task, TaskId, TaskOutcomeKind, TaskOutcomeRecord, TaskStatus,
};
