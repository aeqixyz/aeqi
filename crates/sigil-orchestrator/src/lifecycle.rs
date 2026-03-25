//! Agent Lifecycle Engine — autonomous processes that make agents permanently alive.
//!
//! Each process is a single `provider.chat()` call — no agent loop, no tools, no sub-agents.
//! Cheap model (MiniMax/Haiku), 1500-2000 token budget per call.
//!
//! Three process kinds, gated by agent bond level:
//! - MemoryConsolidation (bond 0): consolidate MEMORY.md
//! - Evolution (bond 3): evolve personality, write EVOLUTION.md
//! - ProactiveScan (bond 5+): audit projects, create tasks, propose ideas
//!   - Per-project mode (bond 5): concrete improvements
//!   - Cross-project mode (bond 8): creative ideation, always dispatched for review

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use sigil_core::traits::{
    ChatRequest, Memory, MemoryCategory, MemoryScope, Message, MessageContent, Provider, Role,
};

use crate::cost_ledger::CostLedger;
use crate::emotional_state::EmotionalState;
use crate::message::{Dispatch, DispatchBus, DispatchKind};
use crate::registry::ProjectRegistry;

// ──────────────────────────────────────────────────────────────
// Bond level derivation
// ──────────────────────────────────────────────────────────────

/// Derive bond level from interaction count.
/// 0-49=0, 50-99=1, 100-199=2, 200-299=3, ..., 800+=9+.
pub fn interaction_count_to_bond_level(count: u64) -> u32 {
    if count < 50 {
        0
    } else if count < 100 {
        1
    } else {
        let level = 2 + ((count - 100) / 100) as u32;
        level.min(10)
    }
}

// ──────────────────────────────────────────────────────────────
// Lifecycle state persistence (one file per agent, shared by all processes)
// ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct LifecycleState {
    last_run: HashMap<String, i64>,
}

impl LifecycleState {
    fn load(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(state) = serde_json::from_str(&content)
        {
            return state;
        }
        Self::default()
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────
// Improvement registry (dedup for ProactiveScan)
// ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImprovementStatus {
    Proposed,
    TaskCreated,
    Dismissed,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement {
    pub subject: String,
    pub status: ImprovementStatus,
    pub proposed_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ImprovementRegistry {
    pub improvements: Vec<Improvement>,
}

impl ImprovementRegistry {
    pub fn load(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(reg) = serde_json::from_str(&content)
        {
            return reg;
        }
        Self::default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn already_proposed(&self, subject: &str) -> bool {
        let prefix: String = subject.chars().take(40).collect();
        self.improvements.iter().any(|i| {
            let ip: String = i.subject.chars().take(40).collect();
            ip.to_lowercase() == prefix.to_lowercase()
        })
    }

    pub fn add(&mut self, subject: &str, status: ImprovementStatus) {
        self.improvements.push(Improvement {
            subject: subject.to_string(),
            status,
            proposed_at: Utc::now().timestamp(),
        });
        if self.improvements.len() > 50 {
            self.improvements = self.improvements.split_off(self.improvements.len() - 50);
        }
    }
}

// ──────────────────────────────────────────────────────────────
// Unified LifecycleProcess — one struct, kind enum
// ──────────────────────────────────────────────────────────────

/// Per-project context for proactive scanning.
pub struct ScanProject {
    pub name: String,
    pub prefix: String,
    pub project_dir: PathBuf,
    pub repo_path: Option<PathBuf>,
}

/// What this process does.
pub enum ProcessKind {
    /// Consolidate MEMORY.md — deduplicate, compress, merge.
    MemoryConsolidation { memory: Option<Arc<dyn Memory>> },
    /// Evolve personality — append to EVOLUTION.md, micro-adjust IDENTITY.md.
    Evolution,
    /// Audit projects, create tasks or dispatch proposals.
    /// When `cross_project` is true: reads all project knowledge, always dispatches
    /// (replaces the old CreativeIdeation). Uses the same improvement registry for dedup.
    ProactiveScan {
        projects: Vec<ScanProject>,
        /// All project knowledge summaries (only used when cross_project=true).
        project_knowledge: HashMap<String, String>,
        registry: Arc<ProjectRegistry>,
        dispatch_bus: Arc<DispatchBus>,
        system_leader: String,
        cross_project: bool,
    },
}

impl ProcessKind {
    fn name(&self) -> &str {
        match self {
            Self::MemoryConsolidation { .. } => "memory_consolidation",
            Self::Evolution => "evolution",
            Self::ProactiveScan { cross_project, .. } => {
                if *cross_project {
                    "creative_ideation"
                } else {
                    "proactive_scan"
                }
            }
        }
    }

    fn required_bond(&self) -> u32 {
        match self {
            Self::MemoryConsolidation { .. } => 0,
            Self::Evolution => 3,
            Self::ProactiveScan { cross_project, .. } => {
                if *cross_project {
                    8
                } else {
                    5
                }
            }
        }
    }
}

/// A single lifecycle process. Shared fields + kind-specific behavior.
pub struct LifecycleProcess {
    pub agent_name: String,
    pub agent_dir: PathBuf,
    pub provider: Arc<dyn Provider>,
    pub model: String,
    pub kind: ProcessKind,
    pub interval_secs: u64,
    last_run: Option<std::time::Instant>,
    /// Cached lifecycle state (loaded once, updated in-memory).
    cached_state: Option<LifecycleState>,
}

impl LifecycleProcess {
    pub fn new(
        agent_name: String,
        agent_dir: PathBuf,
        provider: Arc<dyn Provider>,
        model: String,
        kind: ProcessKind,
        interval_secs: u64,
    ) -> Self {
        Self {
            agent_name,
            agent_dir,
            provider,
            model,
            kind,
            interval_secs,
            last_run: None,
            cached_state: None,
        }
    }

    fn state_path(&self) -> PathBuf {
        self.agent_dir.join(".sigil/lifecycle-state.json")
    }

    fn process_name(&self) -> &str {
        self.kind.name()
    }

    fn required_bond(&self) -> u32 {
        self.kind.required_bond()
    }

    /// Check if this process is due to run.
    fn is_due(&mut self) -> bool {
        if let Some(last) = self.last_run {
            return last.elapsed().as_secs() >= self.interval_secs;
        }
        // Compute these before the mutable borrow on cached_state.
        let path = self.state_path();
        let name = self.kind.name().to_string();
        let state = self
            .cached_state
            .get_or_insert_with(|| LifecycleState::load(&path));
        if let Some(&ts) = state.last_run.get(&name) {
            let elapsed = Utc::now().timestamp() - ts;
            return elapsed >= self.interval_secs as i64;
        }
        true
    }

    /// Persist last run timestamp to disk and in-memory cache.
    fn persist_last_run(&mut self) {
        let path = self.state_path();
        let name = self.kind.name().to_string();
        let state = self
            .cached_state
            .get_or_insert_with(|| LifecycleState::load(&path));
        state.last_run.insert(name, Utc::now().timestamp());
        let _ = state.save(&path);
        self.last_run = Some(std::time::Instant::now());
    }

    /// Execute a single LLM call with the given prompt.
    async fn chat(&self, prompt: &str, max_tokens: u32, temperature: f32) -> Result<(String, f64)> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text(prompt),
            }],
            tools: vec![],
            max_tokens,
            temperature,
        };
        let response = self.provider.chat(&request).await?;
        let cost = sigil_providers::estimate_cost(
            &self.model,
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
        );
        let text = response.content.unwrap_or_default();
        Ok((text, cost))
    }

    /// Run this process. Returns (summary, cost_usd).
    pub async fn run(&mut self, dry_run: bool) -> Result<(String, f64)> {
        match &self.kind {
            ProcessKind::MemoryConsolidation { .. } => self.run_memory_consolidation(dry_run).await,
            ProcessKind::Evolution => self.run_evolution(dry_run).await,
            ProcessKind::ProactiveScan { .. } => self.run_proactive_scan(dry_run).await,
        }
    }

    // ── MemoryConsolidation ─────────────────────────────────

    async fn run_memory_consolidation(&mut self, dry_run: bool) -> Result<(String, f64)> {
        let memory_path = self.agent_dir.join("MEMORY.md");
        let current = std::fs::read_to_string(&memory_path).unwrap_or_default();
        let snippet = truncate_chars(&current, 3000);

        // Read recent SQLite memories (all scopes — Domain, Entity, and System).
        let sqlite_entries = if let ProcessKind::MemoryConsolidation {
            memory: Some(ref mem),
        } = self.kind
        {
            let query = sigil_core::traits::MemoryQuery::new(
                format!("{} recent activity", self.agent_name),
                10,
            );
            match mem.search(&query).await {
                Ok(entries) => entries
                    .iter()
                    .map(|e| {
                        format!(
                            "- [{}] {}: {}",
                            e.scope,
                            e.key,
                            truncate_chars(&e.content, 200)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            }
        } else {
            String::new()
        };

        let prompt = format!(
            "You are the memory consolidation system for agent '{name}'.\n\n\
             ## Current MEMORY.md\n\n{mem}\n\n\
             ## Recent SQLite Memories\n\n{sql}\n\n\
             ## Task\n\n\
             Consolidate, deduplicate, and compress into a clean MEMORY.md.\n\
             Rules:\n\
             - Remove duplicate or redundant entries\n\
             - Merge related observations\n\
             - Keep the most important and recent information\n\
             - Output MUST be under 2000 characters\n\
             - Preserve markdown structure\n\n\
             Output:\n\
             UPDATE MEMORY.md:\n\
             <complete new content>\n\
             END MEMORY.md\n\n\
             If no consolidation is needed, output exactly: NO_CHANGES",
            name = self.agent_name,
            mem = snippet,
            sql = if sqlite_entries.is_empty() {
                "(none)"
            } else {
                &sqlite_entries
            },
        );

        let (text, cost) = self.chat(&prompt, 2000, 0.2).await?;

        let summary = if text.trim() == "NO_CHANGES" {
            "no consolidation needed".to_string()
        } else if let Some(content) = parse_file_update(&text, "MEMORY.md") {
            let content = truncate_chars(&content, 2000);
            if dry_run {
                format!("DRY RUN: would update MEMORY.md ({} chars)", content.len())
            } else {
                std::fs::write(&memory_path, &content)?;
                // Store summary in SQLite.
                if let ProcessKind::MemoryConsolidation {
                    memory: Some(ref mem),
                } = self.kind
                {
                    let key = format!("mem-reflect-{}", Utc::now().timestamp());
                    let _ = mem
                        .store(
                            &key,
                            "Memory consolidated by lifecycle engine",
                            MemoryCategory::Fact,
                            MemoryScope::Entity,
                            None,
                        )
                        .await;
                }
                format!("consolidated MEMORY.md ({} chars)", content.len())
            }
        } else {
            "no valid update parsed".to_string()
        };

        self.persist_last_run();
        Ok((summary, cost))
    }

    // ── Evolution ───────────────────────────────────────────

    async fn run_evolution(&mut self, dry_run: bool) -> Result<(String, f64)> {
        let persona =
            std::fs::read_to_string(self.agent_dir.join("PERSONA.md")).unwrap_or_default();
        let identity_path = self.agent_dir.join("IDENTITY.md");
        let identity = std::fs::read_to_string(&identity_path).unwrap_or_default();
        let memory = std::fs::read_to_string(self.agent_dir.join("MEMORY.md")).unwrap_or_default();
        let evolution_path = self.agent_dir.join("EVOLUTION.md");
        let evolution = std::fs::read_to_string(&evolution_path).unwrap_or_default();
        let emo_path = self.agent_dir.join(".sigil/emotional_state.json");
        let emo_ctx = std::fs::read_to_string(&emo_path).unwrap_or_default();

        let prompt = format!(
            "You are the personality evolution system for agent '{name}'.\n\n\
             ## PERSONA.md (READ-ONLY — never modify)\n\n{persona}\n\n\
             ## Current IDENTITY.md\n\n{identity}\n\n\
             ## MEMORY.md\n\n{memory}\n\n\
             ## Emotional State\n\n{emo}\n\n\
             ## Previous Evolution Log\n\n{evolution}\n\n\
             ## Task\n\n\
             Based on accumulated experience, produce:\n\
             1. A dated evolution entry reflecting on growth, patterns, personality shifts\n\
             2. Optionally, micro-adjustments to IDENTITY.md (expertise list, style descriptors only — \
                NEVER change name, role, or core purpose)\n\n\
             Rules:\n\
             - PERSONA.md is SACRED — never reference changing it\n\
             - Evolution entries: introspective, 2-4 sentences\n\
             - Identity changes must be subtle\n\
             - If EVOLUTION.md would exceed 4000 chars, consolidate old entries first\n\n\
             Output:\n\
             EVOLUTION_ENTRY:\n\
             <dated entry to append>\n\
             END EVOLUTION_ENTRY\n\n\
             Optionally:\n\
             UPDATE IDENTITY.md:\n\
             <complete new content>\n\
             END IDENTITY.md\n\n\
             If no evolution warranted, output exactly: NO_CHANGES",
            name = self.agent_name,
            persona = truncate_chars(&persona, 2000),
            identity = truncate_chars(&identity, 1500),
            memory = truncate_chars(&memory, 1500),
            emo = if emo_ctx.is_empty() {
                "(none)".to_string()
            } else {
                truncate_chars(&emo_ctx, 500)
            },
            evolution = if evolution.is_empty() {
                "(none)".to_string()
            } else {
                truncate_chars(&evolution, 2000)
            },
        );

        let (text, cost) = self.chat(&prompt, 2000, 0.3).await?;

        let summary = if text.trim() == "NO_CHANGES" {
            "no evolution warranted".to_string()
        } else {
            let mut changes = Vec::new();

            if let Some(entry) = parse_block(&text, "EVOLUTION_ENTRY") {
                if dry_run {
                    changes.push(format!("EVOLUTION.md (dry run, {} chars)", entry.len()));
                } else {
                    let mut full = evolution;
                    if full.len() + entry.len() > 4000 {
                        // Truncate at clean paragraph boundary.
                        let keep_from = full.len().saturating_sub(3000);
                        let cut = full[keep_from..]
                            .find("\n\n")
                            .map(|p| keep_from + p + 2)
                            .unwrap_or(keep_from);
                        full = format!(
                            "*(earlier entries consolidated)*\n\n{}\n\n{}",
                            &full[cut..],
                            entry
                        );
                    } else {
                        if !full.is_empty() {
                            full.push_str("\n\n");
                        }
                        full.push_str(&entry);
                    }
                    std::fs::write(&evolution_path, &full)?;
                    changes.push("EVOLUTION.md".to_string());
                }
            }

            if let Some(new_identity) = parse_file_update(&text, "IDENTITY.md") {
                if dry_run {
                    changes.push(format!(
                        "IDENTITY.md (dry run, {} chars)",
                        new_identity.len()
                    ));
                } else {
                    std::fs::write(&identity_path, &new_identity)?;
                    changes.push("IDENTITY.md".to_string());
                }
            }

            if changes.is_empty() {
                "evolution response but no valid updates parsed".to_string()
            } else {
                format!("updated: {}", changes.join(", "))
            }
        };

        self.persist_last_run();
        Ok((summary, cost))
    }

    // ── ProactiveScan (also handles cross-project ideation) ─

    async fn run_proactive_scan(&mut self, dry_run: bool) -> Result<(String, f64)> {
        // Check if cross-project mode.
        let is_cross = matches!(
            &self.kind,
            ProcessKind::ProactiveScan {
                cross_project: true,
                ..
            }
        );

        if is_cross {
            return self.run_cross_project_ideation(dry_run).await;
        }

        let mut total_created = 0usize;
        let mut total_proposed = 0usize;
        let mut total_cost = 0.0f64;

        let agent_name = self.agent_name.clone();
        let project_count = match &self.kind {
            ProcessKind::ProactiveScan { projects, .. } => projects.len(),
            _ => 0,
        };

        for idx in 0..project_count {
            // Re-borrow per iteration to avoid holding across mutable scan_one_project.
            let project_name = match &self.kind {
                ProcessKind::ProactiveScan { projects, .. } => projects[idx].name.clone(),
                _ => unreachable!(),
            };
            match self.scan_one_project(idx, dry_run).await {
                Ok((created, proposed, cost)) => {
                    total_created += created;
                    total_proposed += proposed;
                    total_cost += cost;
                }
                Err(e) => {
                    warn!(agent=%agent_name, project=%project_name, error=%e, "proactive scan failed");
                }
            }
        }

        self.persist_last_run();
        Ok((
            format!(
                "scanned {} projects, created {} tasks, proposed {} ideas",
                project_count, total_created, total_proposed
            ),
            total_cost,
        ))
    }

    async fn scan_one_project(&self, idx: usize, dry_run: bool) -> Result<(usize, usize, f64)> {
        // Extract project data from kind (immutable borrow scoped here).
        let (proj_name, proj_prefix, proj_dir, repo_path) = match &self.kind {
            ProcessKind::ProactiveScan { projects, .. } => {
                let p = &projects[idx];
                (
                    p.name.clone(),
                    p.prefix.clone(),
                    p.project_dir.clone(),
                    p.repo_path.clone(),
                )
            }
            _ => unreachable!(),
        };

        let registry_path = proj_dir.join(".sigil/improvements.json");
        let mut improvement_reg = ImprovementRegistry::load(&registry_path);

        let agents_md = std::fs::read_to_string(proj_dir.join("AGENTS.md")).unwrap_or_default();
        let knowledge_md =
            std::fs::read_to_string(proj_dir.join("KNOWLEDGE.md")).unwrap_or_default();

        // Non-blocking git log.
        let git_log = if let Some(ref rp) = repo_path {
            tokio::process::Command::new("git")
                .args(["log", "--oneline", "-10"])
                .current_dir(rp)
                .output()
                .await
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let prev: Vec<String> = improvement_reg
            .improvements
            .iter()
            .map(|i| {
                format!(
                    "- [{}] {}",
                    serde_json::to_string(&i.status).unwrap_or_default(),
                    i.subject
                )
            })
            .collect();

        let prompt = format!(
            "You are a proactive improvement scanner for project '{name}' (prefix: {prefix}), \
             operating as agent '{agent}'.\n\n\
             ## Project Context (AGENTS.md)\n\n{agents}\n\n\
             ## Project Knowledge (KNOWLEDGE.md)\n\n{knowledge}\n\n\
             ## Recent Git History\n\n{git}\n\n\
             ## Previously Proposed Improvements\n\n{prev}\n\n\
             ## Task\n\n\
             Identify concrete improvements. Focus on:\n\
             - Stale or incomplete work in git history\n\
             - Knowledge gaps, missing docs\n\
             - Potential bugs, security issues, performance\n\
             - Missing tests or monitoring\n\n\
             Rules:\n\
             - Do NOT re-propose anything in 'Previously Proposed'\n\
             - Max 3 proposals, each concrete and actionable\n\
             - Include confidence score (0.0-1.0)\n\n\
             Respond with ONLY valid JSON using this exact schema:\n\
             {{\n\
               \"proposals\": [\n\
                 {{\n\
                   \"subject\": \"<one-line title>\",\n\
                   \"description\": \"<2-3 sentences>\",\n\
                   \"confidence\": <0.0-1.0>\n\
                 }}\n\
               ]\n\
             }}\n\
             If nothing needed, return {{\"proposals\": []}}.",
            name = proj_name,
            prefix = proj_prefix,
            agent = self.agent_name,
            agents = truncate_chars(&agents_md, 2000),
            knowledge = truncate_chars(&knowledge_md, 2000),
            git = truncate_chars(&git_log, 1000),
            prev = if prev.is_empty() {
                "(none)".to_string()
            } else {
                prev.join("\n")
            },
        );

        let (text, cost) = self.chat(&prompt, 1500, 0.2).await?;

        if text.trim() == "NO_GAPS" {
            return Ok((0, 0, cost));
        }

        let proposals = parse_proposals(&text);
        let mut created = 0;
        let mut proposed = 0;

        let ProcessKind::ProactiveScan {
            ref registry,
            ref dispatch_bus,
            ref system_leader,
            ..
        } = self.kind
        else {
            unreachable!()
        };

        for p in proposals {
            if improvement_reg.already_proposed(&p.subject) {
                continue;
            }

            if dry_run {
                info!(agent=%self.agent_name, project=%proj_name, subject=%p.subject,
                    confidence=p.confidence, "DRY RUN: would propose");
                proposed += 1;
                continue;
            }

            if p.confidence >= 0.7 {
                match registry
                    .assign(&proj_name, &p.subject, &p.description)
                    .await
                {
                    Ok(task) => {
                        info!(agent=%self.agent_name, project=%proj_name, task=%task.id,
                            confidence=p.confidence, "proactive scan: auto-created task");
                        improvement_reg.add(&p.subject, ImprovementStatus::TaskCreated);
                        created += 1;
                    }
                    Err(e) => warn!(agent=%self.agent_name, project=%proj_name, error=%e,
                        "proactive scan: failed to create task"),
                }
            } else {
                let dispatch = Dispatch::new_typed(
                    &self.agent_name,
                    system_leader,
                    DispatchKind::TaskProposal {
                        project: proj_name.clone(),
                        prefix: proj_prefix.clone(),
                        subject: p.subject.clone(),
                        description: p.description.clone(),
                        confidence: p.confidence,
                        reasoning: format!("Proactive scan by {}", self.agent_name),
                    },
                );
                dispatch_bus.send(dispatch).await;
                improvement_reg.add(&p.subject, ImprovementStatus::Proposed);
                proposed += 1;
            }
        }

        let _ = improvement_reg.save(&registry_path);
        Ok((created, proposed, cost))
    }

    async fn run_cross_project_ideation(&mut self, dry_run: bool) -> Result<(String, f64)> {
        let memory = std::fs::read_to_string(self.agent_dir.join("MEMORY.md")).unwrap_or_default();
        let evolution =
            std::fs::read_to_string(self.agent_dir.join("EVOLUTION.md")).unwrap_or_default();

        // Load ideas registry for anti-spam.
        let ideas_path = self.agent_dir.join(".sigil/improvements.json");
        let mut reg = ImprovementRegistry::load(&ideas_path);

        let ProcessKind::ProactiveScan {
            ref project_knowledge,
            ref dispatch_bus,
            ref system_leader,
            ..
        } = self.kind
        else {
            unreachable!()
        };

        let knowledge_summary: String = project_knowledge
            .iter()
            .map(|(name, k)| format!("### {name}\n{}", truncate_chars(k, 500)))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prev: Vec<String> = reg
            .improvements
            .iter()
            .map(|i| format!("- {}", i.subject))
            .collect();

        let prompt = format!(
            "You are the creative ideation system for agent '{name}'.\n\n\
             ## Agent Memory\n\n{memory}\n\n\
             ## Agent Evolution\n\n{evolution}\n\n\
             ## Project Knowledge\n\n{knowledge}\n\n\
             ## Previously Proposed (do NOT repeat)\n\n{prev}\n\n\
             ## Task\n\n\
             Generate creative, high-value ideas:\n\
             - New features for existing projects\n\
             - Experiments or research directions\n\
             - Cross-project synergies\n\
             - Process improvements\n\n\
             Rules: max 2 ideas, novel, specific, actionable.\n\n\
             Respond with ONLY valid JSON using this exact schema:\n\
             {{\n\
               \"proposals\": [\n\
                 {{\n\
                   \"subject\": \"<title>\",\n\
                   \"description\": \"<3-5 sentences>\",\n\
                   \"confidence\": <0.0-1.0>\n\
                 }}\n\
               ]\n\
             }}\n\
             If nothing novel, return {{\"proposals\": []}}.",
            name = self.agent_name,
            memory = truncate_chars(&memory, 1000),
            evolution = truncate_chars(&evolution, 500),
            knowledge = truncate_chars(&knowledge_summary, 2000),
            prev = if prev.is_empty() {
                "(none)".to_string()
            } else {
                prev.join("\n")
            },
        );

        let (text, cost) = self.chat(&prompt, 1500, 0.5).await?;

        if text.trim() == "NO_GAPS" {
            self.persist_last_run();
            return Ok(("no novel ideas".to_string(), cost));
        }

        let proposals = parse_proposals(&text);
        let mut dispatched = 0;

        for p in proposals {
            if reg.already_proposed(&p.subject) {
                continue;
            }

            if dry_run {
                info!(agent=%self.agent_name, subject=%p.subject, "DRY RUN: would dispatch idea");
                dispatched += 1;
                continue;
            }

            let dispatch = Dispatch::new_typed(
                &self.agent_name,
                system_leader,
                DispatchKind::TaskProposal {
                    project: String::new(),
                    prefix: String::new(),
                    subject: p.subject.clone(),
                    description: p.description.clone(),
                    confidence: p.confidence,
                    reasoning: format!("Creative ideation by {}", self.agent_name),
                },
            );
            dispatch_bus.send(dispatch).await;
            reg.add(&p.subject, ImprovementStatus::Proposed);
            dispatched += 1;
        }

        let _ = reg.save(&ideas_path);
        self.persist_last_run();
        Ok((format!("dispatched {} ideas", dispatched), cost))
    }
}

// ──────────────────────────────────────────────────────────────
// LifecycleEngine — owns all processes, ticks them
// ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct LifecycleEngine {
    pub processes: Vec<LifecycleProcess>,
    pub bond_levels: HashMap<String, u32>,
    /// Agent dirs for refreshing bond levels from emotional state.
    pub agent_dirs: HashMap<String, PathBuf>,
    /// Cost ledger for recording lifecycle LLM spend.
    pub cost_ledger: Option<Arc<CostLedger>>,
    pub dry_run: bool,
}

impl LifecycleEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_bond_level(&mut self, agent: &str, level: u32) {
        self.bond_levels.insert(agent.to_string(), level);
    }

    pub fn add_process(&mut self, process: LifecycleProcess) {
        self.processes.push(process);
    }

    /// Refresh bond levels from emotional state files on disk.
    fn refresh_bond_levels(&mut self) {
        for (agent, dir) in &self.agent_dirs {
            let emo_path = EmotionalState::path_for_agent(dir);
            let emo = EmotionalState::load(&emo_path, agent);
            let bond = interaction_count_to_bond_level(emo.interaction_count);
            self.bond_levels.insert(agent.clone(), bond);
        }
    }

    /// Tick all due processes. Refreshes bond levels first.
    pub async fn tick(&mut self) -> Vec<LifecycleResult> {
        self.refresh_bond_levels();

        let mut results = Vec::new();
        let dry_run = self.dry_run;

        for process in self.processes.iter_mut() {
            let bond = self
                .bond_levels
                .get(&process.agent_name)
                .copied()
                .unwrap_or(0);
            if bond < process.required_bond() {
                continue;
            }
            if !process.is_due() {
                continue;
            }

            let name = process.process_name().to_string();
            let agent = process.agent_name.clone();

            match process.run(dry_run).await {
                Ok((summary, cost)) => {
                    // Record cost in ledger.
                    if cost > 0.0
                        && let Some(ref ledger) = self.cost_ledger
                    {
                        let entry = crate::cost_ledger::CostEntry {
                            project: "lifecycle".to_string(),
                            task_id: format!("{}:{}", agent, name),
                            worker: format!("lifecycle:{}", name),
                            cost_usd: cost,
                            turns: 1,
                            timestamp: Utc::now(),
                            source: "openrouter".to_string(),
                            tokens: 0,
                        };
                        let _ = ledger.record(entry);
                    }
                    results.push(LifecycleResult {
                        agent,
                        process: name,
                        summary,
                        cost_usd: cost,
                        error: None,
                    });
                }
                Err(e) => {
                    results.push(LifecycleResult {
                        agent,
                        process: name,
                        summary: String::new(),
                        cost_usd: 0.0,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        results
    }

    /// Get process states for IPC / status display.
    pub fn status(&self) -> serde_json::Value {
        let processes: Vec<serde_json::Value> = self
            .processes
            .iter()
            .map(|p| {
                let bond = self.bond_levels.get(&p.agent_name).copied().unwrap_or(0);
                serde_json::json!({
                    "agent": p.agent_name,
                    "process": p.process_name(),
                    "required_bond": p.required_bond(),
                    "current_bond": bond,
                    "gated": bond < p.required_bond(),
                })
            })
            .collect();

        serde_json::json!({
            "ok": true,
            "dry_run": self.dry_run,
            "bond_levels": self.bond_levels,
            "process_count": self.processes.len(),
            "processes": processes,
        })
    }
}

/// Result of a single lifecycle process execution.
#[derive(Debug)]
pub struct LifecycleResult {
    pub agent: String,
    pub process: String,
    pub summary: String,
    pub cost_usd: f64,
    pub error: Option<String>,
}

// ──────────────────────────────────────────────────────────────
// Parsing helpers
// ──────────────────────────────────────────────────────────────

fn truncate_chars(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let safe = max.saturating_sub(30);
    let cut = s[..safe].rfind('\n').unwrap_or(safe);
    format!("{}\n[...truncated]", &s[..cut])
}

fn parse_file_update(text: &str, filename: &str) -> Option<String> {
    let marker = format!("UPDATE {filename}:");
    let end_marker = format!("END {filename}");
    let start = text.find(&marker)?;
    let after = &text[start + marker.len()..];
    let after = after.strip_prefix('\n').unwrap_or(after);
    let end = after.find(&end_marker)?;
    let content = after[..end].trim().to_string();
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

fn parse_block(text: &str, block_name: &str) -> Option<String> {
    let start_marker = format!("{block_name}:");
    let end_marker = format!("END {block_name}");
    let start = text.find(&start_marker)?;
    let after = &text[start + start_marker.len()..];
    let after = after.strip_prefix('\n').unwrap_or(after);
    let end = after.find(&end_marker)?;
    let content = after[..end].trim().to_string();
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

#[derive(Debug, Deserialize)]
struct Proposal {
    subject: String,
    description: String,
    confidence: f32,
}

fn parse_proposals(text: &str) -> Vec<Proposal> {
    if let Some(parsed) = parse_proposals_json(text) {
        return parsed;
    }

    let mut proposals = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let remaining = &text[pos..];
        let Some(start) = remaining.find("PROPOSAL:") else {
            break;
        };
        let after = &remaining[start + 9..];
        let Some(end) = after.find("END PROPOSAL") else {
            break;
        };
        if let Some(p) = parse_proposal_block(&after[..end]) {
            proposals.push(p);
        }
        pos += start + 9 + end + 12;
    }
    proposals
}

fn parse_proposals_json(text: &str) -> Option<Vec<Proposal>> {
    #[derive(Deserialize)]
    struct ProposalPayload {
        #[serde(default)]
        proposals: Vec<Proposal>,
    }

    let candidate = extract_json_block(text)?;
    let proposals = if let Ok(payload) = serde_json::from_str::<ProposalPayload>(candidate) {
        payload.proposals
    } else if let Ok(list) = serde_json::from_str::<Vec<Proposal>>(candidate) {
        list
    } else {
        return None;
    };

    Some(filter_proposals(proposals))
}

fn extract_json_block(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return Some(trimmed);
    }
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + "```json".len()..];
        let after = after.strip_prefix('\n').unwrap_or(after).trim();
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim());
        }
    }
    if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.rfind(']'))
        && start < end
    {
        return Some(trimmed[start..=end].trim());
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && start < end
    {
        return Some(trimmed[start..=end].trim());
    }
    None
}

fn parse_proposal_block(block: &str) -> Option<Proposal> {
    let mut subject = String::new();
    let mut description_lines: Vec<String> = Vec::new();
    let mut confidence: f32 = 0.0;
    let mut in_description = false;

    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(val) = line.strip_prefix("Subject:") {
            subject = val.trim().to_string();
            in_description = false;
        } else if let Some(val) = line.strip_prefix("Description:") {
            let val = val.trim();
            if !val.is_empty() {
                description_lines.push(val.to_string());
            }
            in_description = true;
        } else if let Some(val) = line.strip_prefix("Confidence:") {
            confidence = val.trim().parse().unwrap_or(0.0);
            in_description = false;
        } else if in_description {
            description_lines.push(line.to_string());
        }
    }

    if subject.is_empty() || confidence < 0.5 {
        return None;
    }
    Some(Proposal {
        subject,
        description: description_lines.join(" "),
        confidence,
    })
}

fn filter_proposals(proposals: Vec<Proposal>) -> Vec<Proposal> {
    proposals
        .into_iter()
        .filter(|proposal| !proposal.subject.trim().is_empty() && proposal.confidence >= 0.5)
        .collect()
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bond_level_derivation() {
        assert_eq!(interaction_count_to_bond_level(0), 0);
        assert_eq!(interaction_count_to_bond_level(49), 0);
        assert_eq!(interaction_count_to_bond_level(50), 1);
        assert_eq!(interaction_count_to_bond_level(99), 1);
        assert_eq!(interaction_count_to_bond_level(100), 2);
        assert_eq!(interaction_count_to_bond_level(199), 2);
        assert_eq!(interaction_count_to_bond_level(200), 3);
        assert_eq!(interaction_count_to_bond_level(400), 5);
        assert_eq!(interaction_count_to_bond_level(700), 8);
        assert_eq!(interaction_count_to_bond_level(800), 9);
        assert_eq!(interaction_count_to_bond_level(5000), 10);
    }

    #[test]
    fn test_parse_file_update() {
        let text = "UPDATE MEMORY.md:\nLine 1\nLine 2\nEND MEMORY.md";
        assert_eq!(
            parse_file_update(text, "MEMORY.md"),
            Some("Line 1\nLine 2".to_string())
        );
    }

    #[test]
    fn test_parse_file_update_no_match() {
        assert!(parse_file_update("NO_CHANGES", "MEMORY.md").is_none());
    }

    #[test]
    fn test_parse_block() {
        let text = "EVOLUTION_ENTRY:\n## 2026-03-03\nGrew wiser.\nEND EVOLUTION_ENTRY";
        assert_eq!(
            parse_block(text, "EVOLUTION_ENTRY"),
            Some("## 2026-03-03\nGrew wiser.".to_string())
        );
    }

    #[test]
    fn test_parse_proposals() {
        let text = "PROPOSAL:\nSubject: Add monitoring\nDescription: Add health checks.\nConfidence: 0.85\nEND PROPOSAL";
        let proposals = parse_proposals(text);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].subject, "Add monitoring");
        assert!((proposals[0].confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_parse_proposals_multiple() {
        let text = "PROPOSAL:\nSubject: A\nDescription: Do A.\nConfidence: 0.9\nEND PROPOSAL\n\
                    PROPOSAL:\nSubject: B\nDescription: Do B.\nConfidence: 0.75\nEND PROPOSAL";
        assert_eq!(parse_proposals(text).len(), 2);
    }

    #[test]
    fn test_parse_proposals_json_object() {
        let text = r#"{
            "proposals": [
                {
                    "subject": "Add monitoring",
                    "description": "Add health checks and alerting.",
                    "confidence": 0.85
                }
            ]
        }"#;
        let proposals = parse_proposals(text);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].subject, "Add monitoring");
        assert_eq!(proposals[0].description, "Add health checks and alerting.");
        assert!((proposals[0].confidence - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_proposals_json_array() {
        let text = r#"[
            {
                "subject": "A",
                "description": "Do A.",
                "confidence": 0.9
            },
            {
                "subject": "B",
                "description": "Do B.",
                "confidence": 0.75
            }
        ]"#;
        assert_eq!(parse_proposals(text).len(), 2);
    }

    #[test]
    fn test_parse_proposals_low_confidence_filtered() {
        let text =
            "PROPOSAL:\nSubject: Maybe\nDescription: Unclear.\nConfidence: 0.3\nEND PROPOSAL";
        assert!(parse_proposals(text).is_empty());
    }

    #[test]
    fn test_improvement_registry_dedup() {
        let mut reg = ImprovementRegistry::default();
        reg.add(
            "Add monitoring for API endpoints",
            ImprovementStatus::Proposed,
        );
        assert!(reg.already_proposed("Add monitoring for API endpoints"));
        assert!(!reg.already_proposed("Something completely different"));
    }

    #[test]
    fn test_improvement_registry_cap() {
        let mut reg = ImprovementRegistry::default();
        for i in 0..60 {
            reg.add(&format!("Item {i}"), ImprovementStatus::Proposed);
        }
        assert_eq!(reg.improvements.len(), 50);
    }

    #[test]
    fn test_truncate_chars() {
        assert_eq!(truncate_chars("short", 100), "short");
        let long = "line one\nline two\nline three\nline four";
        let result = truncate_chars(long, 20);
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_lifecycle_state_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("state.json");
        let mut state = LifecycleState::default();
        state.last_run.insert("evolution".to_string(), 12345);
        state.save(&path).unwrap();
        let loaded = LifecycleState::load(&path);
        assert_eq!(loaded.last_run.get("evolution"), Some(&12345));
    }

    #[test]
    fn test_improvement_registry_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("improvements.json");
        let mut reg = ImprovementRegistry::default();
        reg.add("Fix the thing", ImprovementStatus::TaskCreated);
        reg.save(&path).unwrap();
        let loaded = ImprovementRegistry::load(&path);
        assert_eq!(loaded.improvements.len(), 1);
        assert_eq!(
            loaded.improvements[0].status,
            ImprovementStatus::TaskCreated
        );
    }

    #[test]
    fn test_lifecycle_engine_status() {
        let mut engine = LifecycleEngine::new();
        engine.set_bond_level("beta", 5);
        let status = engine.status();
        assert_eq!(status["ok"], true);
        assert_eq!(status["process_count"], 0);
        assert_eq!(status["bond_levels"]["beta"], 5);
    }

    #[test]
    fn test_process_kind_names_and_bonds() {
        let mem = ProcessKind::MemoryConsolidation { memory: None };
        assert_eq!(mem.name(), "memory_consolidation");
        assert_eq!(mem.required_bond(), 0);

        let evo = ProcessKind::Evolution;
        assert_eq!(evo.name(), "evolution");
        assert_eq!(evo.required_bond(), 3);
    }

    #[test]
    fn test_engine_default() {
        let engine = LifecycleEngine::default();
        assert!(engine.processes.is_empty());
        assert!(!engine.dry_run);
        assert!(engine.cost_ledger.is_none());
    }
}
