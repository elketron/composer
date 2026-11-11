---
title: Coding
type: pipeline
needs_project: true
tools: [hunk]
stages:
  - { id: 01_spec, file: stages/01_spec.md, needs: [] }
  - { id: 02_plan, file: stages/02_plan.md, needs: [01_spec] }
  - { id: 03_implement, file: stages/03_implement.md, needs: [02_plan] }
  - { id: 04_review, file: stages/04_review.md, needs: [03_implement] }
---
# Coding

Staged changes to a project: spec → plan → implement → review, with a look from you after each step.

Each piece of work is a folder here (`site://__PIPELINE__/<work>/`) that links a project, normally
`~/Projects/<name>`. The code stays in the project: the implement stage changes it in a ticket worktree, the
review stage merges it back. The work's folder keeps the paper trail: brief, spec, plan, review.

## Stages

| stage | does | gate |
|-------|------|------|
| [01 spec](stages/01_spec.md) | turns the brief into a spec: what changes, what doesn't, how to tell it works | yes |
| [02 plan](stages/02_plan.md) | the steps, files and tests, in order | yes |
| [03 implement](stages/03_implement.md) | makes the change in a ticket worktree | no |
| [04 review](stages/04_review.md) | checks the change against the spec; you merge | yes |

## Review

[review.md](review.md) opens the change in Hunk, a terminal review TUI. You review; the agent
reads your notes with `hunk(action: "notes")` and answers with `hunk(action: "reply")`. It can
also navigate your view and export the review to a page. The Hunk terminal is yours: the agent
drives the live session, not the keys.

## Layout

| path | holds |
|------|-------|
| `_config/` | rules every work follows (`conventions.md`, `review.md`); a work's own `_config/` file wins |
| `stages/` | the stage contracts: what each stage reads, does and writes |
| `<work>/index.md` | the work: its project, brief, state and the stages it is running |
| `<work>/NN_stage/` | what the stage wrote |

## Works
