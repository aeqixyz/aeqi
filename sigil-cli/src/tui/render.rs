//! Terminal rendering — converts AppState into styled terminal output.
//!
//! Uses raw ANSI escape codes for the scrolling output area (printed above
//! the pinned bottom), and ratatui widgets for the fixed bottom area
//! (status bar + input).

use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::Write;

use super::markdown::{StyledLine, StyledSpan};
use super::state::{AgentState, AppState, ChatMessage, MessageKind};

// ---------------------------------------------------------------------------
// Scrolling output (printed via stdout, scrolls above pinned area)
// ---------------------------------------------------------------------------

/// Render a chat message to the terminal (scrolling area).
pub fn print_message(stdout: &mut impl Write, msg: &ChatMessage, state: &AppState, width: u16) {
    match &msg.kind {
        MessageKind::User => {
            let _ = writeln!(
                stdout,
                "\n  \x1b[1;36mYou:\x1b[0m {}",
                msg.content
            );
        }
        MessageKind::AssistantText => {
            let lines = super::markdown::parse_markdown(&msg.content);
            for line in &lines {
                print_styled_line(stdout, line, width);
            }
        }
        MessageKind::ResponseBoxOpen => {
            let (r, g, b) = state.agent.color;
            let face = state.agent.face("greeting");
            let name = &state.agent.display_name;
            let border = "─".repeat((width as usize).saturating_sub(name.len() + 8));
            let _ = writeln!(
                stdout,
                "\n  \x1b[38;2;{r};{g};{b}m╭─ {face} {name} {border}╮\x1b[0m"
            );
        }
        MessageKind::ResponseBoxClose => {
            let (r, g, b) = state.agent.color;
            let border = "─".repeat((width as usize).saturating_sub(4));
            let _ = writeln!(
                stdout,
                "  \x1b[38;2;{r};{g};{b}m╰{border}╯\x1b[0m\n"
            );
        }
        MessageKind::ToolActivity { success, .. } => {
            let color = if *success { "\x1b[90m" } else { "\x1b[31m" };
            let _ = writeln!(stdout, "  {color}{}\x1b[0m", msg.content);
        }
        MessageKind::System => {
            let _ = writeln!(stdout, "  \x1b[90m{}\x1b[0m", msg.content);
        }
        MessageKind::Error => {
            let _ = writeln!(stdout, "  \x1b[31m✗ {}\x1b[0m", msg.content);
        }
        MessageKind::AssistantStreaming => {
            // Handled separately via streaming text.
        }
    }
}

/// Print streaming text delta (appended to current line, no newline).
pub fn print_streaming_delta(stdout: &mut impl Write, delta: &str) {
    let _ = write!(stdout, "{delta}");
    let _ = stdout.flush();
}

/// Print the thinking/working indicator with agent face.
pub fn print_thinking(stdout: &mut impl Write, state: &AppState) {
    let face = match state.agent_state {
        AgentState::Thinking => state.agent.face("thinking"),
        AgentState::Working => state.agent.face("working"),
        _ => state.agent.face("idle"),
    };
    let spinner = state.spinner_frame();
    let _ = write!(stdout, "\r  \x1b[90m{spinner} {face}\x1b[0m\x1b[K");
    let _ = stdout.flush();
}

/// Clear the thinking indicator line.
pub fn clear_thinking(stdout: &mut impl Write) {
    let _ = write!(stdout, "\r\x1b[K");
    let _ = stdout.flush();
}

fn print_styled_line(stdout: &mut impl Write, line: &StyledLine, _width: u16) {
    let indent = " ".repeat(2 + line.indent as usize);
    let _ = write!(stdout, "{indent}");

    for span in &line.spans {
        print_styled_span(stdout, span);
    }

    let _ = writeln!(stdout);
}

fn print_styled_span(stdout: &mut impl Write, span: &StyledSpan) {
    let mut codes = Vec::new();

    if span.bold {
        codes.push("1");
    }
    if span.italic {
        codes.push("3");
    }
    if span.dim {
        codes.push("2");
    }
    if span.code {
        codes.push("48;5;236"); // Dark background for code
        codes.push("38;5;215"); // Orange text for code
    }
    if let Some((r, g, b)) = span.color {
        let _ = write!(stdout, "\x1b[38;2;{r};{g};{b}m");
    }

    if !codes.is_empty() {
        let _ = write!(stdout, "\x1b[{}m", codes.join(";"));
    }

    let _ = write!(stdout, "{}", span.text);

    if !codes.is_empty() || span.color.is_some() {
        let _ = write!(stdout, "\x1b[0m");
    }
}

// ---------------------------------------------------------------------------
// Fixed bottom area (ratatui widgets)
// ---------------------------------------------------------------------------

/// Draw the pinned bottom area: status bar + input.
pub fn draw_bottom(frame: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // status bar
        Constraint::Length(3), // input
    ])
    .split(area);

    draw_status_bar(frame, chunks[0], state);
    draw_input(frame, chunks[1], state);
}

fn draw_status_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let (r, g, b) = state.agent.color;
    let agent_color = Color::Rgb(r, g, b);

    let face = match state.agent_state {
        AgentState::Idle => state.agent.face("idle"),
        AgentState::Thinking => state.agent.face("thinking"),
        AgentState::Working | AgentState::Streaming => state.agent.face("working"),
        AgentState::Error => state.agent.face("error"),
    };

    let streaming_indicator = match state.agent_state {
        AgentState::Streaming | AgentState::Working => {
            format!(" {} ", state.spinner_frame())
        }
        AgentState::Thinking => format!(" {} ", state.spinner_frame()),
        _ => " ".to_string(),
    };

    let model_short = if state.model.is_empty() {
        String::new()
    } else {
        let short = state.model.rsplit('/').next().unwrap_or(&state.model);
        format!(" {short} ")
    };

    let tokens_str = format_number(state.tokens);
    let pct_str = if state.context_pct > 0 {
        format!(" {}% ", state.context_pct)
    } else {
        String::new()
    };

    let cost_str = if state.cost >= 0.50 {
        format!("${:.2}", state.cost)
    } else if state.cost > 0.0 {
        format!("${:.4}", state.cost)
    } else {
        "$0".to_string()
    };

    let elapsed = state.elapsed_str();

    let line = Line::from(vec![
        Span::styled(
            format!(" {face} {} ", state.agent.display_name),
            Style::default()
                .fg(agent_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            streaming_indicator,
            Style::default().fg(Color::Yellow),
        ),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(model_short, Style::default().fg(Color::White)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" {tokens_str} tok "),
            Style::default().fg(Color::White),
        ),
        Span::styled(pct_str, pct_style(state.context_pct)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {cost_str} "), Style::default().fg(Color::White)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {elapsed} "), Style::default().fg(Color::DarkGray)),
    ]);

    let bar = Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(25, 25, 25)));
    frame.render_widget(bar, area);
}

fn draw_input(frame: &mut Frame, area: Rect, state: &AppState) {
    let (r, g, b) = state.agent.color;
    let prompt_color = Color::Rgb(r, g, b);

    let placeholder = match state.agent_state {
        AgentState::Streaming | AgentState::Working | AgentState::Thinking => {
            "type + Enter to interrupt, Ctrl+C to cancel"
        }
        _ => "type a message...",
    };

    let display_text = if state.input.is_empty() {
        format!("❯ \x1b[90m{placeholder}\x1b[0m")
    } else {
        format!("❯ {}", state.input)
    };

    let input = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(prompt_color))
                .border_type(BorderType::Plain),
        )
        .style(Style::default().fg(Color::White));
    frame.render_widget(input, area);

    // Cursor position.
    let cursor_x = area.x + 2 + state.cursor_pos as u16;
    let cursor_y = area.y + 1;
    frame.set_cursor_position((
        cursor_x.min(area.x + area.width.saturating_sub(1)),
        cursor_y,
    ));
}

fn pct_style(pct: u32) -> Style {
    let color = if pct < 50 {
        Color::Green
    } else if pct < 80 {
        Color::Yellow
    } else if pct < 95 {
        Color::Rgb(255, 165, 0) // Orange
    } else {
        Color::Red
    };
    Style::default().fg(color)
}

pub fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
