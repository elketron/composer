---
title: Plans
type: index
tools: []
---
# Plans

Longer-lived intent: what we are trying to achieve and how. A plan outlives tickets; tickets are the units of work that carry a plan out and link back to it.

## Layout

One page per plan: `<name>.md`, or `<name>/index.md` with sub-pages for large plans.

## Page format

```markdown
---
title: Server API
type: plan
status: active        # draft | active | done | dropped
project: site://projects/composer   # the project hub it belongs to
tags: [api]
updated: 2026-10-18
---
# Server API

## Goal
One or two sentences: what done looks like.

## Steps
- [x] Define WS message types
- [ ] HTTP endpoints → [ticket 0003](site://orchestration/tickets/0003)

## Decisions
- JSON over WS, seq-numbered events. Why: replay is reading the log.

## Notes
```

## Rules

- Update `status` and tick steps as work progresses.
- Link each step that became a ticket to that ticket, and each longer run of work to its work in
  the pipeline site. The output itself is in the project's `project/` view, not in the plan.
- Record decisions with their reason, so later work doesn't re-litigate them.
- When a plan is `done`, move lasting knowledge into [memory](site://memory) or the
  [project page](site://projects), then close it.
