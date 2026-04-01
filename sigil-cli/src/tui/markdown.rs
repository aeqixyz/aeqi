//! Markdown → styled terminal lines via pulldown-cmark.
//!
//! Converts markdown text to Vec<StyledLine> for rendering in the TUI.
//! Handles: headings, bold, italic, code spans, fenced code blocks,
//! lists, blockquotes, links, horizontal rules.

/// A styled segment of text.
#[derive(Clone, Debug)]
pub struct StyledSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub code: bool,
    pub color: Option<(u8, u8, u8)>,
}

impl StyledSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            italic: false,
            dim: false,
            code: false,
            color: None,
        }
    }

    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            ..Self::plain("")
        }
    }

    pub fn code(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            code: true,
            ..Self::plain("")
        }
    }

    pub fn dim(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            dim: true,
            ..Self::plain("")
        }
    }
}

/// A line of styled text.
#[derive(Clone, Debug)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
    pub is_code_block: bool,
    pub indent: u16,
}

impl StyledLine {
    pub fn new(spans: Vec<StyledSpan>) -> Self {
        Self {
            spans,
            is_code_block: false,
            indent: 0,
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn plain(text: &str) -> Self {
        Self::new(vec![StyledSpan::plain(text)])
    }
}

/// Parse markdown text into styled lines for terminal rendering.
pub fn parse_markdown(text: &str) -> Vec<StyledLine> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let parser = Parser::new(text);
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut current_spans: Vec<StyledSpan> = Vec::new();
    let mut in_code_block = false;
    let mut in_heading = false;
    let mut in_bold = false;
    let mut in_italic = false;
    let mut list_depth: u16 = 0;
    let mut code_block_lang = String::new();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    in_heading = true;
                    let prefix = "#".repeat(level as usize);
                    current_spans.push(StyledSpan::bold(format!("{prefix} ")));
                }
                Tag::Paragraph => {}
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    code_block_lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                        _ => String::new(),
                    };
                    // Code block header
                    let header = if code_block_lang.is_empty() {
                        "```".to_string()
                    } else {
                        format!("```{code_block_lang}")
                    };
                    lines.push(StyledLine {
                        spans: vec![StyledSpan::dim(header)],
                        is_code_block: true,
                        indent: 0,
                    });
                }
                Tag::Strong => {
                    in_bold = true;
                }
                Tag::Emphasis => {
                    in_italic = true;
                }
                Tag::List(_) => {
                    list_depth += 1;
                }
                Tag::Item => {
                    let prefix = if list_depth > 1 { "    - " } else { "  - " };
                    current_spans.push(StyledSpan::plain(prefix));
                }
                Tag::BlockQuote(_) => {
                    current_spans.push(StyledSpan::dim("  │ "));
                }
                Tag::Link { dest_url, .. } => {
                    // Will be closed with the URL
                    current_spans.push(StyledSpan {
                        text: String::new(),
                        color: Some((100, 149, 237)), // Cornflower blue
                        ..StyledSpan::plain("")
                    });
                    let _ = dest_url; // URL rendered on close
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    in_heading = false;
                    lines.push(StyledLine::new(std::mem::take(&mut current_spans)));
                }
                TagEnd::Paragraph => {
                    if !current_spans.is_empty() {
                        lines.push(StyledLine::new(std::mem::take(&mut current_spans)));
                    }
                    lines.push(StyledLine::empty());
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    lines.push(StyledLine {
                        spans: vec![StyledSpan::dim("```")],
                        is_code_block: true,
                        indent: 0,
                    });
                    code_block_lang.clear();
                }
                TagEnd::Strong => {
                    in_bold = false;
                }
                TagEnd::Emphasis => {
                    in_italic = false;
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                }
                TagEnd::Item => {
                    if !current_spans.is_empty() {
                        let mut line = StyledLine::new(std::mem::take(&mut current_spans));
                        line.indent = list_depth.saturating_sub(1) * 2;
                        lines.push(line);
                    }
                }
                TagEnd::BlockQuote(_) => {
                    if !current_spans.is_empty() {
                        lines.push(StyledLine::new(std::mem::take(&mut current_spans)));
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    // Each line of the code block as a separate styled line.
                    for line in text.lines() {
                        lines.push(StyledLine {
                            spans: vec![StyledSpan {
                                text: format!("  {line}"),
                                dim: true,
                                code: true,
                                ..StyledSpan::plain("")
                            }],
                            is_code_block: true,
                            indent: 0,
                        });
                    }
                } else if in_heading || in_bold {
                    current_spans.push(StyledSpan::bold(text.to_string()));
                } else if in_italic {
                    current_spans.push(StyledSpan {
                        text: text.to_string(),
                        italic: true,
                        ..StyledSpan::plain("")
                    });
                } else {
                    current_spans.push(StyledSpan::plain(text.to_string()));
                }
            }
            Event::Code(code) => {
                current_spans.push(StyledSpan::code(format!("`{code}`")));
            }
            Event::SoftBreak | Event::HardBreak => {
                if !current_spans.is_empty() {
                    lines.push(StyledLine::new(std::mem::take(&mut current_spans)));
                }
            }
            Event::Rule => {
                lines.push(StyledLine::new(vec![StyledSpan::dim(
                    "────────────────────────────────────────",
                )]));
            }
            _ => {}
        }
    }

    // Flush remaining spans.
    if !current_spans.is_empty() {
        lines.push(StyledLine::new(current_spans));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading() {
        let lines = parse_markdown("# Hello World");
        assert!(!lines.is_empty());
        assert!(lines[0].spans.iter().any(|s| s.bold));
    }

    #[test]
    fn test_code_block() {
        let lines = parse_markdown("```rust\nfn main() {}\n```");
        assert!(lines.iter().any(|l| l.is_code_block));
    }

    #[test]
    fn test_bold() {
        let lines = parse_markdown("This is **bold** text");
        assert!(lines[0].spans.iter().any(|s| s.bold));
    }

    #[test]
    fn test_inline_code() {
        let lines = parse_markdown("Use `cargo test` to run");
        assert!(lines[0].spans.iter().any(|s| s.code));
    }

    #[test]
    fn test_list() {
        let lines = parse_markdown("- item 1\n- item 2");
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_empty() {
        let lines = parse_markdown("");
        assert!(lines.is_empty());
    }
}
