---
title: Agents
type: index
---
# Agents

Agent definitions. Each page is one agent: frontmatter configures it, the body is its system prompt. The server builds the full prompt as: agent body → [Home](site://home) → active project → relevant memory.

## Agents

| agent | role |
|-------|------|
| [default](default.md) | general assistant; the agent you chat with |
| [orchestrator](orchestrator.md) | breaks plans into tickets and runs workers |
| [worker](worker.md) | carries out one ticket |

## Page format

```markdown
---
title: Worker
type: agent
name: worker
model: default          # "default" = the provider's model
tools: [open, search, list_sites, write, edit, actions, run, finish]
sites: all              # or a list: [memory, plans]
---
System prompt here.
```

## Rules

- Keep prompts short: [Home](site://home) already explains the web, don't repeat it.
- `tools` and `sites` are enforced by the server, not just advice.
- Changing an agent changes all future sessions using it; tell the user when you do.
