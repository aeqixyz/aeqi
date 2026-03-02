# System Architecture

## Overview

System is a recursive multi-agent orchestration framework written in Rust. A single binary (`rm`, ~7MB release) coordinates autonomous AI agents across isolated projects. Each project has its own repository, task DAG, memory database, identity files, and worker pool.

The key architectural insight: **workers are orchestrators**. Every worker runs as a full Claude Code CLI instance with unrestricted tool access, including the Task tool for spawning sub-agents. This creates a recursive execution tree where any worker can become a coordinator -- unlike flat frameworks (OpenClaw, Gas Town) that cap at one level of delegation.

## Execution Hierarchy

```
Layer 0: Emperor (Human)
         |
Layer 1: Council of Agents
         +-- AgentRouter (Gemini Flash classifier, ~$0.001/call)
         |   routes incoming messages to relevant advisors
         +-- Kael (Iron Fang) -- financial advisor, algostaking scope
         +-- Mira (Starweaver) -- product advisor, riftdecks-shop/entity-legal scope
         +-- Void (Hollow Saint) -- systems advisor, sigil scope
         |
Layer 2: Lead Agent (Aurelia)
         | Claude Opus, full Claude Code mode
         | Receives council input, routes tasks, Telegram interface
         |
Layer 3: Supervisors (per-project supervisors)
         | Control plane -- no LLM calls
         | Patrol loop: discover ready tasks, spawn workers, timeout detection
         | Budget gating: checks can_afford_project() before spawning
         |
Layer 4: Workers (Claude Code executors)
         | Claude Sonnet, full Claude Code CLI with --permission-mode bypassPermissions
         | Each worker is a `claude -p` subprocess with JSON output
         |
Layer 5: Sub-agents (spawned by workers via Task tool)
         | Recursive -- sub-agents can spawn their own sub-agents
         +-- Depth limited only by cost budget
```

## Crate Dependency Graph

```
rm (CLI binary)
 +-- system-core          (traits, config, agent loop, security, identity)
 +-- system-tasks         (task DAG, JSONL storage)
 +-- system-orchestrator  (23 modules -- the brain)
 |    +-- system-core
 |    +-- system-tasks
 |    +-- system-providers (for reflection, gap analysis, agent routing)
 +-- system-memory        (SQLite + FTS5 + vector)
 +-- system-providers     (OpenRouter, Anthropic, Ollama)
 +-- system-gates         (Telegram, Discord, Slack)
 +-- system-tools         (shell, file, git, tasks, delegate)
 +-- system-companions    (gacha system)
```

## system-core

**Path**: `crates/system-core/`

Defines the trait system that makes everything swappable:

| Trait | Purpose | Implementations |
|-------|---------|-----------------|
| `Provider` | LLM chat completion | OpenRouter, Anthropic, Ollama |
| `Tool` | Agent tool execution | Shell, FileRead, FileWrite, Git, Tasks, Delegate, Skill |
| `Memory` | Store/search knowledge | SqliteMemory (FTS5 + vector hybrid) |
| `Observer` | Event logging | LogObserver |
| `Channel` | External messaging | Telegram, Discord, Slack |

**Other modules**:
- `config.rs` -- `SystemConfig` from TOML with `${ENV_VAR}` expansion, `discover()` walks up directory tree
- `identity.rs` -- Loads PERSONA.md + IDENTITY.md + AGENTS.md + KNOWLEDGE.md into layered system prompt
- `security.rs` -- ChaCha20-Poly1305 encrypted `SecretStore` for API keys
- `agent.rs` -- Basic LLM agent loop (used in Agent execution mode, not Claude Code mode)

**Execution modes** (`ExecutionMode` enum):
- `Agent` -- Internal LLM loop: `Provider::chat()` -> tool calls -> repeat
- `ClaudeCode` -- Spawns `claude -p` CLI subprocess with full tool access

## system-tasks

**Path**: `crates/system-tasks/`

Git-native task DAG stored as JSONL files in `.tasks/` per project.

```
.tasks/
  as.jsonl     # AlgoStaking tasks: as-001, as-001.1, as-001.2, ...
  rd.jsonl     # RiftDecks tasks: rd-001, rd-002, ...
```

**Features**:
- Hierarchical IDs: `as-001` (parent), `as-001.1` (child), `as-001.1.1` (grandchild)
- Dependency DAG: `depends_on` / `blocks` with cycle detection
- Priority: Low, Normal, High, Critical
- Status: Pending -> InProgress -> Done | Blocked | Cancelled
- Append-only JSONL: each write appends a new line, periodic compaction
- `TaskBoard`: mutex-guarded store with `ready()` (unblocked + pending), `create()`, `update()`, `close()`

## system-orchestrator

**Path**: `crates/system-orchestrator/`

The brain of System. 23 modules covering execution, coordination, cost control, observability, and self-improvement.

### Execution Pipeline

#### Worker (`worker.rs`)

Ephemeral task executor. Each worker is a tokio task that:

1. Receives a task assignment via its hook
2. Builds context: shared WORKFLOW.md -> PERSONA.md -> IDENTITY.md -> AGENTS.md -> KNOWLEDGE.md -> WORKER_PROTOCOL -> checkpoint context
3. Recalls relevant memories from project memory DB
4. Executes via Claude Code CLI (`claude -p --output-format json --max-turns N`)
5. Parses JSON output -> `TaskOutcome` (Done, Blocked, Failed, Handoff)
6. Records cost + turns in checkpoint
7. Optionally reflects on execution (extracts insights -> memory)
8. Captures external git checkpoint via `WorkerCheckpoint::capture()`

**Context layers** (in injection order):
```
1. Shared WORKFLOW.md      -- base workflow, code standards, R->D->R pipeline
2. PERSONA.md              -- project personality
3. IDENTITY.md             -- name, expertise, repos
4. AGENTS.md               -- project-specific operating instructions
5. KNOWLEDGE.md            -- project knowledge (truncated by ContextBudget)
6. WORKER_PROTOCOL         -- DONE/BLOCKED:/FAILED: output format
7. Checkpoint context      -- predecessor worker's work-in-progress (if any)
8. Memory recall           -- relevant memories from project SQLite
9. Repo CLAUDE.md          -- auto-discovered by Claude Code CLI (via --cwd)
```

**Execution modes**:
- `WorkerExecution::Agent` -- Internal agent loop (Provider + Tools)
- `WorkerExecution::ClaudeCode` -- Claude Code CLI subprocess

**Builder pattern**:
```rust
worker.with_memory(memory_arc)
      .with_reflect(provider_arc, "model-name")
      .with_project_dir(path)
```

#### Supervisor (`supervisor.rs`)

Per-project supervisor. Runs patrol cycles:

```
patrol() {
    1. Reap finished workers (join completed tasks)
    2. Handle outcomes:
       - Done -> close task, record cost, send dispatch
       - Blocked -> escalation chain (project resolver -> lead agent -> human)
       - Failed -> requeue with backoff
       - Handoff -> requeue with checkpoint context
    3. Timeout detection: abort workers exceeding worker_timeout_secs
       - Capture external WorkerCheckpoint before abort
    4. Budget gate: check can_afford_project() before spawning
    5. Spawn workers for ready tasks (up to max_workers)
    6. Report patrol metrics (workers_active, tasks_pending, cycle_seconds)
    7. Send patrol report dispatch to lead agent
}
```

**Escalation chain** (for BLOCKED outcomes):
```
Worker reports BLOCKED: "I need the database schema"
  |
Project resolver (1 attempt): spawn new worker with blocker as task
  | (if still blocked)
Lead agent escalation: dispatch to agent with cross-project context
  | (if still blocked)
Human escalation: Telegram notification to Emperor
```

**Budget gating**: Before spawning any worker, the supervisor checks:
```rust
if let Some(ledger) = &self.cost_ledger {
    if !ledger.can_afford_project(&self.project_name) {
        warn!("budget exhausted for project, skipping spawn");
        return;
    }
}
```

#### Executor (`executor.rs`)

Low-level Claude Code CLI wrapper.

```rust
pub struct ClaudeCodeExecutor {
    workdir: PathBuf,       // --cwd for Claude Code
    max_turns: u32,         // --max-turns
    max_budget_usd: Option<f64>,
    system_prompt: String,  // injected context
}
```

**Features**:
- Retry with exponential backoff (3 attempts, 1s -> 2s -> 4s)
- JSON output parsing (`--output-format json`)
- CLAUDECODE env var stripped to avoid nested-session detection
- `TaskOutcome` parsing from worker's final output:
  - `DONE` / `DONE:` prefix -> `TaskOutcome::Done`
  - `BLOCKED:` prefix -> `TaskOutcome::Blocked`
  - `FAILED:` prefix -> `TaskOutcome::Failed`
  - `HANDOFF:` prefix -> `TaskOutcome::Handoff`

#### Daemon (`daemon.rs`)

Background daemon process. Main loop:

```
daemon.run() {
    loop {
        1. registry.patrol_all()        -- all supervisors patrol in parallel
        2. Check schedule jobs (cron)    -- fire scheduled tasks
        3. Run heartbeat checks          -- periodic heartbeats
        4. Run reflection cycles         -- identity drift detection
        5. Hot reload on SIGHUP          -- re-read system.toml
        6. Persist dispatches + costs    -- JSONL save
        7. Update daily cost gauge       -- metrics
        8. Prune old cost entries         -- 7-day TTL
        9. Sleep patrol_interval_secs
    }
}
```

**IPC socket** at `~/.sigil/rm.sock`:
- `ping` -> `pong`
- `status` -> project counts, worker states, cost summary
- `projects` -> project info JSON
- `dispatches` -> recent dispatch messages
- `metrics` -> Prometheus text exposition
- `cost` -> budget status with per-project breakdown

### Cost & Budget

#### Cost Ledger (`cost_ledger.rs`)

Tracks every worker execution cost:

```rust
CostEntry {
    project: String,          // "algostaking"
    task_id: String,          // "as-001"
    worker: String,           // "as-worker-0"
    cost_usd: f64,            // 0.0342
    turns: u32,               // 7
    timestamp: DateTime<Utc>,
}
```

**Budget enforcement**:
- Global daily cap: `max_cost_per_day_usd` (config)
- Per-project budgets: `project_budgets` map (optional)
- `can_afford()` -> checks global budget
- `can_afford_project(name)` -> checks project budget AND global budget
- `budget_status()` -> `(spent_today, limit, remaining)`
- `project_budget_status(name)` -> per-project breakdown

**Persistence**: JSONL file at `~/.sigil/costs.jsonl`
- `save()` -> append new entries since last save
- `load()` -> restore on startup
- `prune_old()` -> remove entries older than 7 days

**Caching**: `DailyCache` with staleness detection (entry count + 60s TTL)

#### Context Budget (`context_budget.rs`)

Controls worker context window usage:

```rust
ContextBudget {
    max_shared_workflow: 2000,   // chars for WORKFLOW.md
    max_knowledge: 12000,        // chars for KNOWLEDGE.md
    max_checkpoints: 8000,       // chars for checkpoint context
    max_checkpoint_count: 5,     // max checkpoints to include
    max_total: 40000,            // total char budget (~10k tokens)
}
```

- `truncate(text, max)` -> safe truncation at newline boundaries
- `budget_checkpoints(checkpoints)` -> keeps last N verbatim, summarizes older as one-liners

### Observability

#### Metrics (`metrics.rs`)

Zero-dependency Prometheus text exposition format.

**Types**:
- `Counter` -- monotonic (AtomicU64)
- `Gauge` -- bidirectional (AtomicI64, fixed-point x1000 for f64 precision)
- `Histogram` -- pre-defined buckets with running sum/count

**Global metrics**:
```
system_tasks_completed_total{project="..."}
system_tasks_failed_total{project="..."}
system_tasks_blocked_total{project="..."}
system_workers_spawned_total{project="..."}
system_workers_timed_out_total{project="..."}
system_workers_active{project="..."}
system_tasks_pending{project="..."}
system_cost_usd_total{project="..."}
system_patrol_cycles_total{project="..."}
system_worker_duration_seconds{project="...",le="..."}
system_worker_turns{project="...",le="..."}
system_worker_cost_usd{project="...",le="..."}
```

Exposed via IPC socket `metrics` command for Grafana/Prometheus scraping.

#### Checkpoints (`checkpoint.rs`)

External git state capture (inspired by Gas Town's GUPP pattern):

```rust
WorkerCheckpoint {
    task_id: Option<String>,
    worker_name: Option<String>,
    modified_files: Vec<String>,    // git status --porcelain
    last_commit: Option<String>,    // git rev-parse HEAD
    branch: Option<String>,         // git rev-parse --abbrev-ref HEAD
    worktree_path: Option<String>,
    timestamp: DateTime<Utc>,
    session_id: Option<String>,
    progress_notes: Option<String>,
}
```

**Key insight**: Checkpoints are captured **externally** by running git commands against the worker's working directory. Agents are unreliable self-reporters -- git is the source of truth.

**Lifecycle**:
1. Worker times out or produces Handoff outcome
2. Supervisor calls `WorkerCheckpoint::capture(workdir)` -- runs git status/rev-parse
3. Checkpoint saved to `projects/<project>/.sigil/checkpoints/<task_id>.json`
4. Task re-queued with checkpoint context
5. Successor worker receives checkpoint in its context layers

**Staleness**: `is_stale(max_age)` checks timestamp -- stale checkpoints are discarded rather than injected.

#### Dispatch Bus (`dispatch.rs`)

Inter-agent messaging system:

```rust
Dispatch {
    from: String,        // "as-supervisor"
    to: String,          // "agent"
    kind: DispatchKind,  // PatrolReport | WorkerCrashed | TaskProposal | CostAlert | ...
    timestamp: DateTime<Utc>,
}
```

**Implementation**: `HashMap<String, VecDeque<Dispatch>>` indexed by recipient for O(1) lookup.

**Features**:
- TTL expiry: 1 hour default, configurable
- Bounded queues: max 1000 messages per recipient
- JSONL persistence: `~/.sigil/dispatches.jsonl`
- `read(recipient)` -> drain + return all messages
- `unread_count(recipient)` -> peek without consuming

### Self-Improvement

#### Reflection (`reflection.rs`)

Autonomous identity drift detection and self-update:

1. **Fingerprint**: FNV-1a hash of PERSONA.md, IDENTITY.md, AGENTS.md, HEARTBEAT.md, PREFERENCES.md
2. **Detect drift**: Compare current fingerprints to saved state
3. **Feed to LLM**: If drift detected, send file contents (budget-capped at 6k chars) to cheap model
4. **Apply updates**: LLM can update MEMORY.md, HEARTBEAT.md, IDENTITY.md, PREFERENCES.md
5. **Persist state**: Save new fingerprints to `reflection-state.json`

**Budget**: 2k max tokens per reflection, configurable interval.

#### Gap Analysis (`gap_analysis.rs`)

Proactive task generation when project queues empty:

1. Read MEMORY.md + AGENTS.md from project directory
2. Feed to LLM with recent completed tasks as context (max 5, 8k chars)
3. LLM proposes up to 3 `GapProposal` items with confidence scores
4. Confidence >= 0.70 -> auto-create task
5. Confidence < 0.70 -> surface as `DispatchKind::TaskProposal` to lead agent

```rust
GapProposal {
    subject: String,
    description: String,
    priority: Priority,
    confidence: f32,     // 0.0-1.0
    reasoning: String,
}
```

### Multi-Agent Coordination

#### Agent Router (`agent_router.rs`)

Routes incoming messages to relevant council advisors:

```
Message arrives -> Gemini Flash classifier (~$0.001/call)
  -> Returns RouteDecision { advisors: ["kael", "mira"], confidence: 0.85 }
  -> Fan-out: spawn parallel task for each advisor (60s timeout)
  -> Council input injected into Aurelia's context
```

#### Council (`council.rs`)

Forced council debate mode. `/council` command triggers all advisors regardless of router decision:

```
/council "should we add WebSocket support?"
  -> All advisors receive the question
  -> Responses collected with attribution
  -> Aurelia synthesizes with visible debate context
```

**Cost control**: `max_advisor_cost_usd = 0.50`, `advisor_cooldown_secs = 60`

#### Operations (`operations.rs`)

Cross-project operations that track tasks across multiple projects:

```rust
Operation {
    name: String,           // "payment-integration"
    task_ids: Vec<String>,  // ["as-001", "rd-002", "el-003"]
    status: OperationStatus, // Active | Complete | Failed
}
```

When `rm done` closes a task, operation status is updated. Operation completes when all tracked tasks are done.

### Scheduling & Lifecycle

#### Schedule (`schedule.rs`)

Persistent scheduled jobs with cron expressions:

```rust
ScheduleJob {
    name: String,
    project: String,
    prompt: String,
    schedule: CronSchedule,  // "0 */6 * * *" or one-shot timestamp
    isolated: bool,          // use git worktree
}
```

Stored in `~/.sigil/schedule.jsonl`. Evaluated each patrol cycle.

#### Heartbeat (`heartbeat.rs`)

Periodic health checks driven by HEARTBEAT.md:

- Each project's HEARTBEAT.md contains health check instructions
- Supervisor runs heartbeat at configured interval (default 30min)
- Agent executes checks and reports via dispatch

#### Hooks (`hooks.rs`)

Event callbacks that pin work to specific workers:

```rust
Hook {
    worker: String,    // "as-worker-0"
    task_id: String,   // "as-001"
}
```

`rm hook WORKER TASK_ID` creates a hook -- the worker MUST execute that task on next patrol.

#### Session Tracker (`session_tracker.rs`)

Telegram notifications for daemon session lifecycle:

- **Active->Idle**: "Queue empty -- all workers at rest"
- **Idle->Active**: "Workers awakened -- N tasks queued"
- **Sprint check-in**: Periodic progress reports while workers working
- **Idle alarm**: "System idle -- ready for your next command, Architect"
- **Deadline**: One-shot alarm when configured session time elapses
- **Anti-flood**: Min interval between notifications (default 60s)

### Project Registry (`registry.rs`)

Central shared state holding all projects and supervisors:

```rust
ProjectRegistry {
    projects: RwLock<HashMap<String, Arc<Project>>>,
    supervisors: RwLock<HashMap<String, Arc<Mutex<Supervisor>>>>,
    dispatch_bus: Arc<DispatchBus>,
    wake: Arc<Notify>,
    cost_ledger: Arc<CostLedger>,
    metrics: Arc<SystemMetrics>,
}
```

- `register_project()` -> injects cost_ledger + metrics into supervisors
- `patrol_all()` -> parallel patrol across all supervisors via `join_all`
- `assign()` -> create task + wake daemon
- `status()` -> aggregate project statuses + unread dispatches

### Pipeline Templates (`template.rs`)

TOML workflow templates with variable substitution:

```toml
[template]
name = "feature-dev"

[vars]
issue_id = { type = "string", required = true }

[[steps]]
id = "implement"
title = "Implement {{issue_id}}"
needs = ["plan"]
```

- `discover_templates(shared_dir, project_dir)` -> project overrides shared
- `pour(vars)` -> expand variables, return step chain
- Used by `rm mol pour` command

## Data Flow: Task Lifecycle

```
1. rm assign "fix login bug" --rig algostaking
   +-- Registry.assign() -> TaskBoard.create() -> Task{id: "as-042", status: Pending}
       +-- wake.notify_one() -> wakes daemon

2. Daemon patrol cycle
   +-- Registry.patrol_all() -> Supervisor.patrol() for each project

3. Supervisor.patrol() -- algostaking
   +-- Check budget: cost_ledger.can_afford_project("algostaking") -> true
   +-- Find ready: TaskBoard.ready() -> [as-042]
   +-- Load checkpoint: WorkerCheckpoint::load(as-042) -> None (fresh task)
   +-- Spawn worker:
   |   +-- Build identity (7 context layers)
   |   +-- Inject WORKER_PROTOCOL
   |   +-- ClaudeCodeExecutor::execute(prompt)
   |   |   +-- claude -p "..." --output-format json --max-turns 25 --cwd /repo
   |   +-- Parse JSON -> ExecutionResult{result_text, cost_usd, num_turns}
   |   +-- Parse outcome -> TaskOutcome::Done("Fixed login validation...")
   +-- Record cost: cost_ledger.record("algostaking", "as-042", 0.034, 7)
   +-- Update metrics: tasks_completed.inc(), cost_usd.add(0.034)
   +-- Close task: TaskBoard.update(as-042, Done)
   +-- Reflect: Worker.reflect_on_result() -> extract insights -> memory
   +-- Checkpoint: WorkerCheckpoint::capture(/repo) -> save modified files
   +-- Dispatch: PatrolReport{project: "algostaking", active: 0, pending: 0}

4. Gap analysis (queue now empty)
   +-- GapAnalyzer.analyze() -> proposes next high-impact work
```

## Security Model

- **Secret store**: ChaCha20-Poly1305 encrypted, key derived from machine ID
- **Workspace isolation**: workers can only access their project's repo (via `--cwd`)
- **Autonomy levels**: `readonly` (no writes), `supervised` (confirm destructive), `full` (unrestricted)
- **Budget caps**: global daily + per-project limits enforced before worker spawn
- **CLAUDECODE stripping**: env var removed to prevent nested-session detection by Claude Code CLI

## Test Coverage

110 tests across 8 crates:

```
system-orchestrator:  56 tests (checkpoint 11, cost_ledger 12, reflection 6, metrics 4, ...)
system-memory:        17 tests (sqlite, vector, hybrid, chunking)
system-companions:    17 tests (gacha, fusion, store)
system-tasks:          8 tests (create, deps, persistence, compaction)
system-core:           6 tests (config parsing, validation)
system-providers:      6 tests (pricing, model lookup)
```

All passing, zero clippy warnings.
