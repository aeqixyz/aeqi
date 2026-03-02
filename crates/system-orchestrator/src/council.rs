//! Council Mode — Visible council debate orchestration.
//!
//! When triggered (via `/council` command or focal agent's judgment), the chamber
//! spawns a thread where each agent debates visibly before the focal agent
//! synthesizes the final recommendation.

use anyhow::Result;
use system_core::config::PeerAgentConfig;
use system_core::identity::Identity;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::executor::ClaudeCodeExecutor;

/// A chamber debate session.
#[derive(Debug, Clone)]
pub struct CouncilTopic {
    pub id: String,
    pub message: String,
    pub agents: Vec<String>,
    pub responses: HashMap<String, String>,
    pub synthesis: Option<String>,
}

/// Manages chamber debate sessions.
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
        let id = format!("chamber-{:03}", *id_counter);
        *id_counter += 1;

        let topic = CouncilTopic {
            id: id.clone(),
            message: message.to_string(),
            agents,
            responses: HashMap::new(),
            synthesis: None,
        };

        self.topics.lock().await.insert(id.clone(), topic);
        id
    }

    pub async fn record_response(&self, topic_id: &str, agent: &str, response: &str) -> bool {
        let mut topics = self.topics.lock().await;
        if let Some(topic) = topics.get_mut(topic_id) {
            topic.responses.insert(agent.to_string(), response.to_string());
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

    /// Run a chamber debate: spawn all advisor quests, collect responses,
    /// then have the focal agent synthesize.
    pub async fn run_debate(
        &self,
        message: &str,
        advisor_configs: &[(&PeerAgentConfig, PathBuf)],
        lead_identity: &Identity,
        lead_repo: &Path,
        lead_model: &str,
    ) -> Result<(Vec<(String, String)>, String)> {
        let agent_names: Vec<String> = advisor_configs
            .iter()
            .map(|(c, _)| c.name.clone())
            .collect();
        let topic_id = self.open_topic(message, agent_names).await;

        info!(topic = %topic_id, "chamber debate opened");

        let mut handles = Vec::new();
        for (config, agent_dir) in advisor_configs {
            let advisor_name = config.name.clone();
            let advisor_model = config.model.clone().unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            let advisor_identity = Identity::load(agent_dir, None).unwrap_or_default();
            let msg = message.to_string();
            let repo = agent_dir.clone();
            let budget = config.max_budget_usd;
            let tid = topic_id.clone();

            let handle = tokio::spawn(async move {
                let quest_context = format!(
                    "## Council Debate\n\nThe council has been summoned to debate:\n\n{}\n\n\
                     Respond in character with your specialist perspective. Be concise (2-5 sentences).",
                    msg
                );

                let executor = ClaudeCodeExecutor::new(
                    repo,
                    advisor_model,
                    15,
                    budget,
                );

                match executor.execute(&advisor_identity, &quest_context).await {
                    Ok(result) => {
                        info!(
                            agent = %advisor_name,
                            topic = %tid,
                            cost = result.total_cost_usd,
                            "chamber response received"
                        );
                        Some((advisor_name, result.result_text))
                    }
                    Err(e) => {
                        warn!(
                            agent = %advisor_name,
                            topic = %tid,
                            error = %e,
                            "chamber advisor failed"
                        );
                        None
                    }
                }
            });

            handles.push(handle);
        }

        let mut responses: Vec<(String, String)> = Vec::new();
        for handle in handles {
            match tokio::time::timeout(std::time::Duration::from_secs(120), handle).await {
                Ok(Ok(Some((name, text)))) => {
                    self.record_response(&topic_id, &name, &text).await;
                    responses.push((name, text));
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => warn!(error = %e, "chamber task panicked"),
                Err(_) => warn!("chamber advisor timed out"),
            }
        }

        let mut council_input = String::from("## Council Debate Responses\n\n");
        for (name, text) in &responses {
            council_input.push_str(&format!("### {} says:\n{}\n\n", name, text));
        }

        let synthesis_context = format!(
            "## Council Synthesis\n\nThe council debated this topic:\n\n{}\n\n{}\n\n\
             Synthesize the council's input into a unified recommendation. \
             Attribute key insights to the relevant advisor. \
             Present one clear recommendation.",
            message, council_input
        );

        let lead_executor = ClaudeCodeExecutor::new(
            lead_repo.to_path_buf(),
            lead_model.to_string(),
            15,
            None,
        );

        let synthesis = match lead_executor.execute(lead_identity, &synthesis_context).await {
            Ok(result) => {
                info!(topic = %topic_id, cost = result.total_cost_usd, "chamber synthesis complete");
                result.result_text
            }
            Err(e) => {
                warn!(topic = %topic_id, error = %e, "chamber synthesis failed");
                format!("Council debate collected {} responses but synthesis failed: {}", responses.len(), e)
            }
        };

        self.record_synthesis(&topic_id, &synthesis).await;

        Ok((responses, synthesis))
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
    async fn test_chamber_topic_lifecycle() {
        let chamber = Council::new();

        let topic_id = chamber.open_topic("test question", vec!["kael".into(), "void".into()]).await;
        assert!(topic_id.starts_with("chamber-"));

        let all_done = chamber.record_response(&topic_id, "kael", "risk analysis here").await;
        assert!(!all_done);

        let all_done = chamber.record_response(&topic_id, "void", "architecture note").await;
        assert!(all_done);

        let topic = chamber.get_topic(&topic_id).await.unwrap();
        assert_eq!(topic.responses.len(), 2);
    }
}
