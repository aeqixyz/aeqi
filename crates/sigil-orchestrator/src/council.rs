//! Council Mode — Visible council debate orchestration.
//!
//! When triggered (via `/council` command or leader agent's judgment), the council
//! spawns a thread where each agent debates visibly before the leader agent
//! synthesizes the final recommendation.

use anyhow::Result;
use sigil_core::config::PeerAgentConfig;
use sigil_core::identity::Identity;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::executor::ClaudeCodeExecutor;

/// A council debate session.
#[derive(Debug, Clone)]
pub struct CouncilTopic {
    pub id: String,
    pub message: String,
    pub agents: Vec<String>,
    /// Responses indexed by agent name (latest round only, for backward compat).
    pub responses: HashMap<String, String>,
    /// All debate rounds — each round is a map of agent → response.
    pub rounds: Vec<HashMap<String, String>>,
    pub synthesis: Option<String>,
}

/// Manages council debate sessions.
pub struct Council {
    topics: Mutex<HashMap<String, CouncilTopic>>,
    next_id: Mutex<u32>,
}

impl Council {
    pub fn new() -> Self {
        Self {
            topics: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    pub async fn open_topic(&self, message: &str, agents: Vec<String>) -> String {
        let mut id_counter = self.next_id.lock().await;
        let id = format!("council-{:03}", *id_counter);
        *id_counter += 1;

        let topic = CouncilTopic {
            id: id.clone(),
            message: message.to_string(),
            agents,
            responses: HashMap::new(),
            rounds: Vec::new(),
            synthesis: None,
        };

        self.topics.lock().await.insert(id.clone(), topic);
        id
    }

    pub async fn record_response(&self, topic_id: &str, agent: &str, response: &str) -> bool {
        let mut topics = self.topics.lock().await;
        if let Some(topic) = topics.get_mut(topic_id) {
            topic
                .responses
                .insert(agent.to_string(), response.to_string());
            topic.responses.len() == topic.agents.len()
        } else {
            false
        }
    }

    pub async fn record_synthesis(&self, topic_id: &str, synthesis: &str) {
        let mut topics = self.topics.lock().await;
        if let Some(topic) = topics.get_mut(topic_id) {
            topic.synthesis = Some(synthesis.to_string());
        }
    }

    pub async fn get_topic(&self, topic_id: &str) -> Option<CouncilTopic> {
        self.topics.lock().await.get(topic_id).cloned()
    }

    /// Run a multi-round council debate: spawn advisor tasks for each round,
    /// collect responses, check for convergence, then synthesize.
    pub async fn run_debate(
        &self,
        message: &str,
        advisor_configs: &[(&PeerAgentConfig, PathBuf)],
        lead_identity: &Identity,
        lead_repo: &Path,
        lead_model: &str,
        max_rounds: u32,
    ) -> Result<(Vec<Vec<(String, String)>>, String)> {
        let agent_names: Vec<String> = advisor_configs
            .iter()
            .map(|(c, _)| c.name.clone())
            .collect();
        let topic_id = self.open_topic(message, agent_names).await;

        info!(topic = %topic_id, max_rounds, "council debate opened");

        let max_rounds = max_rounds.max(1);
        let mut all_rounds: Vec<Vec<(String, String)>> = Vec::new();

        for round in 1..=max_rounds {
            // Build the prompt for this round.
            let round_prompt = if round == 1 {
                // First round: original debate topic.
                format!(
                    "## Council Debate\n\nThe council has been summoned to debate:\n\n{}\n\n\
                     Respond in character with your specialist perspective. Be concise (2-5 sentences).",
                    message
                )
            } else {
                // Subsequent rounds: include all previous responses for refinement.
                let mut prev_context = String::new();
                for (r_idx, r_responses) in all_rounds.iter().enumerate() {
                    prev_context.push_str(&format!("### Round {} Responses\n\n", r_idx + 1));
                    for (name, text) in r_responses {
                        prev_context.push_str(&format!("**{}**: {}\n\n", name, text));
                    }
                }
                format!(
                    "## Council Debate — Round {round}\n\nTopic: {message}\n\n\
                     ## Previous Round Responses\n{prev_context}\n\
                     Review the other advisors' perspectives and refine your position. \
                     If you agree, say so briefly. If you disagree, explain why."
                )
            };

            // Spawn all advisors in parallel.
            let mut handles = Vec::new();
            for (config, agent_dir) in advisor_configs {
                let advisor_name = config.name.clone();
                let advisor_model = config
                    .model
                    .clone()
                    .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
                let advisor_identity = Identity::load(agent_dir, None).unwrap_or_default();
                let prompt = round_prompt.clone();
                let repo = agent_dir.clone();
                let budget = config.max_budget_usd;
                let tid = topic_id.clone();

                let handle = tokio::spawn(async move {
                    let executor = ClaudeCodeExecutor::new(repo, advisor_model, 15, budget);
                    match executor.execute(&advisor_identity, &prompt).await {
                        Ok(result) => {
                            info!(agent = %advisor_name, topic = %tid, round, cost = result.total_cost_usd, "council response");
                            Some((advisor_name, result.result_text))
                        }
                        Err(e) => {
                            warn!(agent = %advisor_name, topic = %tid, round, error = %e, "council advisor failed");
                            None
                        }
                    }
                });
                handles.push(handle);
            }

            let mut round_responses: Vec<(String, String)> = Vec::new();
            for handle in handles {
                match tokio::time::timeout(std::time::Duration::from_secs(120), handle).await {
                    Ok(Ok(Some((name, text)))) => {
                        self.record_response(&topic_id, &name, &text).await;
                        round_responses.push((name, text));
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => warn!(error = %e, "council task panicked"),
                    Err(_) => warn!("council advisor timed out"),
                }
            }

            // Store this round.
            {
                let round_map: HashMap<String, String> = round_responses
                    .iter()
                    .map(|(n, t)| (n.clone(), t.clone()))
                    .collect();
                let mut topics = self.topics.lock().await;
                if let Some(topic) = topics.get_mut(&topic_id) {
                    topic.rounds.push(round_map);
                }
            }
            all_rounds.push(round_responses.clone());

            // Convergence detection: if all responses are short and contain "agree",
            // stop early.
            if round > 1 {
                let converged = round_responses.iter().all(|(_, text)| {
                    text.len() < 200
                        && (text.to_lowercase().contains("agree")
                            || text.to_lowercase().contains("concur"))
                });
                if converged {
                    info!(topic = %topic_id, round, "council converged early");
                    break;
                }
            }
        }

        // Build synthesis context from all rounds.
        let mut council_input = String::new();
        for (r_idx, round_responses) in all_rounds.iter().enumerate() {
            council_input.push_str(&format!("## Round {} Responses\n\n", r_idx + 1));
            for (name, text) in round_responses {
                council_input.push_str(&format!("### {} says:\n{}\n\n", name, text));
            }
        }

        let synthesis_context = format!(
            "## Council Synthesis\n\nThe council debated this topic:\n\n{}\n\n{}\n\n\
             Synthesize the council's input into a unified recommendation. \
             Attribute key insights to the relevant advisor. \
             Present one clear recommendation.",
            message, council_input
        );

        let lead_executor =
            ClaudeCodeExecutor::new(lead_repo.to_path_buf(), lead_model.to_string(), 15, None);

        let synthesis = match lead_executor
            .execute(lead_identity, &synthesis_context)
            .await
        {
            Ok(result) => {
                info!(topic = %topic_id, cost = result.total_cost_usd, "council synthesis complete");
                result.result_text
            }
            Err(e) => {
                warn!(topic = %topic_id, error = %e, "council synthesis failed");
                format!(
                    "Council debate collected {} round(s) but synthesis failed: {}",
                    all_rounds.len(),
                    e
                )
            }
        };

        self.record_synthesis(&topic_id, &synthesis).await;

        Ok((all_rounds, synthesis))
    }
}

impl Default for Council {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_council_topic_lifecycle() {
        let council = Council::new();

        let topic_id = council
            .open_topic("test question", vec!["beta".into(), "delta".into()])
            .await;
        assert!(topic_id.starts_with("council-"));

        let all_done = council
            .record_response(&topic_id, "beta", "risk analysis here")
            .await;
        assert!(!all_done);

        let all_done = council
            .record_response(&topic_id, "delta", "architecture note")
            .await;
        assert!(all_done);

        let topic = council.get_topic(&topic_id).await.unwrap();
        assert_eq!(topic.responses.len(), 2);
    }
}
