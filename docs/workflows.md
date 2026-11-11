# Workflows and the shared view

Status: design, settled 2026-11-10. Built: `.mcpb` bundles (uncommitted), the pieces the switch
joins (`enter_site`, the launcher, the MCP client), the server half of the switch (`mcp:` grants,
`app:` launching, the shared enter/leave functions in `agent::site_switch`) and the user-side switch
(`site.set`, `:workflow` / `:site` in the TUI). Phase 3's server half (2026-11-10): composer proxies the viewer at `<workspace>/view` (view only for a paired device; the machine's own user, with the workspace's local token, can drive: see "Driving through the proxy" below) and
`/api/sites` and `/api/info` tell clients about workflows and the viewer. The phone's half (built
2026-11-10, board E): the session screen with the workflow chip, the resizable live view and the
newest-first chat, and the inbox / workflows / pages / server bar. Not built: `view:`, `model:`. Everything else here is a proposal.

Read [lean-orchestration.md](lean-orchestration.md) first for the base (a small harness, capabilities
as sites). This page is the layer above it: what a workflow is, what the user and the agent look at
together, and what the clients are for.

## The aim

One agent, and for each workflow a shared view the user and the agent both look at and point into.

The owner's words: "a way to cowork with ai in a shared view of some sort". And for the feeling: the
agent follows me to whatever I am working in; I don't go to where the agent is.

## Where the current design left that path

Found in a review of the tree and of the 197 sessions in the default workspace (2026-11-10):

| observation | evidence |
|---|---|
| where the user is and what the agent is are unrelated | a session has `desk.active` (the tab shown, client-driven) and `session.site` (prompt, tools, write scope); only `enter_site` / `leave_site` write `session.site` |
| the capability mechanism is unused | 0 of 197 sessions have a site; 0 calls of `enter_site`, `show_app`, `show_page`, `desk`, `selection`, `terminal_*`, `hunk`, `watch`; no installed site declares `tools:` in its frontmatter |
| workflows live in agent pages | seven agent pages (coder, reviewer, default, worker, orchestrator, stage, ideation); reviewer and coder are 109 of the 197 sessions |
| "the agent is beside me" is bought in client code | the desktop shell (gpui + CEF, `apps/desktop`) hosts the apps next to the chat; the phone got its own desk |
| what is used | `run` 4639, `edit` 1627, `open` 1574, `answer_approval` 1106 calls |

The desk-era features were days old at the time, so the zeros are partly age. The direction is still
clear: the UI follows the agent, and it does so by hosting everything.

## The workflow folder

A workflow is a site. Its `index.md` body is the instructions; its frontmatter names what comes with
it. Every key is optional.

```yaml
# site://3d/index.md
---
title: 3D
type: workflow
app: blender            # what to launch: an [applications.blender] entry of config.toml, in wayai
mcp: [blender]          # MCP servers whose tools the agent has while here, if their config allows it
view:                   # a view type (below), when no application shows the object
model:                  # later: a named provider/model for this workflow
---
How 3D work is done here: where files go, what to check, when to ask.
```

```yaml
# site://coding/index.md        terminal only: no app, no mcp, no view
---
title: Coding
type: workflow
tools: [merge_ticket, hunk]
---
```

```yaml
# site://video/index.md         a view type over the work's files, tools without an application
---
title: Video
type: workflow
mcp: [comfyui]          # illustrative: no such server is configured yet
view: storyboard
---
```

`app:` is a name, never a command: site pages are writable by agents, so the command comes from
`config.toml`, where declaring it is the user's approval (as with `[[launcher]]`):

```toml
[applications.blender]
command = "blender"      # required
args = []                # optional
env = {}                 # optional, extra environment for the app
mcp = "wayai"            # optional, default "wayai": the MCP server whose `launch` tool starts it
```

`mcp:` is a request, not a grant, for the same reason: which sites may grant a server is decided in
`config.toml`, by the server's own entry. A site not listed there gets none of the server's tools,
whatever its page says; the default (no `sites`) is that no site may.

```toml
[mcp.blender]            # a hand-written server, or the overlay of a bundle of that name
sites = ["3d"]           # the sites whose `mcp: [blender]` grants its tools
```

Entering a site that names a server it may not grant still succeeds, and says so in words, to the
agent (`enter_site`'s result) and to the user (`site.set`'s note): "site 3d names MCP server
blender, but [mcp.blender] does not list it in `sites`". An agent page's own `tools:` grants
(`blender__*`) do not depend on `sites`.

`app:` has a second, older meaning: a *block* of keys (`scope`, `when`, `tile`...) is a micro app's
manifest. A string is always the workflow's application; a site with `app: blender` and a `ui/`
folder is a micro app with the default manifest, opened with a token of its own like any other.

`tools:` (native tools added while inside), `project:` and `project_mode:` keep their meaning from
[lean-orchestration.md](lean-orchestration.md) and [project-hub.md](project-hub.md). `app:`, `mcp:`,
`view:` and `model:` are new.

Planned workflows: coding, cad, 3d, video, image. Each absorbs what an agent page or a pipeline's
stage contracts know today: the coder and reviewer instructions become the coding workflow's pages.

## The switch

Entering a workflow does, in one step:

1. puts the folder's `index.md` in the prompt (built: `Site`, `system_prompt`);
2. adds its `tools:` to the base set (built: `ToolRegistry::specs_for(agent, site)`);
3. adds its `mcp:` servers' tools and drops the previous workflow's (built: every tool of those
   servers whose `[mcp.<name>] sites` lists the site is offered and callable while
   `SessionInfo.site` names it, in addition to the agent page's `tools:`; a server that does not
   list the site is skipped, its calls are unknown tools, and entering says so; approval is
   unchanged; `McpServers::allows` is the one rule);
4. launches its `app:` in wayai (built: the `[applications.<name>]` entry is started through the
   `launch` tool of its MCP server, default `wayai`, unless that server's `list_apps` already
   mentions the command; a failure never stops the entering);
5. later, uses its `model:` (below).

Rules:

- **Either side switches.** The agent with `enter_site` (exists); the user with a command (`:workflow
  cad` in the TUI, the workflows tab on the phone). The user's side is the protocol command
  `site.set` (`session`, `site`; no site leaves), built, with `:workflow [name]` / `:site [name]` in
  the TUI; its reply is a note saying what happened, the application's outcome included (started,
  already running, skipped and why), and the session's new `site` arrives as `session.updated`. The
  phone's workflows tab is still to do.
- **The conversation stays.** Same session, same history, same memory; only the prompt, the tools and
  the view change from the next turn. One workflow at a time.
- **Leaving** drops the tools and the prompt. Whether it closes the application is an open question.

## The shared view

Two kinds, chosen by whether a real application already shows the thing.

### A real application, in wayai

For Blender, FreeCAD and the like. The agent drives the application through its own MCP server, with
wayai's screen and input tools as the fallback. The user looks through wayai's viewer (`/view`), which
is one URL on every device.

| | state |
|---|---|
| viewer: the screen as JPEG over a WebSocket, every 100 ms; annotations the agent reads and answers | exists in wayai (`wayai-viewer`) |
| desktop: the viewer forwards the user's pointer and keys | built (Drive mode in `wayai-viewer`; `--no-drive` disables it) |
| phone: view only | not built in the app; it is a webview on the same URL |
| reaching it from another device | wayai listens on localhost; composer proxies `<workspace>/view` behind the workspace's token (built: `[viewer]` in config.toml, `viewer` in `/api/info`). A paired device's token is view only (its messages are dropped, and the page is asked for without Drive); the local token, which the CLI, TUI and desktop on the machine use, may drive |
| the desktop's live view tab | built: entering a workflow whose application runs puts a `live_view` tab on the session's desk (`DeskTab::LiveView`, id `live`); the shell shows the full page |

- **Who has the controls.** The user and the agent share one pointer and keyboard. The user's input
  pauses the agent's and the agent is told, so it reads the application's state again before acting.
- **One screen per wayai.** Enough for one agent the user follows; a second workflow running at the
  same time needs a second instance.
- **Frame rate.** 10 frames a second is fine for clicking and orbiting, not for sculpting or
  scrubbing; if it matters, frames by damage, in wayai.
- **Per launch: desktop or wayai.** An application on the real desktop is faster to use and invisible
  from the phone.

### A view type, in a shared shell

For objects no application shows: a storyboard, a review, a set of mockups. Modelled on what worked
in Claude's artifacts: the viewer is fixed, the agent only writes data.

| part | what it is | written |
|---|---|---|
| shell | comment threads anchored to things, reply, resolve, live reload, the hand-off to the chat (built: the sdk's `Threads`); versions later | once, by composer |
| view type | a renderer for one kind of object, which names its anchors (a scene, a part, a hunk, a board) | once per workflow |
| instance | the work's files | by the agent, every time |

- The agent never builds UI for a work; it writes files the view type renders.
- Threads are files beside the work, so the agent reads them with `open` and `search`.
- This keeps the rule that each workflow gets its own app (no generic board): the renderer is built
  for the object. What stops being rebuilt per app is everything around it.
- Today's micro apps are each their own Svelte code with one-shot feedback (`select()` puts a chip on
  the next message; `send()` posts text). They keep working; the shell is where new ones go.

#### Threads (built, phase 4)

One markdown page per thread in a `threads/` folder of the site, or of the work folder when the view is
opened on a work. The agent reads them with `open`/`search` (`type:thread status:open site:<site>`),
replies by appending a section with `edit`, and resolves by setting `status: resolved`: no new tool.
Its conventions are in `site://home/threads.md` (seed) and in each view type's `index.md`.

```markdown
---
type: thread
title: "mockup login: the button is too small"
anchor: mockup/login                    # the view's element id (data-anchor)
anchor_label: mockup login              # for people (data-anchor-label)
status: open                            # open | resolved
created: 2026-11-11T14:02:00Z
updated: 2026-11-11T14:05:00Z
---
### you · 2026-11-11T14:02:00Z

The button is too small on a phone.

### default · 2026-11-11T14:05:00Z

Made it full width below 600px.
```

- A message starts at a line `### <author> · <time>` (`you`, or the agent's name; UTC, seconds) and
  runs to the next one. Values that YAML would misread are written as JSON strings.
- Anchor ids are paths: `a/b` is a part of `a`, and a pin on `a` counts the open threads of both.
- File names: `threads/<date>-<label slug>-<4 random>.md`.
- **The sdk.** `threads.ts` holds the pure functions (`parseThread`, `formatThread`, `appendMessage`,
  `setStatus`, `openCount`, `threadUrl`, `agentMessage`; `bun test` in `templates/sdk`), and
  `components/Threads.svelte` the UI: comment mode (hover highlights `[data-anchor]` elements, a click
  picks one), pins with the open count, a panel (messages, reply, send to agent, resolve, reopen, the
  list), live through `onChange(<root>/threads/*)`. It writes with `putPage` and the page's hash; a
  write refused because the agent changed the page meanwhile is re-read and applied again.
- **To the agent.** "Send to agent" writes the thread, then `send()`s `Comment on <anchor_label>
  (thread <url>): <text>` with the thread page (and the view's `attach(anchor)` pages); the session gets
  it as a user message from the app's site. A plain comment is only stored. A reply reopens a
  resolved thread.
- **A view's own anchors.** A view can start a thread on something it picks itself (the mockups view: a
  rectangle dragged on the preview, `mockup/<name>/region/<x>,<y>,<w>,<h>` in percent of the page) with
  `shell.comment({ id, label })`, and renders an element with that `data-anchor` so the pin shows there.
- **Later.** Versions in the shell (the mockups view keeps its own); threads on a work's folder are
  supported (`root`) but no view uses them yet.

## One gesture

Point at something and say something. The agent gets it with the thing attached, answers in place, and
resolves it.

| view | anchored to | exists as |
|---|---|---|
| view type | an element the renderer named | built (the shell: `Threads` in the sdk; mockups) |
| application | pixels on the screen | wayai annotations (`annotations`, `reply_annotation`) |
| a diff in the terminal | a hunk or a line | Hunk notes ([tui-mode.md](tui-mode.md)) |

One model for all three is the goal; the first two are what the phases build.

## `.mcpb` bundles (built)

An MCP Bundle is a zip with a `manifest.json` that says how to start a local server. The design is in
[plan.md](plan.md) ("Bundles (`.mcpb`)"); the code is `crates/server/src/mcp_bundles/`.

- A file in `<home>/mcps/` is the install, shared by every workspace. It is unpacked to
  `<home>/state/mcps/<name>/`, again only when the file's hash changes. No sandbox bind may reach
  `<home>/mcps`.
- Limits (`BUNDLE_LIMITS`): the file at most 256 MiB, 100,000 entries, 256 MiB per unpacked file
  and 1 GiB unpacked in all, counted while the bytes are written; a bundle over one is skipped
  with a warning that names the limit, and nothing of it stays on disk.
- The server cannot write its own code: the unpacked folder is bound read-only in its sandbox.
  What it writes goes to `<home>/state/mcps/.run/<name>/`, the one folder bound read-write, named
  in `COMPOSER_BUNDLE_STATE`. A `uv` bundle's environment and cache are there
  (`UV_PROJECT_ENVIRONMENT=<run>/venv`, `UV_CACHE_DIR=<run>/uv-cache`), and it starts in
  `<run>/project/` instead of the unpacked folder, because `uv run` writes `uv.lock` beside
  `pyproject.toml`: that folder holds copies of the bundle's top-level files and links to its
  top-level folders, is made again on every config load (only `uv.lock` and `*.egg-info` are
  kept), and is what `${__dirname}` means for a `uv` bundle. Other bundles start in the unpacked
  folder. With `[sandbox] enabled = false` none of this is enforced.
- The name is the manifest's `name` as a slug (`Blender` → `blender`).
- No `[mcp.<name>]` entry is needed to register it. One without `command` or `url` overlays the
  bundle: `allow`, `sites`, `enabled`, `env`, binds, and `[mcp.<name>.config]` for the manifest's
  `user_config`.
- A bundle's tools come with a workflow only through that overlay: `[mcp.<name>] sites = ["3d"]`
  names the sites whose `mcp:` may grant them. A bundle without the entry is granted by no site
  (an agent page's `tools:` still can).
- Every tool asks for approval until allowed.
- `McpServerConfig` gained `cwd`, usable in hand-written entries too.

Verified 2026-11-10 with Blender Lab's `blender-1.0.3.mcpb` on a scratch server: unpacked, `ready`
with 26 tools inside the sandbox, an `allow = ["get_*"]` overlay applied, and an agent listed the open
scene through `blender__get_objects_summary`. The bundle's server needs Blender's MCP add-on enabled
and listening (`localhost:9876`); both sides take another port (`BLENDER_MCP_PORT`, the add-on's
preference), which a second Blender in wayai needs.

Known limits:

- a new bundle is seen on config reload or restart, not by a watcher;
- the run folder is per bundle, not per workspace, and a `uv` bundle's environment in it is
  writable by the server: it cannot change the unpacked code, it can change its own `venv`;
- each `uv` bundle has its own cache now (the first start downloads its dependencies again); the
  old shared `<home>/state/mcps/.uv-cache` is no longer used and can be deleted;
- the file is read and hashed on every config load;
- only the `linux` platform override is applied; composer ships no runtimes (`uv`, Node, Python come
  from the machine);
- `user_config` and its missing-settings error are covered by unit tests only;
- the sandbox allows binds under `<home>/state/mcps` (`SandboxSpec::allowed`), which is how the
  code (read-only) and the run folder get in;
- `execute_blender_code` is unrestricted Python in Blender (the add-on's own sandbox only blocks
  `sys.exit()`): leave it on approval.

## Clients

| client | job |
|---|---|
| terminal (TUI) | the chat, coding, approvals: where the user sits |
| a browser tab | the shared view: wayai's viewer, or a view type |
| desktop shell (`apps/desktop`, gpui + CEF) | the same two in one window: the chat beside an embedded web pane showing the shared view |
| phone (Flutter) | check in, approve, watch |

The desktop shell stays (owner, 2026-11-10): on the desktop you still want the interactive things
inside the app, not only in a separate browser. Its web pane shows what a browser tab would: the same
URLs, nothing only the shell can do (the live view tab is `<workspace>/view`; a browser holding the
workspace's local token drives it the same way). What the shell adds is passing keys, modifiers and
drags through to the page for that tab (see [desktop.md](desktop.md)). CEF is what it uses today; a lighter web view that works on
Wayland could replace it behind the same pane (open question 11).

The phone direction is board E of the
[phone mockups](https://claude.ai/artifact/52kT6NtuGqvcUe8kh39rcF) (picked 2026-11-10): one screen
per session with the workflow chip, a live view the user can grow or shrink, the chat with approvals
answered in place, and a bottom bar (inbox, workflows, pages, server). Dark, mono, 2–5 px rounding on
the phone only; the TUI stays square.

In that board, the live view needs the viewer reachable from the phone, and the workflow chip needs
the switch. The rounding, the stop icon, the bottom bar and the newest-first chat need neither.

The test for any client work: does the feeling survive with a terminal and a plain browser tab? If it
does, the shell is a better seat for the same thing, not the only place it works.

## A model per workflow (later)

- Named providers in config (today there is one `[provider]`), and `model:` on the workflow.
- Agent pages already carry `model: default`; `AgentDefinition` does not read it.
- Caution: switching to a small local model in the middle of a conversation hands it the whole
  history. Route spawned work (tickets, stages) by workflow first; let the interactive chat switch only
  when a workflow pins a model.
- A focused workflow is what makes a small model usable: one folder's instructions and a dozen tools.

## What becomes removable

Later cleanup. None of it is needed for the first workflow, and all of it can stay until the simple
path has proven itself.

| thing | why it can go |
|---|---|
| `desk`, `show_app`, `show_page`, `selection` tools | candidates once entering a workflow sets what the shell's pane shows; the desk becomes client view state (open question 12) |
| the seven agent pages | one agent page; roles become workflow instructions |
| `wayai__*` in agent pages' `tools:` | a workflow grants it |
| `pipeline.list` / `work.run` shims, the generic board in `templates/board` and its copies | already planned in project-hub phase E and the workflow-native apps plan |

### Against the earlier pages

- **Kept from [lean-orchestration.md](lean-orchestration.md):** the three layers, capabilities as
  sites, `tools:` on a site, no scheduler, the budget, agents that talk (phase 3) and scoping the
  verbs out (phase 5). This page adds `app:`, `mcp:`, `view:` and `model:` to the same frontmatter.
- **Kept from [project-hub.md](project-hub.md):** the project page, the work, the `project/` mount and
  its modes. A workflow operates on a project the same way a site does there.
- **Changed:** the desktop shell ([desktop.md](desktop.md), plan phase 8) stays, but stops being the
  only place apps live: what its pane shows is a URL any browser can open. "Apps in the chat"
  (`show_app`, plan phase 7) may give way to the workflow deciding the view.
- **Narrowed:** the workflow-native apps plan. A Svelte app per pipeline remains only where no real
  application shows the object.

## Phases

Each is shippable on its own.

### Phase 1: one workflow end to end (3d)

Status: done 2026-11-10 (uncommitted). `site://3d` with Blender runs on the owner's workspace: the
switch, the launch in wayai, tools granted and removed by workflow, and a scene change approved from
the TUI.

- `app:` and `mcp:` in a site's frontmatter; a workflow's `mcp:` grants those servers' tools while the
  session is in it.
- A user-side switch (`:workflow <name>`, the protocol command `site.set`; built, TUI only so far).
- Entering launches the `app:` in wayai when it isn't running.
- `site://3d` with Blender; the add-on set to start its server by itself.

**Done when:** `:workflow 3d` in the TUI opens Blender in the viewer, the agent has `blender__*` and
not before, "add a bevel to the cube" changes what the viewer shows, and `:workflow coding` takes the
Blender tools away again.

### Phase 2: the user's hands in the viewer (wayai)

Status: done 2026-11-10 (uncommitted, in the wayai repository). The viewer has a Drive mode: pointer,
buttons, wheel and keys go to the app; the agent's input tools are refused while a person drives and
its next tool result says a person drove. Frames every 33 ms while driving. `--no-drive` turns it off.
The owner tried it on Blender: little lag, shortcuts work.

- The viewer forwards pointer and keys in a "drive" mode; the agent's input pauses and it is told.

**Done when:** the user orbits and clicks in Blender from the browser tab, and the agent's next step
starts from the state the user left.

### Phase 3: the view on the phone

- Composer proxies wayai's `/view` behind a device token (built); the phone shows it view-only, resizable, on
  the session screen of board E; the workflows tab switches.

**wayai's token.** wayai asks for `Authorization: Bearer <token>` on its MCP endpoint and on every
viewer request, the WebSocket handshake included; the token is the content of a file
(`~/.local/share/wayai/token`). Composer is told where it is, never what it is:

```toml
[mcp.wayai]
url = "http://127.0.0.1:7878/mcp"
bearer_file = "~/.local/share/wayai/token"   # read at every connect; a 401 reconnects and reads again

[viewer]                                      # optional: without it, [mcp.wayai]'s address and file
bearer_file = "~/.local/share/wayai/token"   # read for every proxied request and handshake
```

`[viewer] bearer_file` defaults to `[mcp.wayai]`'s when the viewer is at that server's address
(scheme, host and port), and to nothing for a viewer anywhere else. The proxy adds the header to
what it sends upstream; it takes no `Authorization` from the client, returns none, and sends no
`Origin` (wayai refuses one that is not its own). A file that cannot be read fails clearly: the MCP
connect with the path in its error, the proxy with 502 and the path. The file is in the user's
home, which is empty inside every sandbox (`--tmpfs $HOME`) unless a bind names it. The proxy
refuses (404) a path below `/view` with `..`, an empty segment, a backslash or an encoded dot,
slash or backslash, and rewrites an upstream redirect only when it stays on the viewer.

**Driving through the proxy (desktop).** The proxy relays a client's WebSocket messages to wayai
only when the request was authenticated as the machine's own user: the workspace's local token, or
a browser session that token opened (`Credential::may_drive`,
`crates/server/src/api/credential.rs`, set by `api/auth.rs`; a session a paired device opened is
that device); for a paired device they are dropped
and the proxy asks wayai's page for `drive=0` (a display option that hides the Drive button; the
rule that enforces it is the relay, and wayai's own `--no-drive` stays the switch there). Client
messages are limited to 64 KiB.

**Done when:** the phone, on the tailnet, shows the running application of the open session and
switches its workflow.

### Phase 4: the comment shell and a first view type

- Threads anchored to named elements, reply and resolve, stored beside the work; a thread reaches the
  session with its anchor.
- One view type on it. Mockups is the natural first: it is what the artifact canvas was, and the site
  exists.

**Done when:** a comment on one mockup reaches the agent naming that mockup, the agent changes it,
answers in the thread and resolves it, and the view reloads.

Built (2026-11-11): the shell (see Threads above), the app template marks its page list as anchors, and
a mockups site with card and region anchors (delivered for the owner's workspace, not in the seed).
Checked with a scripted model; a real agent doing the change, the reply and the resolve is not yet run.

### Phase 5: a model per workflow

- Named providers; `model:` on a workflow, applied to spawned work first.

**Done when:** a stage of one workflow runs on a local model while the chat stays on the default.

### Phase 6: cleanup

- The table above, in the order use shows is safe.

**Done when:** one agent page remains, no workflow verb is in the base set, and the shell shows
nothing a browser tab can't.

## Non-goals

- No new client and no new host for apps.
- No remote desktop: the viewer is for looking and for light interaction.
- No generic stage board, not even as a fallback.
- No scheduler (as in lean-orchestration).
- No operating applications from the phone.

## Open questions

1. **One conversation that moves, or one per place with shared memory?** Assumed: one that moves. It
   is what "follows me" means, and it makes model switching harder.
2. **CAD and asset: a real application or a view type?** Probably the application (FreeCAD, Blender),
   with a view type only for the storyboard. Not decided.
3. **Which applications have a usable MCP server?** Blender's official one works. FreeCAD, ComfyUI and
   video editing are unchecked; wayai's pixel tools cover gaps, slowly.
4. **The phone's bottom bar: navigation or the workflow picker?** Drawn as navigation (inbox,
   workflows, pages, server). The owner's comment ("for the picker") may have meant the workflows
   themselves as tabs.
5. **App attachment.** `when:` / pins / scope decide which apps a project lists; the project page's
   `sites:` decides the same thing another way. One should remain.
6. **Does leaving a workflow close its application?** Keeping it open is cheaper to return to; closing
   frees wayai's one screen.
7. **Is a workflow a new `type:` or any site with `app:` / `mcp:` / `view:`?** The examples use
   `type: workflow`; pipelines are `type: pipeline` today.
8. **Where are threads stored?** Decided (phase 4): a file per thread, in `threads/` beside the work.
9. **Bundles per workspace.** Files are shared and every workspace sees every bundle; a workspace can
   turn one off with `enabled = false`. Whether that default should be the other way round.
10. **Mid-turn switches.** A switch applies from the next turn; what a running turn does when the user
    switches under it.
11. **A lighter web view for the desktop shell.** CEF works everywhere the same and costs a 1.5 GB
    runtime. WPE WebKit is lighter and native on Wayland but Linux only; wry was ruled out earlier for
    its Wayland issues. Not urgent: it is a swap behind one pane.
12. **Do `desk`, `show_app`, `show_page` and `selection` stay?** With the shell kept they still have a
    stage to act on. They go only if entering a workflow fully decides what the pane shows.
13. **An allowlist of other sites for desktop panes (later, owner 2026-11-11).** Panes load only the
    workspace server's origin since the security fixes. Self-hosted tools and local dev servers should
    be able to run in a pane too: a config list of exact origins (`[desktop] allow =
    ["http://127.0.0.1:5173", "https://grafana.home.lan"]`), set by the user, never by agents. Open
    points: those origins must get no composer cookie and no reach into the workspace's routes (their
    own CEF request context, not the workspace's); whether an agent may open them as desk tabs; and how
    a dev server's hot-reload WebSocket fits the navigation rules.
