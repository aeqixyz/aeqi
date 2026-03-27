//! Council Mode — Visible council debate orchestration.
//!
//! When triggered (via `/council` command or leader agent's judgment), the council
//! spawns a thread where each agent debates visibly before the leader agent
//! synthesizes the final recommendation.

use anyhow::{Result, anyhow};
use sigil_core::config::PeerAgentConfig;
use sigil_core::identity::Identity;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

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
        let _ = (
            message,
            advisor_configs,
            lead_identity,
            lead_repo,
            lead_model,
            max_rounds,
        );
        Err(anyhow!(
            "council debate execution is temporarily disabled while it is rewired to the native Sigil runtime"
        ))
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
