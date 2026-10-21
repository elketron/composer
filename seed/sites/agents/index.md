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
| [stage](stage.md) | runs one stage of a pipeline work (`run_stage`), with only that stage's context |

Worker agents (`role: worker`) are the ones `spawn_worker` can start. Add more for specific jobs
(a `coder`, a `reviewer`, a `tester`): copy [worker](worker.md), change its prompt and tools, keep
`role: worker` and the `finish` tool, and give it a `description:`. Orchestrators see the list.

## Page format

```markdown
---
title: Worker
type: agent
name: worker
role: worker            # spawn_worker can start it (needs the finish tool)
description: carries out one ticket   # shown to orchestrators
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
