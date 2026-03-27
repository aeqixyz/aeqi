use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::verification::VerificationResult;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    Prime,
    Frame,
    Act,
    Verify,
    Commit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSessionStatus {
    Created,
    Running,
    Completed,
    Blocked,
    Handoff,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub id: String,
    pub phase: RuntimePhase,
    pub summary: String,
    pub status: StepStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    GitCommit,
    GitBranch,
    Worktree,
    Checkpoint,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub label: String,
    pub reference: String,
}

impl Artifact {
    pub fn new(kind: ArtifactKind, label: impl Into<String>, reference: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            reference: reference.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationReport {
    pub checks_run: Vec<String>,
    pub confidence: Option<f32>,
    pub approved: Option<bool>,
    pub warnings: Vec<String>,
    pub evidence_summary: Vec<String>,
}

impl From<&VerificationResult> for VerificationReport {
    fn from(value: &VerificationResult) -> Self {
        let mut checks_run = Vec::new();
        let mut warnings = value.suggestions.clone();
        let mut evidence_summary = Vec::new();

        if !value.signals.is_empty() {
            checks_run.push(format!("signals: {}", value.signals.len()));
        }

        if let Some(ref evidence) = value.evidence {
            if evidence.test_exit_code.is_some() {
                checks_run.push("test_runner".to_string());
            }
            if !evidence.files_changed.is_empty() {
                checks_run.push("git_diff".to_string());
                evidence_summary.push(format!(
                    "files changed: {}",
                    evidence.files_changed.join(", ")
                ));
            }
            if let Some(code) = evidence.test_exit_code {
                evidence_summary.push(format!("test exit code: {code}"));
            }
            if evidence.lines_added > 0 || evidence.lines_removed > 0 {
                evidence_summary.push(format!(
                    "diff stats: +{} -{}",
                    evidence.lines_added, evidence.lines_removed
                ));
            }
        }

        if !value.approved {
            warnings.push(value.reason.clone());
        }

        Self {
            checks_run,
            confidence: Some(value.confidence),
            approved: Some(value.approved),
            warnings,
            evidence_summary,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOutcomeStatus {
    Done,
    Blocked,
    Handoff,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeOutcome {
    pub status: RuntimeOutcomeStatus,
    pub summary: String,
    pub reason: Option<String>,
    pub next_action: Option<String>,
    pub artifacts: Vec<Artifact>,
    pub verification: Option<VerificationReport>,
}

impl RuntimeOutcome {
    pub fn done(summary: impl Into<String>, artifacts: Vec<Artifact>) -> Self {
        Self::new(RuntimeOutcomeStatus::Done, summary, None, None, artifacts)
    }

    pub fn blocked(
        summary: impl Into<String>,
        reason: impl Into<String>,
        artifacts: Vec<Artifact>,
    ) -> Self {
        Self::new(
            RuntimeOutcomeStatus::Blocked,
            summary,
            Some(reason.into()),
            Some("await_operator_input".to_string()),
            artifacts,
        )
    }

    pub fn handoff(summary: impl Into<String>, artifacts: Vec<Artifact>) -> Self {
        let summary = summary.into();
        Self::new(
            RuntimeOutcomeStatus::Handoff,
            summary.clone(),
            Some(summary),
            Some("resume_from_checkpoint".to_string()),
            artifacts,
        )
    }

    pub fn failed(summary: impl Into<String>, artifacts: Vec<Artifact>) -> Self {
        let summary = summary.into();
        Self::new(
            RuntimeOutcomeStatus::Failed,
            summary.clone(),
            Some(summary),
            Some("inspect_failure".to_string()),
            artifacts,
        )
    }

    pub fn from_agent_response(result_text: &str, artifacts: Vec<Artifact>) -> Self {
        let trimmed = result_text.trim();

        if let Some(contract) = RuntimeOutcomeContract::parse(trimmed) {
            return Self::from_contract(contract, artifacts);
        }

        Self::from_legacy_text(trimmed, artifacts)
    }

    pub fn artifact_refs(&self) -> Vec<String> {
        self.artifacts
            .iter()
            .map(|artifact| artifact.reference.clone())
            .collect()
    }

    fn new(
        status: RuntimeOutcomeStatus,
        summary: impl Into<String>,
        reason: Option<String>,
        next_action: Option<String>,
        artifacts: Vec<Artifact>,
    ) -> Self {
        Self {
            status,
            summary: summary.into(),
            reason,
            next_action,
            artifacts,
            verification: None,
        }
    }

    fn from_contract(contract: RuntimeOutcomeContract, artifacts: Vec<Artifact>) -> Self {
        let summary = contract.summary.trim().to_string();
        let summary = if summary.is_empty() {
            contract
                .reason
                .clone()
                .unwrap_or_else(|| "Worker returned empty response".to_string())
        } else {
            summary
        };
        let reason = contract
            .reason
            .map(|reason| reason.trim().to_string())
            .filter(|reason| !reason.is_empty());
        let next_action = contract
            .next_action
            .map(|action| action.trim().to_string())
            .filter(|action| !action.is_empty())
            .or_else(|| Self::default_next_action(contract.status));

        match contract.status {
            RuntimeOutcomeStatus::Done => Self::new(
                RuntimeOutcomeStatus::Done,
                summary,
                None,
                next_action,
                artifacts,
            ),
            RuntimeOutcomeStatus::Blocked => Self::new(
                RuntimeOutcomeStatus::Blocked,
                summary.clone(),
                reason.or_else(|| Some(summary)),
                next_action,
                artifacts,
            ),
            RuntimeOutcomeStatus::Handoff => Self::new(
                RuntimeOutcomeStatus::Handoff,
                summary.clone(),
                reason.or_else(|| Some(summary)),
                next_action,
                artifacts,
            ),
            RuntimeOutcomeStatus::Failed => Self::new(
                RuntimeOutcomeStatus::Failed,
                summary.clone(),
                reason.or_else(|| Some(summary)),
                next_action,
                artifacts,
            ),
        }
    }

    fn from_legacy_text(trimmed: &str, artifacts: Vec<Artifact>) -> Self {
        if trimmed.is_empty() {
            return Self::failed("Worker returned empty response", artifacts);
        }

        let first_line = trimmed
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim();

        if first_line.starts_with("BLOCKED:") {
            let question = if first_line == "BLOCKED:" {
                trimmed.strip_prefix("BLOCKED:").unwrap_or(trimmed).trim()
            } else {
                first_line.strip_prefix("BLOCKED:").unwrap_or("").trim()
            }
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();

            return Self::blocked(trimmed.to_string(), question, artifacts);
        }

        if first_line.starts_with("HANDOFF:") {
            let checkpoint = trimmed
                .strip_prefix("HANDOFF:")
                .unwrap_or(trimmed)
                .trim()
                .to_string();
            return Self::handoff(checkpoint, artifacts);
        }

        if first_line.starts_with("FAILED:") {
            return Self::failed(trimmed.to_string(), artifacts);
        }

        Self::done(trimmed.to_string(), artifacts)
    }

    fn default_next_action(status: RuntimeOutcomeStatus) -> Option<String> {
        match status {
            RuntimeOutcomeStatus::Done => None,
            RuntimeOutcomeStatus::Blocked => Some("await_operator_input".to_string()),
            RuntimeOutcomeStatus::Handoff => Some("resume_from_checkpoint".to_string()),
            RuntimeOutcomeStatus::Failed => Some("inspect_failure".to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeOutcomeContract {
    status: RuntimeOutcomeStatus,
    summary: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    next_action: Option<String>,
}

impl RuntimeOutcomeContract {
    fn parse(text: &str) -> Option<Self> {
        Self::json_candidates(text)
            .into_iter()
            .find_map(|candidate| serde_json::from_str::<Self>(&candidate).ok())
    }

    fn json_candidates(text: &str) -> Vec<String> {
        let trimmed = text.trim();
        let mut candidates = Vec::new();

        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            candidates.push(trimmed.to_string());
        }

        if trimmed.starts_with("```") {
            let mut lines = trimmed.lines();
            let _opening = lines.next();
            let mut fenced = Vec::new();
            for line in lines {
                if line.trim_start().starts_with("```") {
                    break;
                }
                fenced.push(line);
            }
            let fenced = fenced.join("\n");
            let fenced = fenced.trim();
            if fenced.starts_with('{') && fenced.ends_with('}') {
                candidates.push(fenced.to_string());
            }
        }

        if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
            && start < end
        {
            let slice = trimmed[start..=end].trim();
            if slice.starts_with('{') && slice.ends_with('}') {
                candidates.push(slice.to_string());
            }
        }

        candidates
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSession {
    pub session_id: String,
    pub task_id: String,
    pub worker_id: String,
    pub project: String,
    pub model: Option<String>,
    pub status: RuntimeSessionStatus,
    pub phase: RuntimePhase,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub checkpoint_refs: Vec<String>,
    pub steps: Vec<StepRecord>,
}

impl RuntimeSession {
    pub fn new(
        task_id: impl Into<String>,
        worker_id: impl Into<String>,
        project: impl Into<String>,
        model: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            session_id: format!("rt-{}", Uuid::new_v4().simple()),
            task_id: task_id.into(),
            worker_id: worker_id.into(),
            project: project.into(),
            model,
            status: RuntimeSessionStatus::Created,
            phase: RuntimePhase::Prime,
            started_at: now,
            updated_at: now,
            checkpoint_refs: Vec::new(),
            steps: Vec::new(),
        }
    }

    pub fn mark_phase(&mut self, phase: RuntimePhase, summary: impl Into<String>) {
        let now = Utc::now();
        self.phase = phase;
        self.updated_at = now;
        if self.status == RuntimeSessionStatus::Created {
            self.status = RuntimeSessionStatus::Running;
        }
        self.steps.push(StepRecord {
            id: format!("step-{}", self.steps.len() + 1),
            phase,
            summary: summary.into(),
            status: StepStatus::Completed,
            timestamp: now,
        });
    }

    pub fn add_checkpoint_ref(&mut self, reference: impl Into<String>) {
        self.checkpoint_refs.push(reference.into());
        self.updated_at = Utc::now();
    }

    pub fn finish(&mut self, outcome: &RuntimeOutcome) {
        self.phase = RuntimePhase::Commit;
        self.updated_at = Utc::now();
        self.status = match outcome.status {
            RuntimeOutcomeStatus::Done => RuntimeSessionStatus::Completed,
            RuntimeOutcomeStatus::Blocked => RuntimeSessionStatus::Blocked,
            RuntimeOutcomeStatus::Handoff => RuntimeSessionStatus::Handoff,
            RuntimeOutcomeStatus::Failed => RuntimeSessionStatus::Failed,
        };
        self.steps.push(StepRecord {
            id: format!("step-{}", self.steps.len() + 1),
            phase: RuntimePhase::Commit,
            summary: "Committed runtime outcome".to_string(),
            status: StepStatus::Completed,
            timestamp: self.updated_at,
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeExecution {
    pub session: RuntimeSession,
    pub outcome: RuntimeOutcome,
}

#[cfg(test)]
mod tests {
    use super::{RuntimeOutcome, RuntimeOutcomeStatus};

    #[test]
    fn parses_structured_runtime_outcome_json() {
        let runtime = RuntimeOutcome::from_agent_response(
            r#"{"status":"done","summary":"Implemented runtime cards","reason":null,"next_action":null}"#,
            Vec::new(),
        );

        assert_eq!(runtime.status, RuntimeOutcomeStatus::Done);
        assert_eq!(runtime.summary, "Implemented runtime cards");
        assert_eq!(runtime.reason, None);
    }

    #[test]
    fn parses_structured_runtime_outcome_from_code_fence() {
        let runtime = RuntimeOutcome::from_agent_response(
            "```json\n{\"status\":\"blocked\",\"summary\":\"Need API token\",\"reason\":\"Which staging token should I use?\"}\n```",
            Vec::new(),
        );

        assert_eq!(runtime.status, RuntimeOutcomeStatus::Blocked);
        assert_eq!(runtime.summary, "Need API token");
        assert_eq!(
            runtime.reason.as_deref(),
            Some("Which staging token should I use?")
        );
        assert_eq!(runtime.next_action.as_deref(), Some("await_operator_input"));
    }

    #[test]
    fn falls_back_to_legacy_prefixes() {
        let runtime = RuntimeOutcome::from_agent_response(
            "HANDOFF:\nImplemented runtime persistence, remaining: task view rendering.",
            Vec::new(),
        );

        assert_eq!(runtime.status, RuntimeOutcomeStatus::Handoff);
        assert!(runtime.summary.contains("Implemented runtime persistence"));
    }

    #[test]
    fn empty_response_is_failed() {
        let runtime = RuntimeOutcome::from_agent_response("  \n", Vec::new());

        assert_eq!(runtime.status, RuntimeOutcomeStatus::Failed);
        assert_eq!(runtime.summary, "Worker returned empty response");
    }
}
