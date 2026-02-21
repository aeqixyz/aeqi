use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::bead::{Bead, BeadId, BeadStatus};

/// JSONL-based bead store. One file per prefix, git-native.
pub struct BeadStore {
    dir: PathBuf,
    /// In-memory index: all beads keyed by ID.
    beads: HashMap<String, Bead>,
    /// Next sequence number per prefix.
    sequences: HashMap<String, u32>,
}

impl BeadStore {
    /// Open or create a bead store in the given directory.
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create beads dir: {}", dir.display()))?;

        let mut store = Self {
            dir: dir.to_path_buf(),
            beads: HashMap::new(),
            sequences: HashMap::new(),
        };

        store.load_all()?;
        Ok(store)
    }

    /// Load all JSONL files from the store directory.
    fn load_all(&mut self) -> Result<()> {
        let entries = std::fs::read_dir(&self.dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                self.load_file(&path)?;
            }
        }
        Ok(())
    }

    /// Load beads from a single JSONL file.
    fn load_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match serde_json::from_str::<Bead>(line) {
                Ok(bead) => {
                    // Track max sequence for this prefix.
                    let prefix = bead.id.prefix().to_string();
                    if bead.id.depth() == 0 {
                        if let Some(seq_str) = bead.id.0.split('-').nth(1) {
                            // Handle dotted children: take only the root part.
                            let root_seq = seq_str.split('.').next().unwrap_or(seq_str);
                            if let Ok(seq) = root_seq.parse::<u32>() {
                                let entry = self.sequences.entry(prefix).or_insert(0);
                                *entry = (*entry).max(seq);
                            }
                        }
                    }
                    self.beads.insert(bead.id.0.clone(), bead);
                }
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "skipping malformed bead line");
                }
            }
        }

        Ok(())
    }

    /// Persist a bead to its prefix JSONL file (append).
    fn persist(&self, bead: &Bead) -> Result<()> {
        let prefix = bead.id.prefix();
        let path = self.dir.join(format!("{prefix}.jsonl"));

        let line = serde_json::to_string(bead)? + "\n";

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(line.as_bytes())?;

        Ok(())
    }

    /// Rewrite the entire JSONL file for a prefix (after updates).
    fn rewrite_prefix(&self, prefix: &str) -> Result<()> {
        let path = self.dir.join(format!("{prefix}.jsonl"));

        let mut beads: Vec<&Bead> = self
            .beads
            .values()
            .filter(|b| b.id.prefix() == prefix)
            .collect();
        beads.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let mut content = String::new();
        for bead in beads {
            content.push_str(&serde_json::to_string(bead)?);
            content.push('\n');
        }

        std::fs::write(&path, &content)
            .with_context(|| format!("failed to write {}", path.display()))?;

        Ok(())
    }

    /// Create a new bead with auto-generated ID.
    pub fn create(&mut self, prefix: &str, subject: &str) -> Result<Bead> {
        let seq = self.sequences.entry(prefix.to_string()).or_insert(0);
        *seq += 1;
        let id = BeadId::root(prefix, *seq);

        let bead = Bead::new(id, subject);
        self.persist(&bead)?;
        self.beads.insert(bead.id.0.clone(), bead.clone());

        Ok(bead)
    }

    /// Create a child bead under a parent.
    pub fn create_child(&mut self, parent_id: &BeadId, subject: &str) -> Result<Bead> {
        // Count existing children to determine next child seq.
        let child_count = self
            .beads
            .values()
            .filter(|b| {
                b.id.parent().as_ref() == Some(parent_id)
            })
            .count() as u32;

        let id = parent_id.child(child_count + 1);
        let mut bead = Bead::new(id, subject);
        // Inherit prefix from parent.
        bead.depends_on = Vec::new();

        self.persist(&bead)?;
        self.beads.insert(bead.id.0.clone(), bead.clone());

        Ok(bead)
    }

    /// Get a bead by ID.
    pub fn get(&self, id: &str) -> Option<&Bead> {
        self.beads.get(id)
    }

    /// Update a bead. Returns the updated bead.
    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut Bead)) -> Result<Bead> {
        let bead = self
            .beads
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("bead not found: {id}"))?;

        f(bead);
        bead.updated_at = Some(chrono::Utc::now());

        let bead = bead.clone();
        let prefix = bead.id.prefix().to_string();
        self.rewrite_prefix(&prefix)?;

        Ok(bead)
    }

    /// Close a bead (mark as done with reason).
    pub fn close(&mut self, id: &str, reason: &str) -> Result<Bead> {
        self.update(id, |b| {
            b.status = BeadStatus::Done;
            b.closed_at = Some(chrono::Utc::now());
            b.closed_reason = Some(reason.to_string());
        })
    }

    /// Cancel a bead.
    pub fn cancel(&mut self, id: &str, reason: &str) -> Result<Bead> {
        self.update(id, |b| {
            b.status = BeadStatus::Cancelled;
            b.closed_at = Some(chrono::Utc::now());
            b.closed_reason = Some(reason.to_string());
        })
    }

    /// Add a dependency: `id` depends on `dep_id`.
    pub fn add_dependency(&mut self, id: &str, dep_id: &str) -> Result<()> {
        let dep_bead_id = BeadId::from(dep_id);

        // Add to depends_on.
        self.update(id, |b| {
            if !b.depends_on.contains(&dep_bead_id) {
                b.depends_on.push(dep_bead_id.clone());
            }
        })?;

        // Add to blocks on the dependency.
        let blocker_id = BeadId::from(id);
        if self.beads.contains_key(dep_id) {
            self.update(dep_id, |b| {
                if !b.blocks.contains(&blocker_id) {
                    b.blocks.push(blocker_id.clone());
                }
            })?;
        }

        Ok(())
    }

    /// Get all beads that are ready (pending + all deps resolved).
    pub fn ready(&self) -> Vec<&Bead> {
        let resolved = |id: &BeadId| -> bool {
            self.beads.get(&id.0).is_some_and(|b| b.is_closed())
        };

        let mut ready: Vec<&Bead> = self
            .beads
            .values()
            .filter(|b| b.is_ready(&resolved))
            .collect();

        // Sort by priority (highest first), then by creation time.
        ready.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        ready
    }

    /// Get all beads matching a prefix.
    pub fn by_prefix(&self, prefix: &str) -> Vec<&Bead> {
        let mut beads: Vec<&Bead> = self
            .beads
            .values()
            .filter(|b| b.id.prefix() == prefix)
            .collect();
        beads.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        beads
    }

    /// Get all beads.
    pub fn all(&self) -> Vec<&Bead> {
        let mut beads: Vec<&Bead> = self.beads.values().collect();
        beads.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        beads
    }

    /// Get all beads assigned to a specific agent.
    pub fn assigned_to(&self, assignee: &str) -> Vec<&Bead> {
        self.beads
            .values()
            .filter(|b| b.assignee.as_deref() == Some(assignee) && !b.is_closed())
            .collect()
    }

    /// Get children of a bead.
    pub fn children(&self, parent_id: &BeadId) -> Vec<&Bead> {
        self.beads
            .values()
            .filter(|b| b.id.parent().as_ref() == Some(parent_id))
            .collect()
    }

    /// Count open beads by prefix.
    pub fn open_count_by_prefix(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for bead in self.beads.values() {
            if !bead.is_closed() {
                *counts.entry(bead.id.prefix().to_string()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Store directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Total bead count.
    pub fn len(&self) -> usize {
        self.beads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.beads.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (BeadStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BeadStore::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn test_create_and_get() {
        let (mut store, _dir) = temp_store();
        let bead = store.create("as", "Fix login bug").unwrap();
        assert_eq!(bead.id.0, "as-001");
        assert_eq!(bead.subject, "Fix login bug");

        let bead2 = store.create("as", "Add logout button").unwrap();
        assert_eq!(bead2.id.0, "as-002");

        assert!(store.get("as-001").is_some());
        assert!(store.get("as-002").is_some());
        assert!(store.get("as-003").is_none());
    }

    #[test]
    fn test_children() {
        let (mut store, _dir) = temp_store();
        let parent = store.create("as", "Feature X").unwrap();
        let child1 = store.create_child(&parent.id, "Step 1").unwrap();
        let child2 = store.create_child(&parent.id, "Step 2").unwrap();

        assert_eq!(child1.id.0, "as-001.1");
        assert_eq!(child2.id.0, "as-001.2");
        assert_eq!(child1.id.parent().unwrap(), parent.id);
    }

    #[test]
    fn test_dependencies_and_ready() {
        let (mut store, _dir) = temp_store();
        let b1 = store.create("as", "Task 1").unwrap();
        let b2 = store.create("as", "Task 2").unwrap();

        store.add_dependency(&b2.id.0, &b1.id.0).unwrap();

        // b1 is ready, b2 is blocked.
        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, b1.id);

        // Close b1 → b2 becomes ready.
        store.close(&b1.id.0, "completed").unwrap();
        let ready = store.ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, b2.id);
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();

        {
            let mut store = BeadStore::open(dir.path()).unwrap();
            store.create("rd", "Price check").unwrap();
            store.create("rd", "Inventory update").unwrap();
        }

        // Reopen and verify data persisted.
        let store = BeadStore::open(dir.path()).unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.get("rd-001").is_some());
        assert!(store.get("rd-002").is_some());
    }
}
