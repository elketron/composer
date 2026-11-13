# Site packs

Status: proposal, 2026-11-12. The shape is settled; not built.

A **site pack** is one site made portable: a folder you can put on GitHub,
clone on another PC, and install into a workspace. Sites are the unit because
that is where a workflow, its UI and its instructions live.

The repo already has the line this design needs: [`SiteDefinition`](../crates/server/src/library/site_definition.rs)
is exactly what a site **is** (its `index.md`, `justfile`, `ui/` source,
`stages/`, `_config/`, `templates/`, and `library:` extras), while works,
tickets, pages and threads are **content**. A pack can carry either; the
definition is the default.

## Definition or snapshot

| | **Definition pack** (default) | **Snapshot pack** (`--with-content`) |
|---|---|---|
| what | `index.md`, `justfile`, `ui/` source, `stages/`, `_config/`, `templates/`, `library:` extras | the definition **plus** pages, works, boards, threads and assets |
| for | a workflow or app you use in several workspaces | moving a mockups site, a board, or a site with its data |
| safe to publish | yes: no works, tickets, pages or threads | depends on the content; treat it as personal |
| GitHub | one repo per site, diffable | one repo per site, private unless you mean it |

A mockups site's definition is the renderer and shell; its mockups are content.
If you want the mockups, pack a snapshot. If you want the workflow/UI as a
reusable capability, pack the definition.

The existing `composer library publish` is the local sibling: it publishes a
definition to the home's library for every workspace on this machine. A site
pack is the same definition in a portable folder or Git repo.

## Settled 2026-11-12

- **Definition by default.** A pack is the workflow/UI definition unless
  `--with-content` asks for a snapshot.
- **`ui/dist` off by default.** The pack is source-only; `--with-dist` includes
  the built UI when a ready-to-serve pack matters more than a clean diff.
- **One repo per site, for now.** The folder layout still allows several
  `*.site/` folders in one repo if that becomes preferable.
- **Library stays local; packs go to GitHub.** `composer library publish` and
  `composer site pack` share the same `SiteDefinition`, but keep separate
  destinations and lifecycles.

## Pack tree

Definition pack:

```text
mockups.site/
  composer.site.toml       manifest: schema, name, mode, requirements, omissions
  README.md                generated: what it is, install command, what stays out
  index.md                 the site's homepage and instructions
  justfile                 its actions
  ui/                      source (not node_modules or dist)
  stages/                  a pipeline's stages, if any
  _config/                 a pipeline's config, if any
  templates/               a pipeline's templates, if any
  tools/                   a `library:` extra, if declared
```

Snapshot pack adds the site's content at its real paths:

```text
mockups.site/
  ...
  login.html
  settings.html
  threads/
  001-login/
```

The pack is a **copy of the site folder**, not a wrapper: it installs straight
into `workspaces/<workspace>/sites/<site>/`, so relative links and the
`site://` addressing keep working.

An archive is the same tree as `<site>.site.tar.zst` (or `.zip` with
`--format zip`).

## `composer.site.toml`

```toml
schema = 1
name = "mockups"                  # a hint; install --as may rename it
title = "Mockups"
description = "Mockup pages with threaded comments."

[pack]
composer = "0.1.0"
created = "2026-11-12T10:00:00Z"
content = false                   # snapshot when true
dist = false                      # ui/dist included when true

[contents]
files = 18
bytes = 128_400

[requires]
mcp = []                          # from the site's frontmatter, names only
applications = []                 # `app: <name>` entries
tools = []                        # native tools the workflow adds
client_app = false                # app.client: true -> needs composer app approve
project = "optional"              # project: required | optional
project_mode = "read"             # read | write | worktree
model = ""                        # later: a workflow's model
```

`requires` is derived from `index.md` and is informational: no config values,
no secrets, no machine paths.

Everything except `schema` and `name` is optional on install. A `schema` newer
than the installed composer supports is refused with the version in the message.

## What is in and what is out

| Path | Definition | Snapshot | Why |
|---|---|---|---|
| `index.md`, `justfile` | yes | yes | The site's instructions and actions. |
| `ui/` source | yes | yes | The micro app; it is the UI. |
| `ui/node_modules/`, `ui/dist/` | no | no (unless `--with-dist`) | Generated and heavy; `dist` can be rebuilt. |
| `stages/`, `_config/`, `templates/` | yes | yes | A pipeline's definition. |
| paths under `library:` in `index.md` | yes | yes | The site declares what else belongs to its definition. |
| other top-level entries | no | yes | Works, tickets, pages, boards, images. |
| `threads/` | no | yes | Feedback is content, not the renderer. |
| hidden files (`.env`, `.git`, `.DS_Store`) | no | no | Never copied. |
| `*.pem`, `*.key`, `id_*`, `.netrc`, `.git-credentials`, `secrets.*` | no | no | Excluded and reported. |
| symlinks, including `work/project/` mounts | no | no | Never followed; a snapshot records the mount. |
| an app's `ui/dist` | no | `--with-dist` only | The app's build, not its source. |

A definition pack is exactly the set [`SiteDefinition`](../crates/server/src/library/site_definition.rs)
already computes; it has been the line for library publish since the start. A
snapshot uses the definition's `left_out` list to find the content.

## Requirements

A site's `index.md` says what it needs:

```yaml
---
title: 3D
type: workflow
app: blender              # needs [applications.blender] in the workspace/home config
mcp: [blender]            # needs [mcp.blender] and its [sites] grant
tools: [hunk]             # native tools added while inside
project: required         # the target needs a project
project_mode: worktree
---
```

The pack records these names, never the definitions:

- `mcp`: if the target workspace has no `[mcp.blender]`, installing prints
  `mcp blender is not configured; add it to config.toml (or install its .mcpb
  bundle)`.
- `applications`: `app: blender` needs `[applications.blender]`; the target's
  app entry is local.
- `client_app`: a site with `app.client: true` is a client only after
  `composer app approve <site>` on the target; the pack does not approve it.
- `tools`: native tools are part of composer; print if the target's version
  does not have one.
- `project`/`project_mode`: the site is usable without a project when
  `optional`; `required` says so on install.
- `library:` extras are included in the definition; they are not requirements.

No site config is packed. A site does not own `config.toml`; its needs live in
the workspace's config. This avoids copying provider keys, MCP commands,
sandbox binds or project paths into a GitHub repo.

## Projects and mounts

A site snapshot may contain a work whose `project/` is a symlink to a project or
a ticket worktree. The pack records it, never copies it:

```toml
[[mounts]]
work = "game-save"
project = "composer"             # a ProjectRef name, not a path
mode = "worktree"
```

On install, create the site without the symlink. If the project resolves on the
target (by name in `[projects] dir` or its project page), create the link; if
not, print:

```text
project composer is not on this machine; site coding/game-save is readable, its project/ is missing
```

Never copy a project, never follow a symlink out of the site, never invent a
project root.

## Commands

Both commands work on the machine that holds the workspace's files (the
server). A desktop or phone connected to a remote server gets a Pack/Install
action only when the host API exists (phase 4).

### Pack

```sh
composer site pack mockups
# -> mockups.site/ in the current directory

composer site pack mockups --out ~/packs
# -> ~/packs/mockups.site/

composer site pack mockups --with-content --out ~/packs
# -> the mockups and threads too

composer site pack mockups --with-dist --archive --out ~/packs
# -> ~/packs/mockups.site.tar.zst with ui/dist

composer site pack mockups --dry-run
# files, bytes, mode, requirements, left-out content, secret-looking files
```

- Refuses to overwrite an existing output; `--force` is explicit.
- `--with-content` is the snapshot; it prints a warning and the top-level
  content it would include.
- `--with-dist` includes `ui/dist`; default is source only, because a built app
  is generated output and `just ui-build` can make it again.
- `--no-library-extras` omits paths declared under `library:`; default is to
  include them because they are part of the definition.
- `--format zip` with `--archive` for Windows/hand-off.
- Packing is a copy; the source site is not modified.

### Install

```sh
composer site install ./mockups.site
composer site install ~/packs/mockups.site.tar.zst
composer site install https://github.com/me/composer-site-mockups
composer site install https://github.com/me/composer-site-mockups --as mockups-lab
composer site install ./mockups.site --workspace demo
```

- Source may be a local folder, an archive, or a Git URL. A Git URL is cloned
  with `git clone --depth 1` into a staging folder first; `.git` never reaches
  the workspace.
- The target workspace defaults to `$COMPOSER_WORKSPACE`, then `default`; use
  `--workspace` for another.
- `--as` renames the site. A manifest name that is not a valid `SiteName` is
  refused; `--as` is the fix.
- Refuses an existing site name in the target workspace; no `--force` in v1.
- Staged copy, validate, then move into `sites/<name>` in one step; a failure
  leaves no half site.
- If the host is running, v1 installs on disk and says to restart (or the next
  `composer serve`); a live API is phase 4.
- After install, a checklist:

```text
installed mockups into workspace demo at ~/.composer/workspaces/demo/sites/mockups
  definition  18 files, 128.4 KB
  content     none (definition pack)
  build       ui/dist missing: run `just ui-build` in the site if it has a UI
  requires    mcp none · applications none · tools none
  approve     no client app
  projects    none
```

For a snapshot, the content count and any mounts are in the same checklist.

## GitHub in three commands

```sh
composer site pack mockups --out ~/sites
cd ~/sites/mockups.site
git init && git add . && git commit -m "composer site: mockups"
gh repo create composer-site-mockups --private --source=. --push
```

On the other machine:

```sh
composer site install git@github.com:me/composer-site-mockups.git --workspace demo
composer serve
```

For a release:

```sh
composer site pack mockups --with-content --archive --out dist
gh release create mockups-v1 dist/mockups.site.tar.zst
```

The generated `README.md` repeats the install command, the mode (definition or
snapshot), the requirements and the omitted content, so the repo is usable
without this document.

## Library vs pack

| | `composer library publish` | `composer site pack` |
|---|---|---|
| destination | the home's `sites/` library (local git) | a folder, archive or GitHub repo |
| scope | every workspace on this machine | any PC |
| content | never; definition only | definition, or snapshot with `--with-content` |
| updates | three-way against the library commit | later: link + merge; v1 is a snapshot |
| approvals | recipes are re-approved on install | recipes are **not** approved on install |
| UI | TUI tool / `composer library` | CLI first; desktop panel later |

A definition pack is deliberately the same file set as a library site. That
means `composer site pack` can reuse `SiteDefinition`, and a pack can later be
installed into the library without a second definition of "site".

An ICM site adds its method to the definition with `library:` in `index.md`:
`AGENTS.md`, `CONTEXT.md`, `_shared/`, `_templates/`, `_tools/` and
`knowledge/`. Its runs (`works/<ticket>/`) are content, not definition. That is
ICM's factory/product split in the pack: the definition is the method, the
snapshot is the product.

## UI

The CLI is v1. The same two actions belong in the clients later.

### Desktop (GPUI)

A site card (project hub or the site list) gains **Pack…**:

- **Definition** / **Snapshot** radio. Snapshot shows the content it would
  include and a private-data warning.
- **Include built UI** (`ui/dist`), off by default.
- Destination path; a preview of files, bytes and requirements.
- **Pack folder** and **Pack archive**; after, a notice with the path and
  `Copy git commands`.
- **Install site from pack…** takes a path or Git URL, a workspace, an optional
  name, a `Check` step and the checklist.

The panel uses gpui's grammar: amber focus, flat rows, `rounded_md`, 12px
chrome. It is not a web settings page.

### Phone (Flutter)

Site packing is a server-side, rare action; no file picker in v1. The phone
shows **Packed to …** when a pack is started from the desktop or CLI. Install
from a URL can arrive with the host API later. The 44px target and bottom-sheet
grammar apply.

### TUI

`:site` is the workflow switch; do not overload it. `composer site pack` and
`composer site install` are the honest interface until a `:pack`/`:install`
alias exists.

## Security

- Definition packs are safe to publish: `SiteDefinition` never includes works,
  tickets, pages, hidden files, `node_modules`, `dist` or symlinks.
- Snapshot packs warn: content can contain personal pages, threads and images.
- Secret-looking files are excluded and reported in both modes; hidden files
  are never copied.
- Installing does not approve recipes, client apps or MCP tools. The first run
  asks, as with a fresh site.
- A Git URL is cloned to staging, validated, then copied; no repository hooks
  run in the workspace.
- The pack is code from the network. A public pack should be reviewed like any
  repository; the checklist after install is the review prompt.

## Versioning and update

- `schema = 1`; a newer schema is refused.
- `pack.composer` and a future `min_composer` are informational; `install`
  warns when the target is older.
- A pack is a snapshot. A future `composer site link <url>` can remember the
  source; a future `composer site update` can pull and three-way merge like
  `composer library update`. v1 does not.
- A snapshot pack is not a merge base; installing it into an existing site is
  refused. Definition packs are the reusable unit.

## Phases

1. **Definition pack and local install.** Manifest, reuse `SiteDefinition`,
   exclude content/generated/symlinks, staged install, `--dry-run`, tests.
2. **GitHub and archives.** `install <git-url>` with a staged clone,
   `--archive` tar.zst/zip, generated README and git commands.
3. **Snapshots.** `--with-content`, `--with-dist`, mount records, the private
   data warning, content checklist.
4. **UI and live install.** `site.import` on a running host, the GPUI pack
   panel, the phone notice, TUI aliases.
5. **Link and update.** Remember a Git source and merge a site update.

## Open questions

1. **Site config.** A site needs `[mcp]`, `[applications]` and sometimes
   `[apps]`. v1 prints requirements. Should a pack carry a
   `site.local.example.toml`, or is the workspace config the only owner?
2. **Snapshots and updates.** A snapshot cannot be merged. Should `site update`
   support only definition packs, and snapshots stay one-way?
3. **Library round trip.** Should `composer site pack` accept a library site
   directly, and `site install --library` put a pack into the home's library?
4. **Client apps.** A pack can include a client app's source; installing still
   requires `composer app approve`. Should the pack record the build's hash so
   approval can be one explicit click?

## Done when

A user can run `composer site pack mockups`, push `mockups.site/` to a private
GitHub repo, run `composer site install <url> --workspace demo` on another PC,
see the same workflow and UI, add any required MCP/application entries, approve
it as a client app if it is one, and open it — with no works, tickets, pages,
tokens or secrets in the definition repo unless they explicitly packed a
snapshot.
