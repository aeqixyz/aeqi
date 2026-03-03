# Skill: Harness Evolution

**Invoke:** `/evolve-harness` or ask "improve the agent harness"

## Purpose

Spawns a meta-agent that analyzes and improves the agent harness itself—the skills, agents, workflows, and coordination mechanisms that power AlgoStaking development.

## When to Use

- After completing a major feature (lessons learned → harness improvement)
- When you notice friction in the development workflow
- Periodically (weekly/monthly) for harness hygiene
- When adding new services/crates that need skill coverage

## What It Does

The `harness-evolver` agent:

1. **Audits** current agents, skills, and workflows
2. **Identifies** gaps, redundancies, stale content, missing patterns
3. **Proposes** improvements with clear rationale
4. **Implements** approved changes
5. **Documents** the evolution

## Invocation

```
User: "Evolve the harness" or "/evolve-harness"
      ↓
harness-evolver agent spawns
      ↓
Analyzes .claude/ structure
      ↓
Proposes improvements
      ↓
Implements (with approval)
```

## Evolution Categories

| Category | Examples |
|----------|----------|
| **Gap filling** | Missing service skill, undocumented pattern |
| **Consolidation** | Merge redundant skills, dedupe agent logic |
| **Accuracy** | Update stale port numbers, fix outdated patterns |
| **Optimization** | Better agent prompts, tighter skill focus |
| **New patterns** | Extract learnings from recent work |
| **Meta** | Improve the evolver itself |

## Agent Location

`.claude/agents/harness-evolver.md`
