use anyhow::{Context, Result};
use rusqlite::Connection;
use std::sync::Mutex;
use tracing::debug;

/// Cosine similarity between two f32 vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

/// Serialize f32 vector to little-endian bytes for SQLite BLOB storage.
pub fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for val in v {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Deserialize f32 vector from little-endian bytes.
pub fn bytes_to_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Vector store backed by SQLite. Stores embeddings as BLOBs.
pub struct VectorStore {
    conn: Mutex<Connection>,
    dimensions: usize,
}

/// A vector search result.
#[derive(Debug, Clone)]
pub struct VectorResult {
    pub memory_id: String,
    pub similarity: f32,
}

impl VectorStore {
    /// Open or create the vector store (uses same DB as SqliteMemory).
    pub fn open(conn: &Connection, _dimensions: usize) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_embeddings (
                memory_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                dimensions INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_embed_id ON memory_embeddings(memory_id);",
        )
        .context("failed to create embeddings table")?;
        Ok(())
    }

    /// Create a new VectorStore from an existing connection.
    pub fn new(conn: Mutex<Connection>, dimensions: usize) -> Result<Self> {
        {
            let c = conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
            Self::open(&c, dimensions)?;
        }
        Ok(Self { conn, dimensions })
    }

    /// Store an embedding for a memory ID.
    pub fn store(&self, memory_id: &str, embedding: &[f32]) -> Result<()> {
        if embedding.len() != self.dimensions {
            anyhow::bail!(
                "embedding dimensions mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            );
        }

        let bytes = vec_to_bytes(embedding);
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        conn.execute(
            "INSERT OR REPLACE INTO memory_embeddings (memory_id, embedding, dimensions) VALUES (?1, ?2, ?3)",
            rusqlite::params![memory_id, bytes, self.dimensions as i64],
        )?;
        debug!(memory_id = %memory_id, "embedding stored");
        Ok(())
    }

    /// Delete an embedding.
    pub fn delete(&self, memory_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        conn.execute(
            "DELETE FROM memory_embeddings WHERE memory_id = ?1",
            rusqlite::params![memory_id],
        )?;
        Ok(())
    }

    /// Search for the top-k most similar embeddings to the query vector.
    /// This is a brute-force scan — fine for <100K memories per project.
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<VectorResult>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;

        let mut stmt = conn.prepare("SELECT memory_id, embedding FROM memory_embeddings")?;

        let mut results: Vec<VectorResult> = stmt
            .query_map([], |row| {
                let memory_id: String = row.get(0)?;
                let bytes: Vec<u8> = row.get(1)?;
                Ok((memory_id, bytes))
            })?
            .filter_map(|r| r.ok())
            .map(|(memory_id, bytes)| {
                let embedding = bytes_to_vec(&bytes);
                let similarity = cosine_similarity(query, &embedding);
                VectorResult {
                    memory_id,
                    similarity,
                }
            })
            .collect();

        // Sort by similarity descending.
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 1e-6);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_vec_serialization() {
        let v = vec![1.0f32, 2.5, -3.7, 0.0];
        let bytes = vec_to_bytes(&v);
        let restored = bytes_to_vec(&bytes);
        assert_eq!(v, restored);
    }

    #[test]
    fn test_vector_store() {
        let conn = Connection::open_in_memory().unwrap();
        VectorStore::open(&conn, 3).unwrap();
        let store = VectorStore::new(Mutex::new(conn), 3).unwrap();

        store.store("mem-1", &[1.0, 0.0, 0.0]).unwrap();
        store.store("mem-2", &[0.0, 1.0, 0.0]).unwrap();
        store.store("mem-3", &[0.9, 0.1, 0.0]).unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].memory_id, "mem-1"); // Most similar.
        assert_eq!(results[1].memory_id, "mem-3"); // Second most similar.
    }
}
