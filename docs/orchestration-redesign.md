# Orchestration Redesign: Status Quo → Agent-Native Architecture

## Table of Contents
1. [Status Quo: What Exists Today](#status-quo)
2. [What Works Well](#what-works-well)
3. [What's Broken or Disconnected](#whats-broken)
4. [Design Goal](#design-goal)
5. [Proposed Architecture](#proposed-architecture)
6. [Migration Path](#migration-path)

---

## Status Quo: What Exists Today <a name="status-quo"></a>

### The Daemon

Sigil runs as a background daemon (`sigil daemon start`) with a 30-second patrol loop. The daemon owns all shared infrastructure:

- **ProjectRegistry** — holds all projects and their supervisors
- **DispatchBus** — SQLite-backed agent-to-agent message queue with ACK tracking
- **CostLedger** — per-task cost tracking, daily budget enforcement (JSONL)
- **AuditLog** — SQLite decision trail (routing, assignment, timeout, failure)
- **ExpertiseLedger** — SQLite agent success rates by domain (Wilson score ranking)
- **Blackboard** — SQLite inter-agent knowledge store (claims, signals, findings, TTL-based)
- **ConversationStore** — SQLite chat history, channel messages, timeline events
- **AgentRegistry** — SQLite persistent agent identities (UUID, system_prompt, org tree, triggers)
- **TriggerStore** — cron/event/once triggers owned by persistent agents
- **EventBroadcaster** — pub/sub for real-time execution events
- **ChatEngine** — web/Telegram message routing

IPC via Unix socket at `~/.sigil/rm.sock` (JSON-line protocol, 40+ commands).

### The Patrol Loop

Every 30 seconds:

```
1. For each project → Supervisor.patrol()
   ├─ Reload tasks from disk
   ├─ Reap finished workers, detect timeouts
   ├─ Handle blocked tasks (escalation)
   ├─ Assign ready tasks → spawn AgentWorker (tokio::spawn)
   └─ Report to leader (PatrolReport dispatch)
2. Fire due triggers (cron/event/once)
3. Check SIGHUP → hot-reload config
4. Persist: dispatches, cost ledger
5. Retry unacked dispatches, detect dead letters
6. Prune: old costs (7d), expired blackboard, flush memory writes
7. Update metrics
```

### The Supervisor (1 per project, 45 fields)

A Rust struct (`Arc<Mutex<Supervisor>>`) that manages one project's task execution:

- **Worker pool**: spawn up to `max_workers` concurrent tokio tasks, track timeouts, reap finished
- **Agent selection**: `select_agent_for_task()` picks which agent runs a task from the project's team config, using expertise routing + load balancing
- **Context injection** (`create_worker()`): loads persistent agent identity, injects blackboard context, org context, skill prompt, domain hints, builds 9-middleware chain
- **Escalation**: blocked → retry locally (up to `max_resolution_attempts`) → project leader → system leader → human (Telegram/Discord/Slack)
- **Budget enforcement**: check cost ledger before spawning workers
- **Verification**: optional post-execution verification pipeline
- **Metrics/audit**: record routing decisions, assignments, completions, failures

The supervisor is NOT an agent. It has no UUID, no system prompt, no memory. It's static Rust code making orchestration decisions.

### The Agent Worker (ephemeral task executor)

Created per-task by `Supervisor.create_worker()`. Fully configured with:

- Identity (project default + persistent agent override from registry)
- Tools (filtered by skill allow/deny lists)
- Memory (entity-scoped by agent UUID)
- Blackboard context
- Org context (manager, peers, reports — injected as markdown)
- Skill prompt (from TOML skill file)
- 9-middleware chain (loop detection, guardrails, cost tracking, context compression, context budget, memory refresh, clarification, safety net, graph guardrails)
- Event broadcaster
- Reflection provider (cheap model for post-task insight extraction)

Execution flow:
```
worker.execute()
  ├─ middleware.on_start()
  ├─ Build task context (checkpoints, acceptance criteria, resume brief)
  ├─ Enrich identity with memory (query planner + entity recall)
  ├─ Execute agent loop OR Claude Code subprocess
  │   └─ LLM → tools → LLM → repeat
  │       ├─ 3-stage compaction (snip → microcompact → full compact)
  │       ├─ Content replacement for large outputs (>50KB → persist to disk)
  │       ├─ Notification injection for background subagents
  │       ├─ Diminishing returns detection (3 turns < 500 tokens → halt)
  │       └─ Session checkpointing for resume
  ├─ Fire-and-forget reflection (extract insights → dedup → store with graph edges)
  ├─ Parse outcome: Done / Blocked / Handoff / Failed
  │   ├─ Done → checkpoint, close task, dispatch TaskDone
  │   ├─ Blocked → preserve question, dispatch TaskBlocked
  │   ├─ Handoff → re-queue with checkpoint (max 3 retries → auto-cancel)
  │   └─ Failed → LLM failure analysis → classify failure mode → targeted retry/escalate
  └─ Return (TaskOutcome, RuntimeExecution, cost_usd, turns)
```

### The Agent Loop (crates/sigil-core/src/agent.rs)

The core LLM conversation engine (~2,200 lines). Runs inside AgentWorker.

- **Context management**: estimated tokens via `len/4`, compaction at 68% (snip), 80% (full)
- **Snip** (free): remove old API rounds, keep head N + tail M messages
- **Microcompact** (semi-free): clear old tool results (keep 5 most recent), replace with placeholder
- **Full compact** (1 API call): LLM-based structured summary, post-compact file/skill restoration
- **Tool execution**: before_tool hook → execute → after_tool hook, concurrent when safe
- **Content replacement**: large outputs persisted to `.sigil/persist/{worker}/{id}.txt`, replaced with preview
- **Notification drain**: between turns, drain `notification_rx` for background subagent results, inject as User messages
- **Perpetual mode**: after EndTurn, wait for next input via `input_rx` instead of breaking
- **Session files**: checkpoint at `.sigil/sessions/{task_id}.json` for resume after compaction/crash
- **Constants**: max 20 iterations, max 3 compactions per run, diminishing returns at 500 tokens/turn × 3

### Subagent Delegation (DelegateTool)

An agent tool (not a daemon feature) that spawns ephemeral subagents within an agent's runtime.

**Two modes**:

- **Sync** (`run_in_background: false`): parent calls `Agent.run(prompt)` and blocks until result
- **Async** (`run_in_background: true`): parent spawns `tokio::spawn(Agent.run(prompt))`, continues immediately. Result delivered via `LoopNotification` channel, injected as `<task-notification>` XML between turns

**Key mechanics**:
- Subagent inherits parent's provider, tools (minus delegate tool), identity
- Optional tool allowlisting (restrict subagent to specific tools)
- Depth-limited: subagents cannot re-delegate (depth > 0 → error)
- `AgentInfra` struct holds: `registry: HashMap<id, AgentHandle>`, `notification_tx`, `loop_tx`
- `AgentHandle` tracks: id, description, status (Running/Completed/Failed), `notified` dedup flag
- Atomic notification delivery: update handle status + send notification while holding registry lock

**Current gap**: `agent_worker.rs execute_agent()` does NOT wire `notification_rx` or create `AgentInfra`. The DelegateTool infrastructure exists but is not connected in the supervisor worker path. It only works when `sigil chat` or `sigil run` sets it up manually.

### Persistent Agents (AgentRegistry)

SQLite registry at `~/.sigil/agents.db`. Each agent has:

```
id (UUID) — stable identity, entity memory scope
name — human label (NOT unique)
display_name — UI display
template — origin template name
system_prompt — THE AGENT (full personality/instructions)
project — optional project scope (None = root/cross-project)
department — optional department scope (within project)
parent_id — FK to agents.id (org tree)
model — preferred LLM
capabilities — JSON array (spawn_agents, spawn_projects, manage_triggers)
status — Active / Paused / Retired
created_at, last_active, session_count, total_tokens
color, avatar, faces — TUI visual identity
```

**Org tree methods** (all implemented, clean API):
- `parent(id)` → manager agent
- `children(id)` → direct reports
- `siblings(id)` → agents sharing same parent
- `department_name(id)` → self name if has children, else parent's name
- `set_parent(id, parent_id)` → restructure org chart

**Template system**: YAML frontmatter in `agents/*/agent.md` → parsed → spawned via `spawn_from_template()` → creates row in agents.db + creates triggers in trigger store.

### Triggers

Owned by persistent agents, stored in agents.db triggers table:

- **Schedule**: cron expression (e.g., `0 9 * * *`)
- **Event**: pattern match on ExecutionEvent + cooldown (TaskCompleted, TaskFailed, ChannelMessage, DispatchReceived, etc.)
- **Once**: one-shot at ISO 8601 timestamp, auto-disabled after firing

When a trigger fires: daemon creates a task with the trigger's skill, bound to the trigger's owning agent. Advance-before-execute pattern prevents duplicate execution on restart.

### DispatchBus

SQLite-backed message queue. Each `Dispatch` has:

```
from, to — agent names
kind — DispatchKind enum (16 variants)
timestamp, first_sent_at
read — marked true when recipient calls read()
requires_ack — true for critical messages
retry_count, max_retries (default 3)
```

**DispatchKind variants**:

| Kind | Used? | Direction | Purpose |
|------|-------|-----------|---------|
| TaskDone | YES | worker → leader | Task completed |
| TaskBlocked | YES | worker → leader | Needs clarification |
| TaskFailed | YES | worker → leader | Execution failed |
| WorkerCrashed | YES | supervisor → leader | Worker timed out |
| Escalation | YES | supervisor → leader | Blocked after max attempts |
| Resolution | YES | leader → supervisor | Answer to escalation |
| HumanEscalation | YES | supervisor → channels | Terminal escalation |
| PatrolReport | GENERATED, NEVER CONSUMED | supervisor → leader | Active/pending counts |
| CouncilTopic | NO | — | Never implemented |
| CouncilResponse | NO | — | Never implemented |
| CouncilSynthesis | NO | — | Never implemented |
| AgentAdvice | NO | — | Never sent |
| TaskProposal | NO | — | Never sent |
| DependencySuggestion | NO | — | Never sent |

Methods: `send()`, `read(recipient)`, `acknowledge(id)`, `retry_unacked(age)`, `dead_letters()`, `health()`.
TTL: 1 hour default. Max queue: 1000 per recipient. Max retries: 3.

### Blackboard

SQLite-backed coordination surface. Each entry has:

```
key — identifies entry (e.g., "claim:src/api.rs", "finding:auth-bug")
content — free-form text
agent — who posted
project — scoped to project
tags — categorization
durability — Transient (24h TTL) or Durable (7d TTL)
expires_at — auto-calculated
```

**Operations**: post, query (by project + tags), get_by_key, claim (atomic resource lock), release, delete, prune_expired.

**Scoped visibility** (implemented in `query_scoped()` but NEVER USED in production code paths):
- `system:*` → always visible
- `project:{name}:*` → only agents in project
- `dept:{name}:*` → only agents in department
- `agent:{uuid}:*` → only that agent
- No prefix → always visible (legacy)

**Claims**: atomic check/acquire. Same agent → renew. Different agent → Held response. 2h TTL default.

### ConversationStore

SQLite-backed chat history with FTS5 search. Deterministic chat IDs via hashing (project_chat_id, department_chat_id, named_channel_chat_id, agency_chat_id).

**Dead features**: auto-summarization (threshold 30, never triggered), evict_older_than (never called), timeline events (type exists, only messages recorded), search_transcripts (requires channel_type='transcript', none created).

### Config Model

```toml
[sigil]           # name, data_dir, default_runtime
[providers.*]     # OpenRouter, Anthropic, Ollama + API keys
[security]        # autonomy, budget limits
[memory]          # SQLite backend, embedding config
[team]            # leader, agents[], router_model, max_background_cost
[orchestrator]    # expertise_routing, preflight, adaptive_retry, etc.
[web]             # Axum bind, CORS, auth_secret
[repos]           # Global repo pool (name → path)
[[projects]]      # name, prefix, repo, model, team, departments, missions
```

**PeerAgentConfig** (in sigil.toml `[[agents]]` or `agents/*/agent.toml`):
```
name, prefix, model, runtime, role (AgentRole enum), voice
execution_mode, max_workers, max_turns, max_budget_usd
default_repo, expertise[], capabilities[], telegram_token_secret
```

**AgentRole enum**: Orchestrator (default), Worker, Advisor, Executive — NOT used for routing or behavior differentiation.

### Middleware Chain (9 implementations)

| Middleware | Order | Purpose |
|-----------|-------|---------|
| Guardrails | 200 | Block dangerous shell ops (rm -rf, DROP TABLE) |
| MemoryRefresh | 400 | Re-search memory every N tool calls |
| LoopDetection | 500 | MD5 hash sliding window, warn@3, kill@5 repeats |
| CostTracking | 600 | Token/cost accumulation, budget ceiling |
| ContextBudget | 700 | Cap enrichment at ~200 lines |
| GraphGuardrails | — | Block dangerous git ops |
| ContextCompression | — | Compress at 50% window, protect first/last |
| Clarification | — | Structured ask_clarification, halts execution |
| SafetyNet | — | On failure: preserve work artifacts |

### Persistence Summary

| Store | Backend | What's In It |
|-------|---------|-------------|
| agents.db | SQLite | Persistent agent identities, UUIDs, org tree, triggers |
| memory.db | SQLite+FTS5+vectors | Semantic memory per entity (agent UUID scoped) |
| audit.db | SQLite | Every routing decision, assignment, timeout, failure |
| expertise.db | SQLite | Agent success rates by domain (Wilson score ranking) |
| blackboard.db | SQLite | Inter-agent knowledge, resource claims, signals (TTL) |
| conversations.db | SQLite | Chat history, channel messages |
| dispatches.db | SQLite | Agent-to-agent messages with ACK tracking |
| cost_ledger.jsonl | JSONL | Daily per-task cost tracking, budget enforcement |
| .tasks/ (per project) | JSONL | Task DAGs with status, dependencies, outcomes |

---

## What Works Well <a name="what-works-well"></a>

These components are production-quality and should be preserved:

1. **The patrol loop** — reliable 30s heartbeat, fire-and-forget workers, non-blocking IPC, early wake via Notify
2. **Task lifecycle** — Pending → InProgress → Done/Blocked/Failed with checkpoints, retries, escalation
3. **Agent loop** — 3-stage compaction, content replacement, session checkpointing, notification injection, diminishing returns detection
4. **Middleware chain** — 9 composable behaviors, clean trait with 8 hooks, per-worker instantiation
5. **Entity-scoped memory** — persistent agent UUID scopes memory queries, reflection extracts insights, dedup pipeline
6. **Trigger system** — cron/event/once with advance-before-execute, auto-disable one-shot, agent-owned
7. **Blackboard** — resource claims (atomic), signals, findings with TTL (24h/7d), scoped visibility (implemented)
8. **DelegateTool** — sync/async subagent spawning, depth-limited, notification delivery, dedup guard
9. **Context injection** (`create_worker()`) — identity from registry, blackboard, org context, skills, tools, middleware
10. **Adaptive failure analysis** — LLM-based failure mode classification → targeted retry
11. **Preflight assessment** — cheap LLM evaluation before expensive execution
12. **Expertise ledger** — Wilson score ranking, deprioritization of failing agents
13. **IPC protocol** — 40+ commands, try_lock for non-blocking reads, cursor-based event pagination

---

## What's Broken or Disconnected <a name="whats-broken"></a>

### 1. The Org Tree Is Decorative

`agents.db` has `parent_id` with clean API (parent, children, siblings, department_name, set_parent). But:

- `select_agent_for_task()` ignores hierarchy — routes by team config + load balance + expertise
- Escalation doesn't climb the tree — goes to hardcoded `escalation_target`, not up parent chain
- Department field exists but doesn't restrict visibility or assignment
- Org context injected as markdown text — agent knows peers but can't act on relationships

### 2. Communication Is Vertical-Only

DispatchBus supports arbitrary from/to addressing and has 16 message types, but only 7 are used — all vertical (worker → leader, leader → supervisor). No agent-to-agent messaging. Agent A cannot send a directed message to Agent B.

9 dispatch types are dead code (PatrolReport generated but never consumed, Council system never implemented, TaskProposal/DependencySuggestion/AgentAdvice never sent).

### 3. The Supervisor Is Not An Agent

The Supervisor is a Rust struct. It makes routing decisions via static code, not LLM reasoning. No UUID, no memory, no system prompt. Cannot be delegated to or from. All orchestration intelligence is hardcoded.

### 4. `select_agent_for_task()` Solves A Non-Problem

Every task originates from either a trigger (owned by a specific agent) or a delegation decision (by a specific agent). There is no scenario where a task exists without knowing which agent should handle it. The routing logic is solving a problem that doesn't exist.

### 5. DelegateTool Is Not Wired In Workers

The subagent notification infrastructure (AgentInfra, notification channels, LoopNotification drain) exists in delegate.rs and agent.rs, but `agent_worker.rs execute_agent()` does NOT set it up. Workers spawned by the supervisor cannot delegate to subagents.

### 6. PeerAgentConfig Conflates Template And Runtime

Mixes template definition (name, model, expertise) with runtime wiring (telegram_token_secret, default_repo). C-suite agent templates crashed because `role = "executive"` wasn't a valid AgentRole variant.

### 7. Channels Overlap With Blackboard

ConversationStore channels and Blackboard both store scoped text for agent knowledge sharing. Blackboard is strictly more capable (TTL, claims, visibility rules).

### 8. Dead / Unused Features

| Feature | Status |
|---------|--------|
| AgentRole enum | Not used for routing or behavior |
| PatrolReport dispatch | Generated, never consumed |
| Council system (3 dispatch types + config) | Zero implementation |
| TaskProposal, DependencySuggestion, AgentAdvice | Never sent |
| ConversationStore auto-summarization | Never triggered |
| `query_cross_project()` on Blackboard | Zero callers |
| `query_scoped()` on Blackboard | Implemented, never used |
| `department` field | Stored, never enforced |
| TeamConfig.router_model | For removed intent classifier |

---

## Design Goal <a name="design-goal"></a>

**Tasks don't find agents. Agents create tasks.**

The entry point is always an agent, never a free-floating task looking for a home. An agent either:
1. Handles a task itself (agent loop)
2. Spawns an ephemeral subagent (DelegateTool — sync or async)
3. Delegates to a child persistent agent (new: dispatch-based)

No routing decisions. No team matching. No `select_agent_for_task()`. The agent who creates the task already knows who should handle it.

**The org hierarchy lives on departments, not agents:**
- Departments are UUID-identified entities with their own parent/child hierarchy
- Agents belong to a department (`department_id`), no `parent_id` on agents
- Department managers handle escalation — chain climbs department.parent_id until root
- Department members share blackboard scope (findings, signals, claims)

**Four primitives:**
1. **Agent** — persistent identity with UUID, system_prompt, department_id, triggers, memory
2. **Department** — UUID-identified org unit with name, manager_id, parent_id hierarchy
3. **Task** — always agent-bound from creation, never free-floating
4. **Delegation** — unified delegate tool: message, task, subagent, or broadcast via one interface with 5 response modes (origin, perpetual, async, department, none)

### Why This Works

The subagent delegation model (DelegateTool) already proves the pattern:
- Parent spawns child with specific prompt + tools
- Child runs independently
- Result flows back to parent (sync: inline, async: notification channel)
- Depth-limited to prevent recursion

Persistent agent delegation is the SAME pattern, just with:
- Child has its own identity, memory, and tools (from registry)
- Result flows back via DelegateResponse dispatch instead of notification channel
- Child persists across sessions (entity memory accumulates)
- 5 response modes give precise control over where results land

The supervisor's `create_worker()` already handles the context injection that persistent agents need (identity from registry, blackboard, org context, skills, middleware). The infrastructure is all there — it's just wired through the wrong abstraction (project-based supervisor routing instead of agent-based delegation).

### Interaction Model

**Not every interaction is a task.** The unified delegate tool handles four distinct patterns:

| Pattern | create_task | What happens |
|---------|-------------|-------------|
| **Message** | false | Prompt delivered to target, optional response |
| **Task delegation** | true | Task lifecycle (Pending → Done/Failed), outcome tracked |
| **Subagent** | false | Ephemeral agent spawned inline (to: "subagent") |
| **Broadcast** | false | Posted to department, all members' triggers fire |

**Agent ↔ agent is always async sessions.** Perpetual sessions are for user ↔ agent only. When Agent A messages Agent B, B's trigger fires an async session. B processes the message, responds, session ends. No persistent agent-to-agent sessions.

**Response routing prevents infinite loops.** Responses carry `reply_to` and do NOT fire the recipient's generic dispatch trigger. They route via the mode specified in the original request (inject into session, spawn purpose-built response session, post to department, or nothing).

---

## Proposed Architecture <a name="proposed-architecture"></a>

### Agent Runtime

Every persistent agent gets its own worker pool (replaces per-project Supervisor):

```
Agent (persistent, has UUID)
  ├── Identity (system_prompt, persona, memory, skills)
  ├── Department membership (department_id — which department am I in)
  ├── Triggers (cron/event/once → spawn async session for THIS agent)
  ├── Memory (entity-scoped by UUID)
  ├── Blackboard access (scoped by department membership)
  │
  ├── Can execute tasks itself (agent loop + middleware chain)
  ├── Unified delegate tool (one tool for all inter-agent interaction):
  │   ├── Message another agent (response: origin/perpetual/async/department/none)
  │   ├── Delegate task to another agent (create_task: true)
  │   ├── Spawn ephemeral subagent (to: "subagent")
  │   └── Broadcast to department (to: "dept:X")
  │
  └── Worker Pool (infrastructure, not orchestration)
      ├── max_workers concurrent executions
      ├── Timeout tracking + reaping
      ├── Budget enforcement (cost ledger)
      └── Middleware chain injection

Department (UUID-identified org unit)
  ├── name (display label, renameable)
  ├── manager_id → Agent (nullable, swappable)
  ├── parent_id → Department (hierarchy)
  ├── Members: agents WHERE department_id = this
  ├── Blackboard scope: dept:{uuid}:*
  ├── ConversationStore: department_chat_id(uuid) for broadcast log
  └── Escalation: manager handles blocked members, or climbs to parent dept
```

### Task Model

Tasks are always agent-bound:

```
Task {
    id: String,
    agent_id: UUID,        // WHO owns this — always set, never null
    subject: String,
    description: String,
    status: Pending | InProgress | Done | Blocked | Failed | Cancelled,
    created_by: Trigger(trigger_id) | Delegation(parent_agent_id) | Self,
    skill: Option<String>,
    // ... existing fields (priority, labels, checkpoints, outcome)
}
```

No task is ever created without an agent_id. The agent_id determines which worker pool runs it, which identity the worker gets, which memory scope to use, and where to escalate.

### Unified Delegate Tool

Today there are three separate tools: `dispatch_send` (message), `delegate` (subagent), and `project_assign` (task). These are the same operation with different parameters. They consolidate into one tool:

```
delegate(
    to: String,                          // agent name, "dept:Engineering", or "subagent"
    prompt: String,                      // what to do / what to say
    response: "origin" | "perpetual" | "async" | "none",
    create_task: bool,                   // track with task lifecycle (default: false)
    skill: Option<String>,               // skill for the target to use
    tools: Option<Vec<String>>,          // tool allowlist (subagent only)
)
```

**Response modes** — where does the response go:

| Mode | Where response goes | Session requirement | Use case |
|---|---|---|---|
| `origin` | Back into the calling session via LoopNotification | Calling session stays alive until response arrives | "I need this answer to continue my current work" |
| `perpetual` | Into the calling agent's perpetual session | Agent must have a running perpetual session | "Let me know when it's done, I'll be around" |
| `async` | Fresh async session spawned for the calling agent | Always works, no requirement | "Process the result independently with clean context" |
| `department` | Into the caller's department ConversationStore | Caller must be in a department | "Share the result with my whole team" |
| `none` | Nowhere | Fire and forget | FYI messages, broadcasts, notifications |

**No infinite loops:** Responses do NOT fire the recipient's generic dispatch trigger. They either inject into a specific session (`origin`/`perpetual`) or spawn a purpose-built response session (`async`) that processes the result and ends — it doesn't auto-reply.

**Examples:**

```
// Fire-and-forget FYI
delegate(to: "CPO", prompt: "Deploy freeze Thursday", response: "none")

// Question — answer comes back here
delegate(to: "CPO", prompt: "What's the redesign timeline?", response: "origin")

// Task delegation — outcome tracked, result in perpetual session
delegate(to: "Backend Lead", prompt: "Fix the auth bug", response: "perpetual", create_task: true)

// Ephemeral subagent — same as today's DelegateTool
delegate(to: "subagent", prompt: "Research this API", response: "origin", tools: ["web_fetch", "read_file"])

// Department broadcast
delegate(to: "dept:Engineering", prompt: "All hands: deploy freeze Thursday", response: "none")

// Department question — responses come back here from each member
delegate(to: "dept:Engineering", prompt: "Status update on your current tasks?", response: "origin")
```

**What this replaces:**
- `dispatch_send` tool → `delegate(response: "none" | "origin" | ...)`
- `delegate` tool (subagent) → `delegate(to: "subagent", ...)`
- `project_assign` tool → `delegate(create_task: true, ...)`
- `channel_post` / `department_post` → `delegate(to: "dept:X", ...)`

### Escalation Model

Follows department hierarchy (replaces hardcoded escalation_target):

```
API Engineer (in dept "Backend") is blocked
  → Find dept "Backend" → manager is Backend Lead
  → Backend Lead attempts resolution
  → Still blocked → find parent dept "Engineering" → manager is CTO
  → CTO attempts resolution
  → Still blocked → find parent dept "Executive" → manager is Shadow
  → Shadow attempts resolution
  → No parent department → human escalation via gate channels
```

### Departments

Departments are a lightweight primitive — a UUID-identified organizational unit with its own hierarchy.

```sql
departments (
    id UUID PRIMARY KEY,          -- stable identity (all references use this)
    name TEXT,                    -- "Engineering" (display label, can be renamed freely)
    project TEXT,                 -- which project this department belongs to
    manager_id UUID NULL,         -- FK to agents.id (nullable — can be temporarily unmanaged)
    parent_id UUID NULL,          -- FK to departments.id (department hierarchy)
)

agents (
    id UUID PRIMARY KEY,
    department_id UUID NULL,      -- FK to departments.id (which department am I in)
    -- NO parent_id on agents anymore — hierarchy lives on departments, not agents
    ...
)
```

**The org hierarchy lives on departments, not agents.** Agents belong to a department. Departments have managers and parent departments. This means:

- Department UUID is the stable reference — survives manager changes, renames, agent churn
- Department name is a display label — rename "Engineering" to "Platform" without breaking anything
- Manager is a mutable pointer — swap, remove, or leave vacant without affecting members or history
- Blackboard scope is `dept:{department.id}:*` — stable forever
- ConversationStore chat_id is `department_chat_id(department.id)` — stable forever

**Manager vs member separation**: An agent is IN one department (member) and optionally MANAGES another department. The manager belongs to the parent department:

```
Department: "Executive" (manager: Shadow, parent: null)
  Members: CTO, CPO, CFO

Department: "Engineering" (manager: CTO, parent: "Executive")
  Members: Backend Lead, Frontend Lead

Department: "Backend" (manager: Backend Lead, parent: "Engineering")
  Members: API Engineer, DB Engineer

Department: "Frontend" (manager: Frontend Lead, parent: "Engineering")
  Members: UI Engineer, Mobile Engineer
```

CTO is IN "Executive" (peers: CPO, CFO). CTO MANAGES "Engineering". Clean separation — you belong to one layer, you manage the layer below.

**Queries:**
- Members of a department: `SELECT * FROM agents WHERE department_id = ?`
- Manager of a department: `SELECT manager_id FROM departments WHERE id = ?`
- Peers (siblings): agents in the same department (excluding self)
- Sub-departments: `SELECT * FROM departments WHERE parent_id = ?`
- Parent department: `SELECT parent_id FROM departments WHERE id = ?`

**Escalation follows department hierarchy:**
```
API Engineer (in "Backend") is blocked
  → Find department "Backend" → manager is Backend Lead
  → Backend Lead resolves or escalates
  → Find parent department "Engineering" → manager is CTO
  → CTO resolves or escalates
  → Find parent department "Executive" → manager is Shadow
  → Shadow resolves or escalates
  → No parent department → human escalation
```

Stable chain. Swap CTO out → update manager_id on "Engineering" → all members, blackboard, conversation history, escalation chain unchanged.

### Department Broadcast

When an operator (or agent) needs to address a whole department:

```
Operator posts message to "Engineering" department
  → Message stored in ConversationStore (keyed by department_chat_id(dept.id) — stable UUID)
  → Event emitted (DepartmentMessage)
  → For each agent WHERE department_id = engineering.id:
     → Agent's event trigger fires (event: department_message)
     → Async session spawned with:
        - The broadcast message
        - Recent department conversation history (from ConversationStore)
        - Blackboard context (dept:{engineering.id}:* scope)
     → Agent responds independently in its own session
     → Response stored in ConversationStore (same department chat_id)
     → Response shown to operator
```

Each agent runs in its own session with its own model and context window. The ConversationStore is the shared record — a message log, not a shared context window. Agents pull recent history when they need it.

**Preventing cascade loops in department channels:**

Two types of writes to the department ConversationStore:

- **Initiating message** (from operator or via delegate tool): stored with `emit_event: true` → DepartmentMessage event fires → member triggers activate
- **Agent response** (from a trigger-fired session reacting to a department message): stored with `emit_event: false` → no event, no cascade

The `record()` method takes an `emit_event` flag. The delegate tool sets it to `true` for new broadcasts. Agent response skills set it to `false`. Responses are stored for history but don't trigger anyone. No infinite loop.

This covers:
- **Operator → department**: user posts, all agents react independently
- **Agent → department**: agent posts via delegate tool, members' triggers fire
- **Agent response**: stored in channel history, visible to all, but no cascade
- **Department history**: ConversationStore keeps the full record, queryable via stable UUID

### Escalation to User (Shadow as Inbox)

When escalation reaches the root department (no parent), it surfaces to the human operator. No separate inbox concept — **Shadow's perpetual session IS the user's inbox.**

```
Escalation reaches root department
  → Root department's manager_id points to Shadow
  → Escalation injected into Shadow's perpetual session (Telegram/Web/CLI)
  → Shadow presents to user: "Backend Engineer is blocked on X, needs your input"
  → User responds to Shadow
  → Shadow delegates resolution back down: delegate(to: "Backend Engineer", prompt: "User says: ...", response: "none")
```

### Approval / Clarification

The existing `ClarificationMiddleware` already implements this pattern:

1. Agent calls `ask_clarification` tool (structured: type, question, context, options)
2. Middleware intercepts in `before_tool` hook
3. Stores structured request in `WorkerContext::metadata`
4. Returns `MiddlewareAction::Halt` — execution stops cleanly
5. Task goes to Blocked status
6. When answer comes back, task re-queues with the answer

`ClarificationType` already covers all cases: `MissingInfo`, `Choice`, `Confirmation` (= approval).

**What changes:** Today, clarification halts and the supervisor escalates to the hardcoded `escalation_target`. In the new architecture, it routes via the department hierarchy instead:

```
Agent calls ask_clarification(type: "confirmation", question: "Deploy to prod?", context: "3 files changed")
  → Middleware halts execution, task goes Blocked
  → Daemon finds agent's department → department's manager_id
  → Spawn async session for the department's manager with structured request
  → Manager responds → answer injected back, task re-queues, agent continues
  → If no manager (or manager escalates) → climb to parent department → repeat
  → If root department → surfaces in Shadow's perpetual session → user decides
```

No new tool needed. Same `ask_clarification`, same `ClarificationType` enum. Just rewired routing: department chain instead of hardcoded target. The agent doesn't know or care WHO answers — it just asks and gets a response.

### Root Architecture

Shadow is the only agent at the root level. It's the user's proxy in the system.

```
Root Department (project: null, manager: Shadow, parent: null)
│
├── "Sigil Core" (project: sigil, manager: CTO, parent: Root)
│   ├── "Backend" (project: sigil, manager: BackendLead, parent: Sigil Core)
│   │   └── members: API Engineer, DB Engineer
│   └── "Frontend" (project: sigil, manager: FrontendLead, parent: Sigil Core)
│       └── members: UI Engineer
│
└── "Trading" (project: algostaking, manager: TradingLead, parent: Root)
    └── members: StrategyBot, RiskBot
```

- **One root, one inbox**: Shadow's perpetual session is where all unresolved escalations surface
- **Projects are a scope tag** on departments (`project` field), not a separate organizational primitive
- **Escalation path**: any agent → department manager chain → Shadow → user
- **Shadow has unique capabilities**: `spawn_agents`, `spawn_projects`, `create_departments` — it's the only agent that can build the org chart itself

### Blackboard Scoping

Blackboard is the inter-agent knowledge sharing layer (structured, TTL, claims). Separate from ConversationStore (which is the chronological message record).

Project-scoped blackboard: all agents on a project share knowledge regardless of parent/child. This is for cross-cutting concerns (deploy freezes, shared discoveries, etc.).

Visibility follows org tree + project scope:

```
Root Agent (Shadow)
├── CTO — posts finding:api-redesign → visible to siblings + parent
├── CPO — queries findings → sees CTO's finding:api-redesign
└── CFO — posts signal:budget-warning → visible to siblings + parent
```

Rules:
- Agent can read: own entries, siblings' entries, parent's entries, children's entries, project-scoped entries
- Agent can write: own scope only
- Claims are visible within parent scope (siblings see each other's claims)
- Parent can see all children's entries (read down)
- `query_scoped()` already implements this — just needs to be used

### DispatchBus Simplification

Keep only what's needed for agent ↔ agent communication via the unified delegate tool:

| Kind | Direction | Purpose |
|------|-----------|---------|
| DelegateRequest | sender → target | Message, task, or department broadcast |
| DelegateResponse | target → sender | Reply to a request (routed by response mode) |
| HumanEscalation | root dept manager → gate channels | Terminal escalation |

The delegate tool handles all routing via the `response` parameter (origin/perpetual/async/department/none). DelegateResponse carries a `reply_to` field linking it to the originating request. The daemon routes responses based on the mode specified in the original request.

Remove: TaskDone, TaskBlocked, TaskFailed, Resolution, PatrolReport, CouncilTopic/Response/Synthesis, AgentAdvice, TaskProposal, DependencySuggestion, WorkerCrashed — all subsumed by DelegateRequest/DelegateResponse.

### What Dies

| Component | Replacement |
|-----------|-------------|
| `Supervisor` as orchestration engine | Agent's own worker pool (infrastructure only) |
| `select_agent_for_task()` | Tasks are agent-bound at creation |
| `AgentRole` enum | Removed — every agent can do everything |
| `ProjectTeamConfig` | Removed — parent delegates to specific children |
| `parent_id` on agents | Replaced by `department_id` — hierarchy lives on departments, not agents |
| `department` field (string) on agents | Replaced by `department_id` (UUID FK to departments table) |
| ConversationStore as "channel" primitive | Kept as message log for department broadcast + audit only |
| `escalation_target` / `system_escalation_target` | Department hierarchy (dept.parent_id → manager_id chain) |
| 9 unused DispatchKind variants | Removed |
| `PeerAgentConfig` in sigil.toml `[[agents]]` | Agents defined in `agents/*/agent.toml` + agent.md only |
| `TeamConfig.agents[]` | Removed — org tree defines relationships |
| `TeamConfig.router_model` | Removed — intent classifier already deleted |

### What Stays (Unchanged)

| Component | Why |
|-----------|-----|
| Agent loop (compaction, tools, notifications) | Core execution engine, production-quality |
| DelegateTool mechanics (subagent spawning, notification injection) | Clean, working, depth-limited — unified into new delegate tool |
| Middleware chain (9 layers) | Composable, well-tested |
| Entity-scoped memory | UUID-keyed, critical for persistent agents |
| Blackboard (claims, signals, findings, TTL) | Scoped coordination, well-designed |
| Trigger system (cron/event/once) | Agent-owned, advance-before-execute |
| DispatchBus (simplified) | Parent ↔ child communication |
| Expertise ledger | Agent self-knowledge (not routing) |
| Preflight assessment | Agent self-assessment before execution |
| Adaptive failure analysis | LLM-based failure classification |
| create_worker() context injection | Identity, blackboard, skills, tools, middleware |
| IPC protocol | External control plane |
| Patrol loop | Heartbeat for trigger firing + worker reaping |
| CostLedger, AuditLog | Budget enforcement + decision trail |

### What Changes

| Component | Change |
|-----------|--------|
| Worker pool | Moves from per-project Supervisor to per-agent |
| Escalation | Follows department hierarchy (dept.parent_id → manager_id) instead of hardcoded targets |
| Task creation | Requires agent_id (never unassigned) |
| Delegation | Unified delegate tool replaces dispatch_send + delegate + project_assign + channel_post |
| Response routing | 5 modes (origin, perpetual, async, department, none) replace ad-hoc dispatch handling |
| DispatchBus | Simplified to DelegateRequest + DelegateResponse + HumanEscalation |
| Org hierarchy | Departments table (UUID, name, manager_id, parent_id) replaces parent_id on agents |
| Blackboard queries | Use `query_scoped()` with department-based visibility |
| Config | Simplify: remove team routing, role fields; add department definitions |

### Simplified Patrol Loop

```
Every 30 seconds:
  1. For each agent with pending tasks:
     ├─ Reap finished workers
     ├─ Spawn workers up to max_workers (no routing — agent is known)
     ├─ Budget enforcement
     └─ Timeout detection
  2. Fire due triggers (bind task to trigger's owning agent)
  3. Handle dispatch delivery (parent ↔ child results)
  4. Persist state (dispatches, cost)
  5. Prune expired blackboard + old costs
  6. Flush memory writes
```

No `select_agent_for_task()`. No team matching. No expertise routing for assignment. Just: "this agent has pending tasks → spawn workers with this agent's identity."

---

## Migration Path <a name="migration-path"></a>

### Phase 1: Clean Up Dead Code
- Remove `AgentRole` enum (replace match arms, delete field from PeerAgentConfig)
- Remove all unused DispatchKind variants (PatrolReport, Council*, AgentAdvice, TaskProposal, DependencySuggestion)
- Remove `select_agent_for_task()` body (replace with direct agent_id from task)
- Remove `ProjectTeamConfig` and team routing logic
- Remove `TeamConfig.agents[]`, `TeamConfig.router_model`

### Phase 2: Departments Table
- Create `departments` table in agents.db (id UUID, name, project, manager_id, parent_id)
- Add `department_id` column to agents table (FK to departments)
- Remove `parent_id` and `department` (string) columns from agents
- Migrate existing agent relationships into department records
- Update AgentRegistry methods: replace parent/children/siblings with department-based queries
- Update org context injection to use department hierarchy

### Phase 3: Make Tasks Agent-Bound
- Add `agent_id: String` to Task struct (required field)
- Update task creation paths to always specify agent_id
- Update trigger firing to bind task to trigger's owning agent (already mostly does this)
- Update IPC `create_task` to require agent_id
- Validate: no task can exist with empty agent_id

### Phase 4: Replace Supervisor With Agent Worker Pool
- Extract worker pool infrastructure from Supervisor into a standalone struct (spawn, track, reap, timeout, budget)
- Each agent in the registry gets a worker pool (lazy — only created when agent has pending tasks)
- Patrol loop iterates agents with pending tasks, not projects
- create_worker() uses agent's own identity from registry (already does this via lookup)
- Move context injection logic (blackboard, org context, skills, middleware) to worker pool

### Phase 5: Unified Delegate Tool
- Consolidate `dispatch_send`, `delegate` (subagent), `project_assign`, `channel_post` into single `delegate` tool
- Implement 5 response modes:
  - `origin` — inject response into calling session via LoopNotification (session kept alive)
  - `perpetual` — inject into agent's perpetual session
  - `async` — spawn fresh async session for the sender with the response
  - `department` — post response to department ConversationStore
  - `none` — fire and forget
- Wire AgentInfra + notification_rx in worker execution path (currently unwired)
- Implement DelegateRequest/DelegateResponse dispatch types (replace all existing kinds)
- Responses with `reply_to` set do NOT fire triggers — they route via the specified response mode

### Phase 6: Escalation Via Department Hierarchy
- Replace hardcoded `escalation_target` with department manager chain
- On blocked: find agent's department → manager resolves or escalates to parent department's manager
- Root department (parent_id = null) manager → human escalation via gate channels
- Keep three-strikes tracker, just change the target resolution

### Phase 7: Blackboard Scoping
- Switch from `query()` to `query_scoped()` (already implemented)
- Build AgentVisibility from department membership (department_id, project)
- Department members see each other's entries (shared department scope)
- Manager can see managed department's entries (read down)
- Scope key changes: `dept:{department_uuid}:*` instead of string-based

### Phase 8: Rewire Clarification Routing
- Existing `ClarificationMiddleware` + `ask_clarification` tool stay unchanged
- Rewire routing: instead of hardcoded `escalation_target`, route via department chain
- Department chain traversal: find agent's department → manager_id → spawn async session for manager
- If no manager or manager escalates → climb to parent department
- Root department → inject into Shadow's perpetual session → user decides
- No new tool — `ClarificationType::Confirmation` already covers approval

### Phase 9: Department Broadcast
- Implement DepartmentMessage event type in EventBroadcaster
- Wire department_chat_id to use department UUID (stable)
- `delegate(to: "dept:X", ...)` posts to ConversationStore + emits DepartmentMessage
- Members respond via their `department_message` event triggers (opt-in, per-agent)
- IPC command for operator → department broadcast
