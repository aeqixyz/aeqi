use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Master configuration loaded from sigil.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigilConfig {
    pub sigil: SigilMeta,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub runtime_presets: HashMap<String, RuntimePresetConfig>,
    #[serde(default)]
    pub organizations: Vec<OrganizationConfig>,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    /// Global repo pool — all agents can access all repos.
    #[serde(default)]
    pub repos: HashMap<String, String>,
    /// Projects = repos, tasks, knowledge, budget.
    #[serde(default)]
    pub projects: Vec<ProjectConfig>,
    /// Agents = personalities (equal peers).
    #[serde(default)]
    pub agents: Vec<PeerAgentConfig>,
    /// System-level team settings (leader, router, background cost).
    #[serde(default)]
    pub team: TeamConfig,
    /// Session alarm and progress heartbeat settings.
    #[serde(default)]
    pub session: SessionConfig,
    /// Context budget limits for worker system prompts.
    #[serde(default)]
    pub context_budget: ContextBudgetConfig,
    /// Agent lifecycle engine settings (autonomous reflection, evolution, proactive scanning).
    #[serde(default)]
    pub lifecycle: LifecycleConfig,
    /// Orchestration tuning parameters (retries, timeouts, limits).
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    /// Web API server settings.
    #[serde(default)]
    pub web: WebConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigilMeta {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Default runtime preset name used when projects/agents don't override it.
    #[serde(default)]
    pub default_runtime: Option<String>,
    /// Patrol interval in seconds (daemon loop sleep).
    #[serde(default)]
    pub patrol_interval_secs: Option<u64>,
}

fn default_data_dir() -> String {
    "~/.sigil".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub openrouter: Option<OpenRouterConfig>,
    #[serde(default)]
    pub anthropic: Option<AnthropicConfig>,
    #[serde(default)]
    pub ollama: Option<OllamaConfig>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenRouter,
    Anthropic,
    Ollama,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::OpenRouter => "openrouter",
            Self::Anthropic => "anthropic",
            Self::Ollama => "ollama",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePresetConfig {
    pub provider: ProviderKind,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    pub api_key: String,
    #[serde(default = "default_openrouter_model")]
    pub default_model: String,
    #[serde(default)]
    pub fallback_model: Option<String>,
    #[serde(default)]
    pub embedding_model: Option<String>,
}

fn default_openrouter_model() -> String {
    "xiaomi/mimo-v2-pro".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: String,
    #[serde(default = "default_anthropic_model")]
    pub default_model: String,
}

fn default_anthropic_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub url: String,
    #[serde(default = "default_ollama_model")]
    pub default_model: String,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.1:8b".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_autonomy")]
    pub autonomy: Autonomy,
    #[serde(default = "default_true")]
    pub workspace_only: bool,
    #[serde(default = "default_max_cost")]
    pub max_cost_per_day_usd: f64,
    #[serde(default)]
    pub secret_store: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            autonomy: Autonomy::Supervised,
            workspace_only: true,
            max_cost_per_day_usd: 10.0,
            secret_store: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Autonomy {
    Readonly,
    Supervised,
    Full,
}

fn default_autonomy() -> Autonomy {
    Autonomy::Supervised
}

fn default_true() -> bool {
    true
}

fn default_max_cost() -> f64 {
    10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_memory_backend")]
    pub backend: String,
    #[serde(default = "default_embedding_dims")]
    pub embedding_dimensions: usize,
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f64,
    #[serde(default = "default_keyword_weight")]
    pub keyword_weight: f64,
    #[serde(default = "default_decay_halflife")]
    pub temporal_decay_halflife_days: f64,
    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f64,
    #[serde(default = "default_chunk_size")]
    pub chunk_size_tokens: usize,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap_tokens: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: "sqlite".to_string(),
            embedding_dimensions: 1536,
            vector_weight: 0.6,
            keyword_weight: 0.4,
            temporal_decay_halflife_days: 30.0,
            mmr_lambda: 0.7,
            chunk_size_tokens: 400,
            chunk_overlap_tokens: 80,
        }
    }
}

fn default_memory_backend() -> String {
    "sqlite".to_string()
}
fn default_embedding_dims() -> usize {
    1536
}
fn default_vector_weight() -> f64 {
    0.6
}
fn default_keyword_weight() -> f64 {
    0.4
}
fn default_decay_halflife() -> f64 {
    30.0
}
fn default_mmr_lambda() -> f64 {
    0.7
}
fn default_chunk_size() -> usize {
    400
}
fn default_chunk_overlap() -> usize {
    80
}

// ──────────────────────────────────────────────────────────────
// Agent configuration
// ──────────────────────────────────────────────────────────────

/// Role of an agent in the party.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AgentRole {
    /// Primary interface — orchestrates, routes, synthesizes.
    #[default]
    Orchestrator,
    /// Project worker — executes tasks on repos.
    Worker,
    /// Specialist advisor — provides perspective on demand.
    Advisor,
}

/// Whether an agent speaks in group channels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AgentVoice {
    /// Speaks in channels (Telegram, etc.).
    #[default]
    Vocal,
    /// Silent — injects context but doesn't post visible replies.
    Silent,
}

/// Configuration for a single agent (peer entity) in sigil.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAgentConfig {
    pub name: String,
    #[serde(default = "default_agent_prefix")]
    pub prefix: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Runtime preset name. If omitted, falls back to `[sigil].default_runtime`.
    #[serde(default, alias = "runtime_preset")]
    pub runtime: Option<String>,
    #[serde(default)]
    pub role: AgentRole,
    #[serde(default)]
    pub voice: AgentVoice,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default = "default_agent_max_workers")]
    pub max_workers: u32,
    #[serde(default)]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    /// Default repo key (from `[repos]`) this agent works in.
    #[serde(default)]
    pub default_repo: Option<String>,
    /// Domains this agent specializes in (for routing classifier).
    #[serde(default)]
    pub expertise: Vec<String>,
    /// Agent capabilities (e.g. "orchestration", "memory").
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Secret store key for this agent's Telegram bot token.
    #[serde(default)]
    pub telegram_token_secret: Option<String>,
}

fn default_agent_prefix() -> String {
    "ag".to_string()
}
fn default_agent_max_workers() -> u32 {
    1
}

/// System-level team settings — manages the overarching orchestrator and router.
/// This is the "system team" that coordinates across all projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// Name of the team leader (system-level orchestrator).
    #[serde(default = "default_leader")]
    pub leader: String,
    /// Agents in the system team (typically just the orchestrator).
    #[serde(default)]
    pub agents: Vec<String>,
    /// Model used for the router classifier.
    #[serde(default = "default_router_model")]
    pub router_model: String,
    /// Cooldown in seconds before same advisor can be re-invoked.
    #[serde(default = "default_router_cooldown")]
    pub router_cooldown_secs: u64,
    /// Max total cost across all background agents per message, in USD.
    #[serde(default = "default_max_background_cost")]
    pub max_background_cost_usd: f64,
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self {
            leader: "leader".to_string(),
            agents: Vec::new(),
            router_model: default_router_model(),
            router_cooldown_secs: 60,
            max_background_cost_usd: 0.50,
        }
    }
}

/// Per-project team assignment — which agents own this project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTeamConfig {
    /// Optional organization to resolve unit membership from.
    #[serde(default)]
    pub org: Option<String>,
    /// Optional unit inside the organization.
    #[serde(default)]
    pub unit: Option<String>,
    /// The team leader for this project.
    pub leader: String,
    /// Team members (includes leader). If empty, defaults to just the leader.
    #[serde(default)]
    pub agents: Vec<String>,
}

impl ProjectTeamConfig {
    /// Get the effective agent list (leader is always included).
    pub fn effective_agents(&self) -> Vec<String> {
        if self.agents.is_empty() {
            vec![self.leader.clone()]
        } else {
            let mut agents = self.agents.clone();
            if !agents.contains(&self.leader) {
                agents.insert(0, self.leader.clone());
            }
            agents
        }
    }
}

fn default_leader() -> String {
    "leader".to_string()
}
fn default_router_model() -> String {
    "xiaomi/mimo-v2-pro".to_string()
}
fn default_router_cooldown() -> u64 {
    60
}
fn default_max_background_cost() -> f64 {
    0.50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationConfig {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub mission: Option<String>,
    #[serde(default)]
    pub units: Vec<OrgUnitConfig>,
    #[serde(default)]
    pub roles: Vec<OrgRoleConfig>,
    #[serde(default)]
    pub relationships: Vec<OrgRelationshipConfig>,
    #[serde(default)]
    pub rituals: Vec<OrgRitualConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgUnitConfig {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub mission: Option<String>,
    #[serde(default)]
    pub lead: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgRoleConfig {
    pub agent: String,
    pub title: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub mandate: Option<String>,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub budget_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrgRelationshipKind {
    Manages,
    Advises,
    Reviews,
    DelegatesTo,
    EscalatesTo,
    CollaboratesWith,
}

impl std::fmt::Display for OrgRelationshipKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Manages => "manages",
            Self::Advises => "advises",
            Self::Reviews => "reviews",
            Self::DelegatesTo => "delegates_to",
            Self::EscalatesTo => "escalates_to",
            Self::CollaboratesWith => "collaborates_with",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgRelationshipConfig {
    pub from: String,
    pub to: String,
    pub kind: OrgRelationshipKind,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgRitualConfig {
    pub name: String,
    pub owner: String,
    #[serde(default)]
    pub cadence: Option<String>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentOrgContext {
    pub organization: String,
    pub organization_kind: Option<String>,
    pub organization_mission: Option<String>,
    pub title: Option<String>,
    pub unit: Option<String>,
    pub unit_mission: Option<String>,
    pub mandate: Option<String>,
    pub goals: Vec<String>,
    pub permissions: Vec<String>,
    pub budget_usd: Option<f64>,
    pub manager: Option<String>,
    pub direct_reports: Vec<String>,
    pub peers: Vec<String>,
    pub advisors: Vec<String>,
    pub reviewers: Vec<String>,
    pub escalation_targets: Vec<String>,
    pub delegates: Vec<String>,
    pub collaborators: Vec<String>,
    pub rituals: Vec<String>,
}

impl AgentOrgContext {
    pub fn as_identity_context(&self) -> String {
        let mut lines = vec![format!("Organization: {}", self.organization)];

        if let Some(ref kind) = self.organization_kind {
            lines.push(format!("Organization kind: {kind}"));
        }
        if let Some(ref mission) = self.organization_mission {
            lines.push(format!("Org mission: {mission}"));
        }
        if let Some(ref title) = self.title {
            lines.push(format!("Title: {title}"));
        }
        if let Some(ref unit) = self.unit {
            lines.push(format!("Unit: {unit}"));
        }
        if let Some(ref unit_mission) = self.unit_mission {
            lines.push(format!("Unit mission: {unit_mission}"));
        }
        if let Some(ref mandate) = self.mandate {
            lines.push(format!("Mandate: {mandate}"));
        }
        if !self.goals.is_empty() {
            lines.push(format!("Goals: {}", self.goals.join(", ")));
        }
        if !self.permissions.is_empty() {
            lines.push(format!("Permissions: {}", self.permissions.join(", ")));
        }
        if let Some(budget) = self.budget_usd {
            lines.push(format!("Budget authority: ${budget:.2}"));
        }
        if let Some(ref manager) = self.manager {
            lines.push(format!("Manager: {manager}"));
        }
        if !self.direct_reports.is_empty() {
            lines.push(format!(
                "Direct reports: {}",
                self.direct_reports.join(", ")
            ));
        }
        if !self.peers.is_empty() {
            lines.push(format!("Peers: {}", self.peers.join(", ")));
        }
        if !self.advisors.is_empty() {
            lines.push(format!("Advisors: {}", self.advisors.join(", ")));
        }
        if !self.reviewers.is_empty() {
            lines.push(format!("Reviewers: {}", self.reviewers.join(", ")));
        }
        if !self.escalation_targets.is_empty() {
            lines.push(format!(
                "Escalation targets: {}",
                self.escalation_targets.join(", ")
            ));
        }
        if !self.delegates.is_empty() {
            lines.push(format!("Delegates to: {}", self.delegates.join(", ")));
        }
        if !self.collaborators.is_empty() {
            lines.push(format!(
                "Collaborates with: {}",
                self.collaborators.join(", ")
            ));
        }
        if !self.rituals.is_empty() {
            lines.push(format!("Rituals: {}", self.rituals.join(" | ")));
        }

        format!("# Organizational Context\n\n{}", lines.join("\n"))
    }
}

// ──────────────────────────────────────────────────────────────
// Channel, Session, ExecutionMode, Project configs (unchanged)
// ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: Option<TelegramChannelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    pub token_secret: String,
    #[serde(default)]
    pub allowed_chats: Vec<i64>,
    #[serde(default = "default_debounce_window")]
    pub debounce_window_ms: u64,
    #[serde(default)]
    pub main_chat_id: Option<i64>,
    #[serde(default)]
    pub routes: Vec<TelegramChatRouteConfig>,
}

/// Routing rule for a Telegram chat.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramChatRouteConfig {
    pub chat_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
}

fn default_debounce_window() -> u64 {
    3000
}

/// Context budget limits for worker system prompts (char-based, ~4 chars/token).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudgetConfig {
    #[serde(default = "default_budget_max_shared_workflow")]
    pub max_shared_workflow: usize,
    #[serde(default = "default_budget_max_persona", alias = "max_soul")]
    pub max_persona: usize,
    #[serde(default = "default_budget_max_agents")]
    pub max_agents: usize,
    #[serde(default = "default_budget_max_knowledge")]
    pub max_knowledge: usize,
    #[serde(default = "default_budget_max_preferences")]
    pub max_preferences: usize,
    #[serde(default = "default_budget_max_memory")]
    pub max_memory: usize,
    #[serde(default = "default_budget_max_checkpoints")]
    pub max_checkpoints: usize,
    #[serde(default = "default_budget_max_checkpoint_count")]
    pub max_checkpoint_count: usize,
    #[serde(default = "default_budget_max_total")]
    pub max_total: usize,
}

impl Default for ContextBudgetConfig {
    fn default() -> Self {
        Self {
            max_shared_workflow: 2000,
            max_persona: 4000,
            max_agents: 8000,
            max_knowledge: 12000,
            max_preferences: 4000,
            max_memory: 8000,
            max_checkpoints: 8000,
            max_checkpoint_count: 5,
            max_total: 120000,
        }
    }
}

fn default_budget_max_shared_workflow() -> usize {
    2000
}
fn default_budget_max_persona() -> usize {
    4000
}
fn default_budget_max_agents() -> usize {
    8000
}
fn default_budget_max_knowledge() -> usize {
    12000
}
fn default_budget_max_preferences() -> usize {
    4000
}
fn default_budget_max_memory() -> usize {
    8000
}
fn default_budget_max_checkpoints() -> usize {
    8000
}
fn default_budget_max_checkpoint_count() -> usize {
    5
}
fn default_budget_max_total() -> usize {
    120000
}

/// Agent lifecycle engine — autonomous processes that make agents feel alive.
///
/// Controls memory reflection, personality evolution, proactive scanning,
/// and creative ideation. Each process is gated by agent bond level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_lifecycle_memory_interval")]
    pub memory_reflection_interval_hours: u32,
    #[serde(default = "default_lifecycle_evolution_interval")]
    pub evolution_interval_hours: u32,
    #[serde(default = "default_lifecycle_proactive_interval")]
    pub proactive_scan_interval_hours: u32,
    #[serde(default = "default_lifecycle_ideation_interval")]
    pub creative_ideation_interval_hours: u32,
    /// LLM model for lifecycle processes (cheap model preferred).
    #[serde(default)]
    pub model: Option<String>,
    /// Budget for EVOLUTION.md in worker context assembly (chars).
    #[serde(default = "default_lifecycle_evolution_budget")]
    pub evolution_budget: usize,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            memory_reflection_interval_hours: 4,
            evolution_interval_hours: 24,
            proactive_scan_interval_hours: 6,
            creative_ideation_interval_hours: 48,
            model: None,
            evolution_budget: 2000,
        }
    }
}

fn default_lifecycle_memory_interval() -> u32 {
    4
}
fn default_lifecycle_evolution_interval() -> u32 {
    24
}
fn default_lifecycle_proactive_interval() -> u32 {
    6
}
fn default_lifecycle_ideation_interval() -> u32 {
    48
}
fn default_lifecycle_evolution_budget() -> usize {
    2000
}

/// Session alarm and progress heartbeat configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_checkin_interval")]
    pub checkin_interval_mins: u64,
    #[serde(default = "default_alarm_interval")]
    pub alarm_interval_mins: u64,
    #[serde(default = "default_min_flood_interval")]
    pub min_flood_interval_mins: u64,
    #[serde(default)]
    pub deadline_mins: Option<u64>,
    #[serde(default)]
    pub notify_chat_id: Option<i64>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            checkin_interval_mins: 30,
            alarm_interval_mins: 60,
            min_flood_interval_mins: 30,
            deadline_mins: None,
            notify_chat_id: None,
        }
    }
}

fn default_checkin_interval() -> u64 {
    30
}
fn default_alarm_interval() -> u64 {
    60
}
fn default_min_flood_interval() -> u64 {
    30
}

/// Tunable orchestration parameters. All fields have sensible defaults matching
/// the previous hardcoded values, so existing configs work without changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Global kill switch for daemon-side background automation.
    #[serde(default = "default_background_automation_enabled")]
    pub background_automation_enabled: bool,
    /// Max resolution attempts at the project level before escalating to leader.
    #[serde(default = "default_max_resolution_attempts")]
    pub max_resolution_attempts: u32,
    /// Max task retries (handoff/failure) before auto-cancel.
    #[serde(default = "default_max_task_retries")]
    pub max_task_retries: u32,
    /// Shell tool timeout in seconds.
    #[serde(default = "default_shell_timeout_secs")]
    pub shell_timeout_secs: u64,
    /// Max task description size in chars before truncation.
    #[serde(default = "default_max_description_chars")]
    pub max_description_chars: usize,
    /// Max retries for transient worker execution failures.
    #[serde(default = "default_executor_max_retries")]
    pub executor_max_retries: u32,
    /// Dispatch TTL in seconds.
    #[serde(default = "default_dispatch_ttl_secs")]
    pub dispatch_ttl_secs: u64,
    /// Max council debate rounds.
    #[serde(default = "default_council_max_rounds")]
    pub council_max_rounds: u32,
    /// Enable expertise-based routing (Phase 2).
    #[serde(default)]
    pub expertise_routing: bool,
    /// Blackboard transient entry TTL in hours (Phase 3).
    #[serde(default = "default_blackboard_transient_ttl_hours")]
    pub blackboard_transient_ttl_hours: u64,
    /// Blackboard durable entry TTL in days (Phase 3).
    #[serde(default = "default_blackboard_durable_ttl_days")]
    pub blackboard_durable_ttl_days: u64,
    /// Blackboard claim TTL in hours.
    #[serde(default = "default_blackboard_claim_ttl_hours")]
    pub blackboard_claim_ttl_hours: u64,
    /// Enable adaptive retry with failure analysis (Phase 4).
    #[serde(default)]
    pub adaptive_retry: bool,
    /// Model to use for failure analysis (Phase 4).
    #[serde(default)]
    pub failure_analysis_model: String,
    /// Enable pre-flight assessment before worker spawn (Phase 5).
    #[serde(default)]
    pub preflight_enabled: bool,
    /// Model to use for pre-flight assessment (Phase 5).
    #[serde(default)]
    pub preflight_model: String,
    /// Max cost for pre-flight assessment (Phase 5).
    #[serde(default = "default_preflight_max_cost_usd")]
    pub preflight_max_cost_usd: f64,
    /// Model for mission decomposition (Phase 6).
    #[serde(default)]
    pub decomposition_model: String,
    /// Enable auto-redecomposition on stalled missions (Phase 6).
    #[serde(default)]
    pub auto_redecompose: bool,
    /// Confidence threshold for inferred dependencies; 0.0 = disabled (Phase 7).
    #[serde(default)]
    pub infer_deps_threshold: f64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            background_automation_enabled: default_background_automation_enabled(),
            max_resolution_attempts: default_max_resolution_attempts(),
            max_task_retries: default_max_task_retries(),
            shell_timeout_secs: default_shell_timeout_secs(),
            max_description_chars: default_max_description_chars(),
            executor_max_retries: default_executor_max_retries(),
            dispatch_ttl_secs: default_dispatch_ttl_secs(),
            council_max_rounds: default_council_max_rounds(),
            expertise_routing: false,
            blackboard_transient_ttl_hours: default_blackboard_transient_ttl_hours(),
            blackboard_durable_ttl_days: default_blackboard_durable_ttl_days(),
            blackboard_claim_ttl_hours: default_blackboard_claim_ttl_hours(),
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            preflight_enabled: false,
            preflight_model: String::new(),
            preflight_max_cost_usd: default_preflight_max_cost_usd(),
            decomposition_model: String::new(),
            auto_redecompose: false,
            infer_deps_threshold: 0.0,
        }
    }
}

/// Web API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_web_bind")]
    pub bind: String,
    #[serde(default)]
    pub ui_dist_dir: Option<String>,
    #[serde(default)]
    pub cors_origins: Vec<String>,
    #[serde(default)]
    pub auth_secret: Option<String>,
    #[serde(default = "default_ws_poll_interval")]
    pub ws_poll_interval_secs: u64,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: default_web_bind(),
            ui_dist_dir: None,
            cors_origins: Vec::new(),
            auth_secret: None,
            ws_poll_interval_secs: default_ws_poll_interval(),
        }
    }
}

fn default_web_bind() -> String {
    "0.0.0.0:8400".to_string()
}

fn default_ws_poll_interval() -> u64 {
    5
}

fn default_background_automation_enabled() -> bool {
    true
}

fn default_max_resolution_attempts() -> u32 {
    1
}
fn default_max_task_retries() -> u32 {
    3
}
fn default_shell_timeout_secs() -> u64 {
    30
}
fn default_max_description_chars() -> usize {
    8000
}
fn default_executor_max_retries() -> u32 {
    3
}
fn default_dispatch_ttl_secs() -> u64 {
    3600
}
fn default_council_max_rounds() -> u32 {
    1
}
fn default_blackboard_transient_ttl_hours() -> u64 {
    24
}
fn default_blackboard_durable_ttl_days() -> u64 {
    7
}
fn default_blackboard_claim_ttl_hours() -> u64 {
    2
}
fn default_preflight_max_cost_usd() -> f64 {
    0.01
}

/// How workers execute quests.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Native Sigil agent loop.
    Agent,
    /// Delegate execution to Claude Code CLI.
    #[default]
    ClaudeCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub prefix: String,
    /// Repo path (absolute) or key into `[repos]`.
    pub repo: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Runtime preset name. If omitted, falls back to `[sigil].default_runtime`.
    #[serde(default, alias = "runtime_preset")]
    pub runtime: Option<String>,
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    #[serde(default)]
    pub worktree_root: Option<String>,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default = "default_max_turns")]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    #[serde(default = "default_worker_timeout")]
    pub worker_timeout_secs: u64,
    #[serde(default)]
    pub max_cost_per_day_usd: Option<f64>,
    /// Per-project team assignment. If None, falls back to system team.
    #[serde(default)]
    pub team: Option<ProjectTeamConfig>,
    /// Per-project orchestrator overrides. If None, falls back to global [orchestrator].
    #[serde(default)]
    pub orchestrator: Option<OrchestratorConfig>,
    /// Missions defined in project.toml via `[[missions]]`.
    #[serde(default)]
    pub missions: Vec<MissionDef>,
    /// Departments within this project (org chart hierarchy).
    #[serde(default)]
    pub departments: Vec<DepartmentConfig>,
}

/// A department within a project — defines a team channel with its own agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentConfig {
    pub name: String,
    #[serde(default)]
    pub lead: Option<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A mission definition from project.toml `[[missions]]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Cron schedule expression (e.g., "*/30 * * * *").
    #[serde(default)]
    pub schedule: Option<String>,
    /// Skill name to apply when executing this mission's tasks.
    #[serde(default)]
    pub skill: Option<String>,
    /// Arguments / prompt text passed to the worker.
    #[serde(default)]
    pub args: Option<String>,
}

fn default_max_workers() -> u32 {
    2
}
fn default_max_turns() -> Option<u32> {
    Some(25)
}
fn default_worker_timeout() -> u64 {
    1800
}

// ──────────────────────────────────────────────────────────────
// SigilConfig methods
// ──────────────────────────────────────────────────────────────

impl SigilConfig {
    fn built_in_runtime_preset(name: &str) -> Option<RuntimePresetConfig> {
        let provider = match name {
            "openrouter_agent" => (ProviderKind::OpenRouter, ExecutionMode::Agent),
            "anthropic_agent" => (ProviderKind::Anthropic, ExecutionMode::Agent),
            "ollama_agent" => (ProviderKind::Ollama, ExecutionMode::Agent),
            // Legacy preset names retained as aliases so existing configs degrade
            // cleanly to the native Sigil agent runtime.
            "openrouter_claude_code" => (ProviderKind::OpenRouter, ExecutionMode::Agent),
            "anthropic_claude_code" => (ProviderKind::Anthropic, ExecutionMode::Agent),
            "ollama_claude_code" => (ProviderKind::Ollama, ExecutionMode::Agent),
            _ => return None,
        };

        Some(RuntimePresetConfig {
            provider: provider.0,
            execution_mode: Some(provider.1),
            model: None,
        })
    }

    pub fn runtime_preset_named(&self, name: &str) -> Option<RuntimePresetConfig> {
        self.runtime_presets
            .get(name)
            .cloned()
            .or_else(|| Self::built_in_runtime_preset(name))
    }

    pub fn provider_is_configured(&self, provider: ProviderKind) -> bool {
        match provider {
            ProviderKind::OpenRouter => self.providers.openrouter.is_some(),
            ProviderKind::Anthropic => self.providers.anthropic.is_some(),
            ProviderKind::Ollama => self.providers.ollama.is_some(),
        }
    }

    pub fn default_provider_kind(&self) -> Option<ProviderKind> {
        if self.providers.openrouter.is_some() {
            Some(ProviderKind::OpenRouter)
        } else if self.providers.anthropic.is_some() {
            Some(ProviderKind::Anthropic)
        } else if self.providers.ollama.is_some() {
            Some(ProviderKind::Ollama)
        } else {
            None
        }
    }

    pub fn default_model_for_provider(&self, provider: ProviderKind) -> String {
        match provider {
            ProviderKind::OpenRouter => self
                .providers
                .openrouter
                .as_ref()
                .map(|cfg| cfg.default_model.clone())
                .unwrap_or_else(default_openrouter_model),
            ProviderKind::Anthropic => self
                .providers
                .anthropic
                .as_ref()
                .map(|cfg| cfg.default_model.clone())
                .unwrap_or_else(default_anthropic_model),
            ProviderKind::Ollama => self
                .providers
                .ollama
                .as_ref()
                .map(|cfg| cfg.default_model.clone())
                .unwrap_or_else(default_ollama_model),
        }
    }

    fn resolve_runtime_preset(
        &self,
        preset_name: Option<&str>,
        fallback_mode: ExecutionMode,
    ) -> RuntimePresetConfig {
        if let Some(name) = preset_name
            && let Some(mut preset) = self.runtime_preset_named(name)
        {
            if preset.execution_mode.is_none() {
                preset.execution_mode = Some(fallback_mode);
            }
            return preset;
        }

        RuntimePresetConfig {
            provider: self
                .default_provider_kind()
                .unwrap_or(ProviderKind::OpenRouter),
            execution_mode: Some(fallback_mode),
            model: None,
        }
    }

    pub fn runtime_for_project(&self, project_name: &str) -> RuntimePresetConfig {
        let project = self.project(project_name);
        let fallback_mode = project
            .map(|p| p.execution_mode.clone())
            .unwrap_or_default();
        let preset_name = project
            .and_then(|p| p.runtime.as_deref())
            .or(self.sigil.default_runtime.as_deref());
        self.resolve_runtime_preset(preset_name, fallback_mode)
    }

    pub fn runtime_for_agent(&self, agent_name: &str) -> RuntimePresetConfig {
        let agent = self.agent(agent_name);
        let fallback_mode = agent.map(|a| a.execution_mode.clone()).unwrap_or_default();
        let preset_name = agent
            .and_then(|a| a.runtime.as_deref())
            .or(self.sigil.default_runtime.as_deref());
        self.resolve_runtime_preset(preset_name, fallback_mode)
    }

    pub fn execution_mode_for_project(&self, project_name: &str) -> ExecutionMode {
        self.runtime_for_project(project_name)
            .execution_mode
            .unwrap_or_default()
    }

    pub fn execution_mode_for_agent(&self, agent_name: &str) -> ExecutionMode {
        self.runtime_for_agent(agent_name)
            .execution_mode
            .unwrap_or_default()
    }

    /// Load config from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        Self::parse(&content)
    }

    /// Parse config from TOML string.
    pub fn parse(content: &str) -> Result<Self> {
        let mut config: Self = toml::from_str(content).context("failed to parse sigil.toml")?;

        // Resolve environment variables in API keys.
        if let Some(ref mut or) = config.providers.openrouter {
            or.api_key = resolve_env(&or.api_key);
        }
        if let Some(ref mut a) = config.providers.anthropic {
            a.api_key = resolve_env(&a.api_key);
        }

        // Expand ~ in paths.
        config.sigil.data_dir = expand_tilde(&config.sigil.data_dir);
        for path in config.repos.values_mut() {
            *path = expand_tilde(path);
        }
        for project in &mut config.projects {
            project.repo = expand_tilde(&project.repo);
            if let Some(ref mut wt) = project.worktree_root {
                *wt = expand_tilde(wt);
            }
        }

        if let Some(ref mut ui_dist_dir) = config.web.ui_dist_dir {
            *ui_dist_dir = expand_tilde(&resolve_env(ui_dist_dir));
        }

        // Resolve environment variables in web auth secret.
        if let Some(ref secret) = config.web.auth_secret {
            config.web.auth_secret = Some(resolve_env(secret));
        }

        // Validate and warn (non-fatal — partial configs allowed during dev).
        let issues = config.validate();
        for issue in &issues {
            warn!(issue = %issue, "config validation warning");
        }

        Ok(config)
    }

    /// Find config by searching upward from cwd, then ~/.sigil/.
    pub fn discover() -> Result<(Self, PathBuf)> {
        if let Ok(path) = std::env::var("SIGIL_CONFIG") {
            let path = PathBuf::from(path);
            return Ok((Self::load(&path)?, path));
        }

        let config_names = ["sigil.toml"];

        if let Ok(cwd) = std::env::current_dir() {
            let mut dir = cwd.as_path();
            loop {
                for name in &config_names {
                    let candidate = dir.join(name);
                    if candidate.exists() {
                        return Ok((Self::load(&candidate)?, candidate));
                    }
                    let candidate = dir.join(format!("config/{name}"));
                    if candidate.exists() {
                        return Ok((Self::load(&candidate)?, candidate));
                    }
                }
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => break,
                }
            }
        }

        if let Some(home) = dirs::home_dir() {
            for name in &config_names {
                let candidate = home.join(format!(".sigil/{name}"));
                if candidate.exists() {
                    return Ok((Self::load(&candidate)?, candidate));
                }
            }
        }

        anyhow::bail!("No sigil.toml found. Run 'sigil setup' to create one.")
    }

    /// Get project config by name.
    pub fn project(&self, name: &str) -> Option<&ProjectConfig> {
        self.projects.iter().find(|r| r.name == name)
    }

    /// Get agent config by name.
    pub fn agent(&self, name: &str) -> Option<&PeerAgentConfig> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// Get the default model for a project, falling back to provider default.
    pub fn model_for_project(&self, project_name: &str) -> String {
        let runtime = self.runtime_for_project(project_name);
        self.project(project_name)
            .and_then(|r| r.model.clone())
            .or(runtime.model)
            .unwrap_or_else(|| self.default_model_for_provider(runtime.provider))
    }

    /// Get the model for an agent, falling back to provider default.
    pub fn model_for_agent(&self, agent_name: &str) -> String {
        let runtime = self.runtime_for_agent(agent_name);
        self.agent(agent_name)
            .and_then(|a| a.model.clone())
            .or(runtime.model)
            .unwrap_or_else(|| self.default_model_for_provider(runtime.provider))
    }

    /// Get the effective orchestrator config for a project (project override or global).
    pub fn orchestrator_for_project(&self, project_name: &str) -> OrchestratorConfig {
        self.project(project_name)
            .and_then(|p| p.orchestrator.clone())
            .unwrap_or_else(|| self.orchestrator.clone())
    }

    /// Resolve the data directory path.
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.sigil.data_dir)
    }

    /// Get the team leader agent config (point-of-contact).
    pub fn leader_agent(&self) -> Option<&PeerAgentConfig> {
        self.agent(&self.team.leader).or_else(|| {
            self.agents_with_role(AgentRole::Orchestrator)
                .first()
                .copied()
        })
    }

    /// Get all agents with a specific role.
    pub fn agents_with_role(&self, role: AgentRole) -> Vec<&PeerAgentConfig> {
        self.agents.iter().filter(|a| a.role == role).collect()
    }

    /// Get all advisor agents (convenience).
    pub fn advisor_agents(&self) -> Vec<&PeerAgentConfig> {
        self.agents_with_role(AgentRole::Advisor)
    }

    fn organization_contains_agent(org: &OrganizationConfig, agent_name: &str) -> bool {
        org.roles.iter().any(|role| role.agent == agent_name)
            || org.units.iter().any(|unit| {
                unit.lead.as_deref() == Some(agent_name)
                    || unit.members.iter().any(|member| member == agent_name)
            })
            || org
                .relationships
                .iter()
                .any(|rel| rel.from == agent_name || rel.to == agent_name)
            || org.rituals.iter().any(|ritual| {
                ritual.owner == agent_name
                    || ritual
                        .participants
                        .iter()
                        .any(|participant| participant == agent_name)
            })
    }

    pub fn organization(&self, name: &str) -> Option<&OrganizationConfig> {
        self.organizations.iter().find(|org| org.name == name)
    }

    pub fn default_organization(&self) -> Option<&OrganizationConfig> {
        self.organizations
            .iter()
            .find(|org| org.default)
            .or_else(|| self.organizations.first())
    }

    pub fn resolve_organization(&self, name: Option<&str>) -> Option<&OrganizationConfig> {
        name.and_then(|n| self.organization(n))
            .or_else(|| self.default_organization())
    }

    pub fn org_unit(&self, org_name: Option<&str>, unit_name: &str) -> Option<&OrgUnitConfig> {
        self.resolve_organization(org_name)
            .and_then(|org| org.units.iter().find(|unit| unit.name == unit_name))
    }

    pub fn organizations_for_agent(&self, agent_name: &str) -> Vec<&OrganizationConfig> {
        self.organizations
            .iter()
            .filter(|org| Self::organization_contains_agent(org, agent_name))
            .collect()
    }

    pub fn resolve_agent_organization(
        &self,
        agent_name: &str,
        preferred_org: Option<&str>,
    ) -> Option<&OrganizationConfig> {
        if let Some(org_name) = preferred_org {
            let org = self.organization(org_name)?;
            return Self::organization_contains_agent(org, agent_name).then_some(org);
        }

        let organizations = self.organizations_for_agent(agent_name);
        match organizations.len() {
            0 => None,
            1 => Some(organizations[0]),
            _ => {
                if let Some(default_org) = self.organizations.iter().find(|org| org.default)
                    && organizations
                        .iter()
                        .any(|org| org.name.as_str() == default_org.name.as_str())
                {
                    Some(default_org)
                } else {
                    None
                }
            }
        }
    }

    pub fn agent_org_context_for(
        &self,
        agent_name: &str,
        preferred_org: Option<&str>,
    ) -> Option<AgentOrgContext> {
        let org = self.resolve_agent_organization(agent_name, preferred_org)?;

        let role = org.roles.iter().find(|role| role.agent == agent_name);
        let unit = role
            .and_then(|role| role.unit.as_deref())
            .and_then(|name| org.units.iter().find(|unit| unit.name == name))
            .or_else(|| {
                org.units.iter().find(|unit| {
                    unit.lead.as_deref() == Some(agent_name)
                        || unit.members.iter().any(|member| member == agent_name)
                })
            });

        let unit_name = role
            .and_then(|role| role.unit.clone())
            .or_else(|| unit.map(|unit| unit.name.clone()));

        let mut direct_reports = org
            .relationships
            .iter()
            .filter(|rel| rel.from == agent_name && rel.kind == OrgRelationshipKind::Manages)
            .map(|rel| rel.to.clone())
            .collect::<Vec<_>>();
        if let Some(unit) = unit
            && unit.lead.as_deref() == Some(agent_name)
        {
            for member in &unit.members {
                if member != agent_name {
                    direct_reports.push(member.clone());
                }
            }
        }
        direct_reports.sort();
        direct_reports.dedup();

        let manager = org
            .relationships
            .iter()
            .find(|rel| rel.to == agent_name && rel.kind == OrgRelationshipKind::Manages)
            .map(|rel| rel.from.clone())
            .or_else(|| {
                unit.and_then(|unit| {
                    unit.lead
                        .as_ref()
                        .filter(|lead| lead.as_str() != agent_name)
                        .cloned()
                })
            });

        let mut peers = Vec::new();
        if let Some(unit) = unit {
            if let Some(ref lead) = unit.lead
                && lead != agent_name
            {
                peers.push(lead.clone());
            }
            peers.extend(
                unit.members
                    .iter()
                    .filter(|member| member.as_str() != agent_name)
                    .cloned(),
            );
        }
        if let Some(unit_name) = unit_name.as_deref() {
            peers.extend(
                org.roles
                    .iter()
                    .filter(|role| {
                        role.unit.as_deref() == Some(unit_name) && role.agent != agent_name
                    })
                    .map(|role| role.agent.clone()),
            );
        }
        peers.retain(|name| {
            name != agent_name
                && manager.as_deref() != Some(name.as_str())
                && !direct_reports.iter().any(|report| report == name)
        });
        peers.sort();
        peers.dedup();

        let related = |kind: OrgRelationshipKind, inbound: bool| {
            let mut names = org
                .relationships
                .iter()
                .filter_map(|rel| match (inbound, rel.kind == kind) {
                    (true, true) if rel.to == agent_name => Some(rel.from.clone()),
                    (false, true) if rel.from == agent_name => Some(rel.to.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            names.sort();
            names.dedup();
            names
        };

        let rituals = org
            .rituals
            .iter()
            .filter(|ritual| {
                ritual.owner == agent_name
                    || ritual
                        .participants
                        .iter()
                        .any(|participant| participant == agent_name)
            })
            .map(|ritual| {
                let cadence = ritual
                    .cadence
                    .as_deref()
                    .map(|c| format!(" [{c}]"))
                    .unwrap_or_default();
                format!("{}{}", ritual.name, cadence)
            })
            .collect::<Vec<_>>();

        Some(AgentOrgContext {
            organization: org.name.clone(),
            organization_kind: org.kind.clone(),
            organization_mission: org.mission.clone(),
            title: role.map(|role| role.title.clone()),
            unit: unit_name,
            unit_mission: unit.and_then(|unit| unit.mission.clone()),
            mandate: role.and_then(|role| role.mandate.clone()),
            goals: role.map(|role| role.goals.clone()).unwrap_or_default(),
            permissions: role
                .map(|role| role.permissions.clone())
                .unwrap_or_default(),
            budget_usd: role.and_then(|role| role.budget_usd),
            manager,
            direct_reports,
            peers,
            advisors: related(OrgRelationshipKind::Advises, true),
            reviewers: related(OrgRelationshipKind::Reviews, true),
            escalation_targets: related(OrgRelationshipKind::EscalatesTo, false),
            delegates: related(OrgRelationshipKind::DelegatesTo, false),
            collaborators: {
                let mut names = related(OrgRelationshipKind::CollaboratesWith, false);
                names.extend(related(OrgRelationshipKind::CollaboratesWith, true));
                names.sort();
                names.dedup();
                names
            },
            rituals,
        })
    }

    pub fn agent_org_context(&self, agent_name: &str) -> Option<AgentOrgContext> {
        self.agent_org_context_for(agent_name, None)
    }

    /// Resolve a repo key to a path. If the key exists in `[repos]`, return that.
    /// Otherwise treat it as a raw path.
    pub fn resolve_repo(&self, key: &str) -> PathBuf {
        self.repos
            .get(key)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(key))
    }

    /// Resolve all repos to paths.
    pub fn resolve_all_repos(&self) -> HashMap<String, PathBuf> {
        self.repos
            .iter()
            .map(|(k, v)| (k.clone(), PathBuf::from(v)))
            .collect()
    }

    /// Get the effective team for a project.
    /// Returns the project's own team if configured, otherwise builds one from the system team.
    pub fn project_team(&self, project_name: &str) -> ProjectTeamConfig {
        if let Some(project) = self.project(project_name)
            && let Some(ref team) = project.team
        {
            let mut resolved = team.clone();
            if let Some(ref unit_name) = team.unit
                && let Some(unit) = self.org_unit(team.org.as_deref(), unit_name)
            {
                let mut agents = resolved.agents.clone();
                if let Some(ref lead) = unit.lead
                    && lead != &resolved.leader
                {
                    agents.push(lead.clone());
                }
                agents.extend(unit.members.iter().cloned());
                agents.sort();
                agents.dedup();
                resolved.agents = agents;
            }
            return resolved;
        }
        // Fall back to system team.
        ProjectTeamConfig {
            org: None,
            unit: None,
            leader: self.team.leader.clone(),
            agents: if self.team.agents.is_empty() {
                vec![self.team.leader.clone()]
            } else {
                self.team.agents.clone()
            },
        }
    }

    /// Get the system team leader agent name.
    pub fn leader(&self) -> &str {
        &self.team.leader
    }

    /// Validate that all team references point to defined agents.
    /// Skips validation if no agents are defined (they may be discovered from disk later).
    pub fn validate_teams(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let agent_names: std::collections::HashSet<&str> =
            self.agents.iter().map(|a| a.name.as_str()).collect();

        // Skip team validation if no agents defined yet (they'll be discovered from disk).
        if agent_names.is_empty() {
            return errors;
        }

        // Validate system team leader.
        if !agent_names.contains(self.team.leader.as_str()) {
            errors.push(format!(
                "system team leader '{}' is not a defined agent",
                self.team.leader
            ));
        }

        // Validate system team agents.
        for name in &self.team.agents {
            if !agent_names.contains(name.as_str()) {
                errors.push(format!("system team references unknown agent: '{name}'"));
            }
        }

        // Validate per-project teams.
        for project in &self.projects {
            if let Some(ref team) = project.team {
                if !agent_names.contains(team.leader.as_str()) {
                    errors.push(format!(
                        "project '{}' team leader '{}' is not a defined agent",
                        project.name, team.leader
                    ));
                }
                for name in &team.agents {
                    if !agent_names.contains(name.as_str()) {
                        errors.push(format!(
                            "project '{}' team references unknown agent: '{name}'",
                            project.name
                        ));
                    }
                }
            }
        }

        errors
    }

    pub fn validate_organizations(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut org_names = std::collections::HashSet::new();
        let agent_names: std::collections::HashSet<&str> = self
            .agents
            .iter()
            .map(|agent| agent.name.as_str())
            .collect();
        let default_count = self.organizations.iter().filter(|org| org.default).count();
        if default_count > 1 {
            errors.push("multiple organizations are marked as default".to_string());
        }

        for org in &self.organizations {
            if org.name.is_empty() {
                errors.push("organization with empty name".to_string());
            }
            if !org_names.insert(&org.name) {
                errors.push(format!("duplicate organization name: '{}'", org.name));
            }

            let mut unit_names = std::collections::HashSet::new();
            for unit in &org.units {
                if unit.name.is_empty() {
                    errors.push(format!(
                        "organization '{}' has a unit with empty name",
                        org.name
                    ));
                }
                if !unit_names.insert(&unit.name) {
                    errors.push(format!(
                        "organization '{}' has duplicate unit '{}'",
                        org.name, unit.name
                    ));
                }
                if let Some(ref lead) = unit.lead
                    && !agent_names.is_empty()
                    && !agent_names.contains(lead.as_str())
                {
                    errors.push(format!(
                        "organization '{}' unit '{}' references unknown lead '{}'",
                        org.name, unit.name, lead
                    ));
                }
                for member in &unit.members {
                    if !agent_names.is_empty() && !agent_names.contains(member.as_str()) {
                        errors.push(format!(
                            "organization '{}' unit '{}' references unknown member '{}'",
                            org.name, unit.name, member
                        ));
                    }
                }
            }

            for role in &org.roles {
                if !agent_names.is_empty() && !agent_names.contains(role.agent.as_str()) {
                    errors.push(format!(
                        "organization '{}' role '{}' references unknown agent '{}'",
                        org.name, role.title, role.agent
                    ));
                }
                if let Some(ref unit) = role.unit
                    && !org.units.iter().any(|u| u.name == *unit)
                {
                    errors.push(format!(
                        "organization '{}' role '{}' references unknown unit '{}'",
                        org.name, role.title, unit
                    ));
                }
            }

            for rel in &org.relationships {
                for endpoint in [&rel.from, &rel.to] {
                    if !agent_names.is_empty() && !agent_names.contains(endpoint.as_str()) {
                        errors.push(format!(
                            "organization '{}' relationship references unknown agent '{}'",
                            org.name, endpoint
                        ));
                    }
                }
            }

            for ritual in &org.rituals {
                if !agent_names.is_empty() && !agent_names.contains(ritual.owner.as_str()) {
                    errors.push(format!(
                        "organization '{}' ritual '{}' references unknown owner '{}'",
                        org.name, ritual.name, ritual.owner
                    ));
                }
                for participant in &ritual.participants {
                    if !agent_names.is_empty() && !agent_names.contains(participant.as_str()) {
                        errors.push(format!(
                            "organization '{}' ritual '{}' references unknown participant '{}'",
                            org.name, ritual.name, participant
                        ));
                    }
                }
            }
        }

        for project in &self.projects {
            if let Some(ref team) = project.team {
                if let Some(ref org_name) = team.org
                    && self.organization(org_name).is_none()
                {
                    errors.push(format!(
                        "project '{}' team references unknown organization '{}'",
                        project.name, org_name
                    ));
                }
                if let Some(ref unit_name) = team.unit
                    && self.org_unit(team.org.as_deref(), unit_name).is_none()
                {
                    let org_name = team.org.as_deref().unwrap_or("<default>");
                    errors.push(format!(
                        "project '{}' team references unknown org/unit '{}:{}'",
                        project.name, org_name, unit_name
                    ));
                }
            }
        }

        errors
    }

    /// Validate config for logical errors that serde can't catch.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.sigil.name.is_empty() {
            errors.push("system.name is empty".to_string());
        }

        // Project validation.
        let mut seen_names = std::collections::HashSet::new();
        let mut seen_prefixes = std::collections::HashSet::new();
        for d in &self.projects {
            if d.name.is_empty() {
                errors.push("project with empty name".to_string());
            }
            if d.prefix.is_empty() {
                errors.push(format!("project '{}' has empty prefix", d.name));
            }
            if !seen_names.insert(&d.name) {
                errors.push(format!("duplicate project name: '{}'", d.name));
            }
            if !seen_prefixes.insert(&d.prefix) {
                errors.push(format!("duplicate project prefix: '{}'", d.prefix));
            }
            if d.worker_timeout_secs == 0 {
                errors.push(format!("project '{}' has zero worker_timeout_secs", d.name));
            }
            if d.max_workers == 0 {
                errors.push(format!("project '{}' has zero max_workers", d.name));
            }
            if let Some(ref runtime) = d.runtime
                && self.runtime_preset_named(runtime).is_none()
            {
                errors.push(format!(
                    "project '{}' references unknown runtime preset: '{}'",
                    d.name, runtime
                ));
            }
        }

        // Agent validation.
        let orchestrator_count = self
            .agents
            .iter()
            .filter(|a| a.role == AgentRole::Orchestrator)
            .count();
        if !self.agents.is_empty() && orchestrator_count == 0 {
            errors.push("no orchestrator agent configured".to_string());
        }
        let mut seen_agent_names = std::collections::HashSet::new();
        let mut seen_agent_prefixes = std::collections::HashSet::new();
        for a in &self.agents {
            if a.name.is_empty() {
                errors.push("agent with empty name".to_string());
            }
            if !seen_agent_names.insert(&a.name) {
                errors.push(format!("duplicate agent name: '{}'", a.name));
            }
            if !a.prefix.is_empty() && !seen_agent_prefixes.insert(&a.prefix) {
                errors.push(format!("duplicate agent prefix: '{}'", a.prefix));
            }
            if let Some(ref runtime) = a.runtime
                && self.runtime_preset_named(runtime).is_none()
            {
                errors.push(format!(
                    "agent '{}' references unknown runtime preset: '{}'",
                    a.name, runtime
                ));
            }
        }

        if let Some(ref runtime) = self.sigil.default_runtime
            && self.runtime_preset_named(runtime).is_none()
        {
            errors.push(format!(
                "default runtime preset '{}' is not defined",
                runtime
            ));
        }

        for (name, preset) in &self.runtime_presets {
            if !self.provider_is_configured(preset.provider) {
                errors.push(format!(
                    "runtime preset '{}' references unconfigured provider '{}'",
                    name, preset.provider
                ));
            }
        }

        // Repo refs resolve.
        for d in &self.projects {
            if !d.repo.starts_with('/')
                && !d.repo.starts_with('~')
                && !self.repos.contains_key(&d.repo)
            {
                errors.push(format!(
                    "project '{}' references unknown repo key: '{}'",
                    d.name, d.repo
                ));
            }
        }

        // Memory weights.
        let weight_sum = self.memory.vector_weight + self.memory.keyword_weight;
        if (weight_sum - 1.0).abs() > 0.01 {
            errors.push(format!(
                "memory weights sum to {weight_sum:.2} (expected ~1.0): vector={}, keyword={}",
                self.memory.vector_weight, self.memory.keyword_weight
            ));
        }
        if self.memory.chunk_overlap_tokens >= self.memory.chunk_size_tokens {
            errors.push(format!(
                "chunk_overlap_tokens ({}) >= chunk_size_tokens ({})",
                self.memory.chunk_overlap_tokens, self.memory.chunk_size_tokens
            ));
        }

        // Budget sanity.
        if self.security.max_cost_per_day_usd <= 0.0 {
            errors.push("max_cost_per_day_usd must be positive".to_string());
        }

        // Team validation.
        errors.extend(self.validate_teams());
        errors.extend(self.validate_organizations());

        errors
    }
}

// ──────────────────────────────────────────────────────────────
// Agent discovery from disk
// ──────────────────────────────────────────────────────────────

/// Load a single agent's execution config from `agent_dir/agent.toml`.
pub fn load_agent_config(agent_dir: &Path) -> Result<PeerAgentConfig> {
    let path = agent_dir.join("agent.toml");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read agent.toml: {}", path.display()))?;
    let config: PeerAgentConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse agent.toml: {}", path.display()))?;
    Ok(config)
}

/// Discover all agents by scanning subdirectories of `agents_dir`.
/// Skips `shared` and any directory without an `agent.toml`.
/// Returns agents sorted by name for determinism.
pub fn discover_agents(agents_dir: &Path) -> Result<Vec<PeerAgentConfig>> {
    let mut agents = Vec::new();

    let entries = match std::fs::read_dir(agents_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(agents),
        Err(e) => {
            return Err(e).context(format!(
                "failed to read agents dir: {}",
                agents_dir.display()
            ));
        }
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // Skip shared directory.
        if dir_name == "shared" {
            continue;
        }
        let agent_toml = path.join("agent.toml");
        if !agent_toml.exists() {
            continue;
        }
        match load_agent_config(&path) {
            Ok(mut config) => {
                // Validate name matches directory.
                if config.name != dir_name {
                    warn!(
                        dir = %dir_name,
                        config_name = %config.name,
                        "agent.toml name doesn't match directory, using directory name"
                    );
                    config.name = dir_name;
                }
                agents.push(config);
            }
            Err(e) => {
                warn!(dir = %dir_name, error = %e, "failed to load agent.toml, skipping");
            }
        }
    }

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

impl SigilConfig {
    /// Discover agents from disk and merge with any `[[agents]]` in TOML config.
    /// Disk agents take precedence over TOML agents (by name).
    /// Returns warnings for overlaps.
    pub fn discover_and_merge_agents(&mut self, agents_dir: &Path) -> Vec<String> {
        let mut warnings = Vec::new();

        let disk_agents = match discover_agents(agents_dir) {
            Ok(a) => a,
            Err(e) => {
                warnings.push(format!("agent discovery failed: {e}"));
                return warnings;
            }
        };

        if disk_agents.is_empty() {
            // No agent.toml files found — TOML agents still work (backward compat).
            return warnings;
        }

        let disk_names: std::collections::HashSet<&str> =
            disk_agents.iter().map(|a| a.name.as_str()).collect();

        // Warn about overlaps (TOML agents that will be replaced by disk).
        for toml_agent in &self.agents {
            if disk_names.contains(toml_agent.name.as_str()) {
                warnings.push(format!(
                    "agent '{}' found in both [[agents]] and agents/{}/agent.toml — using disk version",
                    toml_agent.name, toml_agent.name,
                ));
            }
        }

        // Keep TOML agents that are NOT on disk, then add all disk agents.
        let mut merged: Vec<PeerAgentConfig> = self
            .agents
            .drain(..)
            .filter(|a| !disk_names.contains(a.name.as_str()))
            .collect();
        merged.extend(disk_agents);
        merged.sort_by(|a, b| a.name.cmp(&b.name));

        self.agents = merged;
        warnings
    }
}

/// Resolve ${ENV_VAR} patterns in strings.
fn resolve_env(s: &str) -> String {
    if s.starts_with("${") && s.ends_with('}') {
        let var_name = &s[2..s.len() - 1];
        std::env::var(var_name).unwrap_or_default()
    } else {
        s.to_string()
    }
}

/// Expand ~ to home directory.
fn expand_tilde(s: &str) -> String {
    if s.starts_with('~')
        && let Some(home) = dirs::home_dir()
    {
        return s.replacen('~', &home.to_string_lossy(), 1);
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
[sigil]
name = "test"

[[projects]]
name = "test-domain"
prefix = "td"
repo = "/tmp/test"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert_eq!(config.sigil.name, "test");
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects[0].name, "test-domain");
        assert!(config.web.ui_dist_dir.is_none());
    }

    #[test]
    fn test_web_ui_dist_dir_expands_tilde_and_env() {
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::set_var("SIGIL_UI_DIST_TEST", "~/sigil/apps/ui/dist") };
        let toml = r#"
[sigil]
name = "test"

[web]
ui_dist_dir = "${SIGIL_UI_DIST_TEST}"

[[projects]]
name = "test-domain"
prefix = "td"
repo = "/tmp/test"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            config.web.ui_dist_dir,
            Some(
                home.join("sigil/apps/ui/dist")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::remove_var("SIGIL_UI_DIST_TEST") };
    }

    #[test]
    fn test_resolve_env() {
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::set_var("TEST_SIGIL_VAR", "hello") };
        assert_eq!(resolve_env("${TEST_SIGIL_VAR}"), "hello");
        assert_eq!(resolve_env("plain"), "plain");
        unsafe { std::env::remove_var("TEST_SIGIL_VAR") };
    }

    #[test]
    fn test_validate_valid_config() {
        let toml = r#"
[sigil]
name = "test"

[team]
leader = "alice"

[[agents]]
name = "alice"
prefix = "al"
role = "orchestrator"

[[projects]]
name = "alpha"
prefix = "al"
repo = "/tmp/alpha"

[[projects]]
name = "beta"
prefix = "bt"
repo = "/tmp/beta"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn test_validate_duplicate_prefix() {
        let toml = r#"
[sigil]
name = "test"

[[projects]]
name = "alpha"
prefix = "ab"
repo = "/tmp/alpha"

[[projects]]
name = "beta"
prefix = "ab"
repo = "/tmp/beta"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(
            issues
                .iter()
                .any(|i| i.contains("duplicate project prefix")),
            "expected duplicate prefix: {issues:?}"
        );
    }

    #[test]
    fn test_validate_bad_memory_weights() {
        let toml = r#"
[sigil]
name = "test"

[memory]
vector_weight = 0.9
keyword_weight = 0.9
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(
            issues.iter().any(|i| i.contains("weights sum")),
            "expected weight warning: {issues:?}"
        );
    }

    #[test]
    fn test_validate_chunk_overlap_too_large() {
        let toml = r#"
[sigil]
name = "test"

[memory]
chunk_size_tokens = 100
chunk_overlap_tokens = 150
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(
            issues.iter().any(|i| i.contains("chunk_overlap_tokens")),
            "expected overlap warning: {issues:?}"
        );
    }

    #[test]
    fn test_parse_agents_config() {
        let toml = r#"
[sigil]
name = "test"

[repos]
sigil = "/home/user/sigil"
backend = "/home/user/backend"

[team]
leader = "alpha"

[[agents]]
name = "alpha"
prefix = "fa"
model = "claude-opus-4-6"
role = "orchestrator"
voice = "vocal"
default_repo = "sigil"

[[agents]]
name = "beta"
prefix = "fk"
role = "advisor"
voice = "vocal"
expertise = ["project-a"]

[[projects]]
name = "project-a"
prefix = "as"
repo = "/home/user/backend"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents[0].name, "alpha");
        assert_eq!(config.agents[0].role, AgentRole::Orchestrator);
        assert_eq!(config.agents[1].role, AgentRole::Advisor);
        assert_eq!(config.team.leader, "alpha");
        assert_eq!(config.repos.len(), 2);
        let leader = config.leader_agent().unwrap();
        assert_eq!(leader.name, "alpha");
        let advisors = config.advisor_agents();
        assert_eq!(advisors.len(), 1);
        assert_eq!(advisors[0].name, "beta");
    }

    #[test]
    fn test_resolve_repo() {
        let toml = r#"
[sigil]
name = "test"

[repos]
sigil = "/home/user/sigil"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert_eq!(
            config.resolve_repo("sigil"),
            PathBuf::from("/home/user/sigil")
        );
        assert_eq!(config.resolve_repo("/raw/path"), PathBuf::from("/raw/path"));
    }

    #[test]
    fn test_per_project_team_config() {
        let toml = r#"
[sigil]
name = "test"

[team]
leader = "alpha"
agents = ["alpha"]

[[agents]]
name = "alpha"
prefix = "au"
role = "orchestrator"

[[agents]]
name = "beta"
prefix = "fk"
role = "advisor"
expertise = ["project-a"]

[[agents]]
name = "gamma"
prefix = "fm"
role = "advisor"
expertise = ["project-b"]

[[projects]]
name = "project-a"
prefix = "as"
repo = "/tmp/algo"
team.leader = "beta"
team.agents = ["beta"]

[[projects]]
name = "project-b"
prefix = "rd"
repo = "/tmp/rift"
team.leader = "gamma"
team.agents = ["gamma"]

[[projects]]
name = "standalone"
prefix = "sa"
repo = "/tmp/standalone"
"#;
        let config = SigilConfig::parse(toml).unwrap();

        // Per-project teams.
        let algo_team = config.project_team("project-a");
        assert_eq!(algo_team.leader, "beta");
        assert_eq!(algo_team.effective_agents(), vec!["beta"]);

        let rift_team = config.project_team("project-b");
        assert_eq!(rift_team.leader, "gamma");
        assert_eq!(rift_team.effective_agents(), vec!["gamma"]);

        // Fallback to system team.
        let standalone_team = config.project_team("standalone");
        assert_eq!(standalone_team.leader, "alpha");
        assert_eq!(standalone_team.effective_agents(), vec!["alpha"]);

        // System leader.
        assert_eq!(config.leader(), "alpha");

        // Validation should pass.
        let issues = config.validate();
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn test_team_validation_unknown_agent() {
        let toml = r#"
[sigil]
name = "test"

[team]
leader = "alpha"

[[agents]]
name = "alpha"
prefix = "au"
role = "orchestrator"

[[projects]]
name = "alpha"
prefix = "al"
repo = "/tmp/alpha"
team.leader = "ghost"
team.agents = ["ghost"]
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let issues = config.validate_teams();
        assert!(
            issues.iter().any(|i| i.contains("ghost")),
            "expected team validation to flag unknown agent 'ghost': {issues:?}"
        );
    }

    #[test]
    fn test_project_team_effective_agents() {
        use super::ProjectTeamConfig;

        // Empty agents list → leader only.
        let team = ProjectTeamConfig {
            org: None,
            unit: None,
            leader: "beta".to_string(),
            agents: vec![],
        };
        assert_eq!(team.effective_agents(), vec!["beta"]);

        // Leader already in agents list.
        let team = ProjectTeamConfig {
            org: None,
            unit: None,
            leader: "beta".to_string(),
            agents: vec!["beta".to_string(), "gamma".to_string()],
        };
        assert_eq!(team.effective_agents(), vec!["beta", "gamma"]);

        // Leader not in agents list → prepended.
        let team = ProjectTeamConfig {
            org: None,
            unit: None,
            leader: "beta".to_string(),
            agents: vec!["gamma".to_string()],
        };
        assert_eq!(team.effective_agents(), vec!["beta", "gamma"]);
    }

    #[test]
    fn test_project_team_resolves_org_unit_members() {
        let toml = r#"
[sigil]
name = "test"

[team]
leader = "leader"
agents = ["leader"]

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[agents]]
name = "researcher"
prefix = "rs"
role = "advisor"

[[agents]]
name = "reviewer"
prefix = "rv"
role = "advisor"

[[organizations]]
name = "core"
default = true

[[organizations.units]]
name = "delivery"
lead = "leader"
members = ["researcher", "reviewer"]

[[projects]]
name = "sigil"
prefix = "sg"
repo = "/tmp/sigil"
team.org = "core"
team.unit = "delivery"
team.leader = "leader"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let team = config.project_team("sigil");
        assert_eq!(team.leader, "leader");
        assert_eq!(
            team.effective_agents(),
            vec!["leader", "researcher", "reviewer"]
        );
    }

    #[test]
    fn test_telegram_channel_routes_parse() {
        let toml = r#"
[sigil]
name = "test"

[channels.telegram]
token_secret = "TELEGRAM_TOKEN"
allowed_chats = [1001, 1002]
main_chat_id = 1002

[[channels.telegram.routes]]
chat_id = 1001
name = "Sigil HQ"

[[channels.telegram.routes]]
chat_id = 1002
project = "sigil"
department = "backend"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let telegram = config.channels.telegram.expect("telegram config");
        assert_eq!(telegram.main_chat_id, Some(1002));
        assert_eq!(telegram.routes.len(), 2);
        assert_eq!(telegram.routes[0].chat_id, 1001);
        assert_eq!(telegram.routes[0].name.as_deref(), Some("Sigil HQ"));
        assert_eq!(telegram.routes[1].project.as_deref(), Some("sigil"));
        assert_eq!(telegram.routes[1].department.as_deref(), Some("backend"));
    }

    #[test]
    fn test_agent_org_context_includes_unit_relationships_and_rituals() {
        let toml = r#"
[sigil]
name = "test"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[agents]]
name = "researcher"
prefix = "rs"
role = "advisor"

[[agents]]
name = "reviewer"
prefix = "rv"
role = "advisor"

[[organizations]]
name = "core"
kind = "workspace"
default = true
mission = "Ship high-quality work."

[[organizations.units]]
name = "delivery"
mission = "Turn plans into shipped work."
lead = "leader"
members = ["researcher", "reviewer"]

[[organizations.roles]]
agent = "leader"
title = "Head of Delivery"
unit = "delivery"
mandate = "Own execution quality."
goals = ["Keep work moving"]
permissions = ["delegate", "approve"]
budget_usd = 25.0

[[organizations.roles]]
agent = "researcher"
title = "Research Lead"
unit = "delivery"

[[organizations.relationships]]
from = "reviewer"
to = "leader"
kind = "advises"

[[organizations.relationships]]
from = "leader"
to = "researcher"
kind = "delegates_to"

[[organizations.relationships]]
from = "leader"
to = "reviewer"
kind = "collaborates_with"

[[organizations.rituals]]
name = "Daily Ops Review"
owner = "leader"
cadence = "daily"
participants = ["researcher", "reviewer"]
"#;
        let ctx = SigilConfig::parse(toml)
            .unwrap()
            .agent_org_context("leader")
            .expect("expected org context");
        assert_eq!(ctx.organization, "core");
        assert_eq!(ctx.organization_kind.as_deref(), Some("workspace"));
        assert_eq!(ctx.title.as_deref(), Some("Head of Delivery"));
        assert_eq!(ctx.unit.as_deref(), Some("delivery"));
        assert_eq!(ctx.manager, None);
        assert_eq!(ctx.direct_reports, vec!["researcher", "reviewer"]);
        assert!(ctx.peers.is_empty());
        assert_eq!(ctx.advisors, vec!["reviewer"]);
        assert_eq!(ctx.delegates, vec!["researcher"]);
        assert_eq!(ctx.collaborators, vec!["reviewer"]);
        assert_eq!(ctx.rituals, vec!["Daily Ops Review [daily]"]);
    }

    #[test]
    fn test_agent_org_context_does_not_assign_unrelated_default_org() {
        let toml = r#"
[sigil]
name = "test"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[agents]]
name = "outsider"
prefix = "ot"
role = "advisor"

[[organizations]]
name = "core"
default = true

[[organizations.roles]]
agent = "leader"
title = "Leader"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert!(config.agent_org_context("outsider").is_none());
    }

    #[test]
    fn test_agent_org_context_uses_preferred_org_when_membership_is_ambiguous() {
        let toml = r#"
[sigil]
name = "test"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[organizations]]
name = "core"
default = true

[[organizations.roles]]
agent = "leader"
title = "Leader"

[[organizations]]
name = "shadow"

[[organizations.roles]]
agent = "leader"
title = "Backup Leader"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert!(config.agent_org_context("leader").is_some());
        let shadow = config
            .agent_org_context_for("leader", Some("shadow"))
            .expect("expected shadow org context");
        assert_eq!(shadow.organization, "shadow");
        assert_eq!(shadow.title.as_deref(), Some("Backup Leader"));
    }

    #[test]
    fn test_agent_org_context_does_not_guess_without_default_or_preference() {
        let toml = r#"
[sigil]
name = "test"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[organizations]]
name = "core"

[[organizations.roles]]
agent = "leader"
title = "Leader"

[[organizations]]
name = "shadow"

[[organizations.roles]]
agent = "leader"
title = "Backup Leader"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert!(config.agent_org_context("leader").is_none());
        assert_eq!(
            config
                .agent_org_context_for("leader", Some("shadow"))
                .and_then(|ctx| ctx.title),
            Some("Backup Leader".to_string())
        );
    }

    #[test]
    fn test_team_agents_field() {
        let toml = r#"
[sigil]
name = "test"

[team]
leader = "alpha"
agents = ["alpha"]

[[agents]]
name = "alpha"
prefix = "au"
role = "orchestrator"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert_eq!(config.team.agents, vec!["alpha"]);
        assert_eq!(config.leader(), "alpha");
    }

    #[test]
    fn test_discover_agents_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");

        // Create agent directories with agent.toml.
        for (name, role) in &[("alice", "orchestrator"), ("bob", "advisor")] {
            let dir = agents_dir.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let toml = format!(
                "name = \"{name}\"\nprefix = \"{}\"\nrole = \"{role}\"\n",
                &name[..2],
            );
            std::fs::write(dir.join("agent.toml"), toml).unwrap();
        }

        // Create shared dir (should be skipped).
        std::fs::create_dir_all(agents_dir.join("shared")).unwrap();
        std::fs::write(agents_dir.join("shared/agent.toml"), "name = \"shared\"\n").unwrap();

        // Create dir without agent.toml (should be skipped).
        std::fs::create_dir_all(agents_dir.join("noconfig")).unwrap();

        let agents = discover_agents(&agents_dir).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "alice");
        assert_eq!(agents[0].role, AgentRole::Orchestrator);
        assert_eq!(agents[1].name, "bob");
        assert_eq!(agents[1].role, AgentRole::Advisor);
    }

    #[test]
    fn test_discover_agents_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        let agents = discover_agents(&agents_dir).unwrap();
        assert!(agents.is_empty());
    }

    #[test]
    fn test_discover_agents_nonexistent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let agents = discover_agents(&tmp.path().join("nope")).unwrap();
        assert!(agents.is_empty());
    }

    #[test]
    fn test_discover_and_merge_disk_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        let dir = agents_dir.join("alice");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("agent.toml"),
            "name = \"alice\"\nprefix = \"al\"\nrole = \"advisor\"\nmodel = \"disk-model\"\n",
        )
        .unwrap();

        let toml_str = r#"
[sigil]
name = "test"

[[agents]]
name = "alice"
prefix = "al"
role = "orchestrator"
model = "toml-model"

[[agents]]
name = "charlie"
prefix = "ch"
role = "advisor"
"#;
        let mut config = SigilConfig::parse(toml_str).unwrap();
        let warnings = config.discover_and_merge_agents(&agents_dir);

        // Disk alice should replace TOML alice.
        assert!(warnings.iter().any(|w| w.contains("alice")));
        assert_eq!(config.agents.len(), 2);

        let alice = config.agent("alice").unwrap();
        assert_eq!(alice.model.as_deref(), Some("disk-model"));
        assert_eq!(alice.role, AgentRole::Advisor); // disk version

        // Charlie from TOML should be preserved.
        assert!(config.agent("charlie").is_some());
    }

    #[test]
    fn test_discover_and_merge_backward_compat() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        // No agent.toml files — TOML agents should remain.

        let toml_str = r#"
[sigil]
name = "test"

[[agents]]
name = "alice"
prefix = "al"
role = "orchestrator"
"#;
        let mut config = SigilConfig::parse(toml_str).unwrap();
        let warnings = config.discover_and_merge_agents(&agents_dir);

        assert!(warnings.is_empty());
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].name, "alice");
    }

    #[test]
    fn test_load_agent_config_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path();
        let config = PeerAgentConfig {
            name: "test".to_string(),
            prefix: "tt".to_string(),
            model: Some("claude-opus-4-6".to_string()),
            runtime: Some("anthropic_agent".to_string()),
            role: AgentRole::Advisor,
            voice: AgentVoice::Vocal,
            execution_mode: ExecutionMode::Agent,
            max_workers: 2,
            max_turns: Some(15),
            max_budget_usd: Some(1.0),
            default_repo: Some("sigil".to_string()),
            expertise: vec!["testing".to_string()],
            capabilities: vec!["memory".to_string()],
            telegram_token_secret: Some("TOKEN".to_string()),
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        std::fs::write(agent_dir.join("agent.toml"), &toml_str).unwrap();

        let loaded = load_agent_config(agent_dir).unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.prefix, "tt");
        assert_eq!(loaded.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(loaded.runtime.as_deref(), Some("anthropic_agent"));
        assert_eq!(loaded.role, AgentRole::Advisor);
        assert_eq!(loaded.execution_mode, ExecutionMode::Agent);
        assert_eq!(loaded.max_workers, 2);
        assert_eq!(loaded.max_turns, Some(15));
        assert_eq!(loaded.expertise, vec!["testing"]);
    }

    #[test]
    fn test_runtime_resolution_uses_defaults() {
        let toml = r#"
[sigil]
name = "test"
default_runtime = "anthropic_agent"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-sonnet-4-20250514"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[projects]]
name = "sigil"
prefix = "sg"
repo = "/tmp/sigil"
"#;
        let config = SigilConfig::parse(toml).unwrap();

        let project_runtime = config.runtime_for_project("sigil");
        assert_eq!(project_runtime.provider, ProviderKind::Anthropic);
        assert_eq!(
            config.execution_mode_for_project("sigil"),
            ExecutionMode::Agent
        );
        assert_eq!(
            config.model_for_project("sigil"),
            "claude-sonnet-4-20250514".to_string()
        );

        let agent_runtime = config.runtime_for_agent("leader");
        assert_eq!(agent_runtime.provider, ProviderKind::Anthropic);
        assert_eq!(
            config.execution_mode_for_agent("leader"),
            ExecutionMode::Agent
        );
    }

    #[test]
    fn test_legacy_claude_runtime_alias_resolves_to_agent_mode() {
        let toml = r#"
[sigil]
name = "test"
default_runtime = "anthropic_claude_code"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-sonnet-4-20250514"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[projects]]
name = "sigil"
prefix = "sg"
repo = "/tmp/sigil"
execution_mode = "claude_code"
"#;
        let config = SigilConfig::parse(toml).unwrap();

        assert_eq!(
            config.execution_mode_for_project("sigil"),
            ExecutionMode::Agent
        );
        assert_eq!(
            config.execution_mode_for_agent("leader"),
            ExecutionMode::Agent
        );
    }

    #[test]
    fn test_runtime_validation_flags_unknown_preset() {
        let toml = r#"
[sigil]
name = "test"
default_runtime = "missing"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("default runtime preset 'missing'")),
            "expected unknown runtime error: {issues:?}"
        );
    }

    #[test]
    fn test_discover_agents_name_mismatch_corrected() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        let dir = agents_dir.join("alice");
        std::fs::create_dir_all(&dir).unwrap();
        // agent.toml has wrong name — should be corrected to dir name.
        std::fs::write(
            dir.join("agent.toml"),
            "name = \"wrong\"\nprefix = \"al\"\nrole = \"advisor\"\n",
        )
        .unwrap();

        let agents = discover_agents(&agents_dir).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "alice");
    }
}
