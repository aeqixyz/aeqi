//! Agent orchestration engine — the operational heart of Sigil.
//!
//! Coordinates worker execution ([`AgentWorker`]), supervisor patrol ([`Supervisor`]),
//! Gemini Flash router classification ([`AgentRouter`]), project registry ([`ProjectRegistry`]),
//! dispatch bus ([`DispatchBus`]), cost ledger ([`CostLedger`]), Prometheus metrics
//! ([`SigilMetrics`]), lifecycle engine ([`LifecycleEngine`]), and conversation storage.
//!
//! Workers spawn via Claude Code (`claude -p`) with full tool access. The supervisor
//! enforces budgets and escalation chains (worker → project leader → system leader → human).

pub mod agent_router;
pub mod agent_worker;
pub mod audit;
pub mod blackboard;
pub mod chat_engine;
pub mod checkpoint;
pub mod context_budget;
pub mod conversation_store;
pub mod cost_ledger;
pub mod council;
pub mod daemon;
pub mod decomposition;
pub mod emotional_state;
pub mod escalation;
pub mod executor;
pub mod expertise;
pub mod failure_analysis;
pub mod heartbeat;
pub mod hook;
pub mod lifecycle;
pub mod message;
pub mod metrics;
pub mod middleware;
pub mod operation;
pub mod pipeline;
pub mod preflight;
pub mod project;
pub mod reflection;
pub mod registry;
pub mod schedule;
pub mod session_tracker;
pub mod supervisor;
pub mod template;
pub mod tools;
pub mod verification;
pub mod watchdog;

pub use agent_router::{AgentRouter, RouteDecision};
pub use agent_worker::{AgentWorker, WorkerState};
pub use audit::{AuditEvent, AuditLog, DecisionType};
pub use blackboard::Blackboard;
pub use chat_engine::ChatEngine;
pub use checkpoint::AgentCheckpoint;
pub use context_budget::ContextBudget;
pub use conversation_store::ConversationStore;
pub use cost_ledger::CostLedger;
pub use council::Council;
pub use daemon::Daemon;
pub use emotional_state::EmotionalState;
pub use executor::{ClaudeCodeExecutor, TaskOutcome};
pub use expertise::ExpertiseLedger;
pub use heartbeat::Heartbeat;
pub use hook::Hook;
pub use lifecycle::LifecycleEngine;
pub use message::{Dispatch, DispatchBus, DispatchHealth, DispatchKind};
pub use metrics::SigilMetrics;
pub use operation::{Operation, OperationStore};
pub use pipeline::{Pipeline, PipelineStep};
pub use project::Project;
pub use reflection::Reflection;
pub use registry::{ProjectRegistry, ProjectSummary, TeamSummary};
pub use schedule::{ScheduleStore, ScheduledJob};
pub use session_tracker::SessionTracker;
pub use supervisor::Supervisor;
pub use template::Template;
pub use watchdog::WatchdogEngine;
