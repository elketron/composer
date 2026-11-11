# The harness, the capabilities, and the workflows

Status: phases 1–2 built (site tools and the project hub); phase 4's file/state model is built
except deleting the `pipeline.*` command shims. Phases 3, 5 and 6 are proposals.

This replaces the earlier "task engine" draft. If the base is *a small harness with capabilities
injected when needed*, then moving the scheduler into the server is the wrong direction. The server
should get smaller, not smarter.

## The base

Three layers:

| layer | what it is | where it lives |
|---|---|---|
| **harness** | the model loop, sessions, the web of pages, base tools, approvals, subagents, micro apps, MCP | `crates/server` |
| **capability** | a site: instructions, actions, a micro app, and the tools it needs | `site://…` |
| **workflow** | a convention written in pages: tickets, pipelines, plans | markdown |

The rule that follows from it:

> If a workflow can be a folder of markdown, it must not be a Rust module.

The harness knows about pages, files, actions, agents, approvals and apps. It does not know about
tickets, pipelines, projects, stages or desks. Those are conventions that a capable agent can read.

The second rule:

> An agent starts with the base tools. A capability adds tools and instructions only while the
> agent is inside it.

That is the whole point of micro apps and sites: the same agent becomes a video editor, a ticket
orchestrator or a project manager by entering a different folder, not by growing the harness.

## The test for every feature

- Is it the loop, the model, the web, sessions, approvals, subagents or the app server? → **harness**.
- Is it instructions + actions + a UI + a few tools? → **capability** (a site).
- Is it a way of organizing work in pages? → **files**.

If a feature fails the test, it gets deleted or moved out. That is the simplification.

## What the current tree says

| thing | today | reads as |
|---|---|---|
| tools | 28 native tools | 12 harness, 16 workflow verbs hidden in Rust |
| `pipeline/` | ~1.9k lines, with `run_gates` and `stage_ended` | a scheduler in the harness |
| site access | read/write scope only | capabilities exist, but tools are not injected |
| `enter_site` | planned (milestone 30), not built | the missing half of the capability model |
| `apps/` | ~2.3k lines | correct layer; keep |
| project handle | a `projects` page plus `path:` strings in works | a page, not a path |
| `templates/sdk` | 568 lines, zero deps | already lean; do not grow it |
| clients | TUI, Flutter, desktop, web chat | three real UIs; a product decision, not a harness one |

The most important line in that table is the third: sites already scope **writes**, but they do not
scope **tools**. An agent gets the same 23-tool list whether it is in `site://memory` or
`site://orchestration`. That is why every workflow verb became a native tool.

## The base tools

The base should be the things that make no sense as a site:

| group | tools |
|---|---|
| web | `open`, `search`, `write`, `edit`, `list_sites` |
| site actions | `actions`, `run` |
| context | `set_project`, `enter_site`, `leave_site` |
| agents | `spawn_agent`, `message_agent`, `stop_agent`, `finish` |
| UI | `show_app` |
| events | `watch` |
| approvals | `answer_approval` |

That is 17, and even that can shrink later. `list_sites` is arguably `open('site://home')`;
`actions` is `run` with no recipe. The point is not a magic number; it is that **no workflow verb
is in this list**. There is no `spawn_worker`, no `new_pipeline`, no `merge_ticket`, no
`terminal_send`.

Everything else is a **site-scoped tool**: the implementation stays in the server for now, but the
tool is only in the set while the agent is in the site that declares it. Something that already has
a Rust implementation and a test does not need to become a shell recipe to be a capability.

| tool | becomes |
|---|---|
| `new_site`, `new_app` | site-scoped in the site-management site |
| `new_project` | site-scoped in the projects site |
| `merge_ticket` | site-scoped in the orchestration site |
| `publish_site` | site-scoped in the library site |
| `terminal_read`, `terminal_send` | site-scoped in the terminals site |
| `new_pipeline`, `new_work`, `run_stage` | deleted; the pipeline site's files and actions |
| `spawn_worker`, `continue_worker`, `stop_worker` | replaced by the base agent tools |
| `desk`, `selection` | deleted; client-side UI |
| `show_page` | `show_app` or a client affordance |

## Capability injection

A site declares what it adds in its `index.md` frontmatter:

```yaml
---
title: Orchestration
type: site
tools: [merge_ticket]          # added to the base while the agent is here
---
# Orchestration

Tickets are pages in `tickets/`. ...
```

Entering a site:

1. adds its `index.md` to the system prompt (the phase-7 "entering sites" idea);
2. adds its `tools:` to the base set for the next turn;
3. narrows writes to that site and the session's project (the existing `SiteAccess` rules);
4. stores `session.site` in `meta.toml` so it survives a restart;
5. `leave_site` restores the base set.

No new agent definitions are needed for a workflow. The agent page is the harness prompt; the site
is the capability. A worker gets the same base plus the site the ticket names (`site:` on the
ticket, or the orchestrator's site when it has none).

This is a small change: `SessionInfo.site` (one field), `enter_site` / `leave_site`, a site-tools
reader, and `ToolRegistry::specs_for(agent, site)`.

## Agents can talk to each other

This is now a base primitive, not a workflow feature. The orchestrator can talk to a subagent
directly, and a subagent can talk to its parent.

| tool | does |
|---|---|
| `spawn_agent` | start a subagent with a task, a context (pages), an agent page, a site and an optional project/worktree |
| `message_agent` | send text to a child (running or idle), or to the parent |
| `stop_agent` | stop a child |
| `finish` | end my run with a summary; the parent is told |

Rules:

- A message to a **running** agent steers it (it arrives as a user message in its next step).
- A message to an **idle** agent resumes it. This replaces `continue_worker`: one verb, not two.
- A child may message its parent. The parent sees it in its chat and can answer.
- Only the parent (or an ancestor) may message a child; siblings talk through the parent.
- The existing `WorkerStatus` / session events already carry what the UI needs. No ticket-shaped
  mailbox is required.

This is what makes the ticket convention optional. A ticket is a page that says what to do; the
orchestrator reads it, spawns an agent with that page in its context, and can keep talking to it.

## Pipelines are files

The current implementation is linear: `stages/01…`, `_stage.md`, a watcher, a server that starts the
next stage. That is why it feels naive: the harness encodes one shape of work.

The replacement is a text contract plus a small app:

**The pipeline** (`site://coding/index.md`) declares a graph:

```yaml
---
title: Coding
type: pipeline
needs_project: true
stages:
  - id: spec
    file: stages/01_spec.md
    needs: []
    gate: true
  - id: plan
    file: stages/02_plan.md
    needs: [spec]
  - id: implement
    file: stages/03_implement.md
    needs: [plan]
    gate: true
  - id: review
    file: stages/04_review.md
    needs: [implement]
    repeat: until-approved
---
```

**The work** (`site://coding/game-save/index.md`) says where it is:

```yaml
---
title: Game save system
type: work
pipeline: site://coding
project: site://projects/A
stage: plan
status: review
done: [spec]
attempts: { plan: 1 }
---
# Brief
...
```

**The stage** (`site://coding/stages/02_plan.md`) is the contract:

```markdown
---
type: stage
inputs:
  - { path: brief, kind: working }
  - { path: _config/conventions.md, kind: reference }
outputs: [plan.md]
---
## Process
...
## Verify
...
```

The server does not parse the graph. The **app** does: it reads the pipeline and the work, validates
the graph (no missing `needs`, no cycle), renders the stages, and writes `stage` / `status` / `done`
back into the work's `index.md` (with its `hash`, so a concurrent edit is not clobbered). The
**agent** reads the same files and follows them. A gate is just
`status: review`; Continue writes `approved` and sends the agent a message. Rerun writes the same
stage and increments `attempts`. A loop is `repeat`; a parallel branch is two stages with the same
`needs`; neither needs a scheduler.

What this fixes:

- **Non-linear work**: branches, loops, reruns and skipped stages are data.
- **Human edits**: the files are the same ones the human edits.
- **No server state**: reload, restart, or open the folder in an editor and nothing is lost.
- **Smaller harness**: `run_gates`, `stage_ended`, `_stage.md`, the watcher, `pipeline.list` and
  `work.run` all go away.

What it gives up:

- The server can no longer start the next stage by itself. Someone (the app, the TUI, the agent, a
  recipe) sends the message. That is the trade: less magic, more legible text. A `just next` recipe
  in the pipeline site is the one-liner for "start the next stage".

## The site and the project

Detailed design: [project-hub.md](project-hub.md).

The project is the thing being worked on; a site is a capability that operates on it. The relation
is many-to-many: one project has a coding site, a video site and a 3D site; one coding site operates
on many projects. A **work** is one instance of a site operating on a project.

```text
project A  ~/Projects/A
├── coding   site://coding    works: save-system, inventory
├── video    site://video     works: trailer
└── 3d       site://3d        works: crashed-ship
```

The project is the hub. Its page lists the sites that operate on it, what each is doing, and the
state of their works. Each site's app opens on the project and gets the project as its context; the
sites that have a `tile` app are the project's tiles. A site never owns a project; a project never
absorbs a site.

Today the pieces exist but the project is not the hub: the edge is a path string copied into every
work, a grant in the app, a `workflow:` field in memory and a scan for `type: work` pages. The path
string is the part that hurts; everything else follows from it.

### The project is a page

Put the project page in the new `site://projects` site; it is the canonical handle:

```yaml
# site://projects/A.md
---
title: A
type: project
root: ~/Projects/A
sites: [coding, video, 3d]   # optional; otherwise derived from the works
tags: [game]
status: active
---
# A

What it is, decisions, current state.
```

The folder is the thing; the page is how the web names it. One object is then the answer to:

- the app's `project()`,
- the project view (the hub),
- the work's `project:`,
- the agent's `set_project`,
- the project list and search,
- the memory index's Projects registry.

The `projects` site is the home: its `index.md` lists the projects, its `ui/` is the hub app, and
`new_project` writes `site://projects/<name>.md`. Old `site://memory/projects/<name>.md` pages keep
working as a fallback; the memory site links the project page. `sites:` is a convenience for
attaching a site before it has a work; normally the list is derived from the works that name the
project. There is no single `workflow:`: coding, video and 3D are peers on the same project. Old
`workflow:` pages are read as a one-entry `sites:`.

### The work mounts the project

A work is a folder in a site that names a project:

```yaml
# site://coding/game-save/index.md
---
type: work
project: site://projects/A
stage: plan
status: review
done: [spec]
---
```

Inside the work, `project/` is a view of the project's folder:

```text
site://coding/game-save/
  index.md          brief, stage, state
  project/          the project's folder (read-only)
  01_spec/          the paper trail
  02_plan/
```

The view is the same for the agent and the app:

```text
agent: open  site://coding/game-save/project/src/main.rs
app:   projectFile('src/main.rs')
```

Both resolve to `~/Projects/A/src/main.rs`. No copy, no symlink committed to the project, no
second API. The app's `get_project_file` / `put_project_file` routes already exist on the server;
the SDK is missing the two wrappers.

Three access modes on the view:

| mode | `project/` is | for |
|---|---|---|
| read (default) | the project folder, read-only | a site that only reads the project |
| write | the project folder, writable in place | video, renders, 3D assets: the site's output is the project |
| worktree | the ticket's worktree | code: the branch under review, merged later |

A stage that writes code sets `worktree: true`; a site that writes assets (video, 3D) sets
`write: true` and writes the project in place. A work without either gets a read-only view;
`write` through it is refused with the reason.

Outputs are just paths. A stage's `outputs` may be work pages (`plan.md`) or project files
(`project/models/ship.glb`, `project/out/trailer.mp4`); the prefix says where the file is, and the
site's `index.md` says which are expected.

### Two modes, one model

| mode | site | project | work | when |
|---|---|---|---|---|
| **capability** (normal) | `site://coding` (global) | `site://projects/A` | `site://coding/A/...` | coding, video and 3D all operate on A |
| **project-local** | `site://A-design` inside the project | A | `site://A-design/...` | a site versioned with the project |

Build the first now: global sites, the project page as the hub, each site's app opened on the
project. The second is the same model with the site's folder inside the project; it is for a site
that should travel with the code (design docs, research, a game's own pipeline). It is a mode, not
a second architecture.

### What this fixes

- **One hub.** The project page lists every site operating on it, what each is doing and the state
  of its works. No scan of every work to answer "what is happening on A".
- **One handle.** A work does not copy a path; it points at the project page. Rename or move the
  folder and one page changes.
- **One path.** `project/` is what the agent reads and what the app reads. "What the app sees" and
  "what the agent sees" stop being two stories.
- **The project stays real.** It is still a folder with git, worktrees, diffs and merge. The page
  is a handle, not a replacement.
- **The site can be deep.** `_config/`, `inputs/`, `outputs/`, `01_spec/` live in the work; the
  project is one subtree of it. Complex folder structures don't leak into the project.
- **The app gets project access.** `projectFile` / `putProjectFile` make the app a real tool on the
  project, not just a viewer of pages.

### What it costs

- a `ProjectRef` that accepts a page URL or a path (legacy);
- a `project/` bind in the existing `Mounts` layer, plus the `open` / `write` / `index` / watcher
  paths following it, with the read / write / worktree choice;
- `new_project`, `project_list` and `project_overview` reading the page;
- two SDK functions and a `page` field on `ProjectInfo`, with its `pipelines` list renamed `sites`;
- the ticket worktree as the bind target while a code stage runs.

### What does not change

- `ProjectPath` canonicalization and workspace ownership stay exactly as they are.
- Worktrees, branches, `merge_ticket`, the project token and the app's own grant stay.
- The `projects` page is the project's memory; the memory site links it, and the old memory page is a
  fallback.

## Micro apps

The SDK keeps its current surface:

- `page` / `putPage`, `file` / `putFile`, `search`, `listActions` / `runAction`, `send`, `emit`,
  `onChange`, `onMessage`, `request`, `project` / `data`.
- Add one generated `protocol.ts` (the protocol already exports TypeScript) and delete the
  hand-copied types.
- Add the two project-file functions the project hub needs (`projectFile` / `putProjectFile`) and
  nothing else. No task helpers, no store, no hooks. If an app needs orchestration, it reads the
  pipeline/work pages and writes them back. If it needs to talk to an agent, it calls `send`.

The app is the capability's human UI: the board writes `approved`, a video app writes a scene,
a CAD app writes dimensions, a project app writes a project. The agent reads the same files. That
is the whole injection model, and it is already how the board works.

## What moves and what dies

| path | outcome |
|---|---|
| `pipeline/run.rs` (`run_gates`, `stage_ended`, `run_stage`) | delete; the app/agent drives |
| `pipeline/work.rs` (`Work`, `stages`, `_stage.md`) | delete from the server; the app reads pages |
| `pipeline/stage_prompt.rs` | replace with a generic `spawn_agent.context` |
| `pipeline/create.rs` | keep only the template install; the rest is `just` recipes |
| `tools/run_stage_tool.rs`, `new_pipeline_tool.rs`, `new_work_tool.rs` | delete |
| `tools/new_site_tool.rs`, `new_app_tool.rs`, `new_project_tool.rs`, `publish_site_tool.rs` | keep the implementation; make them site-scoped tools |
| `tools/desk_tool.rs`, `selection_tool.rs` | delete (client UI) |
| `tools/terminal_*` | keep the implementation; make them site-scoped tools in the terminals site |
| `tools/spawn_worker_tool.rs`, `continue_worker_tool.rs`, `stop_worker_tool.rs` | replace with the three agent tools |
| `protocol` `pipeline.list`, `work.new`, `work.run`, `stage.ended` | delete; the app uses `search` / `page` |
| `seed/sites/agents/*.md` | shrink `tools:` to the base; the sites supply the rest |
| `seed/sites/orchestration/index.md` | becomes the capability: ticket conventions + `just` actions |
| `project/project_list.rs`, `project_overview.rs` | read the project page; no scheduler, no new module |
| `tools/new_project_tool.rs` | writes `site://projects/<name>.md` with `root:` instead of a memory note |
| `projects/<name>.md` | the project page (`type: project`, `root:`); old memory pages read as a fallback |
| `Mounts` / `Web` | one `project/` bind per work; no change to `ProjectPath` |
| `templates/sdk` | `projectFile` / `putProjectFile`; `ProjectInfo` gains `page`; `pipelines` becomes `sites` |

The `worker` module stays: it is the subagent runner. The `harness` stays: it is how a run happens.
The `project`/worktree code stays: it is how a run touches code. Those are harness-shaped.

## Phases

Each phase is shippable on its own.

### Phase 0: measure before cutting

Count tool calls and site entries in a week of real use (`state/sessions/*/transcript.jsonl`). Keep
the base tools that are actually used; everything else is a candidate to move. No code changes.

### Phase 1: the missing half — site tools

- `SessionInfo.site` + `enter_site` / `leave_site` (the phase-7 milestone).
- `tools:` in a site's `index.md`; `ToolRegistry::specs_for(agent, site)`.
- Prompt: the site index replaces `site://home` while inside (as planned in phase 7).
- Shrink the seed agents to the base; put their old tools on the sites.

**Done when:** the default agent in `site://memory` has the base set, the same agent in
`site://orchestration` has the orchestration tools, and `:site` / the app chip still work.

### Phase 2: the project page and the work mount

- Add `site://projects`; `new_project` writes `type: project` with `root:`; accept old
  `site://memory/projects` pages as a fallback.
- `ProjectRef`: a work's `project:` is a page URL or a path.
- A `project/` bind in `Mounts`, resolved from the work's project; `read` / `write` / `worktree`
  from the work or its stage.
- `project_list` / `project_overview` / `project()` read the page; `new_project` writes it.
- SDK: `projectFile` / `putProjectFile`; `ProjectInfo` gains `page`; `pipelines` becomes `sites`.

**Done when:** `open site://coding/game-save/project/src/main.rs` and
`projectFile('src/main.rs')` read the same file; a `write: true` work writes a project asset; a
`worktree: true` stage reads and writes the ticket's worktree; a read-only work refuses the write
with the reason.

### Phase 3: agents talk

- `spawn_agent`, `message_agent`, `stop_agent`; keep `spawn_worker` as a deprecated alias for one
  release.
- Parent ↔ child messages; idle resume; running steer.
- The orchestrator prompt loses the "one worker at a time" micromanagement and gains "talk to them".

**Done when:** an orchestrator spawns two agents, messages one mid-run, and gets the other's reply.

### Phase 4: pipelines become files

- Ship the graph + `state` format and a validator in the board app.
- Delete the server scheduler and the `pipeline.*` commands; keep a read-only shim for one release.
- The board reads `search` / `page`; no new protocol.

**Done when:** a 4-stage graph with a loop, a branch and a gate runs from the app, the TUI can open
the work's page, and killing the server loses nothing.

### Phase 5: scope the verbs out

- Scope `new_*`, `publish`, `merge`, `terminal_*` to their sites; delete the ones that are files.
- Delete the tool names from the seed agents' `tools:`.
- Apply the budget.

**Done when:** the base set is 17 tools, a site only sees what it declares, and the seed agents
list no workflow verb.

### Phase 6: SDK

- Generated `protocol.ts`; the hand-copied types go.
- No new functions beyond what the apps already use.

**Done when:** the board builds against the generated types and the SDK is still under ~600 lines.

## The budget (put this in CLAUDE.md)

- **One scheduler: none.** The harness starts a run when asked; it does not decide what runs next.
- **One source of truth per concept:** pages for work, sessions for runs, `state/` for secrets and
  registries.
- **One new top-level server module requires folding or deleting one.** (Currently 23.)
- **One new native tool requires a base need.** A capability's verb is a site-scoped tool or a site
  action, never a new base tool.
- **Capabilities are sites.** A workflow that cannot be a folder of pages is a design smell.
- **One canonical client per form factor.** New work goes into one first; the others speak the
  protocol.

## Non-goals

- No workflow engine, no DAG library, no scheduler.
- No new database.
- No new client.
- No orchestration-specific SDK.
- No agent hierarchy deeper than the parent/child edge unless a real use case asks for it.

## Risks

- **Site tool injection** changes the prompt; test a worker in each seed site before merging.
- **Direct messaging** can loop. Cap turns per child and let the parent stop it; the existing turn
  budget still applies.
- **Deleting the pipeline scheduler** can strand an in-flight work. Ship the read-only shim and a
  migration note first.
- **The app validator** is JavaScript; test it with a cycle and a missing `needs`.
- **The budget** only works if it is enforced in review; put it in `CLAUDE.md`.

## Open questions

1. **Is `spawn_agent` base or a capability?** Recommendation: base. Spawning an agent is the core
   of an agent harness; the *ticket* is the convention.
2. **Is `worktree` a `spawn_agent` option or a project capability?** Recommendation: an option, kept
   next to `project`, because coding is a first-class use.
3. **Does entering a site happen explicitly (`enter_site`) or implicitly when the agent opens its
   `index.md`?** Recommendation: explicit, as phase 7 planned; implicit entry surprises a reader.
4. **Does the TUI keep a pipeline view, or does the board own it?** Recommendation: the board owns
   it; the TUI opens the app. The TUI keeps sessions, pages and changes.
5. **Does the project page live in `site://memory` or a new `site://projects`?** `site://projects`:
   its `index.md` is the list, its `ui/` is the hub, its pages are the projects. Memory links it.
6. **Is the project page a capability with `tools:`?** Recommendation: yes eventually (project
   files, git), but the first version only needs the `project/` mount.
7. **Who may write the project?** Recommendation: a work is read-only by default; a site that
   writes assets sets `write: true`; a site that writes code sets `worktree: true`. The app uses the
   same rule through `putProjectFile`.
8. **What is the desktop app for?** Product decision; it does not affect this plan.

## Appendix: the 28 tools today, and the two this plan adds

| tool | layer | fate |
|---|---|---|
| `open`, `search`, `write`, `edit`, `list_sites` | harness | keep |
| `actions`, `run` | harness | keep |
| `set_project` | harness | keep |
| `enter_site`, `leave_site` | harness | **add** (phase 1) |
| `watch` | harness | keep |
| `show_app` | harness | keep |
| `spawn_worker`, `continue_worker`, `stop_worker` | harness | → `spawn_agent`, `message_agent`, `stop_agent` |
| `finish` | harness | keep |
| `answer_approval` | harness | keep |
| `new_site`, `new_app` | workflow | site-scoped (site management) |
| `new_project` | workflow | site-scoped (projects) |
| `new_pipeline`, `new_work`, `run_stage` | workflow | delete (files) |
| `merge_ticket` | workflow | site-scoped (orchestration) |
| `publish_site` | workflow | site-scoped (library) |
| `terminal_read`, `terminal_send` | workflow | site-scoped (terminals) |
| `desk`, `selection` | client | delete |
| `show_page` | UI | `show_app` / client |

After the plan: 24 native tools, 17 in the base set. The other 7 only appear inside the site that
declares them, which is the point.
