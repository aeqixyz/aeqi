use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

/// Global platform configuration loaded from /data/platform.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub platform: PlatformMeta,
    #[serde(default)]
    pub providers: PlatformProviders,
    pub web: WebConfig,
    pub tiers: HashMap<String, TierConfig>,
    #[serde(default)]
    pub email: Option<EmailConfig>,
    #[serde(default)]
    pub stripe: Option<StripeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformMeta {
    #[serde(default = "default_name")]
    pub name: String,
    pub base_dir: String,
    pub template_dir: String,
    #[serde(default = "default_max_global_workers")]
    pub max_global_workers: u32,
    #[serde(default = "default_max_global_cost")]
    pub max_global_cost_per_day_usd: f64,
    pub jwt_secret: String,
    #[serde(default)]
    pub admin_tenant_ids: Vec<String>,
}

fn default_name() -> String { "gacha-agency".to_string() }
fn default_max_global_workers() -> u32 { 50 }
fn default_max_global_cost() -> f64 { 100.0 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlatformProviders {
    #[serde(default)]
    pub anthropic: Option<ProviderEntry>,
    #[serde(default)]
    pub openrouter: Option<OpenRouterEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterEntry {
    pub api_key: String,
    #[serde(default = "default_or_model")]
    pub default_model: String,
    #[serde(default)]
    pub embedding_model: Option<String>,
}

fn default_or_model() -> String { "claude-sonnet-4-6".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

fn default_bind() -> String { "0.0.0.0:8420".to_string() }

/// Per-tier resource limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    #[serde(default = "default_max_companions")]
    pub max_companions: u32,
    #[serde(default = "default_max_projects")]
    pub max_projects: u32,
    #[serde(default = "default_max_workers")]
    pub max_workers: u32,
    #[serde(default = "default_tier_cost")]
    pub max_cost_per_day_usd: f64,
    #[serde(default = "default_max_storage")]
    pub max_storage_mb: u64,
    #[serde(default)]
    pub can_execute_code: bool,
    #[serde(default = "default_tier_model")]
    pub model: String,
    #[serde(default = "default_summons_per_day")]
    pub summons_per_day: u32,
    #[serde(default = "default_mana_per_day")]
    pub mana_per_day: u32,
}

fn default_summons_per_day() -> u32 { 3 }
fn default_mana_per_day() -> u32 { 10 }

fn default_max_companions() -> u32 { 5 }
fn default_max_projects() -> u32 { 1 }
fn default_max_workers() -> u32 { 1 }
fn default_tier_cost() -> f64 { 1.0 }
fn default_max_storage() -> u64 { 500 }
fn default_tier_model() -> String { "claude-haiku-4-5".to_string() }

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            max_companions: 5,
            max_projects: 1,
            max_workers: 1,
            max_cost_per_day_usd: 1.0,
            max_storage_mb: 500,
            can_execute_code: false,
            model: "claude-haiku-4-5".to_string(),
            summons_per_day: 3,
            mana_per_day: 10,
        }
    }
}

/// Email service configuration (Resend HTTP API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub api_key: String,
    #[serde(default = "default_from_address")]
    pub from_address: String,
    #[serde(default = "default_from_name")]
    pub from_name: String,
}

fn default_from_address() -> String { "noreply@gacha.agency".to_string() }
fn default_from_name() -> String { "GACHA.AGENCY".to_string() }

/// Stripe integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeConfig {
    pub secret_key: String,
    pub webhook_secret: String,
    pub price_basic: String,
    pub price_pro: String,
}

impl PlatformConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read platform config: {}", path.display()))?;
        let mut config: Self = toml::from_str(&content)
            .with_context(|| "failed to parse platform.toml")?;
        // Resolve env vars in secrets
        config.platform.jwt_secret = resolve_env(&config.platform.jwt_secret);
        if let Some(ref mut p) = config.providers.anthropic {
            p.api_key = resolve_env(&p.api_key);
        }
        if let Some(ref mut p) = config.providers.openrouter {
            p.api_key = resolve_env(&p.api_key);
        }
        if let Some(ref mut e) = config.email {
            e.api_key = resolve_env(&e.api_key);
        }
        if let Some(ref mut s) = config.stripe {
            s.secret_key = resolve_env(&s.secret_key);
            s.webhook_secret = resolve_env(&s.webhook_secret);
        }
        Ok(config)
    }

    pub fn tier(&self, name: &str) -> TierConfig {
        self.tiers.get(name).cloned().unwrap_or_default()
    }

    pub fn base_dir(&self) -> PathBuf {
        PathBuf::from(&self.platform.base_dir)
    }

    pub fn template_dir(&self) -> PathBuf {
        PathBuf::from(&self.platform.template_dir)
    }
}

fn resolve_env(s: &str) -> String {
    if s.starts_with("${") && s.ends_with('}') {
        let var = &s[2..s.len() - 1];
        // Try OS env first, then fall back to encrypted secret store.
        if let Ok(val) = std::env::var(var) {
            tracing::debug!(var, "resolved from env");
            return val;
        }
        let sigil_dir = std::path::PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
        ).join(".sigil").join("secrets");
        match system_core::security::SecretStore::open(&sigil_dir) {
            Ok(store) => match store.get(var) {
                Ok(val) => {
                    tracing::info!(var, "resolved from secret store");
                    return val;
                }
                Err(e) => tracing::warn!(var, error = %e, "secret store get failed"),
            },
            Err(e) => tracing::warn!(dir = %sigil_dir.display(), error = %e, "secret store open failed"),
        }
        String::new()
    } else {
        s.to_string()
    }
}
