use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use sigil_core::traits::{Channel, IncomingMessage, OutgoingMessage};
use tokio::sync::mpsc;
use tracing::{error, info};

const DISCORD_API: &str = "https://discord.com/api/v10";

/// Discord Bot channel using HTTP API (no gateway/websocket for simplicity).
/// Polls for new messages at a configurable interval.
pub struct DiscordChannel {
    client: Client,
    token: String,
    channel_ids: Vec<String>,
    shutdown: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl DiscordChannel {
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
struct DiscordMessage {
    id: String,
    channel_id: String,
    content: String,
    author: DiscordUser,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    bot: Option<bool>,
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn start(&self) -> Result<mpsc::Receiver<IncomingMessage>> {
        let (tx, rx) = mpsc::channel(100);
        let client = self.client.clone();
        let token = self.token.clone();
        let channel_ids = self.channel_ids.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut last_message_ids: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut backoff_secs: u64 = 5;
            const MAX_BACKOFF_SECS: u64 = 60;
            info!("Discord polling started");

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let mut had_error = false;
                for channel_id in &channel_ids {
                    let url = format!("{}/channels/{}/messages?limit=10", DISCORD_API, channel_id);
                    let mut req = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token));

                    if let Some(after) = last_message_ids.get(channel_id) {
                        req = req.query(&[("after", after.as_str())]);
                    }

                    match req.send().await {
                        Ok(response) => {
                            if let Ok(messages) = response.json::<Vec<DiscordMessage>>().await {
                                for msg in messages.iter().rev() {
                                    if msg.author.bot.unwrap_or(false) {
                                        continue;
                                    }

                                    last_message_ids.insert(channel_id.clone(), msg.id.clone());

                                    let incoming = IncomingMessage {
                                        channel: "discord".to_string(),
                                        sender: msg.author.username.clone(),
                                        text: msg.content.clone(),
                                        metadata: serde_json::json!({
                                            "channel_id": msg.channel_id,
                                            "message_id": msg.id,
                                            "author_id": msg.author.id,
                                        }),
                                    };

                                    if tx.send(incoming).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(error = %e, channel = %channel_id, backoff_secs, "Discord polling error");
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
            info!("Discord polling stopped");
        });

        Ok(rx)
    }

    async fn send(&self, message: OutgoingMessage) -> Result<()> {
        let channel_id = message
            .metadata
            .get("channel_id")
            .and_then(|v| v.as_str())
            .context("missing channel_id in metadata")?;

        let url = format!("{}/channels/{}/messages", DISCORD_API, channel_id);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.token))
            .json(&serde_json::json!({
                "content": message.text,
            }))
            .send()
            .await
            .context("failed to send Discord message")?;

        if !response.status().is_success() {
            let body = response.text().await?;
            anyhow::bail!("Discord send failed: {body}");
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "discord"
    }

    async fn stop(&self) -> Result<()> {
        let _ = self.shutdown.send(true);
        Ok(())
    }
}
