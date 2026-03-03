# Development Workflow: Research → Develop → Review

## Overview

All non-trivial code changes follow the **R→D→R pipeline**:

```
┌─────────────────────────────────────────────────────────────────────┐
│                     FEATURE ORCHESTRATOR                            │
│                                                                     │
│  ┌───────────┐      ┌───────────┐      ┌───────────┐              │
│  │ RESEARCH  │ ───▶ │  DEVELOP  │ ───▶ │  REVIEW   │              │
│  │           │      │           │      │           │              │
│  │ • Explore │      │ • Worktree│      │ • HFT     │              │
│  │ • Plan    │      │ • Code    │      │   checks  │              │
│  │ • Handover│      │ • Commit  │      │ • Verdict │              │
│  └───────────┘      └───────────┘      └───────────┘              │
│        │                  │                  │                     │
│        ▼                  ▼                  ▼                     │
│   handover/          worktree/          PASS: merge               │
│   feat-X.yaml        feat/X             FAIL: fix & retry         │
└─────────────────────────────────────────────────────────────────────┘
```

## When to Use This Workflow

**USE for:**
- New features
- Bug fixes affecting multiple files
- Refactoring
- Performance improvements
- Any trading pipeline changes

**SKIP for:**
- Typo fixes
- Config-only changes
- Documentation updates

## Agents

| Agent | Role | Model | Tools |
|-------|------|-------|-------|
| `feature-orchestrator` | Coordinates pipeline | opus | All + Task |
| `researcher` | Explores, plans | sonnet | Read, Grep, Glob |
| `data-pipeline` | Implements data services | sonnet | All |
| `strategy-pipeline` | Implements strategy services | sonnet | All |
| `trading-pipeline` | Implements trading services | opus | All |
| `gateway-pipeline` | Implements gateway services | sonnet | All |
| `crate-modifier` | Modifies shared crates | opus | All |
| `code-reviewer-hft` | Reviews for anti-patterns | opus | Read, Grep, Write |

## Phase 1: Research

**Agent:** `researcher`

**Input:** User's feature request

**Process:**
1. Read relevant skills (pipeline, crates, services)
2. Explore codebase for patterns
3. Identify files to modify
4. Create implementation plan
5. Write handover file

**Output:** `.claude/handover/feat-<name>.yaml`

```yaml
feature_id: "feat-kraken-venue"
status: "research_complete"

request:
  description: "Add Kraken exchange adapter"
  acceptance_criteria:
    - "WebSocket connection"
    - "Trade normalization"
    - "ZMQ publishing"

research:
  files_to_modify:
    - path: "venues/kraken.rs"
      action: "create"
  implementation_plan:
    - step: 1
      description: "Create adapter skeleton"
```

## Phase 2: Develop

**Agent:** Pipeline-specific developer

**Input:** Research handover file

**Process:**
1. Create worktree: `git worktree add .../feat/<name> -b feat/<name>`
2. Implement according to plan
3. Follow canonical patterns from skills
4. Commit changes
5. Update handover file

**Output:** Code in worktree, commits ready

```bash
# Worktree created at
/home/claudedev/worktrees/feat/<name>

# Branch
feat/<name>
```

## Phase 3: Review

**Agent:** `code-reviewer-hft`

**Input:** Worktree with changes

**Process:**
1. Identify all changed files
2. Apply HFT review checklist:
   - Hot path allocations
   - Lock contention
   - Error handling
   - State machine violations (trading)
   - Async anti-patterns
3. Categorize issues by severity
4. Update handover with results
5. Provide verdict

**Output:** Review results in handover, PASS/FAIL verdict

```yaml
review:
  issues_found:
    - severity: "critical"
      issue: "String allocation in hot path"
  verdict: "FAIL"
```

## Phase 4: Iterate or Merge

**If FAIL:**
- Developer fixes issues
- Re-run review
- Repeat until PASS

**If PASS:**
```bash
# Merge to dev (auto-deploys)
cd /home/claudedev/algostaking-backend
git checkout dev
git merge feat/<name>

# Cleanup
git worktree remove /home/claudedev/worktrees/feat/<name>
git branch -d feat/<name>
```

## Handover File Lifecycle

```
1. Created by: researcher
   Status: research_complete

2. Updated by: developer
   Status: development_complete
   Added: commits, worktree path

3. Updated by: reviewer
   Status: review_complete (PASS/FAIL)
   Added: issues, verdict

4. Deleted after: successful merge
```

## Invoking the Pipeline

### Option 1: Full Orchestration (Recommended)

Ask Claude to use the feature-orchestrator:

> "Add Kraken exchange adapter"
> → Claude spawns feature-orchestrator
> → Orchestrator manages R→D→R flow

### Option 2: Manual Phase Control

Invoke agents individually:

```
1. "Research adding Kraken adapter"
   → researcher agent

2. "Implement Kraken adapter based on research"
   → data-pipeline agent

3. "Review the Kraken changes"
   → code-reviewer-hft agent
```

### Option 3: Skip Research (Simple Changes)

For well-understood changes:

```
"Implement X (skip research, I know exactly what's needed)"
→ Direct to developer agent
→ Still requires review
```

## Quality Gates

| Phase | Gate | Enforcement |
|-------|------|-------------|
| Research | Plan completeness | Orchestrator checks handover |
| Develop | Builds clean | `cargo build --release` |
| Review | No critical issues | Reviewer verdict |
| Merge | All gates passed | Manual merge command |

## Example: Full Flow

**User:** "Add rate limiting to the API service"

**1. Research (researcher):**
- Reads `.claude/skills/pipelines/gateway.md`
- Reads `.claude/skills/services/api.md`
- Finds existing middleware pattern
- Creates plan: add rate_limiter.rs, update router

**2. Develop (gateway-pipeline):**
- Creates worktree `feat/api-rate-limit`
- Implements rate_limiter.rs using tower middleware
- Adds config options
- Commits changes

**3. Review (code-reviewer-hft):**
- Checks for hot path issues (rate limiting is per-request)
- Finds: HashMap lookup per request → suggests LRU cache
- Verdict: WARNING (not critical, functional)

**4. Merge:**
- Developer optionally improves based on warning
- Merges to dev
- Auto-deploys
- Cleanup worktree

## Files

| File | Purpose |
|------|---------|
| `.claude/agents/feature-orchestrator.md` | Pipeline coordinator |
| `.claude/agents/researcher.md` | Research phase |
| `.claude/agents/*-pipeline.md` | Development phase |
| `~/.claude/agents/code-reviewer-hft.md` | Review phase |
| `.claude/handover/*.yaml` | State transfer |
| `.claude/skills/development-workflow.md` | This document |
