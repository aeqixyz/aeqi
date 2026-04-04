//! Git-native quest management with JSONL persistence and hierarchical IDs.
//!
//! Quests are organized as a DAG with prefix-based IDs (e.g., `ALG-1`, `ALG-1.1`),
//! support priorities, dependencies, assignees, and checkpoints. Parent quests
//! with children provide natural grouping.
//!
//! Key types: [`Quest`], [`QuestBoard`], [`QuestQuery`].

pub mod dependency_inference;
pub mod query;
pub mod quest;
pub mod store;

pub use dependency_inference::{InferredDependency, infer_dependencies};
pub use query::QuestQuery;
pub use quest::{
    Checkpoint, Priority, Quest, QuestId, QuestOutcomeKind, QuestOutcomeRecord, QuestStatus,
};
pub use store::QuestBoard;
