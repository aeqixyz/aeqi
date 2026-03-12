//! Emotional State Tracking — Tracks trust level, mood, and interaction
//! patterns for each agent to make personality feel alive.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Trust level thresholds.
const TRUST_PROFESSIONAL: u64 = 50;
const TRUST_TRUSTED: u64 = 200;
const TRUST_INTIMATE: u64 = 500;

/// Emotional state for an agent, persisted as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalState {
    /// Agent name.
    pub agent_name: String,
    /// Total interaction count.
    pub interaction_count: u64,
    /// Positive interaction count (successful quests, praised responses).
    pub positive_count: u64,
    /// Negative interaction count (failed quests, corrections).
    pub negative_count: u64,
    /// Current trust level name.
    pub trust_level: String,
    /// Current mood (contextual — set by recent interactions).
    pub mood: String,
    /// Last interaction timestamp.
    pub last_interaction: DateTime<Utc>,
}

impl EmotionalState {
    /// Create a new emotional state for an agent.
    pub fn new(agent_name: &str) -> Self {
        Self {
            agent_name: agent_name.to_string(),
            interaction_count: 0,
            positive_count: 0,
            negative_count: 0,
            trust_level: "stranger".to_string(),
            mood: "neutral".to_string(),
            last_interaction: Utc::now(),
        }
    }

    /// Load from file, or create default if not found.
    pub fn load(path: &Path, agent_name: &str) -> Self {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(state) = serde_json::from_str(&content)
        {
            return state;
        }
        Self::new(agent_name)
    }

    /// Save to file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        debug!(agent = %self.agent_name, trust = %self.trust_level, mood = %self.mood, "emotional state saved");
        Ok(())
    }

    /// Standard path for an agent's emotional state.
    pub fn path_for_agent(agent_dir: &Path) -> PathBuf {
        agent_dir.join(".sigil").join("emotional_state.json")
    }

    /// Record a positive interaction (successful task, praise).
    pub fn record_positive(&mut self) {
        self.interaction_count += 1;
        self.positive_count += 1;
        self.last_interaction = Utc::now();
        self.update_trust_level();
    }

    /// Record a negative interaction (failed task, correction).
    pub fn record_negative(&mut self) {
        self.interaction_count += 1;
        self.negative_count += 1;
        self.last_interaction = Utc::now();
        self.update_trust_level();
    }

    /// Record a neutral interaction.
    pub fn record_interaction(&mut self) {
        self.interaction_count += 1;
        self.last_interaction = Utc::now();
        self.update_trust_level();
    }

    /// Set the current mood.
    pub fn set_mood(&mut self, mood: &str) {
        self.mood = mood.to_string();
    }

    /// Positive ratio (0.0 to 1.0).
    pub fn positive_ratio(&self) -> f64 {
        let total = self.positive_count + self.negative_count;
        if total == 0 {
            0.5
        } else {
            self.positive_count as f64 / total as f64
        }
    }

    /// Update trust level based on interaction count thresholds.
    fn update_trust_level(&mut self) {
        self.trust_level = if self.interaction_count >= TRUST_INTIMATE {
            "intimate".to_string()
        } else if self.interaction_count >= TRUST_TRUSTED {
            "trusted".to_string()
        } else if self.interaction_count >= TRUST_PROFESSIONAL {
            "professional".to_string()
        } else {
            "stranger".to_string()
        };
    }

    /// Format as context for injection into system prompt.
    pub fn as_context(&self) -> String {
        format!(
            "## Emotional State\n\n\
             Trust level: {} ({} interactions, {:.0}% positive)\n\
             Current mood: {}\n\
             Last interaction: {}\n",
            self.trust_level,
            self.interaction_count,
            self.positive_ratio() * 100.0,
            self.mood,
            self.last_interaction.format("%Y-%m-%d %H:%M UTC"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_state() {
        let state = EmotionalState::new("alpha");
        assert_eq!(state.trust_level, "stranger");
        assert_eq!(state.interaction_count, 0);
        assert_eq!(state.positive_ratio(), 0.5);
    }

    #[test]
    fn test_trust_progression() {
        let mut state = EmotionalState::new("alpha");

        for _ in 0..50 {
            state.record_positive();
        }
        assert_eq!(state.trust_level, "professional");

        for _ in 0..150 {
            state.record_interaction();
        }
        assert_eq!(state.trust_level, "trusted");

        for _ in 0..300 {
            state.record_positive();
        }
        assert_eq!(state.trust_level, "intimate");
    }

    #[test]
    fn test_positive_ratio() {
        let mut state = EmotionalState::new("beta");
        state.record_positive();
        state.record_positive();
        state.record_negative();
        assert!((state.positive_ratio() - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("emotional_state.json");

        let mut state = EmotionalState::new("delta");
        state.record_positive();
        state.record_positive();
        state.set_mood("content");
        state.save(&path).unwrap();

        let loaded = EmotionalState::load(&path, "delta");
        assert_eq!(loaded.interaction_count, 2);
        assert_eq!(loaded.mood, "content");
        assert_eq!(loaded.trust_level, "stranger");
    }

    #[test]
    fn test_as_context() {
        let state = EmotionalState::new("gamma");
        let ctx = state.as_context();
        assert!(ctx.contains("stranger"));
        assert!(ctx.contains("neutral"));
        assert!(ctx.contains("Emotional State"));
    }

    #[test]
    fn test_path_for_agent() {
        let path = EmotionalState::path_for_agent(std::path::Path::new("/home/dev/agents/alpha"));
        assert_eq!(
            path,
            std::path::PathBuf::from("/home/dev/agents/alpha/.sigil/emotional_state.json")
        );
    }
}
