use std::collections::HashMap;

/// Hybrid search result merging BM25 keyword scores with vector similarity.
/// Applies temporal decay and MMR re-ranking.
///
/// A scored memory result from any source.
#[derive(Debug, Clone)]
pub struct ScoredResult {
    pub memory_id: String,
    pub keyword_score: f64,
    pub vector_score: f64,
    pub combined_score: f64,
}

/// Merge keyword (BM25) results with vector similarity results.
/// `keyword_weight` + `vector_weight` should sum to 1.0.
pub fn merge_scores(
    keyword_results: &[(String, f64)], // (memory_id, bm25_score)
    vector_results: &[(String, f64)],  // (memory_id, cosine_similarity)
    keyword_weight: f64,
    vector_weight: f64,
) -> Vec<ScoredResult> {
    let mut scores: HashMap<String, ScoredResult> = HashMap::new();

    // Normalize keyword scores to [0, 1].
    let max_kw = keyword_results
        .iter()
        .map(|(_, s)| *s)
        .fold(0.0f64, f64::max);
    let norm_kw = if max_kw > 0.0 { max_kw } else { 1.0 };

    for (id, score) in keyword_results {
        let normalized = score / norm_kw;
        scores
            .entry(id.clone())
            .or_insert_with(|| ScoredResult {
                memory_id: id.clone(),
                keyword_score: 0.0,
                vector_score: 0.0,
                combined_score: 0.0,
            })
            .keyword_score = normalized;
    }

    // Vector scores are already in [0, 1] (cosine similarity).
    for (id, score) in vector_results {
        scores
            .entry(id.clone())
            .or_insert_with(|| ScoredResult {
                memory_id: id.clone(),
                keyword_score: 0.0,
                vector_score: 0.0,
                combined_score: 0.0,
            })
            .vector_score = *score;
    }

    // Compute combined scores.
    let mut results: Vec<ScoredResult> = scores
        .into_values()
        .map(|mut r| {
            r.combined_score = keyword_weight * r.keyword_score + vector_weight * r.vector_score;
            r
        })
        .collect();

    results.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

/// Apply temporal decay to scores.
/// `decay_factor`: value between 0 and 1, computed from age and half-life.
pub fn apply_decay(score: f64, decay_factor: f64) -> f64 {
    score * decay_factor
}

/// Maximal Marginal Relevance (MMR) re-ranking.
/// Balances relevance against diversity by penalizing results similar to already-selected ones.
///
/// `lambda`: 0.0 = maximize diversity, 1.0 = maximize relevance.
/// `similarity_fn`: returns similarity between two memory IDs.
pub fn mmr_rerank<F>(
    candidates: &[ScoredResult],
    top_k: usize,
    lambda: f64,
    similarity_fn: F,
) -> Vec<ScoredResult>
where
    F: Fn(&str, &str) -> f64,
{
    if candidates.is_empty() || top_k == 0 {
        return Vec::new();
    }

    let mut selected: Vec<ScoredResult> = Vec::with_capacity(top_k);
    let mut remaining: Vec<&ScoredResult> = candidates.iter().collect();

    while selected.len() < top_k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let relevance = candidate.combined_score;

            // Max similarity to already-selected items.
            let max_sim = selected
                .iter()
                .map(|s| similarity_fn(&candidate.memory_id, &s.memory_id))
                .fold(0.0f64, f64::max);

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;

            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = i;
            }
        }

        let chosen = remaining.remove(best_idx);
        selected.push(ScoredResult {
            combined_score: best_mmr,
            ..chosen.clone()
        });
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_scores() {
        let kw = vec![("mem-1".to_string(), 5.0), ("mem-2".to_string(), 3.0)];
        let vec_results = vec![("mem-1".to_string(), 0.9), ("mem-3".to_string(), 0.8)];

        let merged = merge_scores(&kw, &vec_results, 0.4, 0.6);
        assert!(!merged.is_empty());
        // mem-1 should be top (appears in both).
        assert_eq!(merged[0].memory_id, "mem-1");
    }

    #[test]
    fn test_mmr_diversifies() {
        let candidates = vec![
            ScoredResult {
                memory_id: "a".to_string(),
                keyword_score: 0.9,
                vector_score: 0.9,
                combined_score: 0.9,
            },
            ScoredResult {
                memory_id: "b".to_string(),
                keyword_score: 0.85,
                vector_score: 0.85,
                combined_score: 0.85,
            },
            ScoredResult {
                memory_id: "c".to_string(),
                keyword_score: 0.5,
                vector_score: 0.5,
                combined_score: 0.5,
            },
        ];

        // Similarity: a and b are very similar, c is different.
        let sim = |a: &str, b: &str| -> f64 {
            match (a, b) {
                ("a", "b") | ("b", "a") => 0.95,
                _ => 0.1,
            }
        };

        let reranked = mmr_rerank(&candidates, 2, 0.7, sim);
        assert_eq!(reranked.len(), 2);
        // First should be "a" (highest relevance).
        assert_eq!(reranked[0].memory_id, "a");
        // Second should be "c" (diverse), not "b" (too similar to "a").
        assert_eq!(reranked[1].memory_id, "c");
    }
}
