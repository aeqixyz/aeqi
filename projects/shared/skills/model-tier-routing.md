```toml
[skill]
name = "model-tier-routing"
description = "Reference: model selection guidance for orchestrators. Use when deciding which model to dispatch for a task. Triggers: model selection, cost optimization, task routing."
phase = "discover"
```

# Model Tier Routing

Guide for selecting the right model tier based on task complexity. Orchestrators and delegation decisions should reference this.

## Tier System

| Tier | Model Class | Use When | Examples |
|------|------------|----------|---------|
| **T1 (Opus)** | Most capable | Architecture decisions, security review, complex debugging, multi-file refactors, production code review | Plan review, threat modeling, root cause analysis |
| **T2 (Sonnet)** | Balanced | Standard implementation, testing, multi-file changes, integration work | Feature implementation, bug fixes, documentation |
| **T3 (Haiku)** | Fast/cheap | Simple queries, formatting, single-file edits, status checks, memory extraction | Code formatting, simple lookups, observation analysis |

## Task Complexity Signals

**Route to T1 (Opus):**
- Touches 5+ files across modules
- Requires architectural judgment
- Security-sensitive code paths
- Production deployment decisions
- Conflict resolution between agents

**Route to T2 (Sonnet):**
- Standard implementation from a clear spec
- 1-4 files, single module
- Test writing, documentation
- Bug fixes with clear reproduction

**Route to T3 (Haiku):**
- < 160 chars input, < 28 words
- No code blocks, URLs, or complex keywords
- Status queries, simple lookups
- Memory consolidation, observation analysis
- Formatting and cleanup tasks

## Cost Awareness

| Model | Input $/1M tokens | Output $/1M tokens | Relative Cost |
|-------|-------------------|---------------------|---------------|
| Opus | $15 | $75 | 10x |
| Sonnet | $3 | $15 | 2x |
| Haiku | $0.25 | $1.25 | 1x (baseline) |

**Rule of thumb:** If T3 can do it, use T3. Don't use Opus for tasks Haiku can handle. The budget saved on simple tasks funds the complex ones.

## Agent-Level Defaults

| Agent Role | Default Tier | Override When |
|------------|-------------|---------------|
| CEO | T1 | Never — strategic decisions need capability |
| CTO | T1 for architecture, T2 for implementation review | T3 for status queries |
| CPO | T2 | T1 for complex UX decisions |
| CFO | T1 for strategy, T2 for analysis | T3 for data lookups |
| COO | T2 | T3 for health checks |
| GC | T1 for legal analysis | T2 for contract review |
| Subagent (explore) | T2 | T3 if simple search |
| Subagent (implement) | T2 | T1 if architectural task |
| Subagent (review) | T1 | T2 if mechanical check |
| Subagent (verify) | T2 | T1 if security verification |
