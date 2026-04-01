//! Sigil Chat TUI — synthesized from CC + Hermes, built better in Rust.
//!
//! Architecture: inline-mode ratatui (NOT alternate screen). Output scrolls
//! naturally above a pinned bottom area with status bar + input.
//! Daemon client model: session survives TUI disconnect.

pub mod markdown;
pub mod render;
pub mod state;

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal;
use sigil_core::ChatStreamEvent;

use crate::helpers::load_config;
use state::{AgentState, AgentVisual, AppState};

// ---------------------------------------------------------------------------
// WebSocket background thread
// ---------------------------------------------------------------------------

enum WsCommand {
    Send(String),
    Quit,
}

fn spawn_ws_thread(
    url: String,
    cmd_rx: mpsc::Receiver<WsCommand>,
    event_tx: mpsc::Sender<ChatStreamEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use tungstenite::Message;

        let mut ws = match tungstenite::connect(&url) {
            Ok((ws, _)) => ws,
            Err(e) => {
                let _ = event_tx.send(ChatStreamEvent::Error {
                    message: format!("WebSocket connect failed: {e}"),
                    recoverable: false,
                });
                return;
            }
        };

        if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = ws.get_ref() {
            tcp.set_nonblocking(true).ok();
        }

        loop {
            // Check outbound commands.
            match cmd_rx.try_recv() {
                Ok(WsCommand::Send(text)) => {
                    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = ws.get_ref() {
                        tcp.set_nonblocking(false).ok();
                    }
                    if ws.send(Message::Text(text.into())).is_err() {
                        break;
                    }
                    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = ws.get_ref() {
                        tcp.set_nonblocking(true).ok();
                    }
                }
                Ok(WsCommand::Quit) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {}
            }

            // Check inbound messages.
            match ws.read() {
                Ok(Message::Text(text)) => {
                    if let Ok(evt) = serde_json::from_str::<ChatStreamEvent>(&text) {
                        if event_tx.send(evt).is_err() {
                            break;
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == io::ErrorKind::WouldBlock =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                _ => {}
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Event processing
// ---------------------------------------------------------------------------

fn process_ws_event(state: &mut AppState, evt: ChatStreamEvent, stdout: &mut impl Write) {
    match evt {
        ChatStreamEvent::TurnStart { model, .. } => {
            state.model = model;
            state.agent_state = AgentState::Thinking;
            state.open_response_box();
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
            // Start streaming line
            let _ = write!(stdout, "  ");
        }
        ChatStreamEvent::TextDelta { text } => {
            if state.agent_state == AgentState::Thinking {
                render::clear_thinking(stdout);
                state.agent_state = AgentState::Streaming;
            }
            state.append_streaming(&text);
            render::print_streaming_delta(stdout, &text);
        }
        ChatStreamEvent::ToolStart {
            tool_name,
            tool_use_id: _,
        } => {
            state.agent_state = AgentState::Working;
            // Newline after any streaming text.
            if !state.streaming_text.is_empty() {
                let _ = writeln!(stdout);
            }
            state.push_system(&format!("  ⚙ {tool_name}..."));
            let _ = writeln!(stdout, "  \x1b[90m⚙ {tool_name}...\x1b[0m");
        }
        ChatStreamEvent::ToolComplete {
            tool_name,
            success,
            duration_ms,
            output_preview,
            ..
        } => {
            let detail = if output_preview.len() > 60 {
                format!("{}...", &output_preview[..57])
            } else {
                output_preview
            };
            state.push_tool_activity(&tool_name, &detail, success, duration_ms);
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
        ChatStreamEvent::TurnComplete {
            prompt_tokens,
            completion_tokens,
            ..
        } => {
            state.tokens = prompt_tokens + completion_tokens;
            state.turns += 1;
        }
        ChatStreamEvent::Complete {
            total_prompt_tokens,
            total_completion_tokens,
            cost_usd,
            ..
        } => {
            // Finalize: newline after streaming, close response box.
            if !state.streaming_text.is_empty() {
                let _ = writeln!(stdout);
            }
            state.close_response_box();
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);

            state.tokens = total_prompt_tokens + total_completion_tokens;
            state.cost = cost_usd;
            state.agent_state = AgentState::Idle;
        }
        ChatStreamEvent::Status { message } => {
            state.push_system(&message);
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
        ChatStreamEvent::Error { message, .. } => {
            state.push_error(&message);
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
        ChatStreamEvent::Compacted {
            original_messages,
            remaining_messages,
            ..
        } => {
            state.push_system(&format!(
                "♻ Compacted {original_messages} → {remaining_messages} messages"
            ));
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
        ChatStreamEvent::DelegateStart {
            worker_name,
            task_subject,
        } => {
            state.push_system(&format!("→ Delegating to {worker_name}: {task_subject}"));
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
        ChatStreamEvent::DelegateComplete {
            worker_name,
            outcome,
        } => {
            state.push_system(&format!("← {worker_name}: {outcome}"));
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
        ChatStreamEvent::MemoryActivity {
            action,
            key,
            preview,
        } => {
            let icon = if action == "recalled" { "📖" } else { "💾" };
            let short = if preview.len() > 60 {
                format!("{}...", &preview[..57])
            } else {
                preview
            };
            state.push_system(&format!("{icon} {action} [{key}]: {short}"));
        }
        ChatStreamEvent::ToolProgress { .. } => {
            // Show spinner during tool execution.
            render::print_thinking(stdout, state);
        }
    }
}

// ---------------------------------------------------------------------------
// Slash command handling
// ---------------------------------------------------------------------------

fn handle_slash_command(
    cmd: &str,
    state: &mut AppState,
    stdout: &mut impl Write,
    cmd_tx: &mpsc::Sender<WsCommand>,
) -> bool {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let command = parts[0].trim_start_matches('/');
    let _args = parts.get(1).unwrap_or(&"");

    match command {
        "exit" | "quit" | "q" => {
            state.should_quit = true;
            return true;
        }
        "new" | "reset" => {
            state.messages.clear();
            state.streaming_text.clear();
            state.tokens = 0;
            state.cost = 0.0;
            state.turns = 0;
            state.start_time = std::time::Instant::now();
            let _ = writeln!(stdout, "\n  \x1b[90m✦ New conversation\x1b[0m\n");
        }
        "status" => {
            let face = state.agent.face("idle");
            let _ = writeln!(
                stdout,
                "\n  {face} {} | {} | {} tokens | {} turns | {} | {}\n",
                state.agent.display_name,
                state.model,
                render::format_number(state.tokens),
                state.turns,
                if state.cost > 0.0 {
                    format!("${:.4}", state.cost)
                } else {
                    "$0".to_string()
                },
                state.elapsed_str(),
            );
        }
        "model" => {
            let _ = writeln!(
                stdout,
                "\n  Current model: {}\n",
                if state.model.is_empty() {
                    "(not set)"
                } else {
                    &state.model
                }
            );
        }
        "help" => {
            let _ = writeln!(stdout, "\n  \x1b[1mSlash Commands\x1b[0m");
            let _ = writeln!(stdout, "  /new      — start fresh conversation");
            let _ = writeln!(stdout, "  /status   — show session stats");
            let _ = writeln!(stdout, "  /model    — show current model");
            let _ = writeln!(stdout, "  /help     — this message");
            let _ = writeln!(stdout, "  /exit     — quit\n");
        }
        _ => {
            // Unknown slash command — send to agent as a regular message.
            let msg = serde_json::json!({
                "message": cmd,
                "agent_id": state.agent_id,
                "project": state.project,
            });
            let _ = cmd_tx.send(WsCommand::Send(msg.to_string()));
            state.push_user(cmd);
            render::print_message(stdout, state.messages.last().unwrap(), state, 80);
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Interactive chat TUI — the sigil chat experience.
pub async fn run(
    config_path: &Option<PathBuf>,
    agent_name: Option<&str>,
    project: Option<&str>,
) -> Result<()> {
    let (config, _) = load_config(config_path)?;
    let data_dir = config.data_dir();

    // Resolve persistent agent.
    let registry = sigil_orchestrator::agent_registry::AgentRegistry::open(&data_dir)?;
    let agent = if let Some(name) = agent_name {
        registry.get_active_by_name(name).await?
    } else {
        registry.default_for_project(project).await?
    };

    let visual = match &agent {
        Some(a) => {
            let color = a
                .color
                .as_ref()
                .map(|c| AgentVisual::parse_hex_color(c))
                .unwrap_or((255, 215, 0));
            let mut faces = std::collections::HashMap::new();
            if let Some(ref f) = a.faces {
                faces = f.clone();
            }
            AgentVisual {
                name: a.name.clone(),
                display_name: a
                    .display_name
                    .as_deref()
                    .unwrap_or(&a.name)
                    .to_string(),
                color,
                avatar: a.avatar.clone().unwrap_or_else(|| "⚕".into()),
                faces,
            }
        }
        None => AgentVisual::default_shadow(),
    };

    let agent_id = agent.as_ref().map(|a| a.id.clone());

    // WebSocket connection.
    let bind = &config.web.bind;
    let port = bind
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8400);
    let ws_url = format!("ws://127.0.0.1:{port}/api/chat/stream");

    let (event_tx, event_rx) = mpsc::channel::<ChatStreamEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<WsCommand>();

    let ws_handle = spawn_ws_thread(ws_url, cmd_rx, event_tx);

    // Enter raw mode for input handling (NOT alternate screen).
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();

    // Print banner.
    let (r, g, b) = visual.color;
    let face = visual.face("greeting");
    eprintln!();
    let _ = writeln!(
        stdout,
        "\r  \x1b[38;2;{r};{g};{b};1m{face} {}\x1b[0m",
        visual.display_name,
    );
    let _ = writeln!(
        stdout,
        "\r  \x1b[90mtype /help for commands, /exit to quit\x1b[0m\n"
    );
    stdout.flush()?;

    // Set up ratatui for the bottom area only.
    // We use a small viewport at the bottom of the terminal.
    let backend = ratatui::backend::CrosstermBackend::new(io::stderr());
    let mut term = ratatui::Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(4), // 4 rows: status bar (1) + input (3)
        },
    )?;

    let mut state = AppState::new(visual);
    state.agent_id = agent_id;
    state.project = project.map(|s| s.to_string());

    // Main event loop.
    loop {
        // Draw the pinned bottom area.
        term.draw(|f| render::draw_bottom(f, f.area(), &state))?;

        // Drain WebSocket events.
        while let Ok(evt) = event_rx.try_recv() {
            process_ws_event(&mut state, evt, &mut stdout);
        }

        // Show thinking indicator during agent work.
        if matches!(
            state.agent_state,
            AgentState::Thinking | AgentState::Working
        ) {
            render::print_thinking(&mut stdout, &state);
        }

        // Poll crossterm events.
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if state.agent_state != AgentState::Idle {
                            // Interrupt the agent (not quit).
                            state.push_system("⏹ Interrupted");
                            let _ = writeln!(stdout, "\r  \x1b[33m⏹ Interrupted\x1b[0m");
                            state.agent_state = AgentState::Idle;
                        } else {
                            state.should_quit = true;
                        }
                    }
                    KeyCode::Esc => {
                        state.should_quit = true;
                    }
                    KeyCode::Enter => {
                        let text = state.input.trim().to_string();
                        if !text.is_empty() {
                            state.input.clear();
                            state.cursor_pos = 0;

                            if text.starts_with('/') {
                                handle_slash_command(
                                    &text,
                                    &mut state,
                                    &mut stdout,
                                    &cmd_tx,
                                );
                            } else {
                                state.push_user(&text);
                                render::print_message(
                                    &mut stdout,
                                    state.messages.last().unwrap(),
                                    &state,
                                    80,
                                );

                                let msg = serde_json::json!({
                                    "message": text,
                                    "agent_id": state.agent_id,
                                    "project": state.project,
                                });
                                let _ = cmd_tx.send(WsCommand::Send(msg.to_string()));
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        if state.cursor_pos > 0 {
                            state.cursor_pos -= 1;
                            state.input.remove(state.cursor_pos);
                        }
                    }
                    KeyCode::Left => {
                        state.cursor_pos = state.cursor_pos.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        if state.cursor_pos < state.input.len() {
                            state.cursor_pos += 1;
                        }
                    }
                    KeyCode::Up => {
                        state.history_up();
                    }
                    KeyCode::Down => {
                        state.history_down();
                    }
                    KeyCode::Home => {
                        state.cursor_pos = 0;
                    }
                    KeyCode::End => {
                        state.cursor_pos = state.input.len();
                    }
                    KeyCode::Char(c) => {
                        state.input.insert(state.cursor_pos, c);
                        state.cursor_pos += 1;
                    }
                    _ => {}
                }
            }
        }

        // Advance spinner.
        state.tick += 1;

        if state.should_quit {
            break;
        }
    }

    // Cleanup.
    let _ = cmd_tx.send(WsCommand::Quit);
    term.clear()?;
    terminal::disable_raw_mode()?;
    let _ = ws_handle.join();

    let face = state.agent.face("idle");
    eprintln!("\n  \x1b[90m{face} goodbye\x1b[0m\n");

    Ok(())
}

