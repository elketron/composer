# TUI pages and terminals

Composer shows terminal programs in two ways: terminals on a session's stage, and `type: tui`
pages that start one trusted program.

## Terminals on the stage

Every session can have terminals, owned by the server. The TUI shows them as tabs in its second
pane and the desktop on its stage; the phone lists the session's background processes and can stop
them.

- In the TUI, `:term [command]` opens a terminal (a shell, or `nvim`, `lazygit`, ...),
  `Ctrl-Space` leaves it, `:ps` lists the session's background processes (an id opens its
  terminal) and `:stop` stops one.
- Terminals start in the folder of the session's site (`folder:` in its `index.md`).
- The agent reads a terminal with `terminal_read` and types into one with `terminal_send`; each
  command is approved by the user (or always, for that command).
- `process_start` runs a long command (a dev server, a watcher) in a terminal of its own. Its exit
  code and last lines come back to the session as a message. Processes end with `process_stop`,
  from a client, or when the server stops.

## `type: tui` pages

A **TUI page** is a site page whose UI is an existing terminal program. The program already
exists, the person uses it, and the agent works through the program's own API, not its keys.

```yaml
---
title: Review
type: tui
command: hunk diff main
control: hunk
cwd: folder
---
# Review
```

- `open` on the page starts `command` in a terminal on the session's stage.
- `control` names a trusted adapter. Only `hunk` exists. The command must be `hunk diff`, `show`,
  `status` or `log` with plain arguments: no shell, no extensions. A page cannot run an arbitrary
  command through `open`.
- `cwd` is `folder` (default: the folder of the session's site), `site`, `home`, or a path inside
  the site or that folder.
- `cols` / `rows` size the terminal (default 120×40).
- The body is the manual: what the person does there, and what the agent may do.

The agent drives Hunk with its CLI (`hunk session …`) through a recipe or a terminal; composer has
no Hunk tool of its own.

## Later

- `control: git` for lazygit: the person uses the TUI, the agent uses `git`.
- A generic screen and key reader for programs with no control API.
