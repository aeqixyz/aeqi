# Templates & Pipelines -- Workflow Templates

Templates are TOML-defined workflow templates that create linked task chains in a single command. When poured, a template creates a parent task with child step tasks linked by dependencies.

## Format

```toml
[template]
name = "feature-dev"
description = "Full feature development lifecycle"

[vars]
issue_id = { type = "string", required = true }
branch_name = { type = "string", required = false }

[[steps]]
id = "research"
title = "Research the requirement"
instructions = "Read issue {{issue_id}}, explore relevant code, understand the problem."
needs = []

[[steps]]
id = "plan"
title = "Plan the implementation"
instructions = "Design the solution approach. Create sub-tasks for discovered work."
needs = ["research"]

[[steps]]
id = "implement"
title = "Implement the solution"
instructions = "Write the code. Use worktree workflow. Commit with descriptive messages."
needs = ["plan"]

[[steps]]
id = "test"
title = "Test the implementation"
instructions = "Run full test suite. Fix any failures. Add missing test coverage."
needs = ["implement"]

[[steps]]
id = "review"
title = "Review and create PR"
instructions = "Self-review the code. Create PR to dev branch. Address any issues."
needs = ["test"]
```

## Usage

### Pour a template

```bash
rm mol pour feature-dev --rig algostaking --var issue_id=as-123
```

Creates a parent task with child steps linked by dependencies:

```
as-042 [Pending] feature-dev
  as-042.1 [Pending] Research the requirement       (ready -- no dependencies)
  as-042.2 [Pending] Plan the implementation        (blocked by as-042.1)
  as-042.3 [Pending] Implement the solution         (blocked by as-042.2)
  as-042.4 [Pending] Test the implementation        (blocked by as-042.3)
  as-042.5 [Pending] Review and create PR           (blocked by as-042.4)
```

Workers execute steps in order. When a step's task is closed, downstream steps become unblocked automatically.

### List available templates

```bash
rm mol list                     # All projects
rm mol list --rig algostaking   # Specific project
```

### Check pipeline progress

```bash
rm mol status as-042
```

Output:
```
as-042 [InProgress] feature-dev
Progress: 2/5

  [x] as-042.1 Research the requirement
  [x] as-042.2 Plan the implementation
  [~] as-042.3 Implement the solution       <- in progress
  [ ] as-042.4 Test the implementation
  [ ] as-042.5 Review and create PR
```

## Variable Interpolation

Variables declared in `[vars]` can be used in step instructions with `{{var_name}}`:

```toml
[vars]
issue_id = { type = "string", required = true }
priority = { type = "string", required = false }

[[steps]]
id = "research"
instructions = "Read issue {{issue_id}} and understand the problem."
```

Pass variables with `--var`:

```bash
rm mol pour template --rig myproject --var issue_id=mp-001 --var priority=high
```

Missing required variables cause an error. Missing optional variables are left as empty strings.

## Step Dependencies

Steps declare dependencies with the `needs` array:

```toml
[[steps]]
id = "test"
needs = ["implement"]    # Can't start until "implement" is done

[[steps]]
id = "deploy"
needs = ["test", "docs"]  # Needs BOTH test and docs completed
```

## Discovery & Override

Templates are loaded from two directories:

1. `projects/shared/pipelines/*.toml` -- shared templates (available to all projects)
2. `projects/<project>/pipelines/*.toml` -- project-specific templates

Project templates **override** shared templates with the same filename. This lets you customize workflows per project while keeping sensible defaults.

## Built-in Templates

### feature-dev.toml
5-step feature development: research -> plan -> implement -> test -> review

### incident.toml
5-step incident response: detect -> diagnose -> fix -> verify -> postmortem

## Location

Pipeline templates live in:
- Shared: `projects/shared/pipelines/*.toml`
- Project-specific: `projects/<project>/pipelines/*.toml`
