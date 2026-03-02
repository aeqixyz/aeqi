# Claude Code Integration

System workers execute as Claude Code CLI subprocesses. This document covers how the integration works and how to configure it.

## How Workers Run

When `execution_mode = "claude_code"` (the default for production projects), each worker spawns:

```bash
claude -p "<system_prompt + task>" \
  --output-format json \
  --max-turns 25 \
  --permission-mode bypassPermissions \
  --cwd /path/to/project/repo
```

**Key flags**:
- `-p` -- print mode (non-interactive, takes prompt as argument)
- `--output-format json` -- structured output with cost, turns, session ID
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

Claude Code returns JSON:

```json
{
  "result": "I fixed the login validation by...",
  "session_id": "abc123",
  "num_turns": 7,
  "total_cost_usd": 0.034,
  "duration_ms": 45000
}
```

The worker parses the `result` field for outcome:
- Starts with `DONE` or `DONE:` -> `TaskOutcome::Done`
- Starts with `BLOCKED:` -> `TaskOutcome::Blocked` (triggers escalation)
- Starts with `FAILED:` -> `TaskOutcome::Failed` (requeue with backoff)
- Starts with `HANDOFF:` -> `TaskOutcome::Handoff` (checkpoint + requeue)
- No prefix -> defaults to `Done` with the full text as summary

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

When the daemon is running, it listens on `~/.sigil/rm.sock` for JSON-line queries:

```bash
# Via the CLI
rm daemon query ping          # -> "pong"
rm daemon query status        # -> project counts, worker states, cost
rm daemon query projects      # -> project info JSON
rm daemon query dispatches    # -> recent dispatch messages
rm daemon query metrics       # -> Prometheus text exposition
rm daemon query cost          # -> budget status per project
```

Programmatic access:
```bash
echo '{"cmd":"status"}' | socat - UNIX-CONNECT:~/.sigil/rm.sock
```

## CLI Commands

```bash
# One-shot execution (creates temporary worker)
rm run "list files in current directory" --rig myproject

# Assign task (picked up by supervisor on next patrol)
rm assign "fix the login bug" --rig myproject --priority high

# Check ready work
rm ready --rig myproject

# Daemon management
rm daemon start       # Start daemon (foreground)
rm daemon stop        # Stop running daemon
rm daemon status      # Check daemon status
rm daemon query ...   # Query via IPC

# Task lifecycle
rm close mp-001 --reason "fixed in commit abc123"
rm done mp-001                 # Close + update operations

# Workflow templates
rm mol pour feature-dev --rig myproject --var issue_id=mp-001
rm mol list --rig myproject
rm mol status mp-042

# Cross-project tracking
rm raid create "payment-flow" as-001 rd-002 el-003
rm raid status payment-flow

# Scheduled jobs
rm cron add "nightly-check" --rig myproject --schedule "0 0 * * *" --prompt "Run health check"

# Skills
rm skill run reviewer --rig myproject --prompt "the auth module"

# Memory
rm recall "how does auth work?" --rig myproject
rm remember "jwt-config" "24h expiry, httpOnly refresh tokens" --rig myproject

# Config
rm config show
rm config reload          # Send SIGHUP to daemon

# Diagnostics
rm doctor --fix
rm status
```

## Setup

1. Build: `cd /path/to/sigil && cargo build --release`
2. Install: `sudo ln -sf /path/to/sigil/target/release/rm /usr/local/bin/rm`
3. Configure: create `config/system.toml` (see [Project Setup](projects.md))
4. Secrets: `rm secrets set OPENROUTER_API_KEY sk-or-...`
5. Start: `rm daemon start`
