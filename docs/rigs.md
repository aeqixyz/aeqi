# Rig Setup Guide

A **rig** is an isolated Business Unit in Sigil. Each rig has its own:
- Git repository (workdir)
- Beads task store (`.beads/`)
- Memory database (`.sigil/memory.db`)
- Identity files (SOUL.md, IDENTITY.md, AGENTS.md)
- Skills and molecule templates

## Creating a Rig

### 1. Add to config

In `config/sigil.toml`:

```toml
[[rigs]]
name = "myproject"
prefix = "mp"                              # Bead ID prefix (e.g. mp-001)
repo = "/home/user/myproject"              # Git repo path
model = "anthropic/claude-sonnet-4-20250514"  # Override default model (optional)
max_workers = 4                            # Max concurrent workers
worktree_root = "/home/user/worktrees"     # Git worktree root (optional)
```

### 2. Create the rig directory

```bash
mkdir -p rigs/myproject/{molecules,skills,.beads,.sigil}
```

Or run `sg doctor --fix` to auto-create missing directories.

### 3. Write identity files

#### SOUL.md — System prompt

The core personality and purpose of the agent. This becomes the system prompt prefix.

```markdown
You are the MyProject development agent. You manage a web application
built with React + Node.js. You prioritize code quality, test coverage,
and clean architecture.
```

#### IDENTITY.md — Agent identity

Name, communication style, and expertise areas.

```markdown
# Identity

- **Name**: MyProject Agent
- **Style**: Concise, technical, action-oriented
- **Expertise**: React, TypeScript, Node.js, PostgreSQL
- **Repo**: /home/user/myproject
```

#### AGENTS.md — Operating instructions

Instructions for how the agent should work. These are appended to the system prompt.

```markdown
# Operating Instructions

## Before starting work
1. Check `beads_ready` for unblocked tasks
2. Pick the highest priority task
3. Update the bead status to `in_progress`

## While working
- Create sub-tasks as beads for discovered work
- Commit frequently with descriptive messages
- Run tests before marking work complete

## After completing work
- Close the bead with `beads_close`
- Report completion via mail
```

#### HEARTBEAT.md — Periodic checks (optional)

Instructions for the heartbeat health check. The agent runs these periodically and reports issues.

```markdown
# Heartbeat Checks

1. Check if the API server is responding: `curl -s http://localhost:3000/health`
2. Check database connectivity
3. Check disk space: `df -h /`
4. Check for error logs in the last hour

If everything is OK, respond with "ALL OK".
If there are issues, describe them clearly.
```

## Skills

Skills are specialized agent behaviors defined as TOML files in `rigs/<name>/skills/`.

```toml
# rigs/myproject/skills/code-reviewer.toml

[skill]
name = "code-reviewer"
description = "Review code for anti-patterns and issues"
triggers = ["review", "code review"]

[prompt]
system = """
You are a code review specialist. Focus on:
- Security vulnerabilities
- Performance anti-patterns
- Code style consistency
- Test coverage gaps
"""
user_prefix = "Review the following: "

[tools]
allow = ["shell", "file_read", "list_dir"]
deny = ["file_write"]
```

Run a skill:

```bash
sg skill run code-reviewer --rig myproject --prompt "the auth module"
```

## Molecules

Molecules are workflow templates in `rigs/<name>/molecules/`. See [molecules.md](molecules.md) for details.

## Beads

Each rig's tasks are stored in `.beads/` as JSONL files, namespaced by the rig's prefix.

```bash
# Create a task
sg assign "Fix login bug" --rig myproject --priority high

# Check ready work
sg ready --rig myproject

# Close a task
sg close mp-001 --reason "fixed in commit abc123"
```

## Memory

Per-rig memory is stored in `.sigil/memory.db` (SQLite + FTS5). The hybrid search engine combines keyword (BM25) and vector similarity with temporal decay.

```bash
# Store a memory
sg remember "auth-flow" "Login uses JWT with 24h expiry, refresh tokens in httpOnly cookies" --rig myproject

# Search memories
sg recall "how does authentication work?" --rig myproject
```

## Verification

Run diagnostics to verify your rig setup:

```bash
sg doctor
```

This checks:
- Config validity
- Rig directory existence
- Identity files (SOUL.md, IDENTITY.md)
- Beads directory
- Skills and molecules
- Memory database
- Secret store
