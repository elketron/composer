# ICM

Design or restructure a workspace where folders carry the method. This site is
the ICM Architect skill: read it before building or auditing an ICM workspace.

## Where things live

| Path | What it holds |
|---|---|
| `CONTEXT.md` | the method in one screen |
| `_shared/icm-architect/SKILL.md` | the skill: invariants, build/restructure, walk test |
| `_shared/icm-architect/references/` | forms, core, system-map, bundle-from-sources |
| `_shared/icm-architect/assets/templates/` | copyable starters |
| `_shared/icm-architect/scripts/` | links.py, index.py, viz.py, quotes.py |
| `index.md` | the web homepage; not the agent contract |

## Route by what you are doing

| If | Go to | Stop at |
|---|---|---|
| designing a new workspace | `_shared/icm-architect/SKILL.md`, then `_shared/icm-architect/references/forms.md` | the walk test |
| restructuring a folder | `_shared/icm-architect/SKILL.md` (Restructure mode), `_shared/icm-architect/references/core.md` | the walk test |
| writing a contract | `_shared/icm-architect/references/core.md`, `_shared/icm-architect/assets/templates/stage-CONTEXT.md` | its check |
| choosing a form | `_shared/icm-architect/references/forms.md` | one form |
| mapping a repo | `_shared/icm-architect/references/system-map.md` | the effects index and walk test |
| building from sources | `_shared/icm-architect/references/bundle-from-sources.md` | `quotes.py` passes |

## The one rule

The folder structure is the architecture; state is files; every step ends at a
check. Copy starters and scripts into the workspace you are building; this site
is the reference, not the workspace.
