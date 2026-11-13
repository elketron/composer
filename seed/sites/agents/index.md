---
title: Agents
type: index
library: ["*.md"]
tools: []
---
# Agents

Agent definitions. Each page is one agent: frontmatter configures it, the body is its system prompt. The server builds the full prompt as: agent body → the site's index (Home when outside a site) → active project → relevant memory.

## Agents

| agent | role |
|-------|------|
| [default](default.md) | the agent you chat with; the same agent runs a ticket as a worker |
| [orchestrator](orchestrator.md) | splits plans into tickets, starts default workers, reviews and merges |

There are two agents, not one per kind of work. How a ticket is worked is **Context**, not an
agent: the ticket links the site pages that say how to code, review or design. A worker is the
default agent with the server's worker mode (it gets the `finish` tool and a worker prompt) and
the ticket in its first message.

`spawn_worker(id)` starts the default agent on a ticket. The ticket's Context must name the
how-to page for that kind of work. `continue_worker` sends a finished worker back with a note;
`stop_worker` ends it.

## Base tools and site tools

`tools:` on an agent page is its **base set**: the verbs it has anywhere. A site's `index.md`
declares its own `tools:`; `enter_site` adds them from the next turn while the agent is inside,
and `leave_site` removes them. The site page also replaces Home in the prompt, so the agent learns the site's
conventions when it gains its verbs.

- Put browsing, editing and coordination in the agent's base set.
- Put a verb that only makes sense on one kind of thing in that site: `merge_ticket` in
  [orchestration](site://orchestration), `new_project` in [projects](site://projects),
  `new_site`/`new_app` in [home](site://home).
- A worker runs the default agent with the worker prompt and the ticket in its first message; the site pages in its Context say how the work is done.
- `sites:` (read/write access) is separate from tools: it decides which sites the agent may read
  and write at all.

## Page format

```markdown
---
title: Worker
type: agent
name: default
description: what this agent is for
model: default          # "default" = the provider's model
tools: [open, search, list_sites, write, edit, set_project, enter_site, leave_site, actions, run, finish]
sites: all              # or a list: [memory, plans]
---
System prompt here.
```

## Rules

- Keep prompts short: the site's index already explains its conventions, and Home explains the
  web; don't repeat either.
- `tools` and `sites` are enforced by the server, not just advice.
- MCP servers set up in the user's `config.toml` (the TUI's `:mcp` lists them) add tools named
  `<server>__<tool>`. List `github__*` (or one, `github__get_issue`) in `tools` to give them to an agent;
  one without a `tools` list gets none. Their calls ask the user unless the server allows them.
- Changing an agent changes all future sessions using it; tell the user when you do.
