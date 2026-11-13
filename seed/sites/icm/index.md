---
title: ICM
type: index
library: [AGENTS.md, CONTEXT.md, _shared/]
---
# ICM

The ICM Architect skill, installed as a site: design a process, idea or body of
knowledge into an ICM workspace, or restructure an existing folder, repo or vault
into one.

An ICM workspace replaces orchestration code with structure: numbered folders
carry sequence, hierarchy carries context scoping, plain markdown carries state.
One agent, reading the right files at the right moment, does the work; a person
can open any folder and see the state, because state is files.

## Start here

- [AGENTS.md](AGENTS.md) — the agent entry: where to go for a task.
- [CONTEXT.md](CONTEXT.md) — the method in one screen.
- [the skill](_shared/icm-architect/SKILL.md) — invariants, build/restructure, walk test.
- [the five forms](_shared/icm-architect/references/forms.md)
- [the core](_shared/icm-architect/references/core.md) — hierarchy, contracts, naming.
- Templates: `_shared/icm-architect/assets/templates/` · scripts: `_shared/icm-architect/scripts/`

## The five forms

| Form | Unit |
|---|---|
| **Pipeline** | the same steps run again, a deliverable each run |
| **Record library** | a record that accumulates (person, client, session, asset) |
| **Knowledge bundle** | the knowledge itself: how someone thinks, the ideas behind an artifact |
| **Context map** | an organization: teams, processes, data |
| **System map** | a tree later agents will edit: nouns, verbs, impact |

## Use it

1. Read `_shared/icm-architect/SKILL.md`.
2. Pick a form with `_shared/icm-architect/references/forms.md`.
3. Scaffold the smallest structure from
   `_shared/icm-architect/assets/templates/`; copy the scripts you need into the
   workspace's `_tools/`.
4. Write the contracts and run the walk test.

This site is the reference copy. Build in the site or folder being structured,
not here.
