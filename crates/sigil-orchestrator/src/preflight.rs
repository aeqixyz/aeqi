//! Worker Pre-flight Assessment.
//!
//! Quick assessment before committing resources to a task. Allows reject/reroute
//! decisions based on estimated difficulty, cost, and agent capability.

use serde::{Deserialize, Serialize};

/// Estimated task difficulty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Trivial,
    Easy,
    Medium,
    Hard,
    Uncertain,
}

pub const ADAPTIVE_PIPELINE_SKILL: &str = "pipeline-executor";

/// Result of a pre-flight assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightAssessment {
    pub approach: String,
    pub estimated_difficulty: Difficulty,
    pub estimated_cost_usd: f64,
    pub estimated_turns: u32,
    pub confidence: f64,
    pub risks: Vec<String>,
}

/// Verdict after evaluating a pre-flight assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightVerdict {
    Proceed,
    Reroute { reason: String },
    Reject { reason: String },
}

impl PreflightAssessment {
    fn parse_json(text: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct JsonAssessment {
            #[serde(default)]
            approach: String,
            #[serde(default, alias = "estimated_difficulty")]
            difficulty: Option<Difficulty>,
            #[serde(default, alias = "cost")]
            estimated_cost_usd: Option<f64>,
            #[serde(default, alias = "turns")]
            estimated_turns: Option<u32>,
            #[serde(default)]
            confidence: Option<f64>,
            #[serde(default)]
            risks: Vec<String>,
        }

        let candidate = extract_json_block(text)?;
        let parsed: JsonAssessment = serde_json::from_str(candidate).ok()?;
        Some(Self {
            approach: parsed.approach,
            estimated_difficulty: parsed.difficulty.unwrap_or(Difficulty::Medium),
            estimated_cost_usd: parsed.estimated_cost_usd.unwrap_or(0.01),
            estimated_turns: parsed.estimated_turns.unwrap_or(10),
            confidence: parsed.confidence.unwrap_or(0.5),
            risks: parsed.risks,
        })
    }

    /// Parse a pre-flight assessment from LLM response text.
    /// Expected format:
    /// ```text
    /// APPROACH: Use the existing parser module
    /// DIFFICULTY: medium
    /// COST: 0.05
    /// TURNS: 10
    /// CONFIDENCE: 0.8
    /// RISKS: complex regex, untested edge cases
    /// ```
    pub fn parse(text: &str) -> Self {
        if let Some(parsed) = Self::parse_json(text) {
            return parsed;
        }

        let mut approach = String::new();
        let mut difficulty = Difficulty::Medium;
        let mut cost = 0.01;
        let mut turns = 10;
        let mut confidence = 0.5;
        let mut risks = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("APPROACH:") {
                approach = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("DIFFICULTY:") {
                difficulty = match rest.trim().to_lowercase().as_str() {
                    "trivial" => Difficulty::Trivial,
                    "easy" => Difficulty::Easy,
                    "medium" => Difficulty::Medium,
                    "hard" => Difficulty::Hard,
                    _ => Difficulty::Uncertain,
                };
            } else if let Some(rest) = line.strip_prefix("COST:") {
                cost = rest.trim().parse().unwrap_or(0.01);
            } else if let Some(rest) = line.strip_prefix("TURNS:") {
                turns = rest.trim().parse().unwrap_or(10);
            } else if let Some(rest) = line.strip_prefix("CONFIDENCE:") {
                confidence = rest.trim().parse().unwrap_or(0.5);
            } else if let Some(rest) = line.strip_prefix("RISKS:") {
                risks = rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }

        Self {
            approach,
            estimated_difficulty: difficulty,
            estimated_cost_usd: cost,
            estimated_turns: turns,
            confidence,
            risks,
        }
    }

    /// Evaluate the assessment against constraints. Returns a verdict.
    pub fn evaluate(&self, budget_remaining: f64, agent_success_rate: f64) -> PreflightVerdict {
        if self.estimated_cost_usd > budget_remaining {
            return PreflightVerdict::Reject {
                reason: format!(
                    "Estimated cost ${:.3} exceeds remaining budget ${:.3}",
                    self.estimated_cost_usd, budget_remaining
                ),
            };
        }

        if self.confidence < 0.3 {
            return PreflightVerdict::Reroute {
                reason: format!(
                    "Low confidence ({:.0}%) — consider a different agent",
                    self.confidence * 100.0
                ),
            };
        }

        if agent_success_rate < 0.4 {
            return PreflightVerdict::Reroute {
                reason: format!(
                    "Agent success rate ({:.0}%) below threshold — reroute to better-suited agent",
                    agent_success_rate * 100.0
                ),
            };
        }

        PreflightVerdict::Proceed
    }

    /// Build an LLM prompt for pre-flight assessment.
    pub fn assessment_prompt(subject: &str, description: &str) -> String {
        format!(
            "Assess this task before execution. Estimate difficulty, cost, and risks.\n\n\
             Task: {subject}\n\
             Description: {description}\n\n\
             Respond with ONLY valid JSON using this exact schema:\n\
             {{\n\
               \"approach\": \"<one sentence>\",\n\
               \"difficulty\": \"<trivial|easy|medium|hard|uncertain>\",\n\
               \"estimated_cost_usd\": <number>,\n\
               \"estimated_turns\": <integer>,\n\
               \"confidence\": <0.0 to 1.0>,\n\
               \"risks\": [\"<risk>\", \"<risk>\"]\n\
             }}\n\
             Do not classify the task into a named pipeline tier. Sigil uses one adaptive execution pipeline for all tasks."
        )
    }

    /// Returns the default execution skill for project work.
    pub fn adaptive_pipeline_skill(&self) -> &'static str {
        ADAPTIVE_PIPELINE_SKILL
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
    fn test_preflight_parse() {
        let text = "APPROACH: Modify the auth middleware\nDIFFICULTY: medium\nCOST: 0.05\nTURNS: 8\nCONFIDENCE: 0.85\nRISKS: token expiry, session management";
        let assessment = PreflightAssessment::parse(text);
        assert_eq!(assessment.approach, "Modify the auth middleware");
        assert_eq!(assessment.estimated_difficulty, Difficulty::Medium);
        assert!((assessment.estimated_cost_usd - 0.05).abs() < 0.001);
        assert_eq!(assessment.estimated_turns, 8);
        assert!((assessment.confidence - 0.85).abs() < 0.001);
        assert_eq!(assessment.risks.len(), 2);
    }

    #[test]
    fn test_preflight_parse_json() {
        let text = r#"{
            "approach": "Modify the auth middleware",
            "difficulty": "medium",
            "estimated_cost_usd": 0.05,
            "estimated_turns": 8,
            "confidence": 0.85,
            "risks": ["token expiry", "session management"]
        }"#;
        let assessment = PreflightAssessment::parse(text);
        assert_eq!(assessment.approach, "Modify the auth middleware");
        assert_eq!(assessment.estimated_difficulty, Difficulty::Medium);
        assert!((assessment.estimated_cost_usd - 0.05).abs() < 0.001);
        assert_eq!(assessment.estimated_turns, 8);
        assert!((assessment.confidence - 0.85).abs() < 0.001);
        assert_eq!(assessment.risks, vec!["token expiry", "session management"]);
    }

    #[test]
    fn test_adaptive_pipeline_skill() {
        let a = PreflightAssessment::parse("APPROACH: test");
        assert_eq!(a.adaptive_pipeline_skill(), ADAPTIVE_PIPELINE_SKILL);
    }

    #[test]
    fn test_evaluate_budget_reject() {
        let assessment = PreflightAssessment {
            approach: "test".to_string(),
            estimated_difficulty: Difficulty::Hard,
            estimated_cost_usd: 1.0,
            estimated_turns: 20,
            confidence: 0.9,
            risks: vec![],
        };
        let verdict = assessment.evaluate(0.5, 0.8);
        assert!(matches!(verdict, PreflightVerdict::Reject { .. }));
    }

    #[test]
    fn test_evaluate_low_confidence_reroute() {
        let assessment = PreflightAssessment {
            approach: "test".to_string(),
            estimated_difficulty: Difficulty::Hard,
            estimated_cost_usd: 0.01,
            estimated_turns: 5,
            confidence: 0.2,
            risks: vec![],
        };
        let verdict = assessment.evaluate(10.0, 0.8);
        assert!(matches!(verdict, PreflightVerdict::Reroute { .. }));
    }

    #[test]
    fn test_evaluate_proceed() {
        let assessment = PreflightAssessment {
            approach: "test".to_string(),
            estimated_difficulty: Difficulty::Easy,
            estimated_cost_usd: 0.01,
            estimated_turns: 3,
            confidence: 0.9,
            risks: vec![],
        };
        let verdict = assessment.evaluate(10.0, 0.8);
        assert_eq!(verdict, PreflightVerdict::Proceed);
    }

    #[test]
    fn test_difficulty_enum_serde() {
        let json = serde_json::to_string(&Difficulty::Hard).unwrap();
        assert_eq!(json, "\"hard\"");
        let parsed: Difficulty = serde_json::from_str("\"easy\"").unwrap();
        assert_eq!(parsed, Difficulty::Easy);
    }
}
