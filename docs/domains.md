# Project Setup Guide

A **project** is an isolated business unit in Sigil. Each project has its own:

- Git repository (working directory)
- Task store (`.tasks/` -- JSONL task DAG)
- Memory database (`.sigil/memory.db` -- SQLite + FTS5)
- Identity files (PERSONA.md, IDENTITY.md, AGENTS.md, KNOWLEDGE.md)
- Skills and workflow templates (pipelines)
- Worker pool (concurrent Claude Code executors)
- Checkpoints (`.sigil/checkpoints/` -- worker work-in-progress)

## Creating a Project

### 1. Add to config

In `config/sigil.toml`:

```toml
[[projects]]
name = "myproject"
prefix = "mp"                                    # Task ID prefix (mp-001, mp-002, ...)
repo = "/home/user/myproject"                    # Git repo path
model = "claude-sonnet-4-6"                      # LLM model for workers
max_workers = 3                                  # Max concurrent workers
execution_mode = "claude_code"                   # "claude_code" or "agent"
worker_timeout_secs = 1800                       # 30 min timeout for hung workers
worktree_root = "/home/user/worktrees"           # Git worktree root (optional)
max_turns = 25                                   # Max agentic turns per worker
```

### 2. Create the project directory

```bash
mkdir -p projects/myproject/{pipelines,skills,.tasks,.sigil/checkpoints}
```

Or run `sigil doctor --fix` to auto-create missing directories.

### 3. Write identity files

#### PERSONA.md -- Personality and Purpose

The core identity. Becomes the system prompt prefix. This defines *who* the agent is.

```markdown
You are the MyProject development agent. You manage a Next.js web application
with a PostgreSQL backend. You prioritize clean architecture, type safety,
and comprehensive test coverage.

You speak concisely and technically. You prefer action over discussion.
When you see a problem, you fix it. When you see an opportunity, you flag it.
```

#### IDENTITY.md -- Agent Identity

Structured metadata: name, style, expertise, repo location.

```markdown
# Identity

- **Name**: MyProject Worker
- **Style**: Concise, technical, action-oriented
- **Expertise**: TypeScript, Next.js, PostgreSQL, Prisma, Tailwind
- **Repo**: /home/user/myproject
- **Worktree**: /home/user/worktrees
```

#### AGENTS.md -- Operating Instructions

How the agent should work. These are project-specific instructions layered on top of the shared `WORKFLOW.md`.

```markdown
# Operating Instructions

## Before starting work
1. Check task status -- pick the highest priority ready task
2. Create a git worktree: `git worktree add ~/worktrees/feat/<name> -b feat/<name>`
3. Read relevant code before making changes

## While working
- Create sub-tasks for discovered work
- Commit frequently with descriptive messages (feat:, fix:, docs:, chore:)
- Run tests before marking work complete
- Follow the adaptive pipeline: Discover -> Plan -> Implement -> Verify -> Finalize

## After completing work
- Run full test suite
- Create PR to dev branch
- Close the task with a summary of changes
```

#### KNOWLEDGE.md -- Project Knowledge Base

Deep project-specific knowledge that workers need for context. This is the longest file and gets truncated by the context budget system (max ~12k chars).

```markdown
# MyProject Knowledge

## Architecture
- Next.js 14 App Router with server components
- PostgreSQL 16 with Prisma ORM
- Redis for session caching
- Deployed on Vercel (frontend) + Railway (API)

## Key Patterns
- All API routes in app/api/ use middleware for auth
- Database migrations in prisma/migrations/
- Shared types in lib/types.ts
- ...
```

#### HEARTBEAT.md -- Periodic Health Checks (optional)

Instructions for the heartbeat system. Workers run these periodically and report issues.

```markdown
# Heartbeat Checks

1. Check API health: `curl -s http://localhost:3000/api/health`
2. Check database: `npx prisma db execute --stdin <<< "SELECT 1"`
3. Check disk space: ensure > 10% free
4. Check for error logs in the last hour

If everything is OK, respond with "ALL OK".
If there are issues, describe them clearly with severity.
```

#### PREFERENCES.md -- Learned Preferences (optional)

Updated automatically by the reflection system. Contains preferences learned over time.

```markdown
# Preferences

- Always use `pnpm` instead of `npm`
- Prefer named exports over default exports
- Use zod for API validation, not manual checks
```

#### MEMORY.md -- Persistent Notes (optional)

Updated by reflection and gap analysis. Contains cross-session knowledge.

```markdown
# Memory

- Auth system uses JWT with 24h expiry, refresh tokens in httpOnly cookies
- The billing module was refactored on 2026-02-15 -- old endpoints deprecated
- Performance bottleneck: the /api/dashboard query joins 5 tables, needs optimization
```

## Directory Structure

After setup, your project should look like:

```
projects/myproject/
  PERSONA.md               <- personality/purpose
  IDENTITY.md              <- name, expertise, repos
  AGENTS.md                <- operating instructions
  KNOWLEDGE.md             <- project knowledge base
  HEARTBEAT.md             <- heartbeat check instructions (optional)
  PREFERENCES.md           <- learned preferences (reflection-updated)
  MEMORY.md                <- persistent notes (optional)
  skills/                  <- skill definitions (TOML)
    researcher.toml
    developer.toml
    reviewer.toml
  pipelines/               <- pipeline/template definitions (TOML)
    feature-dev.toml
    incident.toml
  .tasks/                  <- task storage (JSONL, git-native)
    mp.jsonl
  .sigil/
    memory.db              <- per-project SQLite + FTS5
    checkpoints/           <- worker checkpoint JSONs
      mp-001.json
    reflection-state.json  <- drift detection state
```

## Shared Templates

The `projects/shared/` directory contains templates inherited by all projects:

```
projects/shared/
  WORKFLOW.md              <- base workflow (adaptive execution pipeline, code standards)
  skills/                  <- shared skill archetypes
    researcher.toml
    developer.toml
    reviewer.toml
  pipelines/               <- shared pipeline templates
    feature-dev.toml
    incident.toml
```

Project-specific files in `projects/<name>/` **override** shared templates with the same filename.

## Context Layering

When a worker executes, its system prompt is built from these layers (in order):

```
1. Shared WORKFLOW.md           (max 2k chars)
2. PERSONA.md                   (personality)
3. IDENTITY.md                  (metadata)
4. AGENTS.md                    (instructions)
5. KNOWLEDGE.md                 (max 12k chars)
6. WORKER_PROTOCOL              (output format: DONE/BLOCKED/FAILED)
7. Checkpoint context           (max 8k chars, 5 most recent)
8. Memory recall                (relevant memories from SQLite)
9. Repo CLAUDE.md               (auto-discovered by Claude Code via --cwd)
```

Total budget: ~40k chars (~10k tokens). The `ContextBudget` system truncates layers at newline boundaries and summarizes old checkpoints as one-liners.

## Skills (Magic)

TOML-defined specialized behaviors:

```toml
# projects/myproject/skills/reviewer.toml

[skill]
name = "reviewer"
description = "Code review specialist"
triggers = ["review", "code review", "PR review"]

[prompt]
system = """
You are a code review specialist. Focus on:
- Security vulnerabilities (OWASP top 10)
- Performance anti-patterns
- Type safety gaps
- Missing test coverage
"""
user_prefix = "Review: "

[tools]
allow = ["shell", "file_read", "list_dir"]
deny = ["file_write"]
```

Run: `sigil skill run reviewer --rig myproject --prompt "the auth module"`

## Tasks

Each project's tasks are JSONL files in `.tasks/`:

```bash
# Create a task
sigil assign "Fix login bug" --rig myproject --priority high

# Check ready (unblocked) tasks
sigil ready --rig myproject

# Show all open tasks
sigil beads --rig myproject

# Close a task
sigil close mp-001 --reason "fixed in commit abc123"

# Mark done (also updates operations)
sigil done mp-001
```

Task IDs are hierarchical: `mp-001` (parent) -> `mp-001.1` (child) -> `mp-001.1.1` (grandchild).

## Memory

Per-project memory in `.sigil/memory.db` (SQLite + FTS5 + vector similarity):

```bash
# Store a memory
sigil remember "auth-flow" "Login uses JWT with 24h expiry" --rig myproject

# Search memories
sigil recall "how does authentication work?" --rig myproject
```

Hybrid search: BM25 keyword matching + cosine vector similarity + temporal decay (30-day half-life). Results ranked by configurable weights (`vector_weight`, `keyword_weight`).

## Budget Control

Per-project budgets can be configured alongside the global daily cap:

```toml
# In sigil.toml
[security]
max_cost_per_day_usd = 10.0    # Global cap

# Per-project (optional -- falls back to global)
[project_budgets]
project-alpha = 5.0
project-beta = 3.0
```

The supervisor checks `can_afford_project()` before spawning any worker. Budget status visible via `sigil daemon query cost`.

## Verification

```bash
sigil doctor         # Check all projects
sigil doctor --fix   # Auto-create missing directories/files
```

Checks: config validity, project directories, identity files, task store, skills, memory DB, secret store.
