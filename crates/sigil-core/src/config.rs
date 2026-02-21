use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Master configuration loaded from sigil.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigilConfig {
    pub sigil: SigilMeta,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub familiar: FamiliarConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub rigs: Vec<RigConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigilMeta {
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
    #[serde(default = "default_heartbeat_interval")]
    pub default_interval_minutes: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_interval_minutes: 30,
        }
    }
}

fn default_heartbeat_interval() -> u32 { 30 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamiliarConfig {
    #[serde(default = "default_fa_prefix")]
    pub prefix: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_fa_workers")]
    pub max_workers: u32,
}

impl Default for FamiliarConfig {
    fn default() -> Self {
        Self {
            prefix: "fa".to_string(),
            model: None,
            max_workers: 2,
        }
    }
}

fn default_fa_prefix() -> String { "fa".to_string() }
fn default_fa_workers() -> u32 { 2 }

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigConfig {
    pub name: String,
    pub prefix: String,
    pub repo: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    #[serde(default)]
    pub worktree_root: Option<String>,
}

fn default_max_workers() -> u32 { 2 }

impl SigilConfig {
    /// Load config from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        Self::parse(&content)
    }

    /// Parse config from TOML string.
    pub fn parse(content: &str) -> Result<Self> {
        let mut config: Self = toml::from_str(content)
            .context("failed to parse sigil.toml")?;

        // Resolve environment variables in API keys.
        if let Some(ref mut or) = config.providers.openrouter {
            or.api_key = resolve_env(&or.api_key);
        }
        if let Some(ref mut a) = config.providers.anthropic {
            a.api_key = resolve_env(&a.api_key);
        }

        // Expand ~ in paths.
        config.sigil.data_dir = expand_tilde(&config.sigil.data_dir);
        for rig in &mut config.rigs {
            rig.repo = expand_tilde(&rig.repo);
            if let Some(ref mut wt) = rig.worktree_root {
                *wt = expand_tilde(wt);
            }
        }

        Ok(config)
    }

    /// Find config by searching upward from cwd, then ~/.sigil/, then /etc/sigil/.
    pub fn discover() -> Result<(Self, PathBuf)> {
        // 1. Check SIGIL_CONFIG env var.
        if let Ok(path) = std::env::var("SIGIL_CONFIG") {
            let path = PathBuf::from(path);
            return Ok((Self::load(&path)?, path));
        }

        // 2. Walk up from cwd looking for sigil.toml or config/sigil.toml.
        if let Ok(cwd) = std::env::current_dir() {
            let mut dir = cwd.as_path();
            loop {
                let candidate = dir.join("sigil.toml");
                if candidate.exists() {
                    return Ok((Self::load(&candidate)?, candidate));
                }
                let candidate = dir.join("config/sigil.toml");
                if candidate.exists() {
                    return Ok((Self::load(&candidate)?, candidate));
                }
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => break,
                }
            }
        }

        // 3. Check ~/.sigil/sigil.toml.
        if let Some(home) = dirs::home_dir() {
            let candidate = home.join(".sigil/sigil.toml");
            if candidate.exists() {
                return Ok((Self::load(&candidate)?, candidate));
            }
        }

        anyhow::bail!("No sigil.toml found. Run `sg init` to create one.")
    }

    /// Get rig config by name.
    pub fn rig(&self, name: &str) -> Option<&RigConfig> {
        self.rigs.iter().find(|r| r.name == name)
    }

    /// Get the default model for a rig, falling back to provider default.
    pub fn model_for_rig(&self, rig_name: &str) -> String {
        // Check familiar config first.
        if rig_name == "familiar" {
            if let Some(ref m) = self.familiar.model {
                return m.clone();
            }
        }

        self.rig(rig_name)
            .and_then(|r| r.model.clone())
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
        PathBuf::from(&self.sigil.data_dir)
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
    if s.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return s.replacen('~', &home.to_string_lossy(), 1);
        }
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

[[rigs]]
name = "test-rig"
prefix = "tr"
repo = "/tmp/test"
"#;
        let config = SigilConfig::parse(toml).unwrap();
        assert_eq!(config.sigil.name, "test");
        assert_eq!(config.rigs.len(), 1);
        assert_eq!(config.rigs[0].name, "test-rig");
    }

    #[test]
    fn test_resolve_env() {
        // SAFETY: test runs single-threaded, no concurrent env access.
        unsafe { std::env::set_var("TEST_SIGIL_VAR", "hello") };
        assert_eq!(resolve_env("${TEST_SIGIL_VAR}"), "hello");
        assert_eq!(resolve_env("plain"), "plain");
        unsafe { std::env::remove_var("TEST_SIGIL_VAR") };
    }
}
