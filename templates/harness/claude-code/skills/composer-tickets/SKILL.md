---
name: composer-tickets
description: Use when working a composer ticket (site://orchestration/tickets/<id>), and always before you stop. Explains the ticket format, writing the Result section, and calling finish.
---

# Composer tickets

You were started on exactly one ticket: `site://orchestration/tickets/<id>`.

1. `open` the ticket. Read **Task**, **Context** (open the linked pages) and **Done when**.
2. Do the work in your current folder. When it is a ticket worktree (on branch `ticket/<id>`),
   commit your changes there; the orchestrator merges the branch later.
3. Check every **Done when** item yourself (run the tests the project has, if you may).
4. Before you stop, `edit` the ticket's **Result** section: what you did, where (files, commits,
   pages), and what is left over or uncertain. Don't touch the frontmatter or the Task.
5. Call `finish`:
   - `finish(status: "done", summary: "...")` when every Done-when item holds;
   - `finish(status: "failed", summary: "why")` otherwise.
   The summary is one or two sentences for the orchestrator. Calling `finish` is required: a run
   that ends without it counts as failed.

If you get feedback on the ticket later, work it in, update the Result section, and call
`finish` again.
