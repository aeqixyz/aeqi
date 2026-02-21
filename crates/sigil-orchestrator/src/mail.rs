use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// A durable message between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mail {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
}

impl Mail {
    pub fn new(from: &str, to: &str, subject: &str, body: &str) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            timestamp: Utc::now(),
            read: false,
        }
    }
}

/// In-memory mail bus for agent-to-agent communication.
pub struct MailBus {
    messages: Mutex<VecDeque<Mail>>,
}

impl MailBus {
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(VecDeque::new()),
        }
    }

    /// Send a message.
    pub async fn send(&self, mail: Mail) {
        self.messages.lock().await.push_back(mail);
    }

    /// Read all unread messages for a recipient.
    pub async fn read(&self, recipient: &str) -> Vec<Mail> {
        let mut msgs = self.messages.lock().await;
        let mut result = Vec::new();
        for msg in msgs.iter_mut() {
            if msg.to == recipient && !msg.read {
                msg.read = true;
                result.push(msg.clone());
            }
        }
        result
    }

    /// Get all messages (for status/debugging).
    pub async fn all(&self) -> Vec<Mail> {
        self.messages.lock().await.iter().cloned().collect()
    }

    /// Count unread messages for a recipient.
    pub async fn unread_count(&self, recipient: &str) -> usize {
        self.messages
            .lock()
            .await
            .iter()
            .filter(|m| m.to == recipient && !m.read)
            .count()
    }
}

impl Default for MailBus {
    fn default() -> Self {
        Self::new()
    }
}
