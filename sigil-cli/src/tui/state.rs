//! TUI application state — messages, agent identity, streaming status.

use std::collections::HashMap;
use std::time::Instant;

/// Visual identity for the active agent.
#[derive(Debug, Clone)]
pub struct AgentVisual {
    pub name: String,
    pub display_name: String,
    pub color: (u8, u8, u8), // RGB
    pub avatar: String,
    pub faces: HashMap<String, String>,
}

impl AgentVisual {
    pub fn default_shadow() -> Self {
        let mut faces = HashMap::new();
        faces.insert("greeting".into(), "(◕‿◕)✧".into());
        faces.insert("thinking".into(), "(◔_◔)".into());
        faces.insert("working".into(), "(•̀ᴗ•́)و".into());
        faces.insert("error".into(), "(╥﹏╥)".into());
        faces.insert("complete".into(), "(◕‿◕✿)".into());
        faces.insert("idle".into(), "(￣ω￣)".into());

        Self {
            name: "shadow".into(),
            display_name: "Shadow".into(),
            color: (255, 215, 0), // Gold
            avatar: "⚕".into(),
            faces,
        }
    }

    pub fn face(&self, state: &str) -> &str {
        self.faces
            .get(state)
            .map(|s| s.as_str())
            .unwrap_or(&self.avatar)
    }

    /// Parse hex color "#FFD700" to RGB tuple.
    pub fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(215);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            (r, g, b)
        } else {
            (255, 215, 0) // Default gold
        }
    }
}

/// A single line of chat output with styling info.
#[derive(Clone)]
pub struct ChatMessage {
    pub kind: MessageKind,
    pub content: String,
    pub timestamp: Instant,
}

#[derive(Clone, PartialEq)]
pub enum MessageKind {
    /// User input
    User,
    /// Agent text response (may contain markdown)
    AssistantText,
    /// Agent text being streamed (incomplete)
    AssistantStreaming,
    /// Tool execution line: ┊ emoji verb detail duration
    ToolActivity {
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },
    /// System info (compaction, status, etc.)
    System,
    /// Error
    Error,
    /// Response box open marker
    ResponseBoxOpen,
    /// Response box close marker
    ResponseBoxClose,
}

/// Agent state during streaming.
#[derive(Clone, PartialEq)]
pub enum AgentState {
    Idle,
    Thinking,
    Working,
    Streaming,
    Error,
}

/// Application state for the TUI.
pub struct AppState {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub cursor_pos: usize,
    pub agent: AgentVisual,
    pub agent_state: AgentState,
    pub model: String,
    pub tokens: u32,
    pub context_pct: u32,
    pub cost: f64,
    pub turns: u32,
    pub start_time: Instant,
    pub should_quit: bool,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub agent_id: Option<String>,
    pub project: Option<String>,
    pub streaming_text: String,
    /// Spinner frame counter for animation.
    pub tick: u64,
    /// Input history for up/down arrow.
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl AppState {
    pub fn new(agent: AgentVisual) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            agent,
            agent_state: AgentState::Idle,
            model: String::new(),
            tokens: 0,
            context_pct: 0,
            cost: 0.0,
            turns: 0,
            start_time: Instant::now(),
            should_quit: false,
            scroll_offset: 0,
            auto_scroll: true,
            agent_id: None,
            project: None,
            streaming_text: String::new(),
            tick: 0,
            history: Vec::new(),
            history_index: None,
        }
    }

    pub fn push_user(&mut self, text: &str) {
        self.messages.push(ChatMessage {
            kind: MessageKind::User,
            content: text.to_string(),
            timestamp: Instant::now(),
        });
        self.history.push(text.to_string());
        self.history_index = None;
        self.auto_scroll = true;
    }

    pub fn open_response_box(&mut self) {
        self.messages.push(ChatMessage {
            kind: MessageKind::ResponseBoxOpen,
            content: String::new(),
            timestamp: Instant::now(),
        });
        self.agent_state = AgentState::Streaming;
    }

    pub fn append_streaming(&mut self, delta: &str) {
        self.streaming_text.push_str(delta);
    }

    pub fn close_response_box(&mut self) {
        // Finalize streaming text as a complete message.
        if !self.streaming_text.is_empty() {
            self.messages.push(ChatMessage {
                kind: MessageKind::AssistantText,
                content: std::mem::take(&mut self.streaming_text),
                timestamp: Instant::now(),
            });
        }
        self.messages.push(ChatMessage {
            kind: MessageKind::ResponseBoxClose,
            content: String::new(),
            timestamp: Instant::now(),
        });
        self.agent_state = AgentState::Idle;
    }

    pub fn push_tool_activity(
        &mut self,
        tool_name: &str,
        detail: &str,
        success: bool,
        duration_ms: u64,
    ) {
        let emoji = tool_emoji(tool_name);
        let verb = tool_verb(tool_name);
        let status = if success { "" } else { " [FAIL]" };
        let content = format!(
            "┊ {emoji} {verb:<9} {detail}  {:.1}s{status}",
            duration_ms as f64 / 1000.0
        );
        self.messages.push(ChatMessage {
            kind: MessageKind::ToolActivity {
                tool_name: tool_name.to_string(),
                success,
                duration_ms,
            },
            content,
            timestamp: Instant::now(),
        });
    }

    pub fn push_system(&mut self, text: &str) {
        self.messages.push(ChatMessage {
            kind: MessageKind::System,
            content: text.to_string(),
            timestamp: Instant::now(),
        });
    }

    pub fn push_error(&mut self, text: &str) {
        self.messages.push(ChatMessage {
            kind: MessageKind::Error,
            content: text.to_string(),
            timestamp: Instant::now(),
        });
        self.agent_state = AgentState::Error;
    }

    pub fn elapsed_str(&self) -> String {
        let secs = self.start_time.elapsed().as_secs();
        if secs < 60 {
            format!("{secs}s")
        } else if secs < 3600 {
            format!("{}m{}s", secs / 60, secs % 60)
        } else {
            format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    /// Spinner frames for the current agent state.
    pub fn spinner_frame(&self) -> &str {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[(self.tick as usize) % FRAMES.len()]
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            None => self.history.len().saturating_sub(1),
            Some(i) => i.saturating_sub(1),
        };
        self.history_index = Some(idx);
        self.input = self.history[idx].clone();
        self.cursor_pos = self.input.len();
    }

    pub fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(i) => {
                if i + 1 < self.history.len() {
                    self.history_index = Some(i + 1);
                    self.input = self.history[i + 1].clone();
                } else {
                    self.history_index = None;
                    self.input.clear();
                }
                self.cursor_pos = self.input.len();
            }
        }
    }
}

fn tool_emoji(name: &str) -> &'static str {
    match name {
        "read_file" | "read" => "📖",
        "write_file" | "write" => "✍️",
        "edit_file" | "edit" => "✏️",
        "shell" | "bash" => "💻",
        "grep" => "🔍",
        "glob" => "📁",
        "web_search" | "websearch" => "🌐",
        "web_fetch" | "webfetch" => "🌐",
        "delegate" => "🤖",
        "execute_plan" => "📋",
        "memory_recall" | "sigil_recall" => "📚",
        "memory_store" | "sigil_remember" => "💾",
        "blackboard" | "sigil_blackboard" => "📌",
        "sigil_graph" => "🔗",
        "sigil_skills" => "⚡",
        _ => "⚙️",
    }
}

fn tool_verb(name: &str) -> &'static str {
    match name {
        "read_file" | "read" => "read",
        "write_file" | "write" => "write",
        "edit_file" | "edit" => "edit",
        "shell" | "bash" => "$",
        "grep" => "search",
        "glob" => "glob",
        "web_search" | "websearch" => "search",
        "web_fetch" | "webfetch" => "fetch",
        "delegate" => "delegate",
        "execute_plan" => "plan",
        "memory_recall" | "sigil_recall" => "recall",
        "memory_store" | "sigil_remember" => "store",
        _ => "tool",
    }
}
