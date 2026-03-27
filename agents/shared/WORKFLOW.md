# Shared Workflow

These rules apply to ALL projects. Project-specific AGENTS.md may add to but never contradict these.

## Git Workflow

1. **Always work in worktrees** — never edit `dev` or `master` directly
2. Create worktree: `git worktree add ~/worktrees/feat/<name> -b feat/<name>`
3. Work, test, commit in the worktree
4. Merge to `dev` for auto-deploy to dev environment
5. Test on dev, then merge `dev` → `master` for production
6. Cleanup: `git worktree remove ~/worktrees/feat/<name> && git branch -d feat/<name>`

## Code Standards

| Rule | Rationale |
|------|-----------|
| NO COMMENTS | Code is self-documenting. `//!` and `///` on public APIs only. |
| NO BACKWARD COMPATIBILITY HACKS | No `_unused`, no `#[deprecated]`, no shims. Change everywhere or don't. |
| CONSISTENT NAMING | Same concept = same name across entire codebase. |
| DRY → SHARED CODE | See a pattern twice? Extract it. Three places = refactor. |
| BEST IMPLEMENTATION ONLY | Find the optimal approach. No "good enough". |
| SCHEMA = SOURCE OF TRUTH | DB changes must update schema files. Fresh setup must work. |

## Autonomous Execution Mandate

**Task assignment = full authorization to execute, for hours, without check-ins.**

When a task is assigned, the worker owns it end-to-end:
- Spawn sub-agents freely
- Make architectural decisions within the project
- Commit, deploy to dev, iterate — without pausing for approval
- Ambiguity in the spec? Make the best call. Document the decision. Keep moving.

Mid-task check-ins are a failure mode, not a safety mechanism. The Architect's silence is not confusion — it's trust.

## Worker Protocol

When executing a task, workers must signal their outcome:

- **Preferred format**: End with exactly one JSON object, no markdown fences:
  `{"status":"done|blocked|handoff|failed","summary":"...","reason":"...","next_action":"..."}`
- **done**: `summary` is the concise outcome. Mention files, checks, or deploy state when relevant.
- **blocked**: `summary` is work completed so far. `reason` is the exact question or missing input.
- **handoff**: `summary` is the checkpoint/resume brief for the next worker.
- **failed**: `summary` and `reason` should explain the technical failure and what was tried.
- **Legacy fallback**: If you absolutely cannot return valid JSON, use `BLOCKED:`, `HANDOFF:`, or `FAILED:` prefixes.

### What DOES qualify as BLOCKED

- External credential missing from config and not discoverable anywhere in the codebase
- Build failure that cannot be resolved after genuine attempts
- Architectural conflict where two valid paths require a human choice between funded alternatives (e.g., competing database schemas both of which require migration)

### What does NOT qualify as BLOCKED

- "I need design direction" — pick the best option and build it
- "I'm not sure which approach to take" — research, decide, execute
- "The spec doesn't mention X" — infer from context, make a decision
- "Should I proceed with Y?" — yes. Always yes unless it's destructive and irreversible
- Landing page copy, visual design choices, component structure — just build it

## Batch Dispatch

When multiple tasks are ready for a project, dispatch ALL of them — not sequentially with permission gates. The Architect delegates at the project level. A project with 4 ready tasks should have 4 workers running, not 1 worker running and 3 waiting for human approval that was never required.

The agent (or any orchestrator) must never:
- List ready tasks and ask "which one?"
- Dispatch one and wait for feedback before dispatching the next
- Treat task selection as requiring human input

Task triage is the orchestrator's job. If tasks have dependencies, sequence them. If they're independent, parallelize them.

## Sub-Agent Orchestration

Workers should use Sigil's delegation and tooling surfaces aggressively when available. Each worker IS an orchestrator.

### Adaptive Pipeline

All implementation work uses one adaptive pipeline:

1. **Discover**: inspect the relevant code, constraints, and prior checkpoints
2. **Plan**: define the intended change and verification path before editing
3. **Implement**: make the smallest coherent change that solves the task
4. **Verify**: run the strongest checks justified by the risk and scope
5. **Finalize**: summarize what changed, what was verified, and any remaining risks

The shape stays the same for every task. Only the depth changes:
- tiny tasks move through the phases quickly
- broad or risky tasks use more subagents, deeper planning, and stronger verification
- worktrees are preferred whenever repo changes are involved

### Subagent Prompt Templates

When spawning subagents, use these structured prompts for consistent results:

**Research** (Explore subagent — read-only, fast):
```
Agent tool:
  subagent_type: "Explore"
  description: "Research for <3-5 word summary>"
  prompt: |
    CONTEXT: <what you already know>
    OBJECTIVE: <specific research question>
    SCOPE: <directories/services to focus on>
    OUTPUT: Key Files (file:line), Patterns, Constraints, Recommendation
```

**Plan** (Plan subagent — read-only, architectural):
```
Agent tool:
  subagent_type: "Plan"
  description: "Design plan for <3-5 word summary>"
  prompt: |
    TASK: <what to build>
    RESEARCH: <findings from Explore agents>
    PRODUCE: Ordered file changes, risk assessment, rollback plan
```

**Develop** (general-purpose, isolated worktree):
```
Agent tool:
  subagent_type: "general-purpose"
  isolation: "worktree"
  model: "sonnet"
  description: "Implement <3-5 word summary>"
  prompt: |
    TASK: <implementation objective>
    PLAN: <plan output>
    DONE CRITERIA: compiles, tests pass, committed
```

**Review** (Explore subagent — read-only, thorough):
```
Agent tool:
  subagent_type: "Explore"
  description: "Review <3-5 word summary>"
  prompt: |
    REVIEW: git diff dev...HEAD in <worktree>
    CHECKLIST: security, correctness, patterns, performance, safety
    OUTPUT: PASS or FAIL with file:line issues
```

### Parallel Execution

Launch MULTIPLE subagents in a SINGLE message when their work is independent:
- 3 Explore agents for parallel research (architecture, patterns, impact)
- Multiple developer agents for independent services

### Pipeline Orchestrator Spec

For complex orchestration, read `subagents/pipeline-orchestrator.md` for the full protocol including handover file format and iteration rules.

## Escalation

If you genuinely cannot determine something from the codebase:
1. First try harder — check docs, configs, related code, git history
2. If truly stuck, respond with `BLOCKED:` and a specific question
3. The Supervisor will attempt project-level resolution (spawn another worker with your question)
4. If still stuck, escalates to Lead Agent (cross-project knowledge)
5. If Lead Agent can't resolve, escalates to human via Telegram

## Build & Deploy

Build, test, and deploy commands are specified in your Operating Instructions (AGENTS.md).
Follow those exactly. Do not search for or rely on repo-level CLAUDE.md files.

## Safety

- Never commit secrets or API keys to git
- Never edit files in `/var/www/` (auto-deployed, read-only)
- Never deploy to production without testing on dev first
- Never trust client-side values for server-side operations
