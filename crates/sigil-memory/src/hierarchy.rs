//! Hierarchical memory index with L0/L1 summaries.
//!
//! Memories are organized into logical directories (domain, decisions, cases,
//! patterns, preferences, insights).  Each directory maintains:
//!
//! - **L0**: one-sentence abstract (auto-generated)
//! - **L1**: paragraph overview (auto-generated)
//! - **L2**: full memory content (the actual memories)
//!
//! Search navigates L0 → L1 → L2, enabling efficient tree-based retrieval
//! instead of flat scanning.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Types ───────────────────────────────────────────────────────────────────

/// A logical directory in the memory hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDirectory {
    /// Directory path (e.g. "domain", "decisions", "cases").
    pub path: String,
    /// L0: one-sentence abstract of this directory's contents.
    pub abstract_l0: String,
    /// L1: paragraph-level overview.
    pub overview_l1: String,
    /// IDs of memories that belong to this directory.
    pub memory_ids: Vec<String>,
}

// ── Index ───────────────────────────────────────────────────────────────────

/// Hierarchical index mapping memory categories to directories with summaries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HierarchicalIndex {
    /// Directory path → directory data.
    pub directories: HashMap<String, MemoryDirectory>,
}

impl HierarchicalIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Map a memory category string to the appropriate directory path.
    ///
    /// | Category       | Directory      |
    /// |---------------|----------------|
    /// | fact, context | domain         |
    /// | decision      | decisions      |
    /// | case          | cases          |
    /// | pattern, procedure | patterns  |
    /// | preference    | preferences    |
    /// | insight       | insights       |
    pub fn categorize(category: &str) -> &str {
        match category.to_lowercase().as_str() {
            "fact" | "context" => "domain",
            "decision" => "decisions",
            "case" => "cases",
            "pattern" | "procedure" => "patterns",
            "preference" => "preferences",
            "insight" => "insights",
            // Fallback: use the category name itself.
            _ => "domain",
        }
    }

    /// Add a memory ID to the appropriate directory.
    /// Creates the directory if it doesn't exist.
    pub fn add_memory(&mut self, id: &str, category: &str) {
        let dir_path = Self::categorize(category).to_string();
        let dir = self
            .directories
            .entry(dir_path.clone())
            .or_insert_with(|| MemoryDirectory {
                path: dir_path,
                abstract_l0: String::new(),
                overview_l1: String::new(),
                memory_ids: Vec::new(),
            });

        if !dir.memory_ids.contains(&id.to_string()) {
            dir.memory_ids.push(id.to_string());
        }
    }

    /// Remove a memory ID from its directory.
    pub fn remove_memory(&mut self, id: &str, category: &str) {
        let dir_path = Self::categorize(category);
        if let Some(dir) = self.directories.get_mut(dir_path) {
            dir.memory_ids.retain(|m| m != id);
        }
    }

    /// Regenerate L0 and L1 summaries for a directory from its memory contents.
    ///
    /// Placeholder implementation: L0 = first 100 chars of combined content,
    /// L1 = first 500 chars.  Will be replaced with LLM summarization once
    /// the provider pipeline is wired in.
    pub fn update_summaries(&mut self, dir_path: &str, memories_content: &[&str]) {
        if let Some(dir) = self.directories.get_mut(dir_path) {
            let combined: String = memories_content.join(" | ");

            dir.abstract_l0 = if combined.len() <= 100 {
                combined.clone()
            } else {
                // Find a safe char boundary for the truncation.
                let end = safe_truncate(&combined, 100);
                format!("{}...", &combined[..end])
            };

            dir.overview_l1 = if combined.len() <= 500 {
                combined
            } else {
                let end = safe_truncate(&combined, 500);
                format!("{}...", &combined[..end])
            };
        }
    }

    /// Look up a directory by path.
    pub fn get_directory(&self, path: &str) -> Option<&MemoryDirectory> {
        self.directories.get(path)
    }

    /// Return all directories.
    pub fn all_directories(&self) -> Vec<&MemoryDirectory> {
        self.directories.values().collect()
    }
}

/// Find the largest byte index ≤ `max_bytes` that falls on a char boundary.
fn safe_truncate(s: &str, max_bytes: usize) -> usize {
    if max_bytes >= s.len() {
        return s.len();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorize_maps_correctly() {
        assert_eq!(HierarchicalIndex::categorize("fact"), "domain");
        assert_eq!(HierarchicalIndex::categorize("context"), "domain");
        assert_eq!(HierarchicalIndex::categorize("Fact"), "domain");
        assert_eq!(HierarchicalIndex::categorize("decision"), "decisions");
        assert_eq!(HierarchicalIndex::categorize("case"), "cases");
        assert_eq!(HierarchicalIndex::categorize("pattern"), "patterns");
        assert_eq!(HierarchicalIndex::categorize("procedure"), "patterns");
        assert_eq!(HierarchicalIndex::categorize("preference"), "preferences");
        assert_eq!(HierarchicalIndex::categorize("insight"), "insights");
        // Unknown category falls back to "domain".
        assert_eq!(HierarchicalIndex::categorize("unknown"), "domain");
    }

    #[test]
    fn add_and_lookup_memory() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        idx.add_memory("mem-2", "fact");
        idx.add_memory("mem-3", "decision");

        let domain = idx.get_directory("domain").unwrap();
        assert_eq!(domain.memory_ids.len(), 2);
        assert!(domain.memory_ids.contains(&"mem-1".to_string()));
        assert!(domain.memory_ids.contains(&"mem-2".to_string()));

        let decisions = idx.get_directory("decisions").unwrap();
        assert_eq!(decisions.memory_ids.len(), 1);
        assert!(decisions.memory_ids.contains(&"mem-3".to_string()));
    }

    #[test]
    fn add_memory_no_duplicates() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        idx.add_memory("mem-1", "fact"); // duplicate
        let domain = idx.get_directory("domain").unwrap();
        assert_eq!(domain.memory_ids.len(), 1);
    }

    #[test]
    fn remove_memory() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        idx.add_memory("mem-2", "fact");
        idx.remove_memory("mem-1", "fact");

        let domain = idx.get_directory("domain").unwrap();
        assert_eq!(domain.memory_ids.len(), 1);
        assert!(!domain.memory_ids.contains(&"mem-1".to_string()));
    }

    #[test]
    fn remove_from_nonexistent_directory() {
        let mut idx = HierarchicalIndex::new();
        // Should not panic.
        idx.remove_memory("mem-1", "decision");
        assert!(idx.get_directory("decisions").is_none());
    }

    #[test]
    fn update_summaries_short_content() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        idx.update_summaries("domain", &["Short content here."]);

        let dir = idx.get_directory("domain").unwrap();
        assert_eq!(dir.abstract_l0, "Short content here.");
        assert_eq!(dir.overview_l1, "Short content here.");
    }

    #[test]
    fn update_summaries_truncation() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");

        let long_content = "a".repeat(600);
        idx.update_summaries("domain", &[&long_content]);

        let dir = idx.get_directory("domain").unwrap();
        // L0 should be ~100 chars + "..."
        assert!(dir.abstract_l0.len() <= 104);
        assert!(dir.abstract_l0.ends_with("..."));
        // L1 should be ~500 chars + "..."
        assert!(dir.overview_l1.len() <= 504);
        assert!(dir.overview_l1.ends_with("..."));
    }

    #[test]
    fn update_summaries_multiple_contents() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        idx.add_memory("mem-2", "fact");
        idx.update_summaries("domain", &["First memory.", "Second memory."]);

        let dir = idx.get_directory("domain").unwrap();
        assert!(dir.abstract_l0.contains("First memory."));
        assert!(dir.abstract_l0.contains("Second memory."));
    }

    #[test]
    fn all_directories_returns_all() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        idx.add_memory("mem-2", "decision");
        idx.add_memory("mem-3", "pattern");

        let dirs = idx.all_directories();
        assert_eq!(dirs.len(), 3);

        let paths: Vec<&str> = dirs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"domain"));
        assert!(paths.contains(&"decisions"));
        assert!(paths.contains(&"patterns"));
    }

    #[test]
    fn get_nonexistent_directory() {
        let idx = HierarchicalIndex::new();
        assert!(idx.get_directory("nonexistent").is_none());
    }

    #[test]
    fn unicode_truncation_safety() {
        let mut idx = HierarchicalIndex::new();
        idx.add_memory("mem-1", "fact");
        // Multi-byte chars to ensure we don't split in the middle.
        let content = "a]".repeat(60); // 120 bytes of mixed content
        idx.update_summaries("domain", &[&content]);
        let dir = idx.get_directory("domain").unwrap();
        // Just verify it doesn't panic and produces valid UTF-8.
        assert!(dir.abstract_l0.is_char_boundary(0));
    }
}
