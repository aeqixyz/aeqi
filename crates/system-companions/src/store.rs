use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use tracing::debug;

use crate::companion::{Companion, Rarity};
use crate::gacha::PityState;

pub struct CompanionStore {
    conn: Mutex<Connection>,
}

impl CompanionStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open companion DB: {}", path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS companions (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                rarity TEXT NOT NULL,
                is_familiar INTEGER NOT NULL DEFAULT 0,
                is_rostered INTEGER NOT NULL DEFAULT 0,
                roster_slot INTEGER,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pity (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                pulls_since_s INTEGER NOT NULL DEFAULT 0,
                pulls_since_a INTEGER NOT NULL DEFAULT 0,
                total_pulls INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS pull_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                companion_id TEXT NOT NULL,
                rarity TEXT NOT NULL,
                pulled_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS fusion_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_a TEXT NOT NULL,
                source_b TEXT NOT NULL,
                result_id TEXT NOT NULL,
                source_rarity TEXT NOT NULL,
                result_rarity TEXT NOT NULL,
                fused_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS relationships (
                agent_a TEXT NOT NULL,
                agent_b TEXT NOT NULL,
                data TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (agent_a, agent_b)
            );

            CREATE INDEX IF NOT EXISTS idx_companions_rarity ON companions(rarity);
            CREATE INDEX IF NOT EXISTS idx_companions_familiar ON companions(is_familiar);
            CREATE INDEX IF NOT EXISTS idx_companions_roster ON companions(is_rostered);
            CREATE INDEX IF NOT EXISTS idx_relationships_a ON relationships(agent_a);
            CREATE INDEX IF NOT EXISTS idx_relationships_b ON relationships(agent_b);",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn save_companion(&self, companion: &Companion) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let data = serde_json::to_string(companion)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO companions (id, data, rarity, is_familiar, is_rostered, roster_slot, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, NULL, ?5)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, rarity = excluded.rarity, is_familiar = excluded.is_familiar",
            rusqlite::params![
                companion.id,
                data,
                companion.rarity.to_string(),
                companion.is_familiar as i32,
                now,
            ],
        )?;

        debug!(id = %companion.id, rarity = %companion.rarity, "companion saved");
        Ok(())
    }

    pub fn get_companion(&self, id: &str) -> Result<Option<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare("SELECT data FROM companions WHERE id = ?1")?;
        let result = stmt
            .query_row(rusqlite::params![id], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .optional()?;

        match result {
            Some(data) => Ok(Some(serde_json::from_str(&data)?)),
            None => Ok(None),
        }
    }

    pub fn get_companion_by_name(&self, name: &str) -> Result<Option<Companion>> {
        let all = self.list_all()?;
        Ok(all.into_iter().find(|c| c.name == name))
    }

    pub fn list_all(&self) -> Result<Vec<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare("SELECT data FROM companions ORDER BY created_at")?;
        let results = stmt
            .query_map([], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Companion>(&data).ok())
            .collect();
        Ok(results)
    }

    pub fn list_by_rarity(&self, rarity: Rarity) -> Result<Vec<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare("SELECT data FROM companions WHERE rarity = ?1 ORDER BY created_at")?;
        let results = stmt
            .query_map(rusqlite::params![rarity.to_string()], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Companion>(&data).ok())
            .collect();
        Ok(results)
    }

    pub fn get_familiar(&self) -> Result<Option<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare("SELECT data FROM companions WHERE is_familiar = 1 LIMIT 1")?;
        let result = stmt
            .query_row([], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .optional()?;

        match result {
            Some(data) => Ok(Some(serde_json::from_str(&data)?)),
            None => Ok(None),
        }
    }

    pub fn set_familiar(&self, companion_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        conn.execute("UPDATE companions SET is_familiar = 0, data = json_set(data, '$.is_familiar', json('false')) WHERE is_familiar = 1", [])?;

        let data: String = conn.query_row(
            "SELECT data FROM companions WHERE id = ?1",
            rusqlite::params![companion_id],
            |row| row.get(0),
        )?;

        let mut companion: Companion = serde_json::from_str(&data)?;
        companion.is_familiar = true;
        let updated = serde_json::to_string(&companion)?;

        conn.execute(
            "UPDATE companions SET is_familiar = 1, data = ?1 WHERE id = ?2",
            rusqlite::params![updated, companion_id],
        )?;

        debug!(id = %companion_id, "familiar set");
        Ok(())
    }

    // ── Roster / Party methods ──

    pub fn get_roster(&self) -> Result<Vec<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT data FROM companions WHERE is_rostered = 1 ORDER BY roster_slot ASC",
        )?;
        let results = stmt
            .query_map([], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Companion>(&data).ok())
            .collect();
        Ok(results)
    }

    pub fn set_roster(&self, companion_ids: &[String]) -> Result<()> {
        if companion_ids.len() > 4 {
            anyhow::bail!("roster cannot exceed 4 members");
        }

        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        // Validate all IDs exist.
        for id in companion_ids {
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM companions WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )?;
            if !exists {
                anyhow::bail!("companion not found: {id}");
            }
        }

        // Clear all roster state.
        conn.execute("UPDATE companions SET is_rostered = 0, roster_slot = NULL", [])?;

        // Set each member with slot index.
        for (slot, id) in companion_ids.iter().enumerate() {
            conn.execute(
                "UPDATE companions SET is_rostered = 1, roster_slot = ?1 WHERE id = ?2",
                rusqlite::params![slot as i32, id],
            )?;
        }

        // If current familiar is not in the new roster, auto-set first member as familiar.
        if !companion_ids.is_empty() {
            let familiar_in_roster: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM companions WHERE is_familiar = 1 AND is_rostered = 1",
                [],
                |row| row.get(0),
            )?;

            if !familiar_in_roster {
                // Clear old familiar.
                conn.execute(
                    "UPDATE companions SET is_familiar = 0, data = json_set(data, '$.is_familiar', json('false')) WHERE is_familiar = 1",
                    [],
                )?;
                // Set first roster member as familiar.
                let first_id = &companion_ids[0];
                let data: String = conn.query_row(
                    "SELECT data FROM companions WHERE id = ?1",
                    rusqlite::params![first_id],
                    |row| row.get(0),
                )?;
                let mut companion: Companion = serde_json::from_str(&data)?;
                companion.is_familiar = true;
                let updated = serde_json::to_string(&companion)?;
                conn.execute(
                    "UPDATE companions SET is_familiar = 1, data = ?1 WHERE id = ?2",
                    rusqlite::params![updated, first_id],
                )?;
            }
        }

        debug!(count = companion_ids.len(), "roster set");
        Ok(())
    }

    pub fn get_leader(&self) -> Result<Option<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        // Try: familiar that is also rostered.
        let result = conn
            .query_row(
                "SELECT data FROM companions WHERE is_familiar = 1 AND is_rostered = 1 LIMIT 1",
                [],
                |row| {
                    let data: String = row.get(0)?;
                    Ok(data)
                },
            )
            .optional()?;

        if let Some(data) = result {
            return Ok(Some(serde_json::from_str(&data)?));
        }

        // Fallback: first rostered companion.
        let result = conn
            .query_row(
                "SELECT data FROM companions WHERE is_rostered = 1 ORDER BY roster_slot ASC LIMIT 1",
                [],
                |row| {
                    let data: String = row.get(0)?;
                    Ok(data)
                },
            )
            .optional()?;

        if let Some(data) = result {
            return Ok(Some(serde_json::from_str(&data)?));
        }

        // Final fallback: just the familiar (no roster set).
        let result = conn
            .query_row(
                "SELECT data FROM companions WHERE is_familiar = 1 LIMIT 1",
                [],
                |row| {
                    let data: String = row.get(0)?;
                    Ok(data)
                },
            )
            .optional()?;

        match result {
            Some(data) => Ok(Some(serde_json::from_str(&data)?)),
            None => Ok(None),
        }
    }

    pub fn set_leader(&self, companion_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        // Validate companion is in roster.
        let in_roster: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM companions WHERE id = ?1 AND is_rostered = 1",
            rusqlite::params![companion_id],
            |row| row.get(0),
        )?;

        if !in_roster {
            anyhow::bail!("companion must be in roster to be set as leader");
        }

        drop(conn);
        // Reuse existing set_familiar logic (leader = familiar).
        self.set_familiar(companion_id)?;

        debug!(id = %companion_id, "leader set");
        Ok(())
    }

    pub fn remove_companion(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        conn.execute("DELETE FROM companions WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    pub fn load_pity(&self) -> Result<PityState> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let result = conn
            .query_row(
                "SELECT pulls_since_s, pulls_since_a, total_pulls FROM pity WHERE id = 1",
                [],
                |row| {
                    Ok(PityState {
                        pulls_since_s_or_above: row.get::<_, u32>(0)?,
                        pulls_since_a_or_above: row.get::<_, u32>(1)?,
                        total_pulls: row.get::<_, u64>(2)?,
                    })
                },
            )
            .optional()?;

        Ok(result.unwrap_or_default())
    }

    pub fn save_pity(&self, pity: &PityState) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        conn.execute(
            "INSERT OR REPLACE INTO pity (id, pulls_since_s, pulls_since_a, total_pulls)
             VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![
                pity.pulls_since_s_or_above,
                pity.pulls_since_a_or_above,
                pity.total_pulls as i64,
            ],
        )?;
        Ok(())
    }

    pub fn record_pull(&self, companion: &Companion) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO pull_history (companion_id, rarity, pulled_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![companion.id, companion.rarity.to_string(), now],
        )?;
        Ok(())
    }

    pub fn record_fusion(&self, a: &Companion, b: &Companion, result: &Companion) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO fusion_history (source_a, source_b, result_id, source_rarity, result_rarity, fused_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                a.id,
                b.id,
                result.id,
                a.rarity.to_string(),
                result.rarity.to_string(),
                now,
            ],
        )?;
        Ok(())
    }

    // ── Relationship methods ──

    pub fn save_relationship(&self, rel: &crate::relationship::AgentRelationship) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let data = serde_json::to_string(rel)?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO relationships (agent_a, agent_b, data, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent_a, agent_b) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
            rusqlite::params![rel.agent_a, rel.agent_b, data, now],
        )?;
        Ok(())
    }

    pub fn get_relationship(&self, name_a: &str, name_b: &str) -> Result<Option<crate::relationship::AgentRelationship>> {
        let (a, b) = crate::relationship::AgentRelationship::canonical_key(name_a, name_b);
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let result = conn
            .query_row(
                "SELECT data FROM relationships WHERE agent_a = ?1 AND agent_b = ?2",
                rusqlite::params![a, b],
                |row| {
                    let data: String = row.get(0)?;
                    Ok(data)
                },
            )
            .optional()?;
        match result {
            Some(data) => Ok(Some(serde_json::from_str(&data)?)),
            None => Ok(None),
        }
    }

    /// Get or lazily seed a relationship between two companions.
    pub fn get_or_seed_relationship(
        &self,
        comp_a: &crate::companion::Companion,
        comp_b: &crate::companion::Companion,
    ) -> Result<crate::relationship::AgentRelationship> {
        if let Some(rel) = self.get_relationship(&comp_a.name, &comp_b.name)? {
            return Ok(rel);
        }
        let rel = crate::relationship::seed_from_traits(comp_a, comp_b);
        self.save_relationship(&rel)?;
        Ok(rel)
    }

    /// Get all relationships for a companion (by name).
    pub fn get_relationships_for(&self, companion_name: &str) -> Result<Vec<crate::relationship::AgentRelationship>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT data FROM relationships WHERE agent_a = ?1 OR agent_b = ?1",
        )?;
        let results = stmt
            .query_map(rusqlite::params![companion_name], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<crate::relationship::AgentRelationship>(&data).ok())
            .collect();
        Ok(results)
    }

    pub fn collection_stats(&self) -> Result<CollectionStats> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        let total: u32 = conn.query_row("SELECT COUNT(*) FROM companions", [], |row| row.get(0))?;

        let by_rarity = |r: &str| -> Result<u32> {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM companions WHERE rarity = ?1",
                rusqlite::params![r],
                |row| row.get(0),
            )?)
        };

        let total_pulls: u64 = conn
            .query_row("SELECT COALESCE(total_pulls, 0) FROM pity WHERE id = 1", [], |row| row.get(0))
            .unwrap_or(0);

        let total_fusions: u32 = conn.query_row("SELECT COUNT(*) FROM fusion_history", [], |row| row.get(0))?;

        Ok(CollectionStats {
            total_companions: total,
            c_count: by_rarity("C")?,
            b_count: by_rarity("B")?,
            a_count: by_rarity("A")?,
            s_count: by_rarity("S")?,
            ss_count: by_rarity("SS")?,
            total_pulls,
            total_fusions,
        })
    }

    pub fn companion_count(&self) -> Result<u32> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        Ok(conn.query_row("SELECT COUNT(*) FROM companions", [], |row| row.get(0))?)
    }

    pub fn fusion_eligible_pairs(&self, rarity: Rarity) -> Result<Vec<Companion>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT data FROM companions WHERE rarity = ?1 AND is_familiar = 0 ORDER BY created_at",
        )?;
        let results = stmt
            .query_map(rusqlite::params![rarity.to_string()], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Companion>(&data).ok())
            .collect();
        Ok(results)
    }
}

use rusqlite::OptionalExtension;

#[derive(Debug, Clone)]
pub struct CollectionStats {
    pub total_companions: u32,
    pub c_count: u32,
    pub b_count: u32,
    pub a_count: u32,
    pub s_count: u32,
    pub ss_count: u32,
    pub total_pulls: u64,
    pub total_fusions: u32,
}

impl std::fmt::Display for CollectionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Collection: {} companions | Pulls: {} | Fusions: {}\n\
             C: {} | B: {} | A: {} | S: {} | SS: {}",
            self.total_companions,
            self.total_pulls,
            self.total_fusions,
            self.c_count,
            self.b_count,
            self.a_count,
            self.s_count,
            self.ss_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gacha::{GachaEngine, PityState as GachaPity};
    use tempfile::TempDir;

    fn temp_store() -> (CompanionStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = CompanionStore::open(&dir.path().join("companions.db")).unwrap();
        (store, dir)
    }

    fn make_companion() -> Companion {
        let engine = GachaEngine::default();
        let mut pity = GachaPity::default();
        engine.pull(&mut pity)
    }

    #[test]
    fn test_save_and_get() {
        let (store, _dir) = temp_store();
        let c = make_companion();
        store.save_companion(&c).unwrap();

        let loaded = store.get_companion(&c.id).unwrap().unwrap();
        assert_eq!(loaded.id, c.id);
        assert_eq!(loaded.rarity, c.rarity);
    }

    #[test]
    fn test_list_all() {
        let (store, _dir) = temp_store();
        for _ in 0..5 {
            store.save_companion(&make_companion()).unwrap();
        }
        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_familiar_management() {
        let (store, _dir) = temp_store();
        let mut c1 = make_companion();
        c1.rarity = Rarity::SS;
        c1.familiar_eligible = true;
        store.save_companion(&c1).unwrap();

        let mut c2 = make_companion();
        c2.rarity = Rarity::SS;
        c2.familiar_eligible = true;
        store.save_companion(&c2).unwrap();

        store.set_familiar(&c1.id).unwrap();
        let fam = store.get_familiar().unwrap().unwrap();
        assert_eq!(fam.id, c1.id);
        assert!(fam.is_familiar);

        store.set_familiar(&c2.id).unwrap();
        let fam = store.get_familiar().unwrap().unwrap();
        assert_eq!(fam.id, c2.id);

        let old_fam = store.get_companion(&c1.id).unwrap().unwrap();
        assert!(!old_fam.is_familiar);
    }

    #[test]
    fn test_pity_persistence() {
        let (store, _dir) = temp_store();
        let pity = PityState {
            pulls_since_s_or_above: 15,
            pulls_since_a_or_above: 5,
            total_pulls: 42,
        };
        store.save_pity(&pity).unwrap();

        let loaded = store.load_pity().unwrap();
        assert_eq!(loaded.pulls_since_s_or_above, 15);
        assert_eq!(loaded.pulls_since_a_or_above, 5);
        assert_eq!(loaded.total_pulls, 42);
    }

    #[test]
    fn test_roster_set_and_get() {
        let (store, _dir) = temp_store();
        let c1 = make_companion();
        let c2 = make_companion();
        let c3 = make_companion();
        store.save_companion(&c1).unwrap();
        store.save_companion(&c2).unwrap();
        store.save_companion(&c3).unwrap();

        store.set_roster(&[c2.id.clone(), c1.id.clone(), c3.id.clone()]).unwrap();

        let roster = store.get_roster().unwrap();
        assert_eq!(roster.len(), 3);
        assert_eq!(roster[0].id, c2.id);
        assert_eq!(roster[1].id, c1.id);
        assert_eq!(roster[2].id, c3.id);
    }

    #[test]
    fn test_roster_max_size() {
        let (store, _dir) = temp_store();
        let ids: Vec<String> = (0..5).map(|_| {
            let c = make_companion();
            store.save_companion(&c).unwrap();
            c.id
        }).collect();

        let result = store.set_roster(&ids);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed 4"));
    }

    #[test]
    fn test_set_leader() {
        let (store, _dir) = temp_store();
        let c1 = make_companion();
        let c2 = make_companion();
        store.save_companion(&c1).unwrap();
        store.save_companion(&c2).unwrap();
        store.set_familiar(&c1.id).unwrap();
        store.set_roster(&[c1.id.clone(), c2.id.clone()]).unwrap();

        store.set_leader(&c2.id).unwrap();

        let leader = store.get_leader().unwrap().unwrap();
        assert_eq!(leader.id, c2.id);
        assert!(leader.is_familiar);
    }

    #[test]
    fn test_leader_fallback_chain() {
        let (store, _dir) = temp_store();
        let c1 = make_companion();
        store.save_companion(&c1).unwrap();
        store.set_familiar(&c1.id).unwrap();

        // No roster → falls back to familiar.
        let leader = store.get_leader().unwrap().unwrap();
        assert_eq!(leader.id, c1.id);
    }

    #[test]
    fn test_leader_must_be_in_roster() {
        let (store, _dir) = temp_store();
        let c1 = make_companion();
        let c2 = make_companion();
        store.save_companion(&c1).unwrap();
        store.save_companion(&c2).unwrap();
        store.set_roster(&[c1.id.clone()]).unwrap();

        let result = store.set_leader(&c2.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be in roster"));
    }

    #[test]
    fn test_save_companion_preserves_roster() {
        let (store, _dir) = temp_store();
        let mut c1 = make_companion();
        store.save_companion(&c1).unwrap();
        store.set_roster(&[c1.id.clone()]).unwrap();

        // Re-save companion — roster status should be preserved.
        c1.bond_level = 5;
        store.save_companion(&c1).unwrap();

        let roster = store.get_roster().unwrap();
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].id, c1.id);
        assert_eq!(roster[0].bond_level, 5);
    }

    #[test]
    fn test_roster_auto_sets_familiar() {
        let (store, _dir) = temp_store();
        let c1 = make_companion();
        let c2 = make_companion();
        store.save_companion(&c1).unwrap();
        store.save_companion(&c2).unwrap();

        // No familiar set, setting roster should auto-set first as familiar.
        store.set_roster(&[c2.id.clone(), c1.id.clone()]).unwrap();

        let leader = store.get_leader().unwrap().unwrap();
        assert_eq!(leader.id, c2.id);
    }

    #[test]
    fn test_collection_stats() {
        let (store, _dir) = temp_store();
        for _ in 0..3 {
            store.save_companion(&make_companion()).unwrap();
        }
        let stats = store.collection_stats().unwrap();
        assert_eq!(stats.total_companions, 3);
    }
}
