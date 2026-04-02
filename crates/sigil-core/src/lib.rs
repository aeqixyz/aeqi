//! Foundation crate for the Sigil agent runtime and control plane.
//!
//! Provides core traits ([`traits::Provider`], [`traits::Tool`], [`traits::Memory`],
//! [`traits::Observer`], [`traits::Channel`]), configuration loading ([`SigilConfig`]),
//! two-source identity assembly ([`Identity`]), the generic agent loop, and secret management.
//!
//! All other crates depend on `sigil-core` for trait definitions and shared types.

pub mod agent;
pub mod chat_stream;
pub mod checkpoint;
pub mod config;
pub mod identity;
pub mod security;
pub mod shell_hooks;
pub mod streaming_executor;
pub mod traits;

pub use agent::{
    Agent, AgentConfig, AgentResult, AgentStopReason, ContentReplacementState, LoopNotification,
    NotificationReceiver, NotificationSender, SessionState, SessionType,
};
pub use chat_stream::{ChatStreamEvent, ChatStreamSender};
pub use config::{
    AgentVoice, ContextBudgetConfig, ExecutionMode, MissionDef, PeerAgentConfig, ProjectConfig,
    ProviderKind, RuntimePresetConfig, SigilConfig, TeamConfig, discover_agents, load_agent_config,
};
pub use identity::Identity;
pub use security::SecretStore;
