use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
}

/// A loaded tenant with all subsystems initialized.
pub struct Tenant {
    pub id: TenantId,
    pub display_name: String,
    pub email: Option<String>,
    pub tier: TierConfig,
    pub tier_name: String,
    pub data_dir: PathBuf,
    pub registry: Arc<ProjectRegistry>,
    pub dispatch_bus: Arc<DispatchBus>,
    pub cost_ledger: Arc<CostLedger>,
    pub companion_store: Arc<CompanionStore>,
    pub conversation_store: Arc<ConversationStore>,
    pub last_active: AtomicU64,
    pub created_at: DateTime<Utc>,
}

impl Tenant {
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
}
