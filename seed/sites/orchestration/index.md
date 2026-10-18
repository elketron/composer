---
title: Orchestration
type: index
---
# Orchestration

Tickets: units of work handed to worker agents. Each worker runs in its own session and works on exactly one ticket.

## Layout

| path | holds |
|------|-------|
| `tickets/<id>.md` | one ticket, id is 4 digits: `0001`, `0002`, ... |

## Actions

- `just new-ticket "<title>"` — creates the next ticket from the template and prints its URL. Use this instead of picking an id yourself.

## Ticket format

```markdown
---
title: Implement HTTP page endpoints
type: ticket
id: "0003"
status: open          # open | claimed | done | failed
agent: worker         # agent definition in site://agents
plan: site://plans/server-api
project: ~/Projects/composer
created: 2026-10-18
---
# Implement HTTP page endpoints

## Task
What to do, concretely.

## Context
Links to pages the worker should read first.

## Done when
- Checkable criteria.

## Result
Filled in by the worker: what was done, where, anything left over.
```

## Lifecycle

1. **Orchestrator** creates the ticket (`just new-ticket`), fills Task, Context and Done when, then `spawn_worker(id)`.
2. **Server** sets `status: claimed` when the worker starts.
3. **Worker** does the work, writes **Result**, then calls `finish(done | failed, summary)`.
4. **Orchestrator** gets a message when the worker finishes, reads the Result, updates the plan.

## Rules

- One ticket, one clear outcome. Split anything a worker can't finish in one session.
- Context is links, not copies: point at pages, don't paste them.
- Never edit a `claimed` ticket's Task while a worker is on it.
