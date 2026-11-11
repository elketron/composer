# TUI mode

Status: first slice. Hunk is the control adapter; generic screen + keys is not built.

A **TUI capability** is a site page whose UI is an existing terminal program. It sits between a
micro app (a Svelte UI over pages) and a full app (its own window): the program already exists, the
human uses it, and the agent needs a small control surface — not a custom UI and not a screenshot
loop.

The first rule is: **do not make the agent play the TUI**. Use the program's own API when it has
one. Hunk is the model: its TUI is for the human, and `hunk session …` is for the agent.

## The page

```yaml
---
title: Review
type: tui
command: hunk diff
control: hunk
cwd: project
---
# Review
```

- `open` starts `command` in a terminal on the session's desk; the terminal is the human's view.
- `cwd` is `project` (default), `site`, `home`, or a path. `project` uses the work's `project/`
  view when the page is inside a work, else the session's project.
- `control` names a trusted adapter. The first slice supports `hunk`; the command must be `hunk`
  and plain arguments (no shell). A page cannot run an arbitrary command through `open`.
- `cols` / `rows` size the PTY (default 120×40).
- The body is the manual: what the human does, and which agent actions exist.

The TUI terminal runs outside the agent sandbox, like a desktop window: it is the user's client,
not an agent shell. The trusted `control` adapter decides the program.

## Hunk

`hunk` (the tool) drives the live session through the CLI; the TUI and the tool share the same
daemon:

| action | does |
|---|---|
| `sessions` | live reviews, their repos, selected file |
| `review` | files, hunks, counts, selected hunk |
| `notes` | the user's inline notes (`all: true` includes agent notes) |
| `reply` | answer a note in place (`note`, `text`) |
| `comment` | leave an agent note at `file` + `line` |
| `navigate` | move the user's view to a note, or a file and line |
| `export` | write a `type: review` page from `hunk session review --json` |

The live notes are the writeback: the user presses `c`, the agent reads `notes`, replies with
`reply`, and the reply appears under the note. The review page is the durable artifact, linked
from the work or plan. A hunk the user started outside Composer works too, as long as the session
project or the only live review points at its repo.

The site lists `hunk` in its `tools:`. The coding pipeline template ships `review.md` and
`tools: [hunk]`, so a coding site has the capability as soon as it exists.

## Later

- `control: git` for lazygit: the human uses the TUI, the agent uses `git`; the repo is the shared
  state.
- Generic `tui_screen` / `tui_keys` for programs with no control API: a terminal screen buffer
  (visible grid + cursor), named keys, and a key allowlist. Read-only first; keys only after a
  handoff.
