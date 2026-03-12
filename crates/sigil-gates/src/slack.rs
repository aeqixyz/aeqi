use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use sigil_core::traits::{Channel, IncomingMessage, OutgoingMessage};
use tokio::sync::mpsc;
use tracing::{error, info};

const SLACK_API: &str = "https://slack.com/api";

/// Slack Bot channel using Web API.
/// Uses conversations.history polling (Socket Mode requires websockets).
pub struct SlackChannel {
    client: Client,
    token: String,
    channel_ids: Vec<String>,
    shutdown: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl SlackChannel {
    pub fn new(token: String, channel_ids: Vec<String>) -> Self {
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        Self {
            client: Client::new(),
            token,
            channel_ids,
            shutdown,
            shutdown_rx,
        }
    }
}

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    messages: Option<Vec<SlackMessage>>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct SlackMessage {
    ts: String,
    user: Option<String>,
    text: Option<String>,
    #[serde(default)]
    bot_id: Option<String>,
    _channel: Option<String>,
}

#[async_trait]
impl Channel for SlackChannel {
    async fn start(&self) -> Result<mpsc::Receiver<IncomingMessage>> {
        let (tx, rx) = mpsc::channel(100);
        let client = self.client.clone();
        let token = self.token.clone();
        let channel_ids = self.channel_ids.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut last_ts: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut backoff_secs: u64 = 5;
            const MAX_BACKOFF_SECS: u64 = 60;
            info!("Slack polling started");

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let mut had_error = false;

                for channel_id in &channel_ids {
                    let mut params = vec![("channel", channel_id.as_str()), ("limit", "10")];

                    let oldest_binding;
                    if let Some(ts) = last_ts.get(channel_id) {
                        oldest_binding = ts.clone();
                        params.push(("oldest", &oldest_binding));
                    }

                    let url = format!("{}/conversations.history", SLACK_API);
                    match client
                        .get(&url)
                        .header("Authorization", format!("Bearer {}", token))
                        .query(&params)
                        .send()
                        .await
                    {
                        Ok(response) => {
                            if let Ok(slack_resp) = response.json::<SlackResponse>().await
                                && slack_resp.ok
                            {
                                for msg in slack_resp.messages.unwrap_or_default().iter().rev() {
                                    if msg.bot_id.is_some() {
                                        continue;
                                    }

                                    last_ts.insert(channel_id.clone(), msg.ts.clone());

                                    if let Some(ref text) = msg.text {
                                        let incoming = IncomingMessage {
                                            channel: "slack".to_string(),
                                            sender: msg
                                                .user
                                                .clone()
                                                .unwrap_or_else(|| "unknown".to_string()),
                                            text: text.clone(),
                                            metadata: serde_json::json!({
                                                "channel_id": channel_id,
                                                "ts": msg.ts,
                                            }),
                                        };

                                        if tx.send(incoming).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(error = %e, channel = %channel_id, backoff_secs, "Slack polling error");
                            had_error = true;
                        }
                    }
                }

                if had_error {
                    backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
                } else {
                    backoff_secs = 5;
                }

                tokio::select! {
                    _ = shutdown_rx.changed() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {},
                }
            }
            info!("Slack polling stopped");
        });

        Ok(rx)
    }

    async fn send(&self, message: OutgoingMessage) -> Result<()> {
        let channel_id = message
            .metadata
            .get("channel_id")
            .and_then(|v| v.as_str())
            .context("missing channel_id in metadata")?;

        let url = format!("{}/chat.postMessage", SLACK_API);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&serde_json::json!({
                "channel": channel_id,
                "text": message.text,
            }))
            .send()
            .await
            .context("failed to send Slack message")?;

        let body: SlackResponse = response.json().await?;
        if !body.ok {
            anyhow::bail!("Slack send failed: {}", body.error.unwrap_or_default());
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "slack"
    }

    async fn stop(&self) -> Result<()> {
        let _ = self.shutdown.send(true);
        Ok(())
    }
}
