---
title: Orchestration
type: index
tools: [merge_ticket, new_work, run_stage]
# The ticket board: everything on Home, a project's own tickets inside one (git projects:
# tickets work in worktrees).
app:
  scope: both
  when: [git]
  rail: { icon: board, label: Tickets }
  tile: true
---
# Orchestration

Tickets and pipeline work: units handed to worker agents. Each worker runs in its own session and
works on exactly one ticket. Entering this site adds `merge_ticket`, `new_work` and `run_stage`;
the rest are base tools.

## Layout

| path | holds |
|------|-------|
| `tickets/<id>.md` | one ticket, id is 4 digits: `0001`, `0002`, ... |
| `tickets/` | the board app's state |
| `<pipeline>/<work>/` | one piece of pipeline work (see [Home](site://home)) |

## Actions

- `just new-ticket "<title>"` — creates the next ticket from the template and prints its URL. Use this instead of picking an id yourself.
- `just ui-build` — builds the ticket board app (open it from the TUI with `a`).

## Ticket format

```markdown
---
title: Implement HTTP page endpoints
type: ticket
id: "0003"
status: open          # open | claimed | done | failed
agent: worker         # a worker agent (role: worker) in site://agents; spawn_worker's agent overrides
plan: site://plans/server-api
project: site://projects/composer   # or a path or name; the worker works in its project/ view
attachments: []       # files from the chat, by the refs your message lists: ["<session>/<file>"]
created: 2026-10-18
merge_into: dev       # optional: where merge_ticket lands this ticket's branch
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

1. **Orchestrator** creates the ticket (`run orchestration new-ticket <title>`), fills Task, Context and Done when, then `spawn_worker(id)`. It returns right away; the worker runs in its own process and session.
2. **Server** sets `status: claimed` when the worker starts. In a git project the worker's `project/` is a worktree on branch `ticket/<id>` (the ticket's `branch:`), made from the project's last commit; it writes there, not to the project.
3. **Worker** does the work, writes **Result**, then calls `finish(done | failed, summary)`.
4. **Orchestrator** gets a message when the worker finishes (the server sets `status: done` or `failed`), reads the Result, updates the plan. A worker that dies without `finish` counts as failed.
5. **Merge:** `merge_ticket(id)` merges the branch into the target branch — the `into` argument, the ticket's `merge_into:` frontmatter, or the project's checked-out branch, in that order (a target that isn't checked out is merged in a temporary worktree) — once the user approves (they see the changes), then removes the worktree and branch; the ticket gets `merged:`. The user can also merge or discard it themselves. Conflicts: send the worker a message to merge the project's branch into its own and resolve them.

## Pipelines

A pipeline site (`type: pipeline`) declares its stage graph on its `index.md` (`stages:` with
`id`, `file`, `needs`, `gate`) and keeps a contract per stage in `stages/`. A work is a folder with
its brief and **state on its `index.md`**: `stage`, `status`, `done`, `attempts`, plus each run's
ticket and input hashes. `new_work` makes one on a project; `run_stage` starts a stage explicitly
with only that stage's context. Nothing on the server starts the next stage: Run (the board, the
TUI, an agent) does. A gate is `status: review`; approve it by writing `status: approved` and the
stage into `done:`, then run the next stage. The work's `project/` follows its stage's mode:
`worktree` while code changes, `read` while it reviews.

## Rules

- One ticket, one clear outcome. Split anything a worker can't finish in one session.
- Context is links, not copies: point at pages, don't paste them.
- Never edit a `claimed` ticket's Task while a worker is on it.
- A ticket's output lands in its project's `project/` view. Point the Done-when checks at that
  view (tests, linter, a script), not at the user's screen.
