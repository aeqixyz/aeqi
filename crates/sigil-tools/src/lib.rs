//! Tool implementations for the `Tool` trait, available to agent workers.
//!
//! Provides shell execution ([`ShellTool`]), file read/write ([`FileReadTool`],
//! [`FileWriteTool`], [`ListDirTool`]), git worktree management ([`GitWorktreeTool`]),
//! task CRUD ([`TaskCreateTool`] et al.), cross-agent delegation ([`DelegateTool`]),
//! DNS management via Porkbun ([`PorkbunTool`]), and skill invocation ([`Skill`]).

pub mod delegate;
pub mod file;
pub mod git;
pub mod porkbun;
pub mod shell;
pub mod skill;
pub mod tasks;

pub use delegate::DelegateTool;
pub use file::{FileReadTool, FileWriteTool, ListDirTool};
pub use git::GitWorktreeTool;
pub use porkbun::PorkbunTool;
pub use shell::ShellTool;
pub use skill::Skill;
pub use tasks::{
    TaskCloseTool, TaskCreateTool, TaskDepTool, TaskReadyTool, TaskShowTool, TaskUpdateTool,
};
