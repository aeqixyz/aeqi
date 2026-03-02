use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Master configuration loaded from system.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    #[serde(alias = "realm")]
    pub system: SystemMeta,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default, alias = "pulse")]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    /// Global repo pool — all agents can access all repos.
    #[serde(default)]
    pub repos: HashMap<String, String>,
    /// Projects = repos, tasks, knowledge, budget.
    #[serde(default, alias = "domains")]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMeta {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
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
    "anthropic/claude-sonnet-4.6".to_string()
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

fn default_memory_backend() -> String { "sqlite".to_string() }
fn default_embedding_dims() -> usize { 1536 }
fn default_vector_weight() -> f64 { 0.6 }
fn default_keyword_weight() -> f64 { 0.4 }
fn default_decay_halflife() -> f64 { 30.0 }
fn default_mmr_lambda() -> f64 { 0.7 }
fn default_chunk_size() -> usize { 400 }
fn default_chunk_overlap() -> usize { 80 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_pulse_interval")]
    pub default_interval_minutes: u32,
    #[serde(default)]
    pub reflection_enabled: bool,
    #[serde(default = "default_reflection_interval")]
    pub reflection_interval_minutes: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_interval_minutes: 30,
            reflection_enabled: false,
            reflection_interval_minutes: 240,
        }
    }
}

fn default_pulse_interval() -> u32 { 30 }
fn default_reflection_interval() -> u32 { 240 }

// ──────────────────────────────────────────────────────────────
// Agent configuration (replaces Shadow + Familiar + Council)
// ──────────────────────────────────────────────────────────────

/// Role of an agent in the party.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AgentRole {
    /// Primary interface — orchestrates, routes, synthesizes.
    #[default]
    Orchestrator,
    /// Domain worker — executes quests on repos.
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

/// Configuration for a single agent (peer entity) in system.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAgentConfig {
    pub name: String,
    #[serde(default = "default_agent_prefix")]
    pub prefix: String,
    #[serde(default)]
    pub model: Option<String>,
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
    /// Default repo key (from [repos]) this agent works in.
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

fn default_agent_prefix() -> String { "ag".to_string() }
fn default_agent_max_workers() -> u32 { 1 }

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
            leader: "aurelia".to_string(),
            agents: Vec::new(),
            router_model: "google/gemini-2.0-flash-001".to_string(),
            router_cooldown_secs: 60,
            max_background_cost_usd: 0.50,
        }
    }
}

/// Per-project team assignment — which agents own this project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTeamConfig {
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

fn default_leader() -> String { "aurelia".to_string() }
fn default_router_model() -> String { "google/gemini-2.0-flash-001".to_string() }
fn default_router_cooldown() -> u64 { 60 }
fn default_max_background_cost() -> f64 { 0.50 }

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
}

fn default_debounce_window() -> u64 { 3000 }

/// Context budget limits for spirit system prompts (char-based, ~4 chars/token).
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

fn default_budget_max_shared_workflow() -> usize { 2000 }
fn default_budget_max_persona() -> usize { 4000 }
fn default_budget_max_agents() -> usize { 8000 }
fn default_budget_max_knowledge() -> usize { 12000 }
fn default_budget_max_preferences() -> usize { 4000 }
fn default_budget_max_memory() -> usize { 8000 }
fn default_budget_max_checkpoints() -> usize { 8000 }
fn default_budget_max_checkpoint_count() -> usize { 5 }
fn default_budget_max_total() -> usize { 120000 }

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

fn default_checkin_interval() -> u64 { 30 }
fn default_alarm_interval() -> u64 { 60 }
fn default_min_flood_interval() -> u64 { 30 }

/// How workers execute quests.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Lightweight internal agent loop (for orchestration/triage).
    #[default]
    Agent,
    /// Spawn Claude Code CLI instance (for real code work).
    ClaudeCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub prefix: String,
    /// Repo path (absolute) or key into [repos].
    pub repo: String,
    #[serde(default)]
    pub model: Option<String>,
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
    #[serde(default = "default_worker_timeout", alias = "spirit_timeout_secs")]
    pub worker_timeout_secs: u64,
    #[serde(default)]
    pub max_cost_per_day_usd: Option<f64>,
    /// Per-project team assignment. If None, falls back to system team.
    #[serde(default)]
    pub team: Option<ProjectTeamConfig>,
}

fn default_max_workers() -> u32 { 2 }
fn default_max_turns() -> Option<u32> { Some(25) }
fn default_worker_timeout() -> u64 { 1800 }

// ──────────────────────────────────────────────────────────────
// SystemConfig methods
// ──────────────────────────────────────────────────────────────

impl SystemConfig {
    /// Load config from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        Self::parse(&content)
    }

    /// Parse config from TOML string.
    pub fn parse(content: &str) -> Result<Self> {
        let mut config: Self = toml::from_str(content)
            .context("failed to parse system.toml")?;

        // Resolve environment variables in API keys.
        if let Some(ref mut or) = config.providers.openrouter {
            or.api_key = resolve_env(&or.api_key);
        }
        if let Some(ref mut a) = config.providers.anthropic {
            a.api_key = resolve_env(&a.api_key);
        }

        // Expand ~ in paths.
        config.system.data_dir = expand_tilde(&config.system.data_dir);
        for path in config.repos.values_mut() {
            *path = expand_tilde(path);
        }
        for project in &mut config.projects {
            project.repo = expand_tilde(&project.repo);
            if let Some(ref mut wt) = project.worktree_root {
                *wt = expand_tilde(wt);
            }
        }

        // Validate and warn (non-fatal — partial configs allowed during dev).
        let issues = config.validate();
        for issue in &issues {
            warn!(issue = %issue, "config validation warning");
        }

        Ok(config)
    }

    /// Find config by searching upward from cwd, then ~/.sigil/.
    /// Looks for system.toml first, falls back to realm.toml for backward compat.
    pub fn discover() -> Result<(Self, PathBuf)> {
        if let Ok(path) = std::env::var("SYSTEM_CONFIG")
            .or_else(|_| std::env::var("REALM_CONFIG"))
        {
            let path = PathBuf::from(path);
            return Ok((Self::load(&path)?, path));
        }

        let config_names = ["system.toml", "realm.toml"];

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

        anyhow::bail!("No system.toml found. Run `rm init` to create one.")
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
        self.project(project_name)
            .and_then(|r| r.model.clone())
            .or_else(|| {
                self.providers
                    .openrouter
                    .as_ref()
                    .map(|or| or.default_model.clone())
            })
            .unwrap_or_else(|| "anthropic/claude-sonnet-4.6".to_string())
    }

    /// Get the model for an agent, falling back to provider default.
    pub fn model_for_agent(&self, agent_name: &str) -> String {
        self.agent(agent_name)
            .and_then(|a| a.model.clone())
            .or_else(|| {
                self.providers
                    .openrouter
                    .as_ref()
                    .map(|or| or.default_model.clone())
            })
            .unwrap_or_else(|| "anthropic/claude-sonnet-4.6".to_string())
    }

    /// Resolve the data directory path.
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.system.data_dir)
    }

    /// Get the team leader agent config (point-of-contact).
    pub fn leader_agent(&self) -> Option<&PeerAgentConfig> {
        self.agent(&self.team.leader)
            .or_else(|| self.agents_with_role(AgentRole::Orchestrator).first().copied())
    }

    /// Get all agents with a specific role.
    pub fn agents_with_role(&self, role: AgentRole) -> Vec<&PeerAgentConfig> {
        self.agents.iter().filter(|a| a.role == role).collect()
    }

    /// Get all advisor agents (convenience).
    pub fn advisor_agents(&self) -> Vec<&PeerAgentConfig> {
        self.agents_with_role(AgentRole::Advisor)
    }

    /// Resolve a repo key to a path. If the key exists in [repos], return that.
    /// Otherwise treat it as a raw path.
    pub fn resolve_repo(&self, key: &str) -> PathBuf {
        self.repos
            .get(key)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(key))
    }

    /// Resolve all repos to paths.
    pub fn resolve_all_repos(&self) -> HashMap<String, PathBuf> {
        self.repos.iter().map(|(k, v)| (k.clone(), PathBuf::from(v))).collect()
    }

    /// Get the effective team for a project.
    /// Returns the project's own team if configured, otherwise builds one from the system team.
    pub fn project_team(&self, project_name: &str) -> ProjectTeamConfig {
        if let Some(project) = self.project(project_name)
            && let Some(ref team) = project.team {
                return team.clone();
            }
        // Fall back to system team.
        ProjectTeamConfig {
            leader: self.team.leader.clone(),
            agents: if self.team.agents.is_empty() {
                vec![self.team.leader.clone()]
            } else {
                self.team.agents.clone()
            },
        }
    }

    /// Get the system team leader agent name.
    pub fn system_leader(&self) -> &str {
        &self.team.leader
    }

    /// Validate that all team references point to defined agents.
    pub fn validate_teams(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let agent_names: std::collections::HashSet<&str> = self.agents.iter()
            .map(|a| a.name.as_str())
            .collect();

        // Validate system team leader.
        if !agent_names.is_empty() && !agent_names.contains(self.team.leader.as_str()) {
            errors.push(format!(
                "system team leader '{}' is not a defined agent",
                self.team.leader
            ));
        }

        // Validate system team agents.
        for name in &self.team.agents {
            if !agent_names.contains(name.as_str()) {
                errors.push(format!(
                    "system team references unknown agent: '{name}'"
                ));
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

    /// Validate config for logical errors that serde can't catch.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.system.name.is_empty() {
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
        }

        // Agent validation.
        let orchestrator_count = self.agents.iter()
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
        }

        // Repo refs resolve.
        for d in &self.projects {
            if !d.repo.starts_with('/') && !d.repo.starts_with('~') && !self.repos.contains_key(&d.repo) {
                errors.push(format!("project '{}' references unknown repo key: '{}'", d.name, d.repo));
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
        && let Some(home) = dirs::home_dir() {
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
[realm]
name = "test"

[[projects]]
name = "test-domain"
prefix = "td"
repo = "/tmp/test"
"#;
        let config = SystemConfig::parse(toml).unwrap();
        assert_eq!(config.system.name, "test");
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects[0].name, "test-domain");
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
[realm]
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
        let config = SystemConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn test_validate_duplicate_prefix() {
        let toml = r#"
[realm]
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
        let config = SystemConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("duplicate project prefix")), "expected duplicate prefix: {issues:?}");
    }

    #[test]
    fn test_validate_bad_memory_weights() {
        let toml = r#"
[realm]
name = "test"

[memory]
vector_weight = 0.9
keyword_weight = 0.9
"#;
        let config = SystemConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("weights sum")), "expected weight warning: {issues:?}");
    }

    #[test]
    fn test_validate_chunk_overlap_too_large() {
        let toml = r#"
[realm]
name = "test"

[memory]
chunk_size_tokens = 100
chunk_overlap_tokens = 150
"#;
        let config = SystemConfig::parse(toml).unwrap();
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.contains("chunk_overlap_tokens")), "expected overlap warning: {issues:?}");
    }

    #[test]
    fn test_parse_agents_config() {
        let toml = r#"
[realm]
name = "test"

[repos]
sigil = "/home/user/sigil"
backend = "/home/user/backend"

[team]
leader = "aurelia"

[[agents]]
name = "aurelia"
prefix = "fa"
model = "claude-opus-4-6"
role = "orchestrator"
voice = "vocal"
default_repo = "sigil"

[[agents]]
name = "kael"
prefix = "fk"
role = "advisor"
voice = "vocal"
expertise = ["algostaking"]

[[projects]]
name = "algostaking"
prefix = "as"
repo = "/home/user/backend"
"#;
        let config = SystemConfig::parse(toml).unwrap();
        assert_eq!(config.agents.len(), 2);
        assert_eq!(config.agents[0].name, "aurelia");
        assert_eq!(config.agents[0].role, AgentRole::Orchestrator);
        assert_eq!(config.agents[1].role, AgentRole::Advisor);
        assert_eq!(config.team.leader, "aurelia");
        assert_eq!(config.repos.len(), 2);
        let leader = config.leader_agent().unwrap();
        assert_eq!(leader.name, "aurelia");
        let advisors = config.advisor_agents();
        assert_eq!(advisors.len(), 1);
        assert_eq!(advisors[0].name, "kael");
    }

    #[test]
    fn test_resolve_repo() {
        let toml = r#"
[realm]
name = "test"

[repos]
sigil = "/home/user/sigil"
"#;
        let config = SystemConfig::parse(toml).unwrap();
        assert_eq!(config.resolve_repo("sigil"), PathBuf::from("/home/user/sigil"));
        assert_eq!(config.resolve_repo("/raw/path"), PathBuf::from("/raw/path"));
    }

    #[test]
    fn test_per_project_team_config() {
        let toml = r#"
[system]
name = "test"

[team]
leader = "aurelia"
agents = ["aurelia"]

[[agents]]
name = "aurelia"
prefix = "au"
role = "orchestrator"

[[agents]]
name = "kael"
prefix = "fk"
role = "advisor"
expertise = ["algostaking"]

[[agents]]
name = "mira"
prefix = "fm"
role = "advisor"
expertise = ["riftdecks"]

[[projects]]
name = "algostaking"
prefix = "as"
repo = "/tmp/algo"
team.leader = "kael"
team.agents = ["kael"]

[[projects]]
name = "riftdecks"
prefix = "rd"
repo = "/tmp/rift"
team.leader = "mira"
team.agents = ["mira"]

[[projects]]
name = "standalone"
prefix = "sa"
repo = "/tmp/standalone"
"#;
        let config = SystemConfig::parse(toml).unwrap();

        // Per-project teams.
        let algo_team = config.project_team("algostaking");
        assert_eq!(algo_team.leader, "kael");
        assert_eq!(algo_team.effective_agents(), vec!["kael"]);

        let rift_team = config.project_team("riftdecks");
        assert_eq!(rift_team.leader, "mira");
        assert_eq!(rift_team.effective_agents(), vec!["mira"]);

        // Fallback to system team.
        let standalone_team = config.project_team("standalone");
        assert_eq!(standalone_team.leader, "aurelia");
        assert_eq!(standalone_team.effective_agents(), vec!["aurelia"]);

        // System leader.
        assert_eq!(config.system_leader(), "aurelia");

        // Validation should pass.
        let issues = config.validate();
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn test_team_validation_unknown_agent() {
        let toml = r#"
[system]
name = "test"

[team]
leader = "aurelia"

[[agents]]
name = "aurelia"
prefix = "au"
role = "orchestrator"

[[projects]]
name = "alpha"
prefix = "al"
repo = "/tmp/alpha"
team.leader = "ghost"
team.agents = ["ghost"]
"#;
        let config = SystemConfig::parse(toml).unwrap();
        let issues = config.validate_teams();
        assert!(issues.iter().any(|i| i.contains("ghost")),
            "expected team validation to flag unknown agent 'ghost': {issues:?}");
    }

    #[test]
    fn test_project_team_effective_agents() {
        use super::ProjectTeamConfig;

        // Empty agents list → leader only.
        let team = ProjectTeamConfig {
            leader: "kael".to_string(),
            agents: vec![],
        };
        assert_eq!(team.effective_agents(), vec!["kael"]);

        // Leader already in agents list.
        let team = ProjectTeamConfig {
            leader: "kael".to_string(),
            agents: vec!["kael".to_string(), "mira".to_string()],
        };
        assert_eq!(team.effective_agents(), vec!["kael", "mira"]);

        // Leader not in agents list → prepended.
        let team = ProjectTeamConfig {
            leader: "kael".to_string(),
            agents: vec!["mira".to_string()],
        };
        assert_eq!(team.effective_agents(), vec!["kael", "mira"]);
    }

    #[test]
    fn test_system_team_agents_field() {
        let toml = r#"
[system]
name = "test"

[team]
leader = "aurelia"
agents = ["aurelia"]

[[agents]]
name = "aurelia"
prefix = "au"
role = "orchestrator"
"#;
        let config = SystemConfig::parse(toml).unwrap();
        assert_eq!(config.team.agents, vec!["aurelia"]);
        assert_eq!(config.system_leader(), "aurelia");
    }
}
