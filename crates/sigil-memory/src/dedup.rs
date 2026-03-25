//! Memory deduplication pipeline.
//!
//! Before storing a new memory, the pipeline checks for similar existing
//! memories and decides: **Skip** (near-duplicate), **Create** (novel),
//! **Merge** (enhances existing), or **Supersede** (contradicts existing).
//!
//! This is the heuristic fast-path; a future LLM judgment layer can refine
//! the decision for ambiguous cases.

use serde::{Deserialize, Serialize};

// ── Types ───────────────────────────────────────────────────────────────────

/// Action the pipeline recommends for a candidate memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupAction {
    /// Candidate is a near-duplicate — discard it.
    Skip,
    /// Candidate is novel enough — store as a new memory.
    Create,
    /// Candidate should be merged into an existing memory (by id).
    Merge(String),
    /// Candidate supersedes (contradicts) an existing memory (by id).
    Supersede(String),
}

/// A candidate memory about to be stored.
#[derive(Debug, Clone)]
pub struct DedupCandidate {
    /// Semantic key (e.g. "auth/jwt-rotation").
    pub key: String,
    /// Full content text.
    pub content: String,
    /// Optional pre-computed embedding for vector comparison.
    pub embedding: Option<Vec<f32>>,
}

/// An existing memory that may be similar to the candidate.
#[derive(Debug, Clone)]
pub struct SimilarMemory {
    /// Memory ID in the store.
    pub id: String,
    /// Semantic key.
    pub key: String,
    /// Full content text.
    pub content: String,
    /// Similarity score to the candidate (`0.0..=1.0`).
    pub similarity: f32,
}

// ── Pipeline ────────────────────────────────────────────────────────────────

/// Deduplication pipeline that decides whether a candidate memory should be
/// created, merged, skipped, or used to supersede an existing memory.
pub struct DedupPipeline {
    /// Minimum similarity to consider two memories related (default 0.85).
    pub similarity_threshold: f32,
}

impl Default for DedupPipeline {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.85,
        }
    }
}

impl DedupPipeline {
    /// Create a pipeline with a custom similarity threshold.
    pub fn new(similarity_threshold: f32) -> Self {
        Self {
            similarity_threshold,
        }
    }

    /// Filter existing memories to those above the similarity threshold.
    pub fn find_similar<'a>(
        &self,
        _candidate: &DedupCandidate,
        existing: &'a [SimilarMemory],
    ) -> Vec<&'a SimilarMemory> {
        existing
            .iter()
            .filter(|m| m.similarity >= self.similarity_threshold)
            .collect()
    }

    /// Decide what to do with a candidate given a set of similar memories.
    ///
    /// Decision logic (evaluated in order):
    /// 1. No similar memories → **Create**
    /// 2. Similarity > 0.95 → **Skip** (near-duplicate)
    /// 3. Contradiction detected → **Supersede** the top match
    /// 4. Same key and similarity 0.85–0.95 → **Merge** with top match
    /// 5. Otherwise → **Create** (novel enough)
    pub fn decide(&self, candidate: &DedupCandidate, similar: &[SimilarMemory]) -> DedupAction {
        let above_threshold: Vec<&SimilarMemory> = similar
            .iter()
            .filter(|m| m.similarity >= self.similarity_threshold)
            .collect();

        if above_threshold.is_empty() {
            return DedupAction::Create;
        }

        // Sort by similarity descending to find the top match.
        let top = above_threshold
            .iter()
            .max_by(|a, b| {
                a.similarity
                    .partial_cmp(&b.similarity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap(); // safe: above_threshold is non-empty

        // Near-duplicate: skip.
        if top.similarity > 0.95 {
            return DedupAction::Skip;
        }

        // Contradiction: supersede.
        if is_contradiction(&candidate.content, &top.content) {
            return DedupAction::Supersede(top.id.clone());
        }

        // Same key, moderate similarity: merge.
        if candidate.key == top.key {
            return DedupAction::Merge(top.id.clone());
        }

        // Novel enough.
        DedupAction::Create
    }
}

// ── Contradiction Heuristic ─────────────────────────────────────────────────

/// Negation markers that indicate a statement reverses a previous claim.
const NEGATION_MARKERS: &[&str] = &[
    "not ",
    "no longer",
    "instead of",
    "replaced",
    "removed",
    "deprecated",
    "disabled",
    "don't",
    "doesn't",
    "won't",
    "cannot",
    "shouldn't",
    "never",
];

/// Simple heuristic contradiction check.
///
/// Returns `true` when one text contains negation markers that the other
/// lacks.  This is intentionally coarse — it catches the obvious cases
/// ("we use MySQL" vs "we no longer use MySQL") without requiring NLP.
pub fn is_contradiction(a: &str, b: &str) -> bool {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    let a_negs: Vec<&&str> = NEGATION_MARKERS
        .iter()
        .filter(|m| a_lower.contains(**m))
        .collect();
    let b_negs: Vec<&&str> = NEGATION_MARKERS
        .iter()
        .filter(|m| b_lower.contains(**m))
        .collect();

    // If one side has negation markers and the other doesn't,
    // that's a contradiction signal.
    if a_negs.is_empty() != b_negs.is_empty() {
        return true;
    }

    // If both have negation markers, but different ones, that could also
    // indicate contradiction — but to avoid false positives we only flag
    // the asymmetric case above.
    false
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline() -> DedupPipeline {
        DedupPipeline::default()
    }

    fn candidate(key: &str, content: &str) -> DedupCandidate {
        DedupCandidate {
            key: key.to_string(),
            content: content.to_string(),
            embedding: None,
        }
    }

    fn similar(id: &str, key: &str, content: &str, similarity: f32) -> SimilarMemory {
        SimilarMemory {
            id: id.to_string(),
            key: key.to_string(),
            content: content.to_string(),
            similarity,
        }
    }

    #[test]
    fn no_similar_creates() {
        let p = pipeline();
        let c = candidate("auth/jwt", "JWT rotation every 24h");
        let action = p.decide(&c, &[]);
        assert_eq!(action, DedupAction::Create);
    }

    #[test]
    fn exact_duplicate_skips() {
        let p = pipeline();
        let c = candidate("auth/jwt", "JWT rotation every 24h");
        let existing = vec![similar(
            "mem-1",
            "auth/jwt",
            "JWT rotation every 24h",
            0.97,
        )];
        let action = p.decide(&c, &existing);
        assert_eq!(action, DedupAction::Skip);
    }

    #[test]
    fn similar_same_key_merges() {
        let p = pipeline();
        let c = candidate("auth/jwt", "JWT rotation every 12h with refresh tokens");
        let existing = vec![similar(
            "mem-1",
            "auth/jwt",
            "JWT rotation every 24h",
            0.90,
        )];
        let action = p.decide(&c, &existing);
        assert_eq!(action, DedupAction::Merge("mem-1".to_string()));
    }

    #[test]
    fn novel_content_creates() {
        let p = pipeline();
        let c = candidate("deploy/docker", "Use Docker compose for local dev");
        let existing = vec![similar(
            "mem-1",
            "auth/jwt",
            "JWT rotation every 24h",
            0.88,
        )];
        // Different key → Create even though similarity is above threshold.
        let action = p.decide(&c, &existing);
        assert_eq!(action, DedupAction::Create);
    }

    #[test]
    fn contradiction_supersedes() {
        let p = pipeline();
        let c = candidate("db/backend", "We no longer use MySQL, migrated to PostgreSQL");
        let existing = vec![similar(
            "mem-1",
            "db/backend",
            "We use MySQL for the main database",
            0.90,
        )];
        let action = p.decide(&c, &existing);
        assert_eq!(action, DedupAction::Supersede("mem-1".to_string()));
    }

    #[test]
    fn below_threshold_creates() {
        let p = pipeline();
        let c = candidate("pricing/tiers", "Three pricing tiers: free, pro, enterprise");
        let existing = vec![similar(
            "mem-1",
            "auth/jwt",
            "JWT rotation every 24h",
            0.50,
        )];
        let action = p.decide(&c, &existing);
        assert_eq!(action, DedupAction::Create);
    }

    #[test]
    fn find_similar_filters_by_threshold() {
        let p = DedupPipeline::new(0.90);
        let c = candidate("test", "test content");
        let existing = vec![
            similar("mem-1", "a", "content a", 0.95),
            similar("mem-2", "b", "content b", 0.85),
            similar("mem-3", "c", "content c", 0.92),
        ];
        let result = p.find_similar(&c, &existing);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|m| m.similarity >= 0.90));
    }

    #[test]
    fn contradiction_detection_asymmetric_negation() {
        assert!(is_contradiction(
            "We use MySQL for the database",
            "We no longer use MySQL"
        ));
        assert!(is_contradiction(
            "Feature X was removed",
            "Feature X is available"
        ));
    }

    #[test]
    fn no_contradiction_when_both_neutral() {
        assert!(!is_contradiction(
            "We use PostgreSQL",
            "The database is PostgreSQL"
        ));
    }

    #[test]
    fn no_contradiction_when_both_have_negation() {
        // Both sides have negation markers — ambiguous, not flagged.
        assert!(!is_contradiction(
            "We don't use MySQL",
            "We no longer use Oracle"
        ));
    }
}
