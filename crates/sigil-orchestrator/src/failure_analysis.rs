//! Adaptive Retry with Failure Analysis.
//!
//! Classifies failure modes (missing_context, wrong_approach, tool_failure,
//! external_blocker, budget_exhausted) and mutates retry strategy accordingly.
//! Instead of blind re-queuing, enriches the task with contextual information
//! based on the failure mode.

use serde::{Deserialize, Serialize};

/// Classification of why a task failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    MissingContext,
    WrongApproach,
    ToolFailure,
    BudgetExhausted,
    ExternalBlocker,
    Unknown,
}

/// Structured analysis of a task failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAnalysis {
    pub mode: FailureMode,
    pub reasoning: String,
    pub suggested_approach: Option<String>,
    pub failed_tools: Vec<String>,
    pub missing_context_hints: Vec<String>,
}

impl FailureAnalysis {
    fn parse_json(text: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct JsonFailureAnalysis {
            #[serde(default)]
            mode: Option<FailureMode>,
            #[serde(default)]
            reasoning: String,
            #[serde(default)]
            suggested_approach: Option<String>,
            #[serde(default)]
            failed_tools: Vec<String>,
            #[serde(default)]
            missing_context_hints: Vec<String>,
        }

        let candidate = extract_json_block(text)?;
        let parsed: JsonFailureAnalysis = serde_json::from_str(candidate).ok()?;
        Some(Self {
            mode: parsed.mode.unwrap_or(FailureMode::Unknown),
            reasoning: parsed.reasoning,
            suggested_approach: parsed
                .suggested_approach
                .and_then(|value| if value.trim().is_empty() { None } else { Some(value) }),
            failed_tools: parsed.failed_tools,
            missing_context_hints: parsed.missing_context_hints,
        })
    }

    /// Parse a failure analysis from LLM response text.
    /// Expected format:
    /// ```text
    /// MODE: missing_context
    /// REASONING: The worker couldn't find the database schema
    /// APPROACH: Check migrations/ directory first
    /// TOOLS: shell, file_read
    /// CONTEXT: database schema, migration files
    /// ```
    pub fn parse(text: &str) -> Self {
        if let Some(parsed) = Self::parse_json(text) {
            return parsed;
        }

        let mut mode = FailureMode::Unknown;
        let mut reasoning = String::new();
        let mut suggested_approach = None;
        let mut failed_tools = Vec::new();
        let mut missing_context_hints = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("MODE:") {
                let rest = rest.trim().to_lowercase();
                mode = match rest.as_str() {
                    "missing_context" => FailureMode::MissingContext,
                    "wrong_approach" => FailureMode::WrongApproach,
                    "tool_failure" => FailureMode::ToolFailure,
                    "budget_exhausted" => FailureMode::BudgetExhausted,
                    "external_blocker" => FailureMode::ExternalBlocker,
                    _ => FailureMode::Unknown,
                };
            } else if let Some(rest) = line.strip_prefix("REASONING:") {
                reasoning = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("APPROACH:") {
                let approach = rest.trim().to_string();
                if !approach.is_empty() {
                    suggested_approach = Some(approach);
                }
            } else if let Some(rest) = line.strip_prefix("TOOLS:") {
                failed_tools = rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if let Some(rest) = line.strip_prefix("CONTEXT:") {
                missing_context_hints = rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        Self {
            mode,
            reasoning,
            suggested_approach,
            failed_tools,
            missing_context_hints,
        }
    }

    /// Enrich a task description based on the failure analysis.
    /// Returns the additional context to append to the task description.
    pub fn enrich_description(&self) -> String {
        let mut enrichment = format!(
            "\n\n---\n## Failure Analysis\n\n**Mode**: {}\n**Reasoning**: {}\n",
            serde_json::to_value(self.mode)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", self.mode)),
            self.reasoning,
        );

        match self.mode {
            FailureMode::MissingContext => {
                if !self.missing_context_hints.is_empty() {
                    enrichment.push_str(&format!(
                        "\n**Missing context**: {}\n\
                         Before starting, search the codebase for: {}\n",
                        self.missing_context_hints.join(", "),
                        self.missing_context_hints.join(", "),
                    ));
                }
            }
            FailureMode::WrongApproach => {
                if let Some(ref approach) = self.suggested_approach {
                    enrichment.push_str(&format!(
                        "\n**Suggested alternative approach**: {approach}\n\
                         The previous approach failed. Try this alternative.\n"
                    ));
                }
            }
            FailureMode::ToolFailure => {
                if !self.failed_tools.is_empty() {
                    enrichment.push_str(&format!(
                        "\n**Failed tools**: {}\n\
                         Avoid these tools or use alternatives.\n",
                        self.failed_tools.join(", ")
                    ));
                }
            }
            FailureMode::ExternalBlocker => {
                enrichment.push_str(
                    "\n**External blocker detected.** This task cannot proceed without external input.\n"
                );
            }
            FailureMode::BudgetExhausted => {
                enrichment.push_str(
                    "\n**Budget exhausted.** This task requires a budget increase to continue.\n",
                );
            }
            FailureMode::Unknown => {
                enrichment.push_str("\nTry a different approach to avoid the same failure.\n");
            }
        }

        enrichment
    }

    /// Build an LLM prompt for failure analysis.
    pub fn analysis_prompt(subject: &str, description: &str, error: &str) -> String {
        format!(
            "Analyze this task failure and classify the failure mode.\n\n\
             Task: {subject}\n\
             Description: {description}\n\
             Error: {error}\n\n\
             Respond with ONLY valid JSON using this exact schema:\n\
             {{\n\
               \"mode\": \"<missing_context|wrong_approach|tool_failure|budget_exhausted|external_blocker|unknown>\",\n\
               \"reasoning\": \"<one sentence explanation>\",\n\
               \"suggested_approach\": \"<alternative approach or empty string>\",\n\
               \"failed_tools\": [\"<tool>\", \"<tool>\"],\n\
               \"missing_context_hints\": [\"<hint>\", \"<hint>\"]\n\
             }}"
        )
    }
}

fn extract_json_block(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + "```json".len()..];
        let after = after.strip_prefix('\n').unwrap_or(after).trim();
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim());
        }
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && start < end
    {
        return Some(trimmed[start..=end].trim());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_failure_analysis() {
        let text = "MODE: missing_context\nREASONING: Could not find the config file\nAPPROACH: Check /etc directory\nTOOLS: file_read\nCONTEXT: config path, environment variables";
        let analysis = FailureAnalysis::parse(text);
        assert_eq!(analysis.mode, FailureMode::MissingContext);
        assert_eq!(analysis.reasoning, "Could not find the config file");
        assert_eq!(
            analysis.suggested_approach,
            Some("Check /etc directory".to_string())
        );
        assert_eq!(analysis.failed_tools, vec!["file_read"]);
        assert_eq!(
            analysis.missing_context_hints,
            vec!["config path", "environment variables"]
        );
    }

    #[test]
    fn test_parse_failure_analysis_json() {
        let text = r#"{
            "mode": "missing_context",
            "reasoning": "Could not find the config file",
            "suggested_approach": "Check /etc directory",
            "failed_tools": ["file_read"],
            "missing_context_hints": ["config path", "environment variables"]
        }"#;
        let analysis = FailureAnalysis::parse(text);
        assert_eq!(analysis.mode, FailureMode::MissingContext);
        assert_eq!(analysis.reasoning, "Could not find the config file");
        assert_eq!(
            analysis.suggested_approach,
            Some("Check /etc directory".to_string())
        );
        assert_eq!(analysis.failed_tools, vec!["file_read"]);
        assert_eq!(
            analysis.missing_context_hints,
            vec!["config path", "environment variables"]
        );
    }

    #[test]
    fn test_missing_context_enrichment() {
        let analysis = FailureAnalysis {
            mode: FailureMode::MissingContext,
            reasoning: "Missing schema".to_string(),
            suggested_approach: None,
            failed_tools: vec![],
            missing_context_hints: vec!["database schema".to_string(), "migrations".to_string()],
        };
        let desc = analysis.enrich_description();
        assert!(desc.contains("missing_context"));
        assert!(desc.contains("database schema"));
        assert!(desc.contains("migrations"));
    }

    #[test]
    fn test_external_blocker_escalates() {
        let analysis = FailureAnalysis {
            mode: FailureMode::ExternalBlocker,
            reasoning: "API key expired".to_string(),
            suggested_approach: None,
            failed_tools: vec![],
            missing_context_hints: vec![],
        };
        let desc = analysis.enrich_description();
        assert!(desc.contains("External blocker"));
        assert!(desc.contains("cannot proceed"));
    }

    #[test]
    fn test_wrong_approach_suggests_alternative() {
        let analysis = FailureAnalysis {
            mode: FailureMode::WrongApproach,
            reasoning: "Tried regex but should use parser".to_string(),
            suggested_approach: Some("Use tree-sitter for parsing".to_string()),
            failed_tools: vec![],
            missing_context_hints: vec![],
        };
        let desc = analysis.enrich_description();
        assert!(desc.contains("wrong_approach"));
        assert!(desc.contains("tree-sitter"));
    }

    #[test]
    fn test_tool_failure_records_tools() {
        let analysis = FailureAnalysis {
            mode: FailureMode::ToolFailure,
            reasoning: "Shell command timed out".to_string(),
            suggested_approach: None,
            failed_tools: vec!["shell".to_string(), "git".to_string()],
            missing_context_hints: vec![],
        };
        let desc = analysis.enrich_description();
        assert!(desc.contains("shell"));
        assert!(desc.contains("git"));
        assert!(desc.contains("Avoid these tools"));
    }
}
