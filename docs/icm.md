# Reading ICM correctly

Status: correction, 2026-11-13. Source: [RinDig/icm-architect](https://github.com/RinDig/icm-architect)
(MIT), the skill for the ICM paper (Van Clief & McDermott,
[arXiv:2603.16021](https://arxiv.org/abs/2603.16021)).

## The short version

ICM is **folder structure as agent architecture**. One agent reads the right
files at the right moment; the structure does the orchestration. There is no
scheduler, no stage runner, no board, no database.

Composer's previous reading treated ICM as a **pipeline engine**: numbered
stages, `run_stage`, gates, `_stage.md`, a generic board. That is one *form*
turned into a runtime, and it is exactly the Rust-module-for-a-markdown-folder
mistake that [lean-orchestration.md](lean-orchestration.md) warned about:

> If a workflow can be a folder of markdown, it must not be a Rust module.

The pipeline engine was removed on 2026-11-12. This page explains why that was
the right direction and what replaces it.

## What ICM actually says

From `SKILL.md`:

- "Build workspaces where the folder structure does the orchestration. One
  agent, reading the right files at the right moment, replaces a multi-agent
  framework."
- "Numbered folders carry sequencing, hierarchy carries context scoping, plain
  markdown files carry state."
- "Every ICM is built from three primitives: a **sequence**, a **shelf**, and a
  **catalog**."
- "The forms are recipes over them: **Pipeline**, **Record library**,
  **Knowledge bundle**, **Context map**, and **System map**. They nest."
- "Every step ends at a **check** (a person, a script, or a small model)."
- "One folder, one job"; "Every working folder has a contract" (`CONTEXT.md`);
  "Status lives on disk"; "Instantiate by copying" (`_templates/`);
  "Load only what the step needs"; "The walk test."

From `references/forms.md`:

| form | built from | use when |
|---|---|---|
| **Pipeline** | a sequence | the same steps run again with different input |
| **Record library** | a shelf of records with a status line | the unit accumulates (person, client, session) |
| **Knowledge bundle** | shelves of linked notes | the product is the knowledge |
| **Context map** | shelves of teams, processes and data | the subject is an organization |
| **System map** | nouns and verbs plus an impact index | a tree later agents will edit |

A **Pipeline** is one of those forms. It is folders like
`stages/01_research/{CONTEXT.md,references/,output/}`, a `_shared/` factory, a
`_templates/` shelf and a `_tools/` folder. The agent follows the contracts and
checks; nothing in the server runs the stages.

## Where the composer interpretation went wrong

| composer did | ICM says |
|---|---|
| made **Pipeline** the architecture of every workflow | choose one of five forms; forms nest; many sites are not pipelines |
| built a server `pipeline/` module, `run_stage`, `work.run`, gates and a board | folders, `CONTEXT.md` and `check.md`; one agent; no runner |
| stored stage/status/attempts in work pages and server state | state is what exists in `output/` and `check.md`; generated indexes are rebuilt, never hand-edited |
| treated every site as a workflow/pipeline | a **site** is composer's capability/web layer; an ICM workspace is a folder layout that can live *inside* a site |
| used `index.md` as both homepage and process instructions | `AGENTS.md` is the small entry; each working folder has a `CONTEXT.md`; catalogs hold no content |
| added separate `coder`, `reviewer`, `worker`, `stage` agents | one agent; a review is a **check**, not a separate agent (multi-agent is outside ICM unless the work truly needs a framework) |
| made gates an approval/UI state machine | one concrete check: a person, a script, or a small model; a non-person writes `check.md` with `pass` or `fail: why` |
| used JSON as the orchestrator state | JSON is fine as a stage's **product** or an app's state; the contracts, catalogs and checks are markdown/files |
| built a generic board as the source of truth | the folder is the source of truth; a board/index/graph is generated from files (`_tools/index.py`, `viz.py`) |

The old installed `designer`, `cad`, `asset`, `video` pipeline sites and the
`pipeline.*`/`new_work`/`run_stage` engine were all downstream of that one
category error. They were archived/removed for that reason.

## What composer's reading got right

- **Structure carries context.** The agent should follow folders and contracts,
  not a prompt that contains everything.
- **Checks at the boundaries.** The person's pause is the stage boundary. The
  coding example — code, agent review, user review, merge — is real; it was just
  modelled as agents/gates instead of checks.
- **Factory vs product.** `_shared/` and `_templates/` are stable; `output/` is
  per run. This is the same distinction as the site-pack **definition** versus
  **snapshot**.
- **Plain text and links.** Pages, relative links, YAML frontmatter, one home
  per fact.
- **One agent.** The two-agent cleanup (default + orchestrator) is closer than
  seven agents, but an ICM pipeline itself is one agent following files. The
  orchestrator is composer's layer for parallel/async work, not ICM.

## Mapping ICM onto composer

Composer is the **harness and the web of sites**, not the ICM method. A site can
host an ICM workspace; not every site is one.

```text
site://coding/
  index.md                 the site's web homepage; it may point at AGENTS.md
  AGENTS.md                the small entry: where am I, where do I go
  CONTEXT.md               the pipeline in one screen
  stages/
    01_code/        {CONTEXT.md, references/, output/}
    02_review/      {CONTEXT.md, references/, output/}
    03_user_review/ {CONTEXT.md, references/, output/}
    04_merge/       {CONTEXT.md, references/, output/}
  _shared/                 factory: rules, voice, standards
  _templates/              blank starters
  _tools/                  links.py, checks, index generation
  setup/questionnaire.md   when the factory needs configuring
```

- **One job per folder.** The number is the order; the `CONTEXT.md` names exact
  inputs, process, outputs, and one check.
- **Status** is a scan of `stages/*/output/` plus `check.md`; `_index.md` is
  generated. No board state machine.
- **The agent** opens the entry, follows the contract for the current step,
  does the work, writes the output and waits for the check. It does not need
  `run_stage`.
- **The app** is the **check surface**: for coding, the run's `review.md` is a
  `type: tui` page, so `open` starts Hunk in the run's `project/`; the user's
  notes are read with the `hunk` tool, and `check.md` is the check. A video app
  renders `storyboard.json`; the user's approval is the check. `show_app` stays
  for an app a project attaches.
- **Hermes/threads/cards** are composer's UX around files; they are not ICM
  primitives. An artifact card can show a stage's output or a check.

### Coding as an ICM pipeline

```text
01_code/        input: ticket + project + base
                output: the branch/commit, change.md
                check: tests pass (fmt, clippy, cargo test)
02_review/      input: change.md + the base diff
                output: review.md (verdict, findings)
                check: check_review.py writes check.md (verdict + findings)
03_user_review/ input: the base diff + review.md
                output: user-review.md
                check: the user's verdict in Hunk against base; check.md
04_merge/       input: the user's passing check
                output: merge.md + the merged branch
                check: the user's merge approval
```

The agent review and the user review are **two checks**, not two reviewer
agents: the agent's check is a script over `review.md`; the user's check is the
person, with Hunk showing the base diff in the run's `project/` and the notes
feeding `user-review.md`. ICM itself only needs the files and the verdicts.

### Video as an ICM pipeline

```text
01_story/     output: story.md
02_scenes/    output: storyboard.json      # product, app renders it
03_assets/    output: assets/ + manifest
04_edit/      output: the cut
```

Each stage has a `CONTEXT.md` and one check. The app is how the user looks at
the JSON and writes the check; it is not the pipeline engine.

### Site packs

The site-pack split should follow ICM's factory/product split:

- **Definition pack** = `AGENTS.md`, `CONTEXT.md`, `stages/**/CONTEXT.md`,
  `_shared/`, `_templates/`, `_tools/`, `setup/`, `index.md` pointer.
- **Snapshot pack** = the definition plus `output/` (and other products).
- **Never** a pack of pipeline engine state, a board, or a server scheduler.

The packing design already had this distinction; it only needs the ICM names
and the `AGENTS.md`/`CONTEXT.md` files.

## Pilot: coding (2026-11-13)

The installed `coding` site is now the first ICM workspace:

```text
coding/
  AGENTS.md              entry
  CONTEXT.md             the pipeline in one screen
  index.md               web homepage / catalog
  stages/                (inside _templates/work; copied per ticket)
  _templates/work/       a blank run: 01_code, 02_review, 03_user_review, 04_merge
  works/                 one run per ticket, with project/ and outputs
  _shared/               coding rules, how-to-code, how-to-review,
                         how-to-user-review, sh notes
  knowledge/             codebase + library + workflow shelves, generated _index.md
  _tools/                new_work.py, check_review.py, links.py, index.py, mutate.py
  _archive/              old scratch and one-off scripts
```

- `just new-work <ticket> <project>` copies `_templates/work/` into
  `works/<ticket>/`, fills the ticket/project placeholders and links `project/`.
- The stage contracts live in the template; each run has its own outputs and
  checks, so concurrent tickets do not overwrite each other.
- `_tools/links.py` reports 0 broken links and inputs on the current tree.
- `_tools/index.py` generates the workspace and knowledge indexes.
- The user review is the run's `review.md`, a `type: tui` page. `open` starts
  Hunk (`hunk diff <base>`) in the run's `project/`, against the ticket's
  base branch; the site's
  `tools: [hunk]` gives the agent the notes/reply/navigate/export control.

The old `workflows/` pages moved to `_shared/`; `scratch/` and one-off scripts
moved to `_archive/`; `how-to-code`, `how-to-review` and `how-to-user-review`
are stage references. The four stages are `01_code`, `02_review` (agent),
`03_user_review` (person), `04_merge`; `check_review.py` writes stage 02's
`check.md`.

## Default site: icm (2026-11-13)

The ICM Architect skill ships as a default seed site: `seed/sites/icm`, installed
as `site://icm` in every workspace. It holds the skill under
`_shared/icm-architect/` (SKILL.md, references, templates, scripts) and the site's
own `AGENTS.md`/`CONTEXT.md`/`index.md` route into it. It is a reference site, not
a workspace to build in; `just links` checks the skill's links.

## What to change next

1. **Do not rebuild the pipeline engine.** The removed `pipeline/` module was
   framework code for one form; keep it removed.
2. **Make a site's workflow an ICM workspace when it is a pipeline.** Use
   `AGENTS.md` + `CONTEXT.md` + `stages/` + `_shared/` + `_templates/` +
   `_tools/`, not a new schema.
3. **Let the app be the check.** A web app writes `check.md`/review JSON; for
   coding the check surface is Hunk, a `type: tui` page opened with `open`. The
   agent reads the notes; the next stage reads the output. `show_app` stays.
4. **Keep the orchestrator outside ICM.** Use it for parallel/async tickets and
   workers; an ICM pipeline is one agent. Do not call the orchestrator part of
   ICM.
5. **Generate status, don't store it.** A workspace view can show
   `stages/*/output/` and `check.md`; a board is a generated view, not a source
   of truth.
6. **Correct the docs and seed.** Replace pipeline vocabulary with the five
   forms and the `AGENTS.md`/`CONTEXT.md` shape; mark old boards history.

## The one-line correction

> Composer was building a **runtime for the Pipeline form**. ICM is a **design
> method for five forms**, where the Pipeline is just folders, contracts,
> checks and one agent reading them.
