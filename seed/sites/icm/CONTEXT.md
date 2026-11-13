# ICM - the method

One job: hold the ICM Architect skill so any agent can design or restructure a
workspace from files, without an external skill install.

## In one screen

An ICM workspace is plain folders and markdown. Three primitives: **sequence**
(numbered stages), **shelf** (same-shaped notes) and **catalog** (a small file
that points down). Five forms: Pipeline, Record library, Knowledge bundle,
Context map, System map. Every working folder has a `CONTEXT.md` with inputs,
process, outputs and one check. Factory (`_shared/`, `_templates/`, `_tools/`)
is stable; product (`output/`, records) is new per run. Status is what exists on
disk.

The invariants are in `_shared/icm-architect/SKILL.md`; they are defaults,
broken only on purpose and written down where they break.

## The skill

| Path | Holds |
|---|---|
| `_shared/icm-architect/SKILL.md` | the invariants, build/restructure modes, the walk test |
| `_shared/icm-architect/references/forms.md` | the five forms in depth |
| `_shared/icm-architect/references/core.md` | five-layer hierarchy, contract format, naming |
| `_shared/icm-architect/references/system-map.md` | the System map audit pipeline |
| `_shared/icm-architect/references/bundle-from-sources.md` | a Knowledge bundle from sources |
| `_shared/icm-architect/assets/templates/` | copyable starters |
| `_shared/icm-architect/scripts/` | `links.py`, `index.py`, `viz.py`, `quotes.py` |

## Check

The walk test: an agent with no memory can orient, act and report status from
the files alone. In this site, `just links` must pass.
