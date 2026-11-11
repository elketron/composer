# The project hub

Status: phases A–D built. Phase E's file/state model and no-scheduler half are built; the
read/write commands (`pipeline.list`, `work.run`) remain as explicit shims for one release.

Visual overview: [project-hub-flow.html](project-hub-flow.html) — the shape, a work's `project/`,
the worktree/diff flow, local vs work, and the agent/app parity.

This is the detailed design for the part the lean-orchestration plan calls "the site and the
project". Read [lean-orchestration.md](lean-orchestration.md) first for the base.

## The shape

A **project** is a folder on disk and a page in the web. It is the thing being worked on.

A **site** is a capability: instructions, actions, a micro app, and a few tools. A site operates on
a project; it does not own one.

A **work** is one instance of a site operating on a project: a folder in the site that names the
project and holds the paper trail.

```text
project A  ~/Projects/A
├── coding   site://coding    works: save-system, inventory
├── video    site://video     works: trailer
└── 3d       site://3d        works: crashed-ship
```

The project page is the hub: it lists the sites, what each is doing, and the state of their works.
Each site's app opens on the project and gets the project as its context. Inside a work, `project/`
is the project's folder — the same files for the agent and the app.

The invariant:

> One project + one site + one work → one `project/` view. The project stays a real folder with
> git, worktrees and diffs. The site stays a folder of markdown and a small app. The work is the
> paper trail that ties them together.

## 1. The project page

The `projects` site is the home; its pages are the canonical handle:

```yaml
# site://projects/A.md
---
title: A
type: project
root: ~/Projects/A
sites: [coding, video, 3d]   # optional; otherwise derived from works
tags: [game]
status: active
---
# A

What it is, decisions, current state.
```

- `root:` is the folder. `~` is expanded; the result is canonicalized.
- `sites:` is a convenience for attaching a site before it has a work. Normally the list is
  derived from `type: work` pages whose `project:` is this page.
- `tags:` are the project's tags (the app's `when:` filters use them too).
- The body is the project's memory: what it is, decisions, current state.
- Old pages (`type: memory`, `kind: project`, `path:`, `workflow:`) keep working: the reader
  accepts `path:` as `root:` and reads `workflow:` as a one-entry `sites:`. The old
  `site://memory/projects/<name>.md` is read as a fallback; the memory index links the project page.
- A folder without a page is still a project. It just has no hub page yet; `set_project` can make
  one, and `new_project` always does.

The `projects` site itself:

```text
site://projects/
  index.md          the list of projects
  justfile           new-project (the no-tool path)
  ui/                the hub app: the Project tab
  A.md               one project page
```

`new_project` writes `site://projects/<name>.md`, registers it in `index.md`, and claims the folder
in `state/projects.json`. The memory site keeps memory; the project page is where the project lives
in the web. A project page can be edited like any other page.

### `ProjectRef`

One in-memory value, resolved from anything a work, ticket, session or app may hold:

```rust
pub struct ProjectRef {
    pub name: String,            // the folder's name
    pub page: Option<SiteUrl>,   // site://projects/A.md
    pub root: PathBuf,           // canonical, existing, a directory
    pub sites: Vec<String>,      // the project page's list, if any
    pub tags: Vec<String>,
}
```

`ProjectRef::resolve(server, spec)` accepts:

| spec | meaning |
|---|---|
| `site://projects/A` | the page; its `root:` is the folder |
| `site://memory/projects/A` | an old page; read as a fallback |
| `A` | a folder in `[projects] dir`; look up its page by name or `root:` |
| `~/Projects/A`, `/abs/A` | a path; look up its page by `root:` |

The result is the same canonical `root` for every spelling (`ProjectPath` already does this). The
page is found by name first, then by `root:`. The `state/projects.json` workspace ownership stays
exactly as it is: it is a path→workspace claim, not a project page.

`ProjectRef` is what `set_project`, `session.project`, `ticket.project`, `work.project`,
`project.overview` and the app's `project()` all resolve to. The session keeps storing the canonical
path string; the page is derived. That avoids a migration of every session and ticket.

## 2. The site

A site that operates on a project says so in its `index.md`:

```yaml
---
title: Coding
type: pipeline
project: required        # required | optional (default)
project_mode: worktree   # read | write | worktree (default: read)
---
```

- `project: required` means the site is not usable without a project: `enter_site` refuses, the
  site's app is not listed on Home, and `new_work` demands one. `optional` is the default and
  covers memory, plans, agents and other project-free sites.
- `project_mode` is the default for the site's works:
  - `read` — the site reads the project, never writes it;
  - `write` — the site writes the project in place (video renders, 3D assets, a design writeup);
  - `worktree` — the site writes code on a ticket branch (a git worktree, merged later).
- A work or a stage may override the mode. `worktree: true` on a stage is the old spelling of
  `project_mode: worktree`; the reader accepts both.
- The site's `app:` block is unchanged. `when: [git]` is the display filter; `project: required`
  is the capability requirement.

The site's app is the site operating on the project: `project()` returns the project, `data()` is
the app's state for that project, `runAction` gets `PROJECT_ROOT`, and `projectFile` reads/writes
the project. The app's site pages stay pages; the project's files are the project.

## 3. The work

```yaml
# site://coding/game-save/index.md
---
title: Game save system
type: work
pipeline: site://coding
project: site://projects/A
project_mode: worktree     # optional; the site's default otherwise
stage: plan
status: review
done: [spec]
---
```

Inside the work:

```text
site://coding/game-save/
  index.md          brief, stage, state
  project/          the project's folder, a view
  01_spec/          the paper trail
  02_plan/
```

- `project:` is the project page. A path still works (legacy) and is resolved to the page when one
  exists.
- `project_mode:` is optional; the site's `project_mode` is the default.
- `project/` is a **mount**: a symlink to the project root, or to the ticket's worktree in
  `worktree` mode. It is not a copy.
- The work's own files (brief, stage outputs, review notes) live in the work folder. A stage's
  `outputs:` may name either kind: `plan.md` (a work page) or `project/models/ship.glb` (a project
  file). The prefix says where the file is.

### What the agent and the app see

| | agent | app |
|---|---|---|
| work page | `open site://coding/game-save/index.md` | `page('index.md')` |
| the work's project | `open site://coding/game-save/project/src/main.rs` | `page('project/src/main.rs')` |
| work output | `write` / `edit` | `putPage` |
| project output | `write` / `edit` under `project/` (if the mode allows) | `putPage` under `project/`, or `putProjectFile` |

The agent and the app use the same work view: `open .../project/...` and `page('project/...')`
resolve to the same file, and `write`/`putPage` go through the same mount mode. The app's
`projectFile(path, work)` follows the same work: with `work` it is the project root or the
worktree; without `work` it is the project's main folder. The app's `changes` is the diff of that
same worktree, so the file tree and the diff line up.

## 4. The mount

A `ProjectMount` is one work's view of one project:

```rust
pub struct ProjectMount {
    pub site: String,        // the work's site
    pub work: String,        // the work's folder
    pub link: PathBuf,       // <web>/<site>/<work>/project
    pub target: PathBuf,     // the project root, or the ticket's worktree
    pub mode: MountMode,     // Read | Write | Worktree
}
```

### Implementation: the symlink

The first cut uses a real symlink `<work>/project` → `target`, because the existing `Mounts` layer
already does the hard parts: it follows the link for reads, writes, listings, the index and the
watcher, and it reports events under the web's address. The additions are:

1. **Create it.** `new_work`, `run_stage`, server startup and the first access to a manual work
   ensure the link exists. The target is `ProjectRef.root`.
2. **Register the target.** `Mounts` gains a small registry: `link → mode`. A project outside
   `[projects] dir` is allowed because it is a registered mount target, not because of the global
   allowed list. The guard still refuses another workspace's project.
3. **Distinguish read from write.** `Mounts::check` stays the read check. Add
   `check_write`: if the path is under a `Read` mount, refuse with the reason. `Web::write_file`
   calls `check_write`; `Web::locate`/`read_page` keep calling `check`. That gives one gate for the
   `write`/`edit` tools, `putPage`, `putFile` and the app's routes.
4. **Swap for a worktree.** `run_stage` points the link at the ticket's worktree (the path
   `workers.spawn` already computed). `stage_ended` points it back at the project root. A server
   restart rebuilds the link from each work's `index.md` and the ticket's `branch:`. The link is
   not only restored by a run: startup also links a work made by hand, and a stale link left by a
   stopped run is replaced.
5. **A worktree is still a worktree.** The link is not committed to the project and does not
   change the merge path. `ticket_worktree`, `ticket.branch`, `merge_ticket` and the changes view
   are untouched.

The symlink is visible in the web (`project/` in the work). That is the point: an editor, a page
read, and an app all see the same folder. It is never committed to the project.

### The modes

| mode | `project/` is | write through `project/` | for |
|---|---|---|---|
| `read` | the project root | refused, with the reason | a site that only reads |
| `write` | the project root | allowed | video, renders, 3D assets |
| `worktree` | the ticket's worktree | allowed, on `ticket/<id>` | code |

The app's `putProjectFile(path, work)` follows the work's `project_mode` when a work is given,
else the site's `project_mode`; a `read` work or site refuses with the same reason. The app's
`page`/`putPage` on a work's `project/` follows the work's mount mode; a `read` work refuses.

### What the mount deliberately excludes

- `.git`, `node_modules`, build output (`target`, `dist`, `build`, `out`, `vendor`) are not walked
  by the index or the page search. They are still readable by explicit path if the agent needs a
  file; the walker just does not crawl them. Reuse the `project_facts` skip list.
- Paths are canonicalized and must stay inside `ProjectRef.root` (`..`, symlinks, a mount target
  that leaves the root). The existing `path_inside` rules apply.
- Another workspace's project is still refused by the `ProjectOwnership` guard.
- A project that is a subfolder of a repository maps the same way `ticket_worktree` already does:
  the mount points at the project folder, the worktree at the matching subfolder.

## 5. The project page is the hub

`project.overview` becomes the hub, and the page is its handle:

```rust
pub struct ProjectOverview {
    pub name: String,
    pub page: Option<String>,        // site://projects/A.md
    pub root: String,                // the folder
    pub path: String,                // ~/Projects/A
    pub branch: Option<String>,
    pub uncommitted: u32,
    pub sites: Vec<ProjectSiteSummary>, // the hub
    pub branches: Vec<TicketBranchSummary>,
    pub memory: Option<PageExcerpt>,
    pub harnesses: Vec<HarnessSwitch>,
    pub tool_rules: u32,
}
```

`ProjectSiteSummary` is the site's card on the hub:

```rust
pub struct ProjectSiteSummary {
    pub site: String,
    pub title: String,
    pub works: Vec<WorkSummary>,
    pub running: u32,
    pub waiting: u32,
    pub app: Option<AppSummary>,   // its tile, if it has one
}
```

- `sites` is the union of the project page's `sites:` and the sites with a work whose `project:`
  resolves to this project.
- A site with no work is still listed when it is attached on the page (that is what `sites:` is
  for).
- The app's project screen renders the sites and their works; the app's `tile` apps are the
  project's tiles.
- `project()` for a site's app returns:
  ```rust
  pub struct ProjectInfo {
      pub name: String,
      pub root: String,
      pub page: Option<String>,   // new
      pub branch: Option<String>,
      pub sites: Vec<String>,     // renamed from pipelines
      pub tags: Vec<String>,
  }
  ```
  `pipeline`/`pipelines` stay as deprecated aliases for one release.
- `ProjectSummary` (the list) gains `page`, so the project list can link to the hub.
- The flat `works` list in `ProjectOverview` is replaced by `sites`; a client that wants one list
  flattens the site cards.

The `Project` page is a `type: project` page in `site://projects`, so it is searchable, linkable and
editable like any other. The `projects` site's `ui/` is the hub app; the page body is the project's
memory. The memory site links the page; it no longer has to hold it.

## 6. The app and the agent

### App

The SDK gains two functions, and the app's project-file route takes the work:

```ts
projectFile(path: string, work?: string): Promise<Response>       // GET  .../api/project/files/<path>?work=<work>
putProjectFile(path: string, body: Blob | string, work?: string)  // PUT  .../api/project/files/<path>?work=<work>
```

- `project()` returns the project page's `page` and its `sites`; the app's current work is the
  board's `?work=` (or the app's own state).
- `projectFile(path, work)` follows **the work's project**: the work's `project/` symlink. That is
  the project root in `read`/`write`, and the ticket's worktree in `worktree`. The app's file tree
  and its diff are then the same worktree — which is what reviewing before approving needs.
- Without `work`, `projectFile` is the project's main folder (the app's own project assets).
- `putProjectFile` follows the same rule and the work's `project_mode`; a `read` work refuses.
- The app's review uses the existing `changes` API (`ticket.changes` / the session's diff) against
  the same worktree. Approve and merge stay the existing ticket path.
- `page`/`putPage` stay for the site's pages, including a work's `project/` view.
- `data()` stays the app's state for the project.
- The project hub is the app's project screen; the board is the site's app. They are different
  screens, both opened on the same project.

### Agent

- `set_project` accepts a page or a path; the session stores the canonical root; the prompt loads
  the project page, the branch, the site's `index.md` and the work's `project/` mode.
- `enter_site` with `project: required` refuses without an active project; the prompt says how to
  set one.
- `open`, `search`, `write` and `edit` follow `project/`. The `write`/`edit` path goes through
  `check_write`, so a `read` work refuses on the first write.
- `run` recipes in the site get `PROJECT_ROOT` only when the site's `project_mode` is `write` or
  `worktree`. A `read` site's recipes do not get it.
- `new_work` creates the work's `project/` link; `run_stage` swaps it to the worktree; `stage_ended`
  swaps it back.
- The orchestrator spawns an agent with the work page + the project page in `context`; the
  subagent's `project/` is the same mount. The parent can `message_agent` it while it runs.

## 7. Flows

### Create a project

1. `new_project(name, about)` creates `~/Projects/<name>` and
   `site://projects/<name>.md` (`type: project`, `root: ~/Projects/<name>`, `sites: []`).
   It lists the page in `site://projects/index.md` and, if there is one, links it from memory.
2. It claims the folder in `state/projects.json` for the workspace.
3. It sets the session's project. The project page is the hub.

### Attach a site

1. `site://coding/index.md` says `project: required`, `project_mode: worktree`.
2. The project page's `sites:` or the first `type: work` in that site attaches it.
3. The project hub lists it; the app's tile list shows it (`project_home`).

### New work

1. `new_work(pipeline, name, project)` writes `site://coding/game-save/index.md` and the stage
   files.
2. It resolves `ProjectRef`, creates `<work>/project` → the project root, and records the
   `worktree`/`write`/`read` mode.
3. The work appears under the site on the project hub.

### Run a stage

1. The stage's `project_mode` (or the work's, or the site's) is `worktree`.
2. `run_stage` creates the ticket, `workers.spawn` makes the worktree, and the link points at it.
3. The agent reads and writes `project/`; the app's `projectFile(path, work)` reads the same
   worktree, and the app's `changes` shows the diff.
4. On ending, the link points back at the project root; the branch waits to merge.

### Review a worktree

1. The app opens the work (`?work=game-save`); `projectFile(path, work)` is the worktree.
2. The app's file tree is the worktree, and `changes` is the diff of that worktree against the
   ticket's base. The same branch is what `merge_ticket` would land.
3. The reviewer inspects files and diff in one screen, edits in place if needed (a `worktree` work
   allows it), then approves or sends the worker back with a note.
4. Merge removes the worktree and deletes `ticket/<id>`; the app falls back to the project root.

The source of the worktree is a `ticket_worktree` detail: locally it is a git worktree under
`state/worktrees`; at work it can be a feature branch in the same repository. The app's
`projectFile(path, work)` and `changes` use the worktree path, so both are the same shape.

### Open the app

1. The project hub's site card opens `/<workspace>/p/<project>/<site>/`.
2. The app's `project()` returns the page and the sites; `projectFile(path, work)` reads the work's
   project (or the main project without a work); `runAction` runs the site's recipes with
   `PROJECT_ROOT`.
3. The app's `send()` goes to the session that opened it; the agent answers with the site and the
   project in context.

## 8. Edge cases

| case | behavior |
|---|---|
| project page missing | `ProjectRef` has `page: None`; the folder is the project; `set_project` can create the page |
| page without `root:` | not a project page; `ProjectRef::resolve` falls back to the name/path |
| folder missing | error naming the page and the root; the page is not silently used |
| project outside `[projects] dir` | allowed as a registered mount target; the ownership guard still applies |
| nested projects | `ProjectPath` and `ProjectOwnership` already treat nested folders as one project; the mount uses the canonical root |
| two works, same project | each work has its own `project/` link; both point at the same root |
| two works, one worktree | only the ticket's work has the worktree target; the other keeps the project root |
| worktree removed by hand | the next `run_stage` recreates it (`ticket_worktree` reuses or recreates); the link is rebuilt |
| server restart | the startup pass rebuilds every work's link from its `project:` and the ticket's `branch:` (not only a run's link; a hand-written work is linked there too) |
| manual work (`write`, no `new_work`) | the first `open`/`write` under `project/` ensures the link, or says to set `project:` |
| read-only write | `web.write_file`/`putPage`/`putProjectFile` refuse with the mode and the reason |
| `.git`/build output | not indexed or searched; readable by explicit path |
| project symlinks | canonicalized; a link that leaves the root is refused, like `path_inside` |
| another workspace's project | the `ProjectOwnership` guard refuses the mount and the app grant |
| app on Home | `project()` is null; `projectFile` is 403; the site is listed only on projects |
| old `_stage.md` work | no `project_mode`; default `read`; the old `workflow:`/`path:` read as before |
| old `project:` path | resolved to the page when one exists; otherwise a synthetic page-less project |

## 9. Implementation plan

### Phase A: the page (no mount)

- `ProjectRef` + `ProjectRef::resolve`; `new_project` writes `site://projects/<name>.md`
  (`type: project`, `root:`, `sites:`); the `projects` site's `index.md` lists it.
- `project_list`, `project_overview`, `project_info` read the page; old `site://memory/projects`
  pages read as a fallback.
- `ProjectInfo` gains `page`; `pipelines` becomes `sites` (old field kept).
- `ProjectOverview` gains `page` and `sites`; `ProjectSiteSummary`.
- Tests: page/path/name resolve to one root; old page read; folder without page.

### Phase B: the work and the mount

- `Work` resolves `project:` via `ProjectRef`; `project_mode` from the work/stage/site.
- `Mounts`: the link registry, `check_write`, the mode; `Web::write_file` uses it.
- `new_work` / `run_stage` / `stage_ended` / startup ensure and swap the link.
- `open`/`write`/`edit` through the link; `search` indexes only the safe folders.
- Tests: read/write/worktree; path escape; `.git` not indexed; worktree swap; restart rebuild.

### Phase C: the app

- `projectFile(path, work)` / `putProjectFile(path, body, work)` wrappers; without `work`, the
  main project. `Project` gains `page`/`sites`.
- The project hub reads `ProjectOverview.sites`; the site cards open the apps.
- The app's review uses `changes` against the same worktree `projectFile` reads.
- `putProjectFile` honors the work's `project_mode`; `putPage` under `project/` honors the work's
  mount mode.

### Phase D: the site and the agent

- `project: required` / `project_mode:` in the site's `index.md`; `enter_site` checks it.
- The prompt: the project page, the site index, the work's `project/` mode.
- `run` recipes: `PROJECT_ROOT` only for `write`/`worktree`.
- The orchestrator's `spawn_agent` context includes the work + project pages.

### Phase E: the old pipeline

- Done: the pipeline's `index.md` declares the graph; the work's `index.md` holds `stage`,
  `status`, `done`, `attempts` and each run's record. `_stage.md` is no longer written (old works
  still read it). `run_gates` and the gate watcher are gone; a stage starts only when asked.
- Remaining: delete `pipeline.list`/`work.run` after one release, and let the board read the
  pipeline/work pages directly instead of the command shim.

## 10. Non-goals

- No scheduler, no database, no new app, no new SDK framework.
- No copy of the project into the site and no copy of the site into the project.
- No per-work branch for a `write` site; `write` is in place, `worktree` is the branch.
- No project-local site yet. It is the same model with the site's folder inside the project; build
  it after the global case is stable.
- No new agent tools. The `project/` view uses `open`/`write`/`edit`/`run`; the app uses its file
  API.
- No multi-project work. A work names one project; a site can have many works.

## 11. Open questions

1. **The page's home.** `site://projects` is the answer: its `index.md` is the list, its `ui/` is
   the hub app, its pages are the projects. Memory keeps memory and links the page.
2. **`project_mode` on the site vs the stage.** The site gives the default; the work or stage
   overrides. A video site defaults `write`, a coding site `worktree`.
3. **One page per folder, or many pages?** One; `ProjectRef` picks the page whose `root:` matches.
   A second page for the same folder is an error the resolver can report.
4. **The app's `projectFile` vs the work's `project/`.** `projectFile(path, work)` follows the
   work, so it is the same view as the work's `project/`: the project root, or the worktree when
   the stage writes code. Without `work`, it is the project's main folder. The app's `changes` is
   the diff of that same worktree, so review reads the branch the merge would land.
5. **Does the hub live in the app or on the page?** The `projects` site's `ui/` is the hub app; the
   project page is the data. A site's app is still the site's UI.
