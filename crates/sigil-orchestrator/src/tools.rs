use anyhow::Result;
use async_trait::async_trait;
use sigil_core::traits::{Channel, ToolResult, ToolSpec};
use sigil_core::traits::Tool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::mail::{Mail, MailBus};
use crate::registry::RigRegistry;

/// Tool for querying rig health, bead counts, and worker states.
pub struct RigStatusTool {
    registry: Arc<RigRegistry>,
}

impl RigStatusTool {
    pub fn new(registry: Arc<RigRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for RigStatusTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let rig_filter = args.get("rig").and_then(|v| v.as_str());

        let status = self.registry.status().await;
        let mut output = String::new();

        for rs in &status.rigs {
            if let Some(filter) = rig_filter
                && rs.name != filter
            {
                continue;
            }
            output.push_str(&format!(
                "{}: {} open, {} ready | workers: {} idle, {} working, {} hooked\n",
                rs.name, rs.open_beads, rs.ready_beads,
                rs.workers_idle, rs.workers_working, rs.workers_hooked,
            ));
        }

        if rig_filter.is_none() {
            output.push_str(&format!("\nUnread mail: {}\n", status.unread_mail));
        }

        if output.is_empty() {
            if let Some(filter) = rig_filter {
                return Ok(ToolResult::error(format!("Rig not found: {filter}")));
            }
            output = "No rigs registered.\n".to_string();
        }

        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "rig_status".to_string(),
            description: "Get rig health, bead counts, and worker states. Optionally filter by rig name.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rig": { "type": "string", "description": "Optional rig name to filter (omit for all rigs)" }
                }
            }),
        }
    }

    fn name(&self) -> &str { "rig_status" }
}

/// Tool for assigning a task (bead) to a target rig.
pub struct RigAssignTool {
    registry: Arc<RigRegistry>,
}

impl RigAssignTool {
    pub fn new(registry: Arc<RigRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for RigAssignTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let rig = args.get("rig")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing rig"))?;
        let subject = args.get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing subject"))?;
        let description = args.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match self.registry.assign(rig, subject, description).await {
            Ok(bead) => Ok(ToolResult::success(format!(
                "Assigned {} [{}] {} to rig '{}'",
                bead.id, bead.priority, bead.subject, rig
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to assign: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "rig_assign".to_string(),
            description: "Assign a task to a specific rig by creating a bead on it.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rig": { "type": "string", "description": "Target rig name (e.g. algostaking, riftdecks)" },
                    "subject": { "type": "string", "description": "Task title" },
                    "description": { "type": "string", "description": "Detailed task description" }
                },
                "required": ["rig", "subject"]
            }),
        }
    }

    fn name(&self) -> &str { "rig_assign" }
}

/// Tool for listing all registered rigs with metadata.
pub struct RigListTool {
    registry: Arc<RigRegistry>,
}

impl RigListTool {
    pub fn new(registry: Arc<RigRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for RigListTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let rigs = self.registry.rigs_info().await;

        if rigs.is_empty() {
            return Ok(ToolResult::success("No rigs registered."));
        }

        let mut output = String::new();
        for rig in &rigs {
            output.push_str(&format!(
                "{} (prefix: {}, model: {}, max_workers: {})\n",
                rig["name"].as_str().unwrap_or("?"),
                rig["prefix"].as_str().unwrap_or("?"),
                rig["model"].as_str().unwrap_or("?"),
                rig["max_workers"].as_u64().unwrap_or(0),
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "rig_list".to_string(),
            description: "List all registered rigs with their prefix, model, and worker count.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str { "rig_list" }
}

/// Tool for reading unread mail addressed to the familiar.
pub struct MailReadTool {
    mail_bus: Arc<MailBus>,
}

impl MailReadTool {
    pub fn new(mail_bus: Arc<MailBus>) -> Self {
        Self { mail_bus }
    }
}

#[async_trait]
impl Tool for MailReadTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let messages = self.mail_bus.read("familiar").await;

        if messages.is_empty() {
            return Ok(ToolResult::success("No unread mail."));
        }

        let mut output = String::new();
        for m in &messages {
            output.push_str(&format!(
                "[{}] from={} subject={}\n{}\n\n",
                m.timestamp.format("%H:%M:%S"),
                m.from, m.subject, m.body,
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "mail_read".to_string(),
            description: "Read all unread mail addressed to the familiar.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str { "mail_read" }
}

/// Tool for sending mail through the bus.
pub struct MailSendTool {
    mail_bus: Arc<MailBus>,
}

impl MailSendTool {
    pub fn new(mail_bus: Arc<MailBus>) -> Self {
        Self { mail_bus }
    }
}

#[async_trait]
impl Tool for MailSendTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let to = args.get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing to"))?;
        let subject = args.get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing subject"))?;
        let body = args.get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        self.mail_bus.send(Mail::new("familiar", to, subject, body)).await;
        Ok(ToolResult::success(format!("Mail sent to '{to}': {subject}")))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "mail_send".to_string(),
            description: "Send a mail message to another rig or agent through the mail bus.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string", "description": "Recipient (rig name or agent name)" },
                    "subject": { "type": "string", "description": "Mail subject" },
                    "body": { "type": "string", "description": "Mail body" }
                },
                "required": ["to", "subject"]
            }),
        }
    }

    fn name(&self) -> &str { "mail_send" }
}

/// Tool for listing all unblocked beads across all rigs.
pub struct AllReadyTool {
    registry: Arc<RigRegistry>,
}

impl AllReadyTool {
    pub fn new(registry: Arc<RigRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for AllReadyTool {
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        let ready = self.registry.all_ready().await;

        if ready.is_empty() {
            return Ok(ToolResult::success("No ready work across any rig."));
        }

        let mut output = String::new();
        for (rig_name, bead) in &ready {
            output.push_str(&format!(
                "[{}] {} [{}] {} — {}\n",
                rig_name, bead.id, bead.priority, bead.subject,
                if bead.description.is_empty() { "(no description)" } else { &bead.description }
            ));
        }
        Ok(ToolResult::success(output))
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "all_ready".to_string(),
            description: "List all unblocked beads across all rigs that are ready for work.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn name(&self) -> &str { "all_ready" }
}

/// Tool for replying to a channel message (Telegram, Discord, etc.)
pub struct ChannelReplyTool {
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
}

impl ChannelReplyTool {
    pub fn new(channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>) -> Self {
        Self { channels }
    }
}

#[async_trait]
impl Tool for ChannelReplyTool {
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let channel_name = args.get("channel")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing channel"))?;
        let text = args.get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing text"))?;

        // Build metadata from args (pass through chat_id etc.)
        let mut metadata = serde_json::Map::new();
        if let Some(chat_id) = args.get("chat_id") {
            metadata.insert("chat_id".to_string(), chat_id.clone());
        }
        if let Some(message_id) = args.get("message_id") {
            metadata.insert("message_id".to_string(), message_id.clone());
        }

        let channels = self.channels.read().await;
        let channel = channels.get(channel_name)
            .ok_or_else(|| anyhow::anyhow!("channel not found: {channel_name}"))?;

        let outgoing = sigil_core::traits::OutgoingMessage {
            channel: channel_name.to_string(),
            recipient: String::new(),
            text: text.to_string(),
            metadata: serde_json::Value::Object(metadata),
        };

        match channel.send(outgoing).await {
            Ok(()) => Ok(ToolResult::success(format!("Reply sent via {channel_name}"))),
            Err(e) => Ok(ToolResult::error(format!("Failed to send via {channel_name}: {e}"))),
        }
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "channel_reply".to_string(),
            description: "Send a reply through a messaging channel (Telegram, Discord, etc.)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Channel name (telegram, discord, slack)" },
                    "chat_id": { "type": "integer", "description": "Chat ID to reply to" },
                    "text": { "type": "string", "description": "Message text to send" }
                },
                "required": ["channel", "chat_id", "text"]
            }),
        }
    }

    fn name(&self) -> &str { "channel_reply" }
}

/// Build orchestration tools for the familiar rig.
pub fn build_orchestration_tools(
    registry: Arc<RigRegistry>,
    mail_bus: Arc<MailBus>,
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(RigStatusTool::new(registry.clone())),
        Arc::new(RigAssignTool::new(registry.clone())),
        Arc::new(RigListTool::new(registry.clone())),
        Arc::new(MailReadTool::new(mail_bus.clone())),
        Arc::new(MailSendTool::new(mail_bus)),
        Arc::new(AllReadyTool::new(registry)),
        Arc::new(ChannelReplyTool::new(channels)),
    ]
}
