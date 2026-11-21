# ICM in composer

Source: [RinDig/icm-architect](https://github.com/RinDig/icm-architect) (MIT), the skill for the
ICM paper (Van Clief & McDermott, [arXiv:2603.16021](https://arxiv.org/abs/2603.16021)). It ships as
the seed site `site://icm`.

## What ICM is

ICM is **folder structure as agent architecture**. One agent reads the right files at the right
moment; the structure does the orchestration. There is no scheduler, no stage runner, no board and
no database.

From `SKILL.md`:

- Numbered folders carry sequence, hierarchy carries context scoping, plain markdown carries state.
- Every ICM is built from three primitives: a **sequence**, a **shelf** and a **catalog**.
- Every step ends at a **check**: a person, a script or a small model.
- One folder, one job. Every working folder has a contract (`CONTEXT.md`). Status lives on disk.
  Instantiate by copying (`_templates/`). Load only what the step needs.

The five forms (`references/forms.md`):

| form | built from | use when |
|---|---|---|
| **Pipeline** | a sequence | the same steps run again with different input |
| **Record library** | a shelf of records with a status line | the unit accumulates (person, client, session) |
| **Knowledge bundle** | shelves of linked notes | the product is the knowledge |
| **Context map** | shelves of teams, processes and data | the subject is an organization |
| **System map** | nouns and verbs plus an impact index | a tree later agents will edit |

## How it maps onto composer

Composer is the harness and the web of sites, not the ICM method. A site can hold ICM folders; not
every site is one.

- **A workflow folder is an ICM folder.** A folder inside a site with a `CONTEXT.md` (or
  `index.md`) holds the instructions for one kind of job. A single shot sends only those
  instructions and the input to the model. A workflow run or an agentic run gets them after its
  site's `index.md`. A webhook (`[hooks.<name>]` with `site` and `folder`) or `spawn_run` names the
  folder.
- **Composer runs nothing on the stages.** The agent opens the entry, follows the contract for the
  current step, writes the output and waits for the check. Status is what exists on disk.
- **Checks are the person, a script or a small model.** A recipe in the site's `justfile` can be the
  script; a `type: tui` page (Hunk, see [tui-mode.md](tui-mode.md)) can be where the person looks.
- **Tickets, merges and reviews are conventions** of the workflows that want them: pages and folders
  in a site, and shell commands in a terminal. Composer has no verbs for them.
- **A project is a site** whose `folder:` names its code. Its workflows live in folders inside it.

## The icm site

`seed/sites/icm` is installed as `site://icm` by `composer init`. It holds the skill under
`_shared/icm-architect/` (SKILL.md, references, templates, scripts); the site's own `AGENTS.md`,
`CONTEXT.md` and `index.md` route into it. It is a reference, not a place to build in; `just links`
checks the skill's links.
