use anyhow::Result;
use chrono::Utc;
use realm_core::traits::{ChatRequest, Message, MessageContent, Provider, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Files observed for drift detection (input side).
const TRACKED_FILES: &[&str] = &[
    "SOUL.md",
    "IDENTITY.md",
    "AGENTS.md",
    "HEARTBEAT.md",
    "PREFERENCES.md",
];

/// Files the LLM is allowed to update (output side).
const UPDATABLE_FILES: &[&str] = &[
    "MEMORY.md",
    "HEARTBEAT.md",
    "IDENTITY.md",
    "PREFERENCES.md",
];

/// Max characters fed to the LLM across all identity files.
const MAX_INPUT_CHARS: usize = 6000;

/// Max tokens for the LLM response (budget cap).
const REFLECTION_MAX_TOKENS: u32 = 2000;

/// Persisted state for drift detection between reflection cycles.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ReflectionState {
    last_run_ts: Option<i64>,
    /// FNV-1a fingerprints of tracked files from the last run.
    file_fingerprints: HashMap<String, u64>,
}

impl ReflectionState {
    fn load(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(state) = serde_json::from_str(&content) {
                return state;
            }
        Self::default()
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Autonomous self-reflection system.
///
/// Runs on a configurable interval. Reads identity files from the rig directory,
/// detects drift via content fingerprints, and — when drift is found — issues a
/// budget-capped (2 k token) direct LLM call to synthesize updates.
///
/// Updates are written back to MEMORY.md, HEARTBEAT.md, IDENTITY.md, and/or
/// PREFERENCES.md inside the rig directory. No sub-agents are spawned; the call
/// is a single synchronous `provider.chat()` invocation.
pub struct Reflection {
    pub domain_name: String,
    pub interval_secs: u64,
    /// Rig directory where identity files and `.sigil/` data live.
    pub rig_dir: PathBuf,
    pub provider: Arc<dyn Provider>,
    pub model: String,
    last_run: Option<std::time::Instant>,
    state_path: PathBuf,
}

impl Reflection {
    pub fn new(
        domain_name: String,
        interval_secs: u64,
        rig_dir: PathBuf,
        provider: Arc<dyn Provider>,
        model: String,
    ) -> Self {
        let state_path = rig_dir.join(".sigil/reflection-state.json");
        Self {
            domain_name,
            interval_secs,
            rig_dir,
            provider,
            model,
            last_run: None,
            state_path,
        }
    }

    /// Returns true if the reflection cycle is due.
    pub fn is_due(&self) -> bool {
        match self.last_run {
            None => true,
            Some(last) => last.elapsed().as_secs() >= self.interval_secs,
        }
    }

    /// Run one reflection cycle. Returns a summary of what changed (or "no drift").
    pub async fn run(&mut self) -> Result<String> {
        debug!(domain = %self.domain_name, "starting reflection cycle");

        let mut state = ReflectionState::load(&self.state_path);

        // Read tracked files and compute fingerprints.
        let mut current_fingerprints: HashMap<String, u64> = HashMap::new();
        let mut file_contents: HashMap<String, String> = HashMap::new();

        for filename in TRACKED_FILES {
            let path = self.rig_dir.join(filename);
            if let Ok(content) = std::fs::read_to_string(&path)
                && !content.trim().is_empty() {
                    current_fingerprints.insert(filename.to_string(), fnv1a(&content));
                    file_contents.insert(filename.to_string(), content);
                }
        }

        // Also read MEMORY.md even though it's not tracked for drift.
        for filename in UPDATABLE_FILES {
            if !file_contents.contains_key(*filename) {
                let path = self.rig_dir.join(filename);
                if let Ok(content) = std::fs::read_to_string(&path)
                    && !content.trim().is_empty() {
                        file_contents.insert(filename.to_string(), content);
                    }
            }
        }

        // Detect drift: first run, file added, or fingerprint changed.
        let drift_detected = state.file_fingerprints.is_empty()
            || current_fingerprints
                .iter()
                .any(|(k, v)| state.file_fingerprints.get(k) != Some(v))
            || current_fingerprints.len() != state.file_fingerprints.len();

        if !drift_detected {
            debug!(domain = %self.domain_name, "no drift detected — skipping reflection");
            self.last_run = Some(std::time::Instant::now());
            return Ok("no drift detected".to_string());
        }

        info!(
            domain = %self.domain_name,
            files = file_contents.len(),
            "drift detected — running reflection LLM call"
        );

        let prompt = build_prompt(&self.domain_name, &file_contents);

        // Single direct LLM call — no agent loop, no sub-agents, budget-capped.
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text(&prompt),
            }],
            tools: vec![],
            max_tokens: REFLECTION_MAX_TOKENS,
            temperature: 0.2,
        };

        let response = self.provider.chat(&request).await?;

        let Some(text) = response.content else {
            warn!(domain = %self.domain_name, "reflection LLM returned empty response");
            self.last_run = Some(std::time::Instant::now());
            return Ok("empty LLM response".to_string());
        };

        if text.trim() == "NO_CHANGES" {
            info!(domain = %self.domain_name, "reflection: no changes needed");
            state.file_fingerprints = current_fingerprints;
            state.last_run_ts = Some(Utc::now().timestamp());
            if let Err(e) = state.save(&self.state_path) {
                warn!(domain = %self.domain_name, error = %e, "failed to save reflection state");
            }
            self.last_run = Some(std::time::Instant::now());
            return Ok("no changes needed".to_string());
        }

        // Parse and apply file updates.
        let updates = parse_updates(&text);
        let mut updated_files = Vec::new();

        for (filename, new_content) in &updates {
            if UPDATABLE_FILES.contains(&filename.as_str()) {
                let path = self.rig_dir.join(filename);
                if let Err(e) = std::fs::write(&path, new_content) {
                    warn!(
                        domain = %self.domain_name,
                        file = %filename,
                        error = %e,
                        "failed to write reflection update"
                    );
                } else {
                    info!(
                        domain = %self.domain_name,
                        file = %filename,
                        bytes = new_content.len(),
                        "reflection updated file"
                    );
                    updated_files.push(filename.clone());
                }
            } else {
                warn!(
                    domain = %self.domain_name,
                    file = %filename,
                    "reflection attempted to update non-updatable file — ignored"
                );
            }
        }

        // Persist updated state.
        state.file_fingerprints = current_fingerprints;
        state.last_run_ts = Some(Utc::now().timestamp());
        if let Err(e) = state.save(&self.state_path) {
            warn!(domain = %self.domain_name, error = %e, "failed to save reflection state");
        }

        self.last_run = Some(std::time::Instant::now());

        let summary = if updated_files.is_empty() {
            "drift detected but no valid updates generated".to_string()
        } else {
            format!("updated: {}", updated_files.join(", "))
        };

        info!(domain = %self.domain_name, summary = %summary, "reflection cycle complete");
        Ok(summary)
    }
}

/// Build the reflection prompt from the current identity files.
fn build_prompt(domain_name: &str, files: &HashMap<String, String>) -> String {
    let mut prompt = format!(
        "You are the self-reflection system for the '{domain_name}' familiar. \
         Your task: review the current identity files for drift, staleness, or missing \
         information, then produce concise targeted updates.\n\n\
         ## Current Identity Files\n\n"
    );

    let mut total_chars = 0usize;

    let mut seen = std::collections::HashSet::new();

    for &filename in TRACKED_FILES.iter().chain(UPDATABLE_FILES.iter()) {
        if !seen.insert(filename) {
            continue;
        }
        if total_chars >= MAX_INPUT_CHARS {
            break;
        }
        if let Some(content) = files.get(filename) {
            let available = MAX_INPUT_CHARS.saturating_sub(total_chars);
            let chunk = if content.len() > available {
                &content[..available]
            } else {
                content.as_str()
            };
            prompt.push_str(&format!("### {filename}\n\n{chunk}\n\n"));
            total_chars += chunk.len();
        }
    }

    prompt.push_str(
        "## Task\n\n\
         Review the files above. Produce updates ONLY for files that genuinely need \
         them. Do not rewrite files just to reformat.\n\n\
         Rules:\n\
         - MEMORY.md: add new observations, correct stale facts, or create it if missing\n\
         - HEARTBEAT.md: revise operational checks if they are stale or wrong\n\
         - IDENTITY.md: update only if name/role/style has changed\n\
         - PREFERENCES.md: add new confirmed preferences or correct wrong ones\n\n\
         Output format (for each file that needs updating):\n\
         UPDATE <FILENAME>:\n\
         <complete new file content>\n\
         END <FILENAME>\n\n\
         If no files need updating, output exactly: NO_CHANGES\n\n\
         Be concise — you have a 2000-token budget for the entire response.",
    );

    prompt
}

/// Parse `UPDATE <FILE>:\n<content>\nEND <FILE>` blocks from LLM output.
/// Returns a map of filename -> new content.
fn parse_updates(text: &str) -> HashMap<String, String> {
    let mut updates = HashMap::new();
    let mut pos = 0;

    while pos < text.len() {
        let remaining = &text[pos..];
        let Some(update_start) = remaining.find("UPDATE ") else {
            break;
        };

        let after_update = &remaining[update_start + 7..];
        let Some(colon_newline) = after_update.find(":\n") else {
            break;
        };

        let filename = after_update[..colon_newline].trim().to_string();
        let after_colon = &after_update[colon_newline + 2..];
        let end_marker = format!("END {filename}");

        if let Some(end_pos) = after_colon.find(&end_marker) {
            let content = after_colon[..end_pos].trim().to_string();
            if !filename.is_empty() && !content.is_empty() {
                updates.insert(filename, content);
            }
            pos += update_start + 7 + colon_newline + 2 + end_pos + end_marker.len();
        } else {
            // No END marker found — stop parsing.
            break;
        }
    }

    updates
}

/// FNV-1a 64-bit hash for stable content fingerprinting (no external deps).
fn fnv1a(content: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in content.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_updates_single() {
        let text = "UPDATE MEMORY.md:\nLine 1\nLine 2\nEND MEMORY.md";
        let updates = parse_updates(text);
        assert_eq!(updates.get("MEMORY.md").map(|s| s.as_str()), Some("Line 1\nLine 2"));
    }

    #[test]
    fn test_parse_updates_multiple() {
        let text =
            "UPDATE MEMORY.md:\nfoo\nEND MEMORY.md\nUPDATE HEARTBEAT.md:\nbar\nEND HEARTBEAT.md";
        let updates = parse_updates(text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates.get("MEMORY.md").map(|s| s.as_str()), Some("foo"));
        assert_eq!(updates.get("HEARTBEAT.md").map(|s| s.as_str()), Some("bar"));
    }

    #[test]
    fn test_parse_updates_no_changes() {
        let text = "NO_CHANGES";
        let updates = parse_updates(text);
        assert!(updates.is_empty());
    }

    #[test]
    fn test_fnv1a_stable() {
        let h1 = fnv1a("hello world");
        let h2 = fnv1a("hello world");
        assert_eq!(h1, h2);
        let h3 = fnv1a("hello world!");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_drift_detection_logic() {
        let mut fp1 = HashMap::new();
        fp1.insert("SOUL.md".to_string(), fnv1a("original content"));

        let mut fp2 = HashMap::new();
        fp2.insert("SOUL.md".to_string(), fnv1a("modified content"));

        let drift = fp1
            .iter()
            .any(|(k, v)| fp2.get(k) != Some(v));
        assert!(drift);

        let no_drift = fp1
            .iter()
            .all(|(k, v)| fp2.get(k) == Some(v));
        assert!(!no_drift);
    }
}
