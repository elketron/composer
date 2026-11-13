# Legacy cleanup

Status: in progress, 2026-11-12. This is the work log for removing the
previous version's ideas; it is not a feature.

## Done

### Installed workspace

Archived out of `~/.composer/workspaces/default/sites/` (moved, not deleted):

| site | why |
|---|---|
| `designer` | pipeline toolbox |
| `cad` | pipeline-era CAD site |
| `asset` | pipeline-era asset site |
| `video` | pipeline-era video site |
| `calendar` | old feature site |
| `ideas` | old feature site tied to the ideation agent |

Backup: `~/.composer/archive/2026-11-12/`
- `workspace-sites.tar.gz` — the sites folder before the move
- `library-sites.tar.gz` — the home's site library
- `home-index.md` — the old Home page
- `legacy-sites/` — the six moved folders

Restore one:

```sh
mv ~/.composer/archive/2026-11-12/legacy-sites/cad \
   ~/.composer/workspaces/default/sites/cad
```

Added `projects` from the current seed (it was missing from the installed
workspace). The workspace Home now lists only:

`memory`, `projects`, `plans`, `orchestration`, `agents`, `chat`, `coding`, `3d`,
`mockups`.

### Seed

- `seed/sites/home/index.md` no longer teaches `new_pipeline` or pipeline sites;
  it teaches workflows (`app:` / `mcp:`), sites as capabilities, and the project
  hub.
- `seed/sites/home/threads.md` is installed into the workspace Home as well.
- The installed `agents/default.md` no longer carries site tools
  (`new_site`, `new_app`, `new_project`, `new_pipeline`, `new_work`, `run_stage`)
  in its base set. It enters home/projects/orchestration for them; the base set
  is browsing, editing, coordination and the desk.
- The TUI no longer offers `:pipeline` or `:work`; help, completion, parsing and
  the project view's stage rows/`R` are gone.
- Flutter's pipeline UI is gone: `ui/pipelines/`, stage dots, the New work
  dialog, and the Inbox gate tile. The project overview lists sites, branches,
  sessions and memory, not works.
- `seed/sites/orchestration/index.md` grants only `merge_ticket`; its pipeline
  section is a workflow/app/JSON note. Home and the orchestrator prompt no
  longer mention `new_work`/`run_stage`.

### Pipeline engine

Removed on 2026-11-12:

- `crates/server/src/pipeline/` (stages, contracts, prompts, run, learning,
  work) and the `new_pipeline`, `new_work`, `run_stage` tools
- `pipeline.list`, `pipeline.new`, `work.new`, `work.run` protocol commands
- `PipelineSummary`, `StageSummary`, `WorkSummary`, `StageState`,
  `ProjectOverview.works`, `ProjectSiteSummary.works/running/waiting`,
  `ProjectInfo.pipeline/pipelines`, `ServerMessage.StageEnded` and
  `ApprovalKind.Amendment`
- `templates/pipelines/` and `templates/board/`
- `[pipelines]` config (`build_board`, `learn_after`)
- Flutter pipeline state/effects/actions and tests
- the pipeline/stage tests, fixtures and project-mount/app-file tests
- `seed/sites/agents/stage.md`, the stage role and the stage row in Agents

Regenerated `apps/composer_protocol` with `dartgen`; all Rust, Dart protocol and
Flutter tests pass.

### Two agents

- Only `default` and `orchestrator` remain in `seed/sites/agents/` and the
  installed workspace. `worker`, `stage`, `coder`, `reviewer` and `ideation`
  are gone as agent pages.
- A worker is the **default agent in worker mode**: the server adds `finish`
  and a worker prompt; the ticket's Context and its site say how the work is
  done.
- The coding how-to moved to `site://coding/_shared/how-to-code.md` and
  `site://coding/_shared/how-to-review.md`; the orchestrator creates a review
  ticket that links the latter instead of picking an `agent:`. The coding site
  is now an ICM pipeline with a copied run under `works/<ticket>/`.
- `spawn_worker(id)` no longer takes an agent. The installed and seed
  orchestrator pages say so.

## What is still conflicting

These are the code/UI pieces that still describe the old shape. Removing them
needs a branch and tests; the installed-site archive does not remove them.

| area | pieces | target |
|---|---|---|
| desk tools | `desk`, `selection` in the base agent set and clients | remove; keep `show_app` and `show_page` — the orchestrator opens pages and apps on the stage |
| old UI surfaces | mobile `desk_sheet`; desktop stage tab logic tied to `desk` | site cards, stage by view, focus mode |
| old mockups | `docs/mockups/{app,apps,desktop,tui,pipelines}.html` and the 01–05 single-chrome pass | keep the platform studies; old boards are history, not UI |

Left alone on purpose: `memory`, `plans`, tickets and session transcripts still
link to the archived sites. They are records, not the current UI; editing them
would rewrite history.

## Cleanup order

1. ✅ **Pipeline vocabulary.** Removed the generic engine, tools, commands,
   stages/gates, templates, board, config, protocol types and tests. The
   replacement is site + app + JSON ([workflow-state.md](workflow-state.md)).
2. ✅ **Two agents.** Keep default + orchestrator; a worker is the default
   agent in worker mode. Coding/review roles moved to site pages.
3. **Desk to workflow view.** Keep `show_app` and `show_page`; remove `desk`
   and `selection`. Entering a workflow decides the app; the clients derive the
   stage from the workflow and client view state.
4. **UI surfaces.** Remove the old mobile desk screen; add the site card and
   focus mode from [site-artifacts.md](site-artifacts.md).
5. **Mockup history.** Keep `docs/mockups/next/`; move the old boards to a
   clearly marked archive or leave them as references. Do not present them as
   the current UI.

## Acceptance

- The workspace UI lists only current sites: no designer/cad/asset/video/
  calendar/ideas.
- No picker, menu or base tool offers `new_pipeline` or a pipeline board.
- Two agent pages: default and orchestrator; a worker is the default agent
  with the worker prompt and the ticket/site how-to.
- The stage is decided by the workflow/site, not by a free-floating desk tool.
- The old boards are clearly history, not a competing direction.
