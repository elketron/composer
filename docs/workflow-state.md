# Workflow state

Status: proposal, 2026-11-12; corrected against ICM 2026-11-13. This replaces the generic pipeline/stage/board
model.

> ICM view: if the work is a repeated sequence, the folder itself is the
> pipeline (`AGENTS.md`, `CONTEXT.md`, `stages/`, `check.md`); JSON here is the
> stage's product or the app's state, not the orchestrator. See [icm.md](icm.md).

A workflow is a **site**. The site owns its instructions, its app and its state:

```text
sites/video/
  index.md          how this workflow is done
  ui/               the app: the cowork surface
  storyboard.json   the state the app and the agent both edit
```

There is no generic stage contract, gate, board or work engine. A site defines
its own structure in `index.md`, and the app interprets the JSON it owns. The
agent writes the same JSON with `open` / `write` / `edit`; the app reads and
writes it with the SDK. Both see one object.

## Why JSON

- Models understand JSON well enough without a schema engine.
- A site can choose its own shape: a storyboard, a review, a shot list, a
  checklist, a small graph.
- The app can render it however the object deserves; a generic board cannot.
- A page is still markdown; JSON is for the state that needs structure. Do not
  move prose or instructions into JSON.
- The shape is documented in the site's `index.md`; a JSON Schema is optional,
  not required.

## The cowork surface

- **Pages** are opened with `show_page` (or `open`, to read). The orchestrator
  is good at opening the page it needs to discuss.
- **Apps** are opened with `show_app`; this is the part worth keeping. A
  storyboard, a live application and a view type use it. A coding review is a
  `type: tui` page, opened with `open`, and Hunk runs in the run's `project/`.
- The **desk** and **selection** tools go away. A workflow decides its app; the
  user and agent look at the same app; the app owns the layout.
- Threads stay: a comment is anchored in the app's view, stored beside the work.

## Coding

An ICM pipeline of four stages, one copied run per ticket:

1. The orchestrator writes a coding ticket whose Context links the coding
   template's stage 01 (`site://coding/_templates/work/stages/01_code/CONTEXT.md`);
   `spawn_worker(id)` starts the **default agent** in worker mode. Its first step
   is `run coding new-work <ticket> <project>`, which copies `_templates/work/`
   into `works/<ticket>/`, links the run's `project/`, and records the review
   `base` (the ticket's `merge_into:`, else the project's checked-out branch).
2. **Stage 02, agent review:** the same default worker follows
   `works/<ticket>/stages/02_review/CONTEXT.md`, writes `output/review.md`, and
   `check_review.py` writes `check.md`. It is Context, not an `agent: reviewer`.
3. **Stage 03, user review:** the run's `review.md` is a `type: tui` page; `open`
   starts `hunk diff <base>` in the run's `project/`, against the base branch. The user
   leaves notes; the agent reads them with the `hunk` tool, writes
   `output/user-review.md`, and writes `output/check.md` from the user's verdict.
4. **Stage 04, merge:** `merge_ticket` is the final action; the user approves it.

Hunk is the user-check surface; the orchestrator opens the run's `review.md` with
`open` and reads the notes with the `hunk` tool.
There are two agents — default and orchestrator — and every role is a how-to
page.

## Video

A workflow with a structured state:

```json
{
  "title": "enhance that ant",
  "aspect": "16:9",
  "scenes": [
    {
      "id": "s1",
      "look": "macro, dusk",
      "shots": [
        { "id": "s1-1", "prompt": "ant on a leaf, backlit", "asset": null, "status": "todo" }
      ]
    }
  ]
}
```

The site's app renders the storyboard; the agent edits `storyboard.json` with
`edit`; generated images/films are files beside it. No `stages/`, no gate
runner, no board. The site's `index.md` says what the state means and in what
order to work.

## What replaces what

| old pipeline | new workflow |
|---|---|
| `type: pipeline` + `stages/` + gate runner | `index.md` instructions + `ui/` app + JSON state |
| generic board (`templates/board`) | the site's own app |
| `new_pipeline`, `new_work`, `run_stage`, `pipeline.*`, `work.run` | `new_site`/`new_app` and the site's tools; work is a folder/page the app interprets |
| stage contracts | the site's `index.md`, plus JSON shape docs |
| stage gate approvals | the app's review action / `merge_ticket` / the user's approval |
| `desk`, `selection` | `show_app` / `show_page`; the workflow decides the stage |

Tickets and workers stay where they earn their keep: a coding ticket, a
reviewer, a merge. They are orchestration, not a generic pipeline engine.

## Migration

- Existing pipeline sites and works stay readable as pages; they are not
  migrated into a new stage engine.
- `pipeline.list`, `work.run`, `new_work` and `run_stage` were removed on
  2026-11-12; see [cleanup.md](cleanup.md).
- The installed old pipeline sites were archived on 2026-11-12; see
  [cleanup.md](cleanup.md).
- A workflow that needs structure starts with a JSON file and a small app, not
  a template contract.

## Open questions

1. **One state file per work, or per site?** Most workflows want a file per
   work (`work/storyboard.json`); a small site can have one at its root.
2. **Who owns writes?** The app and agent both use the SDK's hash-checked
   writes; conflicts should be rare and visible.
3. **Schemas.** Keep them optional in the site's `index.md`; a JSON Schema is a
   convention, not a server feature.
4. **Review approvals.** Hunk is the review surface; the merge stays in the
   chat/approval flow. Hunk does not call `merge_ticket`.
5. **Old work pages.** Keep reading them; do not create new ones.
