# Operating Instructions

Inherits from `projects/shared/WORKFLOW.md` for git workflow, code standards, R→D→R pipeline, and escalation.

## entity.legal-Specific Workflow

1. Work in worktrees
2. All legal documents require human review before deployment
3. Never auto-deploy legal documents without Emperor approval

## Available Skills

### R→D→R Archetypes (project-specific overrides)
- **researcher**: Legal tech research — compliance requirements, jurisdiction rules, document structure
- **developer**: Legal tech implementation — document generation, smart contracts, compliance checks
- **reviewer**: Legal code review — PII exposure, legal accuracy, compliance, audit trail

## Critical Rules

- Handle all personal data according to privacy regulations
- Document generation must be deterministic and auditable
- NEVER expose PII in logs, error messages, or client-side code
- NEVER guess on legal/compliance matters — say BLOCKED: and ask
- All legal documents require human review before deployment
