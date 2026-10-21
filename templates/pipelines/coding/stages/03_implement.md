---
title: Implement
type: stage
gate: false
harness: claude-code
worktree: true
---
# 03 · Implement

## Inputs

| | file | for |
|---|---|---|
| working | `02_plan/plan.md` | the steps |
| reference | `01_spec/spec.md` | what it should do |
| reference | `_config/conventions.md` | how code is written here |

## Process

Work through the plan in the project's ticket worktree, committing as you go. Run the project's checks.
When a step can't be done as planned, say why in the notes rather than changing the plan's intent.

## Outputs

- commits on the ticket's branch
- `03_implement/notes.md`: what was done, what differs from the plan, how it was checked
