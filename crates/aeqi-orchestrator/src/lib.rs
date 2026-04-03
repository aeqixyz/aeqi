#![allow(clippy::too_many_arguments)]
//! Agent orchestration engine — the operational heart of AEQI.
//!
//! Coordinates worker execution ([`AgentWorker`]), worker pool patrol ([`WorkerPool`]),
//! router classification ([`AgentRouter`]), company registry ([`CompanyRegistry`]),
//! dispatch bus ([`DispatchBus`]), cost ledger ([`CostLedger`]), Prometheus metrics
//! ([`AEQIMetrics`]), and session storage.
//!
//! Workers run through AEQI's native agent loop. The worker pool enforces budgets
//! and escalation chains (worker → project leader → system leader → human).

pub mod agent_registry;
pub mod agent_router;
pub mod agent_worker;
pub mod audit;
pub mod chat_engine;
pub mod checkpoint;
pub mod claude_code;
pub mod company;
pub mod context_budget;
pub mod session_store;
pub mod cost_ledger;
pub mod council;
pub mod daemon;
pub mod escalation;
pub mod execution_events;
pub mod executor;
pub mod expertise;
pub mod failure_analysis;
pub mod hook;
pub mod intent;
pub mod message;
pub mod metrics;
pub mod middleware;
pub mod notes;
pub mod operation;
pub mod pipeline;
pub mod preflight;
pub mod registry;
pub mod runtime;
pub mod session_tracker;
pub mod template;
pub mod tools;
pub mod trigger;
pub mod unified_delegate;
pub mod verification;
pub mod worker_pool;

pub use agent_registry::Department;
pub use agent_router::{AgentRouter, RouteDecision};
pub use agent_worker::{AgentWorker, WorkerState};
pub use audit::{AuditEvent, AuditLog, DecisionType};
pub use chat_engine::ChatEngine;
pub use checkpoint::AgentCheckpoint;
pub use company::Company;
pub use context_budget::ContextBudget;
pub use session_store::SessionStore;
pub use cost_ledger::CostLedger;
pub use council::Council;
pub use daemon::Daemon;
pub use execution_events::{EventBroadcaster, ExecutionEvent};
pub use executor::TaskOutcome;
pub use expertise::ExpertiseLedger;
pub use hook::Hook;
pub use message::{Dispatch, DispatchBus, DispatchHealth, DispatchKind};
pub use metrics::AEQIMetrics;
pub use notes::{AgentVisibility, Notes};
pub use operation::{Operation, OperationStore};
pub use pipeline::{Pipeline, PipelineStep};
pub use registry::{CompanyRegistry, CompanySummary};
pub use runtime::{
    Artifact, ArtifactKind, RuntimeExecution, RuntimeOutcome, RuntimeOutcomeStatus, RuntimePhase,
    RuntimeSession, RuntimeSessionStatus, VerificationReport,
};
pub use session_tracker::SessionTracker;
pub use template::Template;
pub use trigger::{EventPattern, Trigger, TriggerStore, TriggerType};
pub use worker_pool::WorkerPool;
