use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use system_companions::CompanionStore;
use system_orchestrator::{ProjectRegistry, ConversationStore, CostLedger, DispatchBus};

use crate::config::TierConfig;

/// Opaque tenant identifier (UUID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata stored in tenant.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantMeta {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub tier: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub active_project: Option<String>,
    /// External path to read project dirs from instead of data_dir/projects/.
    #[serde(default)]
    pub projects_source: Option<String>,
}

/// A loaded tenant with all subsystems initialized.
pub struct Tenant {
    pub id: TenantId,
    pub display_name: String,
    pub email: Option<String>,
    pub tier: TierConfig,
    pub tier_name: String,
    pub data_dir: PathBuf,
    /// External projects source dir (overrides data_dir/projects/ for scanning).
    pub projects_source: Option<PathBuf>,
    pub registry: Arc<ProjectRegistry>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub cost_ledger: Arc<CostLedger>,
    pub companion_store: Arc<CompanionStore>,
    pub conversation_store: Arc<ConversationStore>,
    pub last_active: AtomicU64,
    pub created_at: DateTime<Utc>,
    pub active_project: RwLock<Option<String>>,
}

impl Tenant {
    /// The directory to read project subdirs from.
    /// Returns `projects_source` if set, otherwise `data_dir/projects`.
    pub fn projects_dir(&self) -> PathBuf {
        self.projects_source.clone().unwrap_or_else(|| self.data_dir.join("projects"))
    }

    /// Update last_active timestamp to now.
    pub fn touch(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_active.store(now, std::sync::atomic::Ordering::Relaxed);
    }

    /// Seconds since last activity.
    pub fn idle_secs(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let last = self.last_active.load(std::sync::atomic::Ordering::Relaxed);
        now.saturating_sub(last)
    }

    /// Check if this tenant can afford another API call.
    pub fn can_afford(&self) -> bool {
        let (spent, _, _) = self.cost_ledger.budget_status();
        spent < self.tier.max_cost_per_day_usd
    }

    /// The team leader (familiar companion). None if no companions yet.
    pub fn leader(&self) -> Option<String> {
        self.companion_store.get_familiar().ok().flatten().map(|c| c.name)
    }

    /// The team (rostered squad). Falls back to just the leader if no roster set.
    pub fn team(&self) -> Vec<String> {
        if let Ok(roster) = self.companion_store.get_roster()
            && !roster.is_empty()
        {
            return roster.into_iter().map(|c| c.name).collect();
        }
        self.leader().into_iter().collect()
    }

    /// Leader name for dispatch routing. "system" fallback for fresh tenants.
    pub fn leader_or_default(&self) -> String {
        self.leader().unwrap_or_else(|| "system".to_string())
    }

    /// Get the currently active project name.
    pub async fn active_project(&self) -> Option<String> {
        self.active_project.read().await.clone()
    }

    /// Set the active project and persist to tenant.toml.
    pub async fn set_active_project(&self, name: Option<String>) -> anyhow::Result<()> {
        *self.active_project.write().await = name.clone();
        // Persist: read current meta, update, write back
        let meta_path = self.data_dir.join("tenant.toml");
        let content = std::fs::read_to_string(&meta_path)?;
        let mut meta: TenantMeta = toml::from_str(&content)?;
        meta.active_project = name;
        std::fs::write(&meta_path, toml::to_string_pretty(&meta)?)?;
        Ok(())
    }
}
