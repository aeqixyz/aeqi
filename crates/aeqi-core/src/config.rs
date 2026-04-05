use crate::SecretStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Master configuration loaded from aeqi.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AEQIConfig {
    pub aeqi: AEQIMeta,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub runtime_presets: HashMap<String, RuntimePresetConfig>,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    /// Global repo pool — all agents can access all repos.
    #[serde(default)]
    pub repos: HashMap<String, String>,
    /// Shared primer injected into ALL agents regardless of project.
    #[serde(default)]
    pub shared_primer: Option<String>,
    /// Agent spawn configs = repos, tasks, knowledge, budget.
    #[serde(default, alias = "companies")]
    pub agent_spawns: Vec<AgentSpawnConfig>,
    /// Agents = personalities (equal peers).
    #[serde(default)]
    pub agents: Vec<PeerAgentConfig>,
    /// System-level team settings (leader, router, background cost).
    #[serde(default)]
    pub team: TeamConfig,
    /// Context budget limits for worker system prompts.
    #[serde(default)]
    pub context_budget: ContextBudgetConfig,
    /// Orchestration tuning parameters (retries, timeouts, limits).
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    /// Model tier mapping — agents declare intent (capable/balanced/fast/cheapest),
    /// this config resolves to actual model names.
    #[serde(default)]
    pub models: ModelTierConfig,
    /// Web API server settings.
    #[serde(default)]
    pub web: WebConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AEQIMeta {
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
    "~/.aeqi".to_string()
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

fn default_agent_role() -> String {
    "orchestrator".to_string()
}

/// Model tier mapping. Agents declare capability intent, config resolves to model names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTierConfig {
    /// Most capable model — architecture, security, complex decisions.
    #[serde(default = "default_model_capable")]
    pub capable: String,
    /// Balanced model — standard implementation, review, testing.
    #[serde(default = "default_model_balanced")]
    pub balanced: String,
    /// Fast/cheap model — simple queries, formatting, observation analysis.
    #[serde(default = "default_model_fast")]
    pub fast: String,
    /// Cheapest available — health checks, memory consolidation, status.
    #[serde(default = "default_model_cheapest")]
    pub cheapest: String,
}

impl Default for ModelTierConfig {
    fn default() -> Self {
        Self {
            capable: default_model_capable(),
            balanced: default_model_balanced(),
            fast: default_model_fast(),
            cheapest: default_model_cheapest(),
        }
    }
}

impl ModelTierConfig {
    /// Resolve a tier name to an actual model string.
    /// Falls back to balanced if tier is unknown.
    pub fn resolve(&self, tier: &str) -> &str {
        match tier {
            "capable" => &self.capable,
            "balanced" => &self.balanced,
            "fast" => &self.fast,
            "cheapest" => &self.cheapest,
            _ => {
                warn!(tier = %tier, "unknown model tier, falling back to balanced");
                &self.balanced
            }
        }
    }
}

fn default_model_capable() -> String {
    "anthropic/claude-sonnet-4-6".to_string()
}
fn default_model_balanced() -> String {
    "anthropic/claude-sonnet-4-6".to_string()
}
fn default_model_fast() -> String {
    "anthropic/claude-haiku-4-5".to_string()
}
fn default_model_cheapest() -> String {
    "anthropic/claude-haiku-4-5".to_string()
}

/// Configuration for a single agent (peer entity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAgentConfig {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "default_agent_prefix")]
    pub prefix: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Model capability tier: capable, balanced, fast, cheapest.
    /// Resolved to actual model name via [models] config section.
    /// Takes priority over `model` when both are set.
    #[serde(default)]
    pub model_tier: Option<String>,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default = "default_agent_role")]
    pub role: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default = "default_agent_max_workers")]
    pub max_workers: u32,
    #[serde(default)]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    #[serde(default)]
    pub default_repo: Option<String>,
    #[serde(default)]
    pub expertise: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub telegram_token_secret: Option<String>,
    /// Visual identity.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub faces: Option<HashMap<String, String>>,
    /// Triggers defined in agent template.
    #[serde(default)]
    pub triggers: Vec<AgentTriggerConfig>,
    /// System prompt — the agent's personality and instructions.
    #[serde(default)]
    pub prompt: Option<AgentPromptConfig>,
}

impl PeerAgentConfig {
    /// Resolve the actual model name.
    /// Priority: model_tier (via [models] config) > explicit model > balanced default.
    pub fn resolve_model(&self, tiers: &ModelTierConfig) -> String {
        if let Some(ref tier) = self.model_tier {
            return tiers.resolve(tier).to_string();
        }
        if let Some(ref model) = self.model {
            return model.clone();
        }
        tiers.balanced.clone()
    }
}

/// Trigger defined in agent template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTriggerConfig {
    pub name: String,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub skill: Option<String>,
}

/// Agent prompt configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptConfig {
    pub system: String,
}

fn default_agent_prefix() -> String {
    "ag".to_string()
}
fn default_agent_max_workers() -> u32 {
    1
}

/// System-level team settings — manages the overarching orchestrator.
/// This is the "system team" that coordinates across all projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// Name of the team leader (system-level orchestrator).
    #[serde(default = "default_leader")]
    pub leader: String,
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
            router_cooldown_secs: 60,
            max_background_cost_usd: 0.50,
        }
    }
}

fn default_leader() -> String {
    "leader".to_string()
}
fn default_router_cooldown() -> u64 {
    60
}
fn default_max_background_cost() -> f64 {
    0.50
}

// ──────────────────────────────────────────────────────────────
// Channel, ExecutionMode, Project configs (unchanged)
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
    #[serde(default = "default_budget_max_persona")]
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
    /// Notes transient entry TTL in hours (Phase 3).
    #[serde(default = "default_notes_transient_ttl_hours")]
    pub notes_transient_ttl_hours: u64,
    /// Notes durable entry TTL in days (Phase 3).
    #[serde(default = "default_notes_durable_ttl_days")]
    pub notes_durable_ttl_days: u64,
    /// Notes claim TTL in hours.
    #[serde(default = "default_notes_claim_ttl_hours")]
    pub notes_claim_ttl_hours: u64,
    /// Enable adaptive retry with failure analysis (Phase 4).
    #[serde(default)]
    pub adaptive_retry: bool,
    /// Model to use for failure analysis (Phase 4).
    #[serde(default)]
    pub failure_analysis_model: String,
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
            notes_transient_ttl_hours: default_notes_transient_ttl_hours(),
            notes_durable_ttl_days: default_notes_durable_ttl_days(),
            notes_claim_ttl_hours: default_notes_claim_ttl_hours(),
            adaptive_retry: false,
            failure_analysis_model: String::new(),
            infer_deps_threshold: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// No authentication — dashboard is open (self-hosted default).
    None,
    /// Shared secret passphrase (current behavior).
    #[default]
    Secret,
    /// Full user accounts with email/password + Google OAuth.
    Accounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: AuthMode,
    #[serde(default)]
    pub google_client_id: Option<String>,
    #[serde(default)]
    pub google_client_secret: Option<String>,
    /// Base URL for OAuth redirects (e.g. "https://app.aeqi.ai").
    #[serde(default)]
    pub base_url: Option<String>,
}

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
    #[serde(default)]
    pub auth: AuthConfig,
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
            auth: AuthConfig::default(),
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
fn default_notes_transient_ttl_hours() -> u64 {
    24
}
fn default_notes_durable_ttl_days() -> u64 {
    7
}
fn default_notes_claim_ttl_hours() -> u64 {
    2
}
/// How workers execute tasks.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Native AEQI agent loop (default).
    #[default]
    Agent,
    /// Delegate execution to Claude Code CLI.
    ClaudeCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpawnConfig {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub prefix: String,
    /// Repo path (absolute) or key into `[repos]`.
    pub repo: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Runtime preset name. If omitted, falls back to `[aeqi].default_runtime`.
    #[serde(default)]
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
    /// Per-project orchestrator overrides. If None, falls back to global [orchestrator].
    #[serde(default)]
    pub orchestrator: Option<OrchestratorConfig>,
    /// Departments within this project (org chart hierarchy).
    #[serde(default)]
    pub departments: Vec<DepartmentConfig>,
    /// Domain hints: keyword → skill/doc file mappings. Used by the WorkerPool
    /// to inject domain-specific context when tasks match keyword patterns.
    /// Replaces the hardcoded keyword map in worker_pool.rs.
    #[serde(default)]
    pub domain_hints: Vec<DomainHintConfig>,
    /// Custom compaction instructions for this project. Appended to the 9-section
    /// compaction prompt when agents working on this project need to compact.
    #[serde(default)]
    pub compact_instructions: Option<String>,
    /// Project primer — injected into every agent's context when working on this project.
    /// This is the company/project brief that all agents receive.
    #[serde(default)]
    pub primer: Option<String>,
}

/// Domain keyword → file mapping for automatic context injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainHintConfig {
    /// Keywords that trigger this hint (matched case-insensitively against task subject + labels).
    pub keywords: Vec<String>,
    /// Skill/doc files to inject when keywords match (relative to project skills dir).
    pub files: Vec<String>,
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
// AEQIConfig methods
// ──────────────────────────────────────────────────────────────

impl AEQIConfig {
    fn built_in_runtime_preset(name: &str) -> Option<RuntimePresetConfig> {
        let provider = match name {
            "openrouter_agent" => (ProviderKind::OpenRouter, ExecutionMode::Agent),
            "anthropic_agent" => (ProviderKind::Anthropic, ExecutionMode::Agent),
            "ollama_agent" => (ProviderKind::Ollama, ExecutionMode::Agent),
            // Legacy preset names retained as aliases so existing configs degrade
            // cleanly to the native AEQI agent runtime.
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

    pub fn runtime_for_company(&self, project_name: &str) -> RuntimePresetConfig {
        let project = self.agent_spawn(project_name);
        let fallback_mode = project
            .map(|p| p.execution_mode.clone())
            .unwrap_or_default();
        let preset_name = project
            .and_then(|p| p.runtime.as_deref())
            .or(self.aeqi.default_runtime.as_deref());
        self.resolve_runtime_preset(preset_name, fallback_mode)
    }

    pub fn runtime_for_agent(&self, agent_name: &str) -> RuntimePresetConfig {
        let agent = self.agent(agent_name);
        let fallback_mode = agent.map(|a| a.execution_mode.clone()).unwrap_or_default();
        let preset_name = agent
            .and_then(|a| a.runtime.as_deref())
            .or(self.aeqi.default_runtime.as_deref());
        self.resolve_runtime_preset(preset_name, fallback_mode)
    }

    pub fn execution_mode_for_project(&self, project_name: &str) -> ExecutionMode {
        self.runtime_for_company(project_name)
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
        let mut config = Self::parse(&content)?;

        // Ensure every project has a stable UUID.
        let data_dir = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
            .join(".aeqi");
        if let Err(e) = config.ensure_project_ids(&data_dir) {
            warn!(error = %e, "failed to assign project IDs");
        }

        Ok(config)
    }

    /// Parse config from TOML string.
    pub fn parse(content: &str) -> Result<Self> {
        let mut config: Self = toml::from_str(content).context("failed to parse aeqi.toml")?;

        // Resolve API keys: try env vars first, then the encrypted SecretStore.
        let secret_store = {
            let data_dir = expand_tilde(&config.aeqi.data_dir);
            SecretStore::open(Path::new(&data_dir).join("secrets").as_path()).ok()
        };

        if let Some(ref mut or) = config.providers.openrouter {
            or.api_key = resolve_env(&or.api_key);
            if or.api_key.is_empty()
                && let Some(ref store) = secret_store
            {
                or.api_key = store.get("OPENROUTER_API_KEY").unwrap_or_default();
            }
            if or.api_key.is_empty() {
                warn!("OpenRouter API key is empty — check environment variable or secret store");
            }
        }
        if let Some(ref mut a) = config.providers.anthropic {
            a.api_key = resolve_env(&a.api_key);
            if a.api_key.is_empty()
                && let Some(ref store) = secret_store
            {
                a.api_key = store.get("ANTHROPIC_API_KEY").unwrap_or_default();
            }
            if a.api_key.is_empty() {
                warn!("Anthropic API key is empty — check environment variable or secret store");
            }
        }

        // Expand ~ in paths.
        config.aeqi.data_dir = expand_tilde(&config.aeqi.data_dir);
        for path in config.repos.values_mut() {
            *path = expand_tilde(path);
        }
        for project in &mut config.agent_spawns {
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

    /// Find config by searching upward from cwd, then ~/.aeqi/.
    pub fn discover() -> Result<(Self, PathBuf)> {
        if let Ok(path) = std::env::var("AEQI_CONFIG") {
            let path = PathBuf::from(path);
            return Ok((Self::load(&path)?, path));
        }

        let config_names = ["aeqi.toml"];

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
                let candidate = home.join(format!(".aeqi/{name}"));
                if candidate.exists() {
                    return Ok((Self::load(&candidate)?, candidate));
                }
            }
        }

        anyhow::bail!("No aeqi.toml found. Run 'aeqi setup' to create one.")
    }

    /// Get agent spawn config by name.
    pub fn agent_spawn(&self, name: &str) -> Option<&AgentSpawnConfig> {
        self.agent_spawns.iter().find(|r| r.name == name)
    }

    /// Compat alias — old callers used `.company(name)`.
    pub fn company(&self, name: &str) -> Option<&AgentSpawnConfig> {
        self.agent_spawn(name)
    }

    /// Get agent config by name.
    pub fn agent(&self, name: &str) -> Option<&PeerAgentConfig> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// Get the default model for a project, falling back to provider default.
    pub fn model_for_company(&self, project_name: &str) -> String {
        let runtime = self.runtime_for_company(project_name);
        self.agent_spawn(project_name)
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
    pub fn orchestrator_for_company(&self, project_name: &str) -> OrchestratorConfig {
        self.agent_spawn(project_name)
            .and_then(|p| p.orchestrator.clone())
            .unwrap_or_else(|| self.orchestrator.clone())
    }

    /// Resolve the data directory path.
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.aeqi.data_dir)
    }

    /// Get the team leader agent config (point-of-contact).
    pub fn leader_agent(&self) -> Option<&PeerAgentConfig> {
        self.agent(&self.team.leader)
            .or_else(|| self.agents.iter().find(|a| a.role == "orchestrator"))
    }

    /// Get all agents with a specific role string.
    pub fn agents_with_role(&self, role: &str) -> Vec<&PeerAgentConfig> {
        self.agents.iter().filter(|a| a.role == role).collect()
    }

    /// Get all advisor agents (convenience).
    pub fn advisor_agents(&self) -> Vec<&PeerAgentConfig> {
        self.agents_with_role("advisor")
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

        errors
    }

    /// Validate config for logical errors that serde can't catch.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.aeqi.name.is_empty() {
            errors.push("system.name is empty".to_string());
        }

        // Project validation.
        let mut seen_names = std::collections::HashSet::new();
        let mut seen_prefixes = std::collections::HashSet::new();
        for d in &self.agent_spawns {
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
            .filter(|a| a.role == "orchestrator")
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

        if let Some(ref runtime) = self.aeqi.default_runtime
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
        for d in &self.agent_spawns {
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

        errors
    }

    /// Assign a stable UUID to every project that lacks one.
    ///
    /// IDs are persisted in `data_dir/project_ids.json` so they survive
    /// config reloads and are deterministic per project name.
    pub fn ensure_project_ids(&mut self, data_dir: &Path) -> Result<()> {
        let ids_path = data_dir.join("project_ids.json");
        let mut ids: HashMap<String, String> = if ids_path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&ids_path)?)?
        } else {
            HashMap::new()
        };

        let mut changed = false;
        for project in &mut self.agent_spawns {
            if project.id.is_none() {
                let id = ids.entry(project.name.clone()).or_insert_with(|| {
                    changed = true;
                    uuid::Uuid::new_v4().to_string()
                });
                project.id = Some(id.clone());
            }
        }

        if changed {
            if let Some(parent) = ids_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&ids_path, serde_json::to_string_pretty(&ids)?)?;
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────
// Agent discovery from disk
// ──────────────────────────────────────────────────────────────

/// Load a single agent's config from `agent_dir/agent.md` (YAML frontmatter + prompt body).
pub fn load_agent_config(agent_dir: &Path) -> Result<PeerAgentConfig> {
    let path = agent_dir.join("agent.md");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read agent.md: {}", path.display()))?;
    let (json, body) = crate::frontmatter::parse_frontmatter(&content)
        .with_context(|| format!("failed to parse agent.md: {}", path.display()))?;
    let mut config: PeerAgentConfig = serde_json::from_value(json)
        .with_context(|| format!("failed to deserialize agent.md: {}", path.display()))?;
    // The body is the system prompt.
    if !body.is_empty() {
        config.prompt = Some(AgentPromptConfig { system: body });
    }
    Ok(config)
}

/// Discover all agents by scanning subdirectories of `agents_dir`.
/// Skips `shared` and any directory without an `agent.md`.
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
        let agent_md = path.join("agent.md");
        if !agent_md.exists() {
            continue;
        }
        match load_agent_config(&path) {
            Ok(mut config) => {
                // Validate name matches directory.
                if config.name != dir_name {
                    warn!(
                        dir = %dir_name,
                        config_name = %config.name,
                        "agent.md name doesn't match directory, using directory name"
                    );
                    config.name = dir_name;
                }
                agents.push(config);
            }
            Err(e) => {
                warn!(dir = %dir_name, error = %e, "failed to load agent.md, skipping");
            }
        }
    }

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

impl AEQIConfig {
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
            // No agent.md files found — fall back to [[agents]] in TOML config.
            return warnings;
        }

        let disk_names: std::collections::HashSet<&str> =
            disk_agents.iter().map(|a| a.name.as_str()).collect();

        // Warn about overlaps (TOML agents that will be replaced by disk).
        for toml_agent in &self.agents {
            if disk_names.contains(toml_agent.name.as_str()) {
                warnings.push(format!(
                    "agent '{}' found in both [[agents]] and agents/{}/agent.md — using disk version",
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
[aeqi]
name = "test"

[[companies]]
name = "test-domain"
prefix = "td"
repo = "/tmp/test"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        assert_eq!(config.aeqi.name, "test");
        assert_eq!(config.agent_spawns.len(), 1);
        assert_eq!(config.agent_spawns[0].name, "test-domain");
        assert!(config.web.ui_dist_dir.is_none());
    }

    #[test]
    fn test_web_ui_dist_dir_expands_tilde_and_env() {
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::set_var("AEQI_UI_DIST_TEST", "~/aeqi/apps/ui/dist") };
        let toml = r#"
[aeqi]
name = "test"

[web]
ui_dist_dir = "${AEQI_UI_DIST_TEST}"

[[companies]]
name = "test-domain"
prefix = "td"
repo = "/tmp/test"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            config.web.ui_dist_dir,
            Some(
                home.join("aeqi/apps/ui/dist")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::remove_var("AEQI_UI_DIST_TEST") };
    }

    #[test]
    fn test_resolve_env() {
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::set_var("TEST_AEQI_VAR", "hello") };
        assert_eq!(resolve_env("${TEST_AEQI_VAR}"), "hello");
        assert_eq!(resolve_env("plain"), "plain");
        unsafe { std::env::remove_var("TEST_AEQI_VAR") };
    }

    #[test]
    fn test_validate_valid_config() {
        let toml = r#"
[aeqi]
name = "test"

[team]
leader = "alice"

[[agents]]
name = "alice"
prefix = "al"
role = "orchestrator"

[[companies]]
name = "alpha"
prefix = "al"
repo = "/tmp/alpha"

[[companies]]
name = "beta"
prefix = "bt"
repo = "/tmp/beta"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn test_validate_duplicate_prefix() {
        let toml = r#"
[aeqi]
name = "test"

[[companies]]
name = "alpha"
prefix = "ab"
repo = "/tmp/alpha"

[[companies]]
name = "beta"
prefix = "ab"
repo = "/tmp/beta"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
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
[aeqi]
name = "test"

[memory]
vector_weight = 0.9
keyword_weight = 0.9
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(
            issues.iter().any(|i| i.contains("weights sum")),
            "expected weight warning: {issues:?}"
        );
    }

    #[test]
    fn test_validate_chunk_overlap_too_large() {
        let toml = r#"
[aeqi]
name = "test"

[memory]
chunk_size_tokens = 100
chunk_overlap_tokens = 150
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(
            issues.iter().any(|i| i.contains("chunk_overlap_tokens")),
            "expected overlap warning: {issues:?}"
        );
    }

    #[test]
    fn test_parse_agents_config() {
        let toml = r#"
[aeqi]
name = "test"

[repos]
aeqi = "/home/user/aeqi"
backend = "/home/user/backend"

[team]
leader = "alpha"

[[agents]]
name = "alpha"
prefix = "fa"
model = "claude-opus-4-6"
role = "orchestrator"
voice = "vocal"
default_repo = "aeqi"

[[agents]]
name = "beta"
prefix = "fk"
role = "advisor"
voice = "vocal"
expertise = ["project-a"]

[[companies]]
name = "project-a"
prefix = "as"
repo = "/home/user/backend"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents[0].name, "alpha");
        assert_eq!(config.agents[0].role, "orchestrator".to_string());
        assert_eq!(config.agents[1].role, "advisor".to_string());
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
[aeqi]
name = "test"

[repos]
aeqi = "/home/user/aeqi"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        assert_eq!(
            config.resolve_repo("aeqi"),
            PathBuf::from("/home/user/aeqi")
        );
        assert_eq!(config.resolve_repo("/raw/path"), PathBuf::from("/raw/path"));
    }

    #[test]
    fn test_telegram_channel_routes_parse() {
        let toml = r#"
[aeqi]
name = "test"

[channels.telegram]
token_secret = "TELEGRAM_TOKEN"
allowed_chats = [1001, 1002]
main_chat_id = 1002

[[channels.telegram.routes]]
chat_id = 1001
name = "AEQI HQ"

[[channels.telegram.routes]]
chat_id = 1002
project = "aeqi"
department = "backend"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        let telegram = config.channels.telegram.expect("telegram config");
        assert_eq!(telegram.main_chat_id, Some(1002));
        assert_eq!(telegram.routes.len(), 2);
        assert_eq!(telegram.routes[0].chat_id, 1001);
        assert_eq!(telegram.routes[0].name.as_deref(), Some("AEQI HQ"));
        assert_eq!(telegram.routes[1].project.as_deref(), Some("aeqi"));
        assert_eq!(telegram.routes[1].department.as_deref(), Some("backend"));
    }

    #[test]
    fn test_team_leader_field() {
        let toml = r#"
[aeqi]
name = "test"

[team]
leader = "alpha"

[[agents]]
name = "alpha"
prefix = "au"
role = "orchestrator"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
        assert_eq!(config.leader(), "alpha");
    }

    #[test]
    fn test_discover_agents_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");

        // Create agent directories with agent.md.
        for (name, role) in &[("alice", "orchestrator"), ("bob", "advisor")] {
            let dir = agents_dir.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let md = format!(
                "---\nname: {name}\nprefix: {}\nrole: {role}\n---\n\nSystem prompt for {name}.",
                &name[..2],
            );
            std::fs::write(dir.join("agent.md"), md).unwrap();
        }

        // Create shared dir (should be skipped).
        std::fs::create_dir_all(agents_dir.join("shared")).unwrap();
        std::fs::write(
            agents_dir.join("shared/agent.md"),
            "---\nname: shared\n---\n",
        )
        .unwrap();

        // Create dir without agent.md (should be skipped).
        std::fs::create_dir_all(agents_dir.join("noconfig")).unwrap();

        let agents = discover_agents(&agents_dir).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "alice");
        assert_eq!(agents[0].role, "orchestrator".to_string());
        assert_eq!(agents[1].name, "bob");
        assert_eq!(agents[1].role, "advisor".to_string());
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
            dir.join("agent.md"),
            "---\nname: alice\nprefix: al\nrole: advisor\nmodel: disk-model\n---\n\nAlice prompt.",
        )
        .unwrap();

        let toml_str = r#"
[aeqi]
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
        let mut config = AEQIConfig::parse(toml_str).unwrap();
        let warnings = config.discover_and_merge_agents(&agents_dir);

        // Disk alice should replace TOML alice.
        assert!(warnings.iter().any(|w| w.contains("alice")));
        assert_eq!(config.agents.len(), 2);

        let alice = config.agent("alice").unwrap();
        assert_eq!(alice.model.as_deref(), Some("disk-model"));
        assert_eq!(alice.role, "advisor".to_string()); // disk version

        // Charlie from TOML should be preserved.
        assert!(config.agent("charlie").is_some());
    }

    #[test]
    fn test_discover_and_merge_fallback_to_toml_config() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        // No agent.md files — [[agents]] from TOML config should remain.

        let toml_str = r#"
[aeqi]
name = "test"

[[agents]]
name = "alice"
prefix = "al"
role = "orchestrator"
"#;
        let mut config = AEQIConfig::parse(toml_str).unwrap();
        let warnings = config.discover_and_merge_agents(&agents_dir);

        assert!(warnings.is_empty());
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].name, "alice");
    }

    #[test]
    fn test_load_agent_config_from_md() {
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path();
        let md = r#"---
name: test
prefix: tt
model: claude-opus-4-6
runtime: anthropic_agent
role: advisor
max_workers: 2
max_turns: 15
expertise: [testing]
capabilities: [memory]
---

You are a test agent."#;
        std::fs::write(agent_dir.join("agent.md"), md).unwrap();

        let loaded = load_agent_config(agent_dir).unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.prefix, "tt");
        assert_eq!(loaded.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(loaded.runtime.as_deref(), Some("anthropic_agent"));
        assert_eq!(loaded.role, "advisor".to_string());
        assert_eq!(loaded.max_workers, 2);
        assert_eq!(loaded.max_turns, Some(15));
        assert_eq!(loaded.expertise, vec!["testing"]);
        assert_eq!(loaded.prompt.unwrap().system, "You are a test agent.");
    }

    #[test]
    fn test_runtime_resolution_uses_defaults() {
        let toml = r#"
[aeqi]
name = "test"
default_runtime = "anthropic_agent"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-sonnet-4-20250514"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[companies]]
name = "aeqi"
prefix = "sg"
repo = "/tmp/aeqi"
"#;
        let config = AEQIConfig::parse(toml).unwrap();

        let project_runtime = config.runtime_for_company("aeqi");
        assert_eq!(project_runtime.provider, ProviderKind::Anthropic);
        assert_eq!(
            config.execution_mode_for_project("aeqi"),
            ExecutionMode::Agent
        );
        assert_eq!(
            config.model_for_company("aeqi"),
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
[aeqi]
name = "test"
default_runtime = "anthropic_claude_code"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
default_model = "claude-sonnet-4-20250514"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"

[[companies]]
name = "aeqi"
prefix = "sg"
repo = "/tmp/aeqi"
execution_mode = "claude_code"
"#;
        let config = AEQIConfig::parse(toml).unwrap();

        assert_eq!(
            config.execution_mode_for_project("aeqi"),
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
[aeqi]
name = "test"
default_runtime = "missing"

[[agents]]
name = "leader"
prefix = "ld"
role = "orchestrator"
"#;
        let config = AEQIConfig::parse(toml).unwrap();
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
        // agent.md has wrong name — should be corrected to dir name.
        std::fs::write(
            dir.join("agent.md"),
            "---\nname: wrong\nprefix: al\nrole: advisor\n---\n\nPrompt.",
        )
        .unwrap();

        let agents = discover_agents(&agents_dir).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "alice");
    }
}
