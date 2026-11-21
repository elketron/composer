---
title: Plans
type: index
tools: []
---
# Plans

Longer-lived intent: what we are trying to achieve and how. A plan outlives the sessions that carry it out. Some workflows also use tickets (pages of `type: ticket`, see [the template](_templates/ticket.md)) as units of work that link back to their plan.

## Layout

One page per plan: `<name>.md`, or `<name>/index.md` with sub-pages for large plans.

## Page format

```markdown
---
title: Server API
type: plan
status: active        # draft | active | done | dropped
project: composer      # the site of the project it belongs to
tags: [api]
updated: 2026-10-18
---
# Server API

## Goal
One or two sentences: what done looks like.

## Steps
- [x] Define WS message types
- [ ] HTTP endpoints → [ticket 0003](site://composer/tickets/0003.md)

## Decisions
- JSON over WS, seq-numbered events. Why: replay is reading the log.

## Notes
```

## Rules

- Update `status` and tick steps as work progresses.
- Link each step that became a ticket or a page of its own to it. The output itself is in the
  project's folder, not in the plan.
- Record decisions with their reason, so later work doesn't re-litigate them.
- When a plan is `done`, move lasting knowledge into [memory](site://memory) or the project's
  site, then close it.
