//! Context Budget — Controls token usage per spirit by truncating and
//! summarizing context layers that exceed configurable limits.

use tracing::debug;

/// Budget limits for each context layer (char-based, ~4 chars/token).
pub struct ContextBudget {
    pub max_shared_workflow: usize,
    pub max_persona: usize,
    pub max_agents: usize,
    pub max_knowledge: usize,
    pub max_preferences: usize,
    pub max_memory: usize,
    pub max_checkpoints: usize,
    pub max_checkpoint_count: usize,
    pub max_total: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_shared_workflow: 2000,
            max_persona: 4000,
            max_agents: 8000,
            max_knowledge: 12000,
            max_preferences: 4000,
            max_memory: 8000,
            max_checkpoints: 8000,
            max_checkpoint_count: 5,
            max_total: 120000,
        }
    }
}

impl ContextBudget {
    /// Create from realm.toml config.
    pub fn from_config(cfg: &system_core::ContextBudgetConfig) -> Self {
        Self {
            max_shared_workflow: cfg.max_shared_workflow,
            max_persona: cfg.max_persona,
            max_agents: cfg.max_agents,
            max_knowledge: cfg.max_knowledge,
            max_preferences: cfg.max_preferences,
            max_memory: cfg.max_memory,
            max_checkpoints: cfg.max_checkpoints,
            max_checkpoint_count: cfg.max_checkpoint_count,
            max_total: cfg.max_total,
        }
    }

    /// Truncate text to fit within char budget.
    pub fn truncate(text: &str, max_chars: usize) -> String {
        if text.len() <= max_chars {
            return text.to_string();
        }
        let safe_end = max_chars.saturating_sub(40);
        let cut = text[..safe_end].rfind('\n').unwrap_or(safe_end);
        format!(
            "{}\n\n[... truncated, {} chars omitted]",
            &text[..cut],
            text.len() - cut
        )
    }

    /// Summarize old checkpoints, keep recent ones verbatim.
    pub fn budget_checkpoints(&self, checkpoints: &[system_tasks::Checkpoint]) -> String {
        if checkpoints.is_empty() {
            return String::new();
        }

        let mut out = String::from("## Previous Attempts\n\n");

        if checkpoints.len() <= self.max_checkpoint_count {
            for (i, cp) in checkpoints.iter().enumerate() {
                out.push_str(&format!(
                    "### Attempt {} (by {}, {} turns, ${:.4})\n{}\n\n",
                    i + 1,
                    cp.worker,
                    cp.turns_used,
                    cp.cost_usd,
                    cp.progress
                ));
            }
        } else {
            let split = checkpoints.len() - self.max_checkpoint_count;
            out.push_str(&format!("*{split} earlier attempts summarized:*\n"));
            for cp in &checkpoints[..split] {
                let first_line = cp.progress.lines().next().unwrap_or("(no summary)");
                let line = if first_line.len() > 120 {
                    &first_line[..120]
                } else {
                    first_line
                };
                out.push_str(&format!(
                    "- {} ({} turns, ${:.4}): {}\n",
                    cp.worker, cp.turns_used, cp.cost_usd, line
                ));
            }
            out.push('\n');

            for (i, cp) in checkpoints[split..].iter().enumerate() {
                out.push_str(&format!(
                    "### Attempt {} (by {}, {} turns, ${:.4})\n{}\n\n",
                    split + i + 1,
                    cp.worker,
                    cp.turns_used,
                    cp.cost_usd,
                    cp.progress
                ));
            }
        }

        Self::truncate(&out, self.max_checkpoints)
    }

    /// Apply budget to a full identity system prompt.
    pub fn apply_to_identity(&self, identity: &system_core::Identity) -> String {
        let mut parts = Vec::new();

        if let Some(ref shared) = identity.shared_workflow {
            parts.push(format!(
                "# Shared Workflow\n\n{}",
                Self::truncate(shared, self.max_shared_workflow)
            ));
        }
        if let Some(ref soul) = identity.persona {
            parts.push(format!(
                "# Soul\n\n{}",
                Self::truncate(soul, self.max_persona)
            ));
        }
        if let Some(ref ident) = identity.identity {
            // Identity is kept small by convention; no separate budget.
            parts.push(format!("# Identity\n\n{ident}"));
        }
        if let Some(ref operational) = identity.operational {
            parts.push(format!(
                "# Operational Instructions\n\n{}",
                Self::truncate(operational, self.max_agents)
            ));
        }
        if let Some(ref agents) = identity.agents {
            parts.push(format!(
                "# Operating Instructions\n\n{}",
                Self::truncate(agents, self.max_agents)
            ));
        }
        if let Some(ref knowledge) = identity.knowledge {
            parts.push(format!(
                "# Domain Knowledge\n\n{}",
                Self::truncate(knowledge, self.max_knowledge)
            ));
        }
        if let Some(ref preferences) = identity.preferences {
            parts.push(format!(
                "# Architect Preferences\n\n{}",
                Self::truncate(preferences, self.max_preferences)
            ));
        }
        if let Some(ref memory) = identity.memory {
            parts.push(format!(
                "# Persistent Memory\n\n{}",
                Self::truncate(memory, self.max_memory)
            ));
        }

        let combined = if parts.is_empty() {
            "You are a helpful AI agent.".to_string()
        } else {
            parts.join("\n\n---\n\n")
        };

        if combined.len() > self.max_total {
            debug!(
                total = combined.len(),
                budget = self.max_total,
                "context exceeds budget, truncating"
            );
            Self::truncate(&combined, self.max_total)
        } else {
            combined
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(ContextBudget::truncate("hello", 100), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let text = "line one\nline two\nline three\nline four\nline five";
        let result = ContextBudget::truncate(text, 30);
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_budget_checkpoints_few() {
        let budget = ContextBudget::default();
        let cps = vec![system_tasks::Checkpoint {
            timestamp: chrono::Utc::now(),
            worker: "s1".into(),
            progress: "did thing 1".into(),
            cost_usd: 0.05,
            turns_used: 3,
        }];
        let result = budget.budget_checkpoints(&cps);
        assert!(result.contains("did thing 1"));
    }

    #[test]
    fn test_budget_checkpoints_many() {
        let budget = ContextBudget {
            max_checkpoint_count: 2,
            ..Default::default()
        };
        let cps: Vec<_> = (0..10)
            .map(|i| system_tasks::Checkpoint {
                timestamp: chrono::Utc::now(),
                worker: format!("s{i}"),
                progress: format!("progress for attempt {i}"),
                cost_usd: 0.01 * i as f64,
                turns_used: i as u32,
            })
            .collect();
        let result = budget.budget_checkpoints(&cps);
        assert!(result.contains("8 earlier attempts summarized"));
    }
}
