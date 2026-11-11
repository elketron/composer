---
title: Review
type: tui
command: hunk diff
control: hunk
cwd: project
---
# Review

A live Hunk review of this work's project (the worktree while a coding stage runs). Opening this
page starts Hunk in a terminal on your desk; the user reviews there.

## For the user

- Press `c` on a line to leave a note for the agent.
- `?` shows Hunk's keys; `q` quits.
- The agent answers in place, so notes become a conversation, not a report.

## For the agent

Do not send keys to the Hunk terminal. Use the `hunk` tool against the live session:

- `hunk(action: "sessions")` — live reviews and their repos.
- `hunk(action: "review")` — files, hunks and counts.
- `hunk(action: "notes")` — the user's notes (add `all: true` for agent notes too).
- `hunk(action: "reply", note: "<id>", text: "…")` — answer a note in place.
- `hunk(action: "comment", file: "src/x.rs", line: 12, text: "…")` — leave an agent note.
- `hunk(action: "navigate", note: "<id>")` or `file` + `line` — move the user's view.
- `hunk(action: "export", url: "site://__PIPELINE__/review-result.md")` — a durable review page.

If Hunk says there is no live session, `open` this page first. If the review is in a worktree and
the session points at the project, pass `repo` with the work's `project/` path.
