//! Agent Router — classifies incoming messages to determine which peer agents
//! should be consulted alongside the leader agent.
//!
//! Uses a cheap Gemini Flash call (~$0.001, ~100ms) to classify message intent,
//! then maps to relevant advisor agents.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sigil_core::config::{AgentRole, PeerAgentConfig};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{info, warn};

/// Classification result from the router.
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// Names of advisor agents to invoke (empty = leader-only).
    pub advisors: Vec<String>,
    /// Classification category for logging.
    pub category: String,
    /// Time taken for classification.
    pub classify_ms: u64,
}

/// Tracks cooldowns to prevent re-invoking the same advisor too quickly.
pub struct AgentRouter {
    /// OpenRouter API key for cheap classifier calls.
    api_key: String,
    /// Shared HTTP client (reuses connection pool across calls).
    client: reqwest::Client,
    /// Map of (chat_id, agent_name) -> last invocation time for per-conversation cooldowns.
    last_invoked: HashMap<(i64, String), Instant>,
    /// Cooldown in seconds before same advisor can be re-invoked for the same chat.
    cooldown_secs: u64,
    /// Router model (e.g., "google/gemini-2.0-flash-001").
    model: String,
}

/// The classifier's JSON output.
#[derive(Debug, Deserialize, Serialize)]
struct ClassifierOutput {
    category: String,
    advisors: Vec<String>,
}

impl AgentRouter {
    pub fn new(api_key: String, cooldown_secs: u64) -> Self {
        Self {
            api_key,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build reqwest client"),
            last_invoked: HashMap::new(),
            cooldown_secs,
            model: "google/gemini-2.0-flash-001".to_string(),
        }
    }

    /// Set the classifier model.
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Classify a message and determine which advisors to invoke.
    /// `chat_id` is used for per-conversation cooldown tracking.
    pub async fn classify(
        &mut self,
        message: &str,
        available_agents: &[&PeerAgentConfig],
        chat_id: i64,
    ) -> Result<RouteDecision> {
        let start = Instant::now();

        // Build agent descriptions dynamically from config expertise fields.
        let agent_descriptions: Vec<String> = available_agents
            .iter()
            .map(|a| {
                let expertise = if a.expertise.is_empty() {
                    "general".to_string()
                } else {
                    a.expertise.join(", ")
                };
                format!("- {}: expertise=[{}]", a.name, expertise)
            })
            .collect();

        // Build dynamic classification rules from agent expertise.
        let mut rules = String::from(
            "Classification rules:\n- \"casual\": Simple chat, greetings, personal talk -> no advisors needed\n",
        );
        for agent in available_agents {
            if agent.role != AgentRole::Advisor || agent.expertise.is_empty() {
                continue;
            }
            rules.push_str(&format!(
                "- If the message relates to {} -> include \"{}\"\n",
                agent.expertise.join(" or "),
                agent.name,
            ));
        }
        rules.push_str("- \"strategic\": Major decisions spanning multiple concerns -> include all relevant advisors\n");

        let system_prompt = format!(
            r#"You are a message classifier for an AI council system. Given a user message, determine which specialist advisors should be consulted.

Available advisors:
{}

{rules}
Respond with ONLY a JSON object (no markdown, no code fences):
{{"category": "<category>", "advisors": ["<name1>", "<name2>"]}}

Use empty array for "casual" messages. Only include advisors whose expertise is relevant."#,
            agent_descriptions.join("\n")
        );

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": message}
            ],
            "max_tokens": 80,
            "temperature": 0.0
        });

        let response = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        let classify_ms = start.elapsed().as_millis() as u64;

        let decision = match response {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(v) => {
                    let text = v
                        .pointer("/choices/0/message/content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .trim();

                    let clean = text
                        .strip_prefix("```json")
                        .or_else(|| text.strip_prefix("```"))
                        .unwrap_or(text)
                        .strip_suffix("```")
                        .unwrap_or(text)
                        .trim();

                    match serde_json::from_str::<ClassifierOutput>(clean) {
                        Ok(parsed) => {
                            info!(
                                category = %parsed.category,
                                advisors = ?parsed.advisors,
                                ms = classify_ms,
                                "message classified"
                            );
                            parsed
                        }
                        Err(e) => {
                            warn!(error = %e, raw = %text, "classifier parse failed, defaulting to casual");
                            ClassifierOutput {
                                category: "casual".to_string(),
                                advisors: Vec::new(),
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "classifier response parse failed");
                    ClassifierOutput {
                        category: "casual".to_string(),
                        advisors: Vec::new(),
                    }
                }
            },
            Err(e) => {
                warn!(error = %e, "classifier request failed, defaulting to leader-only");
                ClassifierOutput {
                    category: "casual".to_string(),
                    advisors: Vec::new(),
                }
            }
        };

        // Filter by cooldown (per-conversation).
        let now = Instant::now();
        let valid_agent_names: Vec<String> = available_agents
            .iter()
            .filter(|a| a.role == AgentRole::Advisor)
            .map(|a| a.name.clone())
            .collect();

        let advisors: Vec<String> = decision
            .advisors
            .into_iter()
            .filter(|name| {
                if !valid_agent_names.contains(name) {
                    return false;
                }
                let key = (chat_id, name.clone());
                if let Some(last) = self.last_invoked.get(&key)
                    && now.duration_since(*last).as_secs() < self.cooldown_secs
                {
                    info!(agent = %name, chat_id, "advisor on cooldown for this chat, skipping");
                    return false;
                }
                true
            })
            .collect();

        // Update last-invoked timestamps (per-conversation).
        for name in &advisors {
            self.last_invoked.insert((chat_id, name.clone()), now);
        }

        Ok(RouteDecision {
            advisors,
            category: decision.category,
            classify_ms,
        })
    }

    /// Classify a message within a project team's scope.
    /// Only considers agents from the project's team, not all system agents.
    pub async fn classify_for_project(
        &mut self,
        message: &str,
        project_team_agents: &[&PeerAgentConfig],
        chat_id: i64,
    ) -> Result<RouteDecision> {
        self.classify(message, project_team_agents, chat_id).await
    }

    /// Classify at system level — determines which project should handle a message.
    /// Uses all available agents for global routing.
    pub async fn classify_system(
        &mut self,
        message: &str,
        all_agents: &[&PeerAgentConfig],
        chat_id: i64,
    ) -> Result<RouteDecision> {
        self.classify(message, all_agents, chat_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classifier_output_parse() {
        let json = r#"{"category": "financial", "advisors": ["beta"]}"#;
        let parsed: ClassifierOutput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.category, "financial");
        assert_eq!(parsed.advisors, vec!["beta"]);
    }

    #[test]
    fn test_empty_advisors() {
        let json = r#"{"category": "casual", "advisors": []}"#;
        let parsed: ClassifierOutput = serde_json::from_str(json).unwrap();
        assert!(parsed.advisors.is_empty());
    }
}
