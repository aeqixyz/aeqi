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
             Respond with EXACTLY this format (one line per field):\n\
             APPROACH: <your planned approach in one sentence>\n\
             DIFFICULTY: <one of: trivial, easy, medium, hard, uncertain>\n\
             COST: <estimated cost in USD, e.g. 0.05>\n\
             TURNS: <estimated number of turns needed>\n\
             CONFIDENCE: <0.0 to 1.0, your confidence in completing this>\n\
             RISKS: <comma-separated risk factors, or empty>"
        )
    }
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
