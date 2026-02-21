use anyhow::Result;
use sigil_beads::BeadStore;
use sigil_core::config::RigConfig;
use sigil_core::identity::Identity;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A Rig is an isolated Business Unit container.
/// Each rig has its own bead store, identity, workers, and worktree root.
pub struct Rig {
    pub name: String,
    pub prefix: String,
    pub repo: PathBuf,
    pub worktree_root: PathBuf,
    pub model: String,
    pub max_workers: u32,
    pub identity: Identity,
    pub beads: Arc<Mutex<BeadStore>>,
}

impl Rig {
    /// Create a rig from configuration.
    pub fn from_config(config: &RigConfig, rig_dir: &std::path::Path, default_model: &str) -> Result<Self> {
        let identity = Identity::load(rig_dir).unwrap_or_default();

        let beads_dir = rig_dir.join(".beads");
        let beads = BeadStore::open(&beads_dir)?;

        let worktree_root = config
            .worktree_root
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&config.repo).join("..").join("worktrees"));

        Ok(Self {
            name: config.name.clone(),
            prefix: config.prefix.clone(),
            repo: PathBuf::from(&config.repo),
            worktree_root,
            model: config.model.clone().unwrap_or_else(|| default_model.to_string()),
            max_workers: config.max_workers,
            identity,
            beads: Arc::new(Mutex::new(beads)),
        })
    }

    /// Create a bead in this rig's store.
    pub async fn create_bead(&self, subject: &str) -> Result<sigil_beads::Bead> {
        let mut store = self.beads.lock().await;
        store.create(&self.prefix, subject)
    }

    /// Get ready beads for this rig.
    pub async fn ready_beads(&self) -> Vec<sigil_beads::Bead> {
        let store = self.beads.lock().await;
        store.ready().into_iter().cloned().collect()
    }

    /// Get all open beads for this rig.
    pub async fn open_beads(&self) -> Vec<sigil_beads::Bead> {
        let store = self.beads.lock().await;
        store
            .by_prefix(&self.prefix)
            .into_iter()
            .filter(|b| !b.is_closed())
            .cloned()
            .collect()
    }
}
