# Claude Code Integration

System workers execute as Claude Code CLI subprocesses. This document covers how the integration works and how to configure it.

## How Workers Run

When `execution_mode = "claude_code"` (the default for production projects), each worker spawns:

```bash
claude -p "<system_prompt + task>" \
  --output-format stream-json \
  --max-turns 25 \
  --permission-mode bypassPermissions \
  --cwd /path/to/project/repo
```

**Key flags**:
- `-p` -- print mode (non-interactive, takes prompt as argument)
- `--output-format stream-json` -- structured event stream with tool activity and a final result record
- `--max-turns 25` -- agentic turn limit (configurable per project)
- `--permission-mode bypassPermissions` -- full tool access (Edit, Grep, Glob, Task, etc.)
- `--cwd` -- working directory set to the project's repo

**Why this matters**: Unlike Agent mode (internal LLM loop), Claude Code mode gives workers access to the full Claude Code toolset including the **Task tool for spawning sub-agents**. This creates recursive orchestration -- any worker can become a coordinator.

## Environment

The `CLAUDECODE` environment variable is stripped from the worker's process environment. This prevents Claude Code from detecting that it's running inside another Claude Code session (which would block nested execution).

## Retry Logic

Worker execution includes automatic retry with exponential backoff:

```
Attempt 1: execute
  | (failure)
Wait 1 second
Attempt 2: execute
  | (failure)
Wait 2 seconds
Attempt 3: execute
  | (failure)
Return error -> task marked as Failed
```

Max 3 retries. Common failure modes: Claude Code CLI not found, API rate limits, timeout.

## Output Parsing

Claude Code emits a JSON event stream. The final `result` event contains the completed response:

```json
{
  "result": "I fixed the login validation by...",
  "session_id": "abc123",
  "num_turns": 7,
  "total_cost_usd": 0.034,
  "duration_ms": 45000
}
```

The worker parses the final `result` text for outcome:
- Starts with `DONE` or `DONE:` -> `TaskOutcome::Done`
- Starts with `BLOCKED:` -> `TaskOutcome::Blocked` (triggers escalation)
- Starts with `FAILED:` -> `TaskOutcome::Failed` (requeue with backoff)
- Starts with `HANDOFF:` -> `TaskOutcome::Handoff` (checkpoint + requeue)
- No prefix -> defaults to `Done` with the full text as summary

Current limitation:
- tool progress is visible during the stream
- total cost is known only on the final `result` event, so cost enforcement is final-result-based rather than true mid-run accounting

## Worker Protocol

Every worker receives the `WORKER_PROTOCOL` in its system prompt:

```markdown
## Worker Protocol

You are a System worker executing a task. Follow these rules strictly.

### Completion
When you successfully complete the task, provide a clear summary of what you changed.

### Blocked
If you cannot proceed and need information from outside your project, respond with:
BLOCKED: <description of what you need>

### Failed
If the task cannot be completed due to an error, respond with:
FAILED: <description of the error>

### Handoff
If you've made partial progress but the task needs a fresh context, respond with:
HANDOFF: <summary of what you did and what remains>
```

## Context Assembly

The worker's system prompt is assembled from multiple layers:

```
+-------------------------------------+
| Shared WORKFLOW.md (max 2k chars)   | <- base standards, R->D->R pipeline
+-------------------------------------+
| PERSONA.md                          | <- project personality
+-------------------------------------+
| IDENTITY.md                         | <- name, expertise, repos
+-------------------------------------+
| AGENTS.md                           | <- operating instructions
+-------------------------------------+
| KNOWLEDGE.md (max 12k chars)        | <- project knowledge base
+-------------------------------------+
| WORKER_PROTOCOL                     | <- output format rules
+-------------------------------------+
| Checkpoint context (max 8k chars)   | <- predecessor's work-in-progress
+-------------------------------------+
| Memory recall                       | <- relevant memories from SQLite
+-------------------------------------+
                +
+-------------------------------------+
| Repo CLAUDE.md                      | <- auto-discovered by Claude Code
+-------------------------------------+
```

Total budget: ~40k chars (~10k tokens). The `ContextBudget` system handles truncation.

## IPC Socket

When the daemon is running, it listens on `~/.sigil/sigil.sock` for JSON-line queries:

```bash
# Via the CLI
sigil daemon query ping          # -> "pong"
sigil daemon query status        # -> project counts, worker states, cost
sigil daemon query projects      # -> project info JSON
sigil daemon query dispatches    # -> recent dispatch messages
sigil daemon query metrics       # -> Prometheus text exposition
sigil daemon query cost          # -> budget status per project
```

Programmatic access:
```bash
echo '{"cmd":"status"}' | socat - UNIX-CONNECT:~/.sigil/sigil.sock
```

## CLI Commands

```bash
# One-shot execution (creates temporary worker)
sigil run "list files in current directory" --rig myproject

# Assign task (picked up by supervisor on next patrol)
sigil assign "fix the login bug" --rig myproject --priority high

# Check ready work
sigil ready --rig myproject

# Daemon management
sigil daemon start       # Start daemon (foreground)
sigil daemon stop        # Stop running daemon
sigil daemon status      # Check daemon status
sigil daemon query ...   # Query via IPC

# Task lifecycle
sigil close mp-001 --reason "fixed in commit abc123"
sigil done mp-001                 # Close + update operations

# Workflow templates
sigil pipelinepour feature-dev --rig myproject --var issue_id=mp-001
sigil pipelinelist --rig myproject
sigil pipeline status mp-042

# Cross-project tracking
sigil operation create "payment-flow" mp-001 xx-002 yy-003
sigil operation status payment-flow

# Scheduled jobs
sigil cron add "nightly-check" --rig myproject --schedule "0 0 * * *" --prompt "Run health check"

# Skills
sigil skill run reviewer --rig myproject --prompt "the auth module"

# Memory
sigil recall "how does auth work?" --rig myproject
sigil remember "jwt-config" "24h expiry, httpOnly refresh tokens" --rig myproject

# Config
sigil config show
sigil config reload          # Send SIGHUP to daemon

# Diagnostics
sigil doctor --fix
sigil status
```

## Setup

1. Build: `cd /path/to/sigil && cargo build --release`
2. Install: `sudo ln -sf /path/to/sigil/target/release/sigil /usr/local/bin/sigil`
3. Configure: create `config/sigil.toml` (see [Project Setup](projects.md))
4. Secrets: `sigil secrets set OPENROUTER_API_KEY sk-or-...`
5. Start: `sigil daemon start`
