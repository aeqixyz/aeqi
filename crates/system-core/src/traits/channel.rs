use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// An incoming message from a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub channel: String,
    pub sender: String,
    pub text: String,
    pub metadata: serde_json::Value,
}

/// An outgoing message to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub channel: String,
    pub recipient: String,
    pub text: String,
    pub metadata: serde_json::Value,
}

/// Messaging channel trait (Telegram, Discord, Slack, etc.)
#[async_trait]
pub trait Channel: Send + Sync {
    /// Start listening for messages. Returns a receiver stream.
    async fn start(&self) -> anyhow::Result<tokio::sync::mpsc::Receiver<IncomingMessage>>;

    /// Send a message through the channel.
    async fn send(&self, message: OutgoingMessage) -> anyhow::Result<()>;

    /// Set reaction on a message.
    async fn react(&self, chat_id: i64, message_id: i64, emoji: &str) -> anyhow::Result<()> {
        let _ = (chat_id, message_id, emoji);
        Ok(())
    }

    /// Channel name.
    fn name(&self) -> &str;

    /// Stop listening.
    async fn stop(&self) -> anyhow::Result<()>;
}
