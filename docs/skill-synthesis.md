# Skill System Synthesis

Sources: Sigil, Deer Flow, Hermes, Superpowers, Everything-Claude-Code, Claude-Skills, Learn-Claude-Code, Open SWE

## Current Sigil Skill Lifecycle

```
TOML on disk → Task.skill field → Supervisor loads prompt.system → Identity injection → Worker executes
```

Works for prompt injection. Missing: verification, composition, scoping, conditional activation, compliance.

## The Evolved Skill Schema

```toml
[skill]
name = "deploy-service"
description = "Deploy a service to production with rollback safety"
version = "1.2.0"
triggers = ["deploy", "release", "ship", "production"]

[skill.conditions]
requires_tools = ["shell", "git_worktree"]       # Hide if tools unavailable
requires_expertise = ["engineer"]                  # Route to these agents
platforms = ["linux"]                              # OS filter
estimated_cost_tokens = 50000
estimated_minutes = 10

[skill.verification]
commands = ["cargo test", "curl -s http://localhost/health"]
expected_patterns = ["0 failed", "ok"]
evidence_required = true                           # Must show output, not just claim

[skill.phases]
order = ["preflight", "execute", "verify", "rollback-check"]

[skill.phases.preflight]
description = "Check prerequisites"
steps = ["Verify branch is clean", "Run existing tests", "Check deployment target"]
gate = true                                        # Must pass before next phase

[skill.phases.execute]
description = "Deploy the service"
steps = ["Build release", "Push to target", "Run migrations"]

[skill.phases.verify]
description = "Confirm deployment"
steps = ["Health check endpoint", "Smoke test critical paths", "Check error rates"]

[skill.phases.rollback-check]
description = "Ensure rollback is possible"
steps = ["Verify previous version tagged", "Test rollback procedure"]

[skill.red_flags]
patterns = ["skip tests", "force push", "just deploy", "works on my machine"]
action = "halt"                                    # halt or warn

[skill.composition]
next_skill = "post-deploy-monitor"                 # Chain to next skill on completion
fallback_skill = "rollback-service"                # On failure, invoke this

[tools]
allow = ["shell", "read_file", "write_file", "git_worktree"]
deny = ["delegate"]

[prompt]
system = """You are deploying a service to production..."""
```

## What's New vs Current Sigil

| Field | Current | New | Source |
|-------|---------|-----|--------|
| `conditions.requires_tools` | No | Filter skill visibility by available tools | Hermes |
| `conditions.requires_expertise` | No | Route to capable agents only | Sigil-native |
| `verification.commands` | No | Run actual checks before marking DONE | Superpowers |
| `verification.evidence_required` | No | Workers must show output, not just claim | Superpowers |
| `phases` | No | Multi-step with gates between phases | Superpowers |
| `red_flags` | No | Detect rationalizations, halt or warn | Superpowers |
| `composition.next_skill` | No | Skill A → Skill B chaining | Superpowers |
| `composition.fallback_skill` | No | On failure, invoke fallback | Sigil-native |
| `version` | No | Track skill evolution | Hermes |
| `estimated_cost_tokens` | No | Budget prediction | Sigil-native |
| `platforms` | No | OS/environment filter | Hermes |

## The 5 Gaps to Close

### 1. Verification Criteria (from Superpowers)

Superpowers' core insight: "NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE."

Every skill should encode what "done" looks like as executable checks, not prose.
Integration: VerificationPipeline reads `skill.verification.commands` and runs them
after worker reports DONE. If checks fail, task is rejected.

### 2. Phased Execution with Gates (from Superpowers)

Skills define phases. Gates between phases are hard stops — cannot proceed until
the gate passes. Supervisor tracks phase state in task metadata.

```
preflight [gate] → execute → verify [gate] → rollback-check
```

If preflight gate fails, worker doesn't proceed to execute. This prevents
"implementation before design" and "deploy before test" patterns.

### 3. Project-Scoped Learning (from Everything-Claude-Code)

Current Sigil: all pattern memories are per-project but skill promotion is global.
ECC insight: scope instincts to projects, promote to global only when seen in 2+ projects.

Implementation:
- SkillPromoter tracks which project each pattern came from
- Patterns from same project cluster into project-specific skill candidates
- Only promote to shared/skills/ when pattern appears across 2+ projects
- Prevents cross-contamination (trading patterns don't become engineering skills)

### 4. Conditional Activation (from Hermes)

Skills should be hidden when their requirements aren't met:
- `requires_tools`: hide if shell tool not available
- `requires_expertise`: only show to agents with matching expertise
- `platforms`: hide on incompatible OS

Integration: Supervisor's `load_skill_prompt()` checks conditions before injection.
If conditions fail, skill is silently skipped and next candidate is tried.

### 5. Compliance Testing (from Everything-Claude-Code)

Can workers actually follow skills? Measure it.
- Auto-generate expected behavioral sequences from skill phases
- Run test tasks with skill active
- Classify tool calls against expected steps
- Score compliance: 85%+ = skill is effective, <85% = skill needs revision

Long-term: feed compliance scores back into skill promotion confidence.

## Implementation Priority

1. **Extend Skill struct** with new fields (conditions, verification, phases, red_flags, composition)
2. **Wire tool gating** — the `is_tool_allowed()` method exists but is never called in supervisor
3. **Wire verification checks** into the existing VerificationPipeline
4. **Add phase tracking** to task metadata
5. **Add composition** — supervisor checks `next_skill` after DONE and creates follow-up task
6. **Conditional activation** — supervisor filters skills by agent expertise + available tools
7. **Project-scoped promotion** — SkillPromoter tracks source project per pattern

## The Workflow

```
Synthesis: read competitor repos → extract patterns → compare to Sigil →
design superior solution → implement → test → document → repeat

This workflow itself is a skill. It can be applied to any capability area:
memory, execution, verification, proactive, chat, identity.

The goal: excellence through informed synthesis, not imitation.
```
