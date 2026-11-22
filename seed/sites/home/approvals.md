---
title: Approvals policy
type: policy
tags: [approvals, system1]
---
# Approvals policy

The rules system-1 (`[system1]` in `config.toml`) follows when it approves, rejects or forwards
the agent's actions for you: terminal commands, background processes, MCP and outside agents'
tool calls, recipe runs. A site can add its own rules in `<site>/approvals.md`. This page counts
only in a version you approved; after an edit you are asked again.

## Approve

- Reading, listing and searching files; `git status`, `git diff`, `git log`.
- Building, testing, linting and formatting the project the session works on (`cargo test`,
  `npm test`, `just check`, ...), and running its dev server.
- Editing files inside the site's folder when that is what I asked for.
- Read-only MCP tools (get, list, search, screenshot).

## Reject

- Deleting or overwriting files outside the site's folder, `rm -rf` on anything broad.
- Reading or printing secrets: keys, tokens, `.env` files, password stores, `~/.ssh`.
- Sending data to other machines, except fetching dependencies for a build.
- `sudo`, installing system-wide, changing system settings or permissions.

## Ask me

- `git push`, `git reset --hard`, rewriting history, tags and releases.
- Publishing, deploying, sending messages or email, anything that costs money.
- Anything irreversible I didn't clearly ask for.
