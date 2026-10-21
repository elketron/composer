# composer — plan

Status: phase 1 done (all milestones below). Phase 2, the Flutter desktop/mobile app: [plan-app.md](plan-app.md). The original idea: [idea.md](idea.md).

## Concept

Agents access their memory, plans, agent definitions and scripts as a **browser**.
The workspace is the web; each top-level folder is a **site**.

- Every site has an `index.md` (its homepage).
- A site may have a `justfile` (its actions) and a `ui/` Svelte micro app (its human-facing UI).
- Everything is text first. Pages are markdown with optional YAML frontmatter.
- Any page can link to any other page, across sites.

## Workspace layout (single global workspace)

```
~/.composer/
  config.toml            provider url, model, api key, worker limit, port
  sites/                 the web — agent-visible
    home/index.md        root page (site://home): lists sites, always in the system prompt
    memory/
    plans/
    agents/
    orchestration/
  state/                 NOT a site — agent cannot read or write
    sessions/<id>/       transcript.jsonl, meta.toml
    approvals.json
    index.db             (later) frontmatter/link index
  templates/
    app/                 micro app scaffold, copied into <site>/ui/
    sdk/                 shared library: theme, components, server client
```

### Addressing

- `site://<site>/<path>` — e.g. `site://memory/projects/composer.md`
- `site://<site>` resolves to `<site>/index.md`
- Relative markdown links resolve within the current site.

### Frontmatter

Optional on any page; indexed by the server so agents can query it.

```yaml
---
title: Login mockup feedback
type: ticket            # page, memory, ticket, agent, ...
tags: [design, auth]
status: open
---
```

## Architecture

Cargo workspace:

| crate    | role |
|----------|------|
| `protocol` | WS/HTTP message types shared by server and TUI; generates TS types for the sdk |
| `core`   | workspace/site model, URL resolution, link + frontmatter parsing, index |
| `server` | harness: agent loop, provider, tools, sessions, WS/HTTP API, static app serving, file watcher, workers |
| `tui`    | Ratatui client of the server |

Plus `templates/sdk` (TS) and `templates/app` (Svelte scaffold).

### Server / harness

- **Provider:** OpenAI-compatible `/v1/chat/completions` on your own model server (`provider.base_url` in config.toml), tool calling + streaming + images. Behind a small `Provider` trait.
  - Model: `qwen3.8 flash next` — the only model on the server, so the id is taken from `GET /v1/models` at startup (overridable in config).
  - Context: 256K. Compaction threshold configurable, default well below max (e.g. 160K) since long contexts are slow locally.
  - **Parallelism 2:** ~40–50 t/s with one stream, ~20–30 t/s each with two. The parent server owns a **global slot limiter** (default 2 slots) shared by all sessions, workers and compaction turns.
- **Sessions:** free-form, many per project. A session may have an **active project** (a real code directory outside the workspace), set with `:project <name>` in the TUI, `composer --project <name>`, or by the agent via `set_project(path)`. A name (or relative path) is a folder in the projects directory (`[projects] dir` in config.toml, default `~/Projects`, also where `new_project` creates projects); `~/...` and absolute paths are taken as they are, and the CLI's `--project .` is the current directory. `:project none` (or the × by the project in the app's chat header) unsets it; `new_project` makes the new project active. A new session (`:new`) starts in the current session's project. Persisted in `state/sessions/`.
- **File watcher:** keeps the index fresh, pushes page changes to connected clients.
- **Static serving:** built micro apps at `http://localhost:PORT/<site>/`, opened with `xdg-open`.

### Agent tools

| tool | purpose |
|------|---------|
| `open(url)` | returns raw markdown of the page |
| `search(query)` | full text + frontmatter filters, e.g. `type:ticket status:open site:orchestration` |
| `list_sites` | sites with their index summaries |
| `write(url, content)` / `edit(url, ...)` | create/modify pages (incl. memory, during a session) |
| `actions(site)` | list justfile recipes |
| `run(site, recipe, args)` | run a recipe (approval rules below) |
| `spawn_worker(ticket, agent?)` | orchestrator only |
| `new_site(name, title, about)` | create `<name>/index.md` (stub layout, page format, rules) and list it on Home |
| `new_app(site)` | copy app scaffold into `<site>/ui/` |
| `new_project(name, about, template?)` | create `<projects.dir>/<name>` (README, or a copy of `<template site>/templates/project/`), its memory page and registry entry |
| `set_project(path)` | set the session's active project (shown in the TUI) |
| `finish(status, summary)` | worker only: ends the worker, sets ticket status, notifies the orchestrator |

The agent reads raw markdown; the server parses links on the side (back-links, TUI navigation).

### Reminders

A page with `type: reminder` goes off when it is due: `due: 2026-10-19` (+ `time: "20:00"`, or
`due: 2026-10-19 20:00`; without a time, `[reminders] default_time`, 09:00), local time, `status: open`.
The server checks its index every 30 s, so edits count right away and anything missed while it was off
or asleep fires on the first check (marked late). Each one fires once: the server writes `notified:` into
the page. Delivery: a `reminder.due` event to every client (system notification in the app, the status
bar in the TUI), and a message (source `reminder`, starts a turn) into the reminder's session. A reminder
belongs to the session that wrote it: `write` / `edit` fill in `session:` when a reminder page has none.
Without a session (or when it's gone), a new session is started for it, for `agent:` (default `default`).

### Recipe approval

- First use of a recipe requires approval in the TUI (shows the recipe body; approve / deny / view in nvim).
- Approval is keyed by `site + recipe + hash`. The hash covers the recipe, its dependencies (transitively) and the justfile's variables and settings, from `just --dump --dump-format json`. Any change (the agent can edit justfiles) needs approval again; adding unrelated recipes does not.
- The hash is checked again right before running, so a justfile edited while the prompt was open is not run.
- Recipes run in their site folder with `COMPOSER_SITE`, `COMPOSER_SESSION`, `COMPOSER_PROJECT`; stdout and stderr share one pipe (true order); killed after `agent.recipe_timeout_secs` or on cancel.
- The request goes to every client attached to the session (and is re-sent on attach); `approval.resolved` closes it everywhere.
- Stored in `state/approvals.json`, outside agent reach.

### Agent definitions

Pages in `site://agents/`. Frontmatter configures, body is the system prompt.

```markdown
---
type: agent
name: orchestrator
model: <model-id>
tools: [open, search, write, edit, spawn_worker]
sites: [orchestration, plans, memory]
---
You coordinate work by writing tickets in site://orchestration ...
```

### System prompt assembly

1. Agent definition body
2. `site://home` (root page listing all sites)
3. Active project info (path, git branch) if set
4. Relevant memory (e.g. `site://memory/projects/<project>.md`)

Each site's `index.md` explains its purpose and conventions; the agent opens it when entering the site.

### Memory and compaction

- `site://memory` is a normal site; the agent writes to it any time during a session.
- Suggested structure: `memory/index.md`, `memory/projects/<name>.md`, `memory/topics/...`, with `type: memory` frontmatter.
- The system prompt lists every memory page (title, URL, kind), so the agent knows what it knows without searching.
- **Compaction** when the context reaches `agent.compaction_threshold` (checked before each model call), or on demand with `:compact`:
  1. **Memory pass:** the model gets the old conversation, an instruction to save what's worth keeping, and only `open`/`search`/`write`/`edit`. Its tool calls show in the chat; nothing of it enters the history.
  2. **Summary:** a tool-less call summarizes goals, decisions, state, open threads and URLs.
  3. **Replace:** everything before the turn in progress becomes `[summary] + ack`; the turn in progress is kept. The old `history.jsonl` is archived, never deleted. A `session.compacted` event shows as a `▸ compacted` row (`T` shows the summary).

### Sub-agents / orchestration

- Orchestrator writes a ticket: `site://orchestration/tickets/0001.md`.
- `spawn_worker(ticket)` creates the worker session (ticket + parent set, visible at once), sets the ticket `claimed` and starts `composer worker --session … --ticket … --agent … --parent http://…` as a child process (`agent.max_workers`, default 1). Its log: `state/workers/<id>.log`.
- Worker agents: an agent page with `role: worker` (or, from before `role:`, no role and the `finish` tool) is one `spawn_worker` can start, e.g. a `coder`, `reviewer` or `tester`. Which one: `spawn_worker`'s `agent`, else the ticket's `agent:`, else `worker`; anything else is refused with the list. Agents that may call `spawn_worker` get the worker agents and their `description:` in their Environment.
- The worker process is a server instance without an API: model slots are lent by the parent (`slot.acquire`/`slot.release`), recipe approvals are asked through the parent (`approval.ask`; approval prompts reach every client), and its session events go to the parent (`worker.event`), which re-sequences them into the worker session's transcript. The worker keeps its model history in `state/workers/sessions/<id>/`.
- **Harnesses** (`crates/server/src/harness/`): the registry claims the ticket and creates the session, then a harness starts the run: `composer` (own agent loop, the default) or the one a ticket's `harness:` names. Runs are child processes in their own process group. Cancelling a worker session (`turn.cancel`) stops the whole group and fails the ticket as "cancelled by the user"; a message to a running worker goes to its harness (own workers refuse it for now).
- `finish(done|failed, summary)` (worker only) ends it: the server sets the ticket status and sends the orchestrator a message (source: the worker), which starts its next turn. A worker that ends its turn without `finish` is reminded once; a process that exits without it fails the ticket.
- **Plans vs tickets:** plans in `site://plans` hold longer-lived intent; tickets are units of work that link back to a plan.
- Ticket frontmatter drives lifecycle: `open → claimed → done | failed`. Worker writes its result into the ticket.
- On completion the orchestrator gets a push message.
- TUI lists running workers and can attach to a worker session.
- Later: pipelines as ticket templates / DAG pages in the same site.

## TUI (Ratatui)

Keyboard first, vim-style modal. A client of the server — multiple TUIs/apps can attach to one session.

### Layout: two panes, focused

```
┌ chat · orchestrator · composer ─────────┐┌ site://memory/projects/composer.md ──┐
│ you: plan the api for the tui           ││ ---                                  │
│                                         ││ type: memory  tags: [composer]       │
│ ▸ thinking (412 tok)                    ││ ---                                  │
│ ▸ open site://memory/projects/...       ││ # composer                           │
│ ▸ search type:ticket status:open        ││ Rust server + Ratatui TUI ...        │
│                                         ││ See [plan](site://plans/composer)    │
│ agent: I looked at the plan and...      ││                                      │
│ ┌─────────────────────────────────────┐ ││─ backlinks ──────────────────────────│
│ │ > _                                 │ ││ site://plans/composer                │
│ └─────────────────────────────────────┘ ││                                      │
└─────────────────────────────────────────┘└──────────────────────────────────────┘
 NORMAL │ ~/Projects/composer │ ctx 42K/160K │ slots 1/2 │ following agent
```

- **Chat** (left): tool calls and thinking collapsed into expandable `▸` rows.
- **Page viewer** (right): rendered markdown for the human (agent gets raw), frontmatter header, backlinks.
- **Status bar:** mode, active project, context vs compaction threshold, LLM slots, follow state.
- Either pane can be maximised; narrow terminals show one pane at a time.

### Floating pickers

Everything else lives in floating windows (fuzzy finder, telescope-style) instead of permanent panes:

| key | picker |
|-----|--------|
| `Space s` | sites (enter → site index) |
| `Space p` | pages (fuzzy over all URLs + titles) |
| `Space /` | search (full text + frontmatter filters) |
| `Space w` | sessions + workers |
| `Space o` | this orchestration: the orchestrator and its workers, with what each is doing (also `:workers`) |
| `Space t` | tickets |

### Modes

| mode | for |
|------|-----|
| Normal | navigation, actions |
| Insert | composing a message |
| Command | `:` commands |
| Hint | `f` labels every visible link; type the label to follow (Vimium-style) |

### Keybindings (defaults, configurable in `config.toml`)

| key | action |
|-----|--------|
| `Ctrl-w h/l`, `Tab` | switch pane |
| `Ctrl-w o` | maximise / restore pane |
| `j/k`, `Ctrl-d/u`, `gg/G` | scroll |
| `f` | link hints (page viewer and chat) |
| `H` / `L` | back / forward in page viewer |
| `F` | toggle follow agent (**on by default**) |
| `i`, `Esc` | compose / back to Normal |
| `Enter` | Insert: newline · Normal: send the draft (empty draft: start typing) |
| `Ctrl-c` | cancel current turn |
| `t` | toggle thinking |
| `t` / `T` | expand all thinking / tool results (per-row `Space` later) |
| `e` | edit current page in nvim |
| `gd` | changes view of the active project (toggle); `j`/`k`, `{`/`}` files, `Enter` → editor at that line |
| `a` | open current site's micro app |
| `Ctrl-o` | back to the previous chat (e.g. from a worker to the main chat) |
| `]w` / `[w` | next / previous session of this orchestration |
| `?` | help overlay (also `:help`): modes, every binding (generated from the keymap), pane keys, commands |

Commands: `:new [agent]`, `:attach <id>`, `:project <name>`, `:open <url>`, `:app [site]`, `:search <q>`, `:pair`, `:q`.

### Architecture: Flux state + components

```
 crossterm keys ─┐                         ┌──────────── Effects ────────────┐
 server events ──┼─► Action ─► Dispatcher ─► Store::reduce ─► AppState       │
 tick / resize ──┤      ▲                                       │            ▼
 effect results ─┘      │                                       │      EffectRunner
                        │                                       ▼   (ws send, fetch page,
                   Components ◄──────────── render(&AppState) ──┘    nvim, xdg-open)
                   (key → Action)
```

- **One direction.** Everything that changes state is an `Action`. Inputs (keys, server events, ticks, finished effects) become actions; the dispatcher feeds them to the stores; components render from the resulting state.
- **Stores own all shared state.** `AppState` is composed of stores, each with `reduce(&mut self, &Action) -> Vec<Effect>`:

  | store | holds |
  |-------|-------|
  | `SessionStore` | sessions, chat items per session, streaming buffers, turn state, jumplist |
  | `PageStore` | current page, back/forward history, follow-agent flag, page cache |
  | `UiStore` | mode, focused pane, maximised pane, open overlay |
  | `PickerStore` | picker kind, query, results, selection |
  | `ApprovalStore` | pending approval requests |
  | `ConnectionStore` | server URL, connection state, slots |
  | `ProjectStore` | active project, git status + diff for the changes view |

- **Reducers are pure: no IO.** Side effects are returned as `Effect` values (`SendCommand`, `FetchPage`, `OpenEditor`, `OpenBrowser`, `Quit`) and run by the `EffectRunner`. Its results come back as actions. Reducers stay unit-testable without a terminal or server.
- **Components** implement one trait:
  ```rust
  trait Component {
      fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action>;
      fn render(&self, frame: &mut Frame, area: Rect, state: &AppState);
  }
  ```
  A component keeps only **local view state** (scroll offset, textarea cursor). Anything another part of the UI needs goes in a store.
- Components: `Root` (layout), `ChatPane`, `Composer`, `PageViewer`, `ChangesView`, `StatusBar`, `Picker`, `ApprovalPopup`, `HintOverlay`, `Help`.
- **Keys:** the `Keymap` (from config) maps `(Mode, chord) → Action` first; unbound keys go to the focused component. In Insert mode the `Composer` gets every key except `Esc`.
- **Hint mode:** during render, components register visible links with their screen positions in a per-frame link registry; `HintOverlay` labels them.
- **Event loop** (tokio): `select!` over the crossterm `EventStream`, the WS client, a render tick and the action channel. Render only when state changed.
- **Shared protocol:** WS/HTTP message types live in a `protocol` crate used by server and TUI, with TS types generated for the sdk, so the three can't drift apart.

### Module layout

```
crates/tui/src/
  main.rs
  app.rs                  App: event loop
  action.rs               Action + sub-enums (no impls)
  effect.rs               Effect enum (no impls)
  dispatcher.rs
  effect_runner.rs
  keymap.rs
  theme.rs                tokens from theme.css as Color::Rgb
  editor.rs               suspend TUI, run nvim, restore
  client.rs               WS + HTTP client
  store/
    app_state.rs
    session_store.rs  page_store.rs  ui_store.rs  picker_store.rs
    approval_store.rs  connection_store.rs  project_store.rs
    types.rs              plain data: ChatItem, Mode, Pane, ... (no impls)
  components/
    component.rs          the trait
    root.rs  chat_pane.rs  composer.rs  page_viewer.rs  changes_view.rs
    status_bar.rs  picker.rs  approval_popup.rs  hint_overlay.rs  help.rs
  markdown/
    renderer.rs           markdown → ratatui Text, registers links
```

### Worker sessions

Attaching to a worker **replaces** the main chat; the title shows a breadcrumb (`orchestrator › worker #0001`). `Ctrl-o` jumps back (a jumplist, like vim).

Switching within an orchestration: the title shows `1/3 workers busy`; `Space o` lists the orchestrator and its workers
with their state (running, waiting, approval, idle, done, failed) and starts on the likely switch (from the orchestrator,
the first busy worker; from a worker, the orchestrator), so `Space o Enter` flips between them. `]w` / `[w` step through.
When the worker you are watching finishes, the chat goes back to the orchestrator, which gets the worker's result next
(`Ctrl-o` returns to the worker); not while you are in Insert mode. The app does the same with a bar of the
orchestration's sessions under the chat header, and stays put while the composer holds a draft.

### Other behaviour

- **Open in nvim:** TUI suspends (leaves alternate screen), runs `nvim <file>[+line]`, restores on exit. Remote: temp-file round trip.
- **Approval popup:** shows recipe body; `y` approve, `n` deny, `e` view in nvim.
- **Workflow views** (built-in, registered by name, shown in the page viewer pane): changes view, later more.
- Opening a micro app passes `?session=<id>&token=<t>`.

## Micro apps (Svelte)

- Belong to a site, live in `<site>/ui/`, each a standalone Svelte app.
- Created by `new_app <site>`: copies `~/.composer/templates/app` into `<site>/ui/` and adds a `ui-build` recipe (`bun install && bun run build`) to the site's justfile, so building goes through the approval flow. The scaffold imports theme, components (`AppShell`, `Button`, `Kbd`, `Badge`) and the client from `$sdk` (a Vite alias to `~/.composer/templates/sdk`), so fixes land in every app on its next build.
- Served by the server at `/<site>/` from `<site>/ui/dist`. Opening with `?token=` sets an HttpOnly, SameSite=Strict cookie that authenticates the app's assets and API calls; the client removes the token from the address bar.
- sdk client: `page`, `putPage` (hash check), `file`, `putFile` (`/api/files/...`, any bytes), `search`, `send(text, {pages, images})`, `onChange(patterns, cb)`, `isConnected()`, `site`, `session`.
- Approving `ui-build` approves the recipe, not the app's code: a build runs `vite.config` and packages the agent may have edited (as with any approved recipe that runs project code).
- **Shared workspace:** app state is stored as files in the site, never hidden in the app. Example:
  ```
  design/mockups/login/
    mockup.json        boxes, positions
    annotations.md     agent-readable comments
    snapshot.png       sent to the model as an image
  ```
- **Active push:** `sdk.send({ text, pages, images })` posts a message into the session. If the agent is mid-turn it is queued for the next turn.
- Agent edits flow back to the app live over WebSocket.

## Milestones

1. ✅ **core + protocol** — Cargo workspace; site model, URL resolution, link + frontmatter parsing, in-memory index; shared message types.
2. ✅ **server** — provider, agent loop, page tools (`open`, `search`, `list_sites`, `write`, `edit`, `set_project`), sessions, WS + HTTP API, token auth, file watcher, `composer init / serve / ask`.
3. ✅ **tui** — chat + page viewer against the server, pickers, hints, `:` commands, nvim round trip, headless script mode for testing.
4. ✅ **actions** — `actions` / `run` tools, approvals per recipe version (recipe + dependencies + variables/settings hashed), approval popup, seed recipes pre-approved by `init`.
5. ✅ **memory** — compaction (memory pass → summary → history replaced, old history archived), `:compact`, memory page list and agent identity in the prompt.
6. ✅ **micro apps** — `templates/sdk` + `templates/app` (Svelte 5 + Vite, bun), `new_app` + `ui-build` action, apps served at `/<site>/` with cookie auth, files API, `a` / `:app` in the TUI, messages from apps. The first real app (ticket board) comes with milestone 7.
7. ✅ **orchestration** — `spawn_worker` starts `composer worker` processes linked to the parent (slots, approvals and events over WS), `finish`, ticket status by the server, worker sessions nested in the TUI, ticket board app (seed).
8. ✅ **workflow views** — changes view (`gd`): git changes of the active project against HEAD (untracked files included), refreshed every 2s and after the agent's tool calls; `j`/`k`, `{`/`}` between files, `Enter` opens `$EDITOR +line file`.
9. ✅ **remote** — device pairing (`pair`, QR, `/pair`, `login`), per-device tokens with revoke, `tailscale serve` support (forwarded origin, Secure cookies), remote TUI over `wss://`, `site://chat` phone app.

Auth (tokens) and relative URLs are built in from milestone 2, so remote access is not a retrofit.

## Server API (draft)

Server listens on `127.0.0.1:PORT` (axum). One WebSocket for live traffic, HTTP for pages, app pushes and static apps.
All messages are JSON: `{ "type": "...", ...fields }`. Commands carry an `id`; the server answers with `{ "type": "ok" | "error", "id": ... }`.

### WebSocket `/ws` — client → server

| type | fields | notes |
|------|--------|-------|
| `session.create` | `agent`, `project?` | returns session id |
| `session.list` | | |
| `session.attach` | `session`, `since?` | replays events after seq `since`, then streams live |
| `session.detach` | `session` | |
| `message.send` | `session`, `text`, `pages?[]`, `images?[]` | images are site URLs, not blobs |
| `turn.cancel` | `session` | |
| `project.set` | `session`, `path` | |
| `approval.respond` | `request`, `decision: approve\|deny` | |
| `subscribe` | `urls[]` (globs, e.g. `site://design/**`) | page change notifications |

### WebSocket `/ws` — server → client (events)

Every session event has a monotonically increasing `seq` and is appended to `transcript.jsonl`, so attach/replay is just reading the log.

| type | fields |
|------|--------|
| `turn.started` / `turn.done` | `session`, `seq` |
| `turn.queued` | `session`, `source` (tui, app, worker) |
| `thinking.delta` | `session`, `seq`, `text` (streaming reasoning tokens from the model) |
| `message.delta` | `session`, `seq`, `text` (streaming) |
| `message.done` | `session`, `seq`, `message` |
| `tool.call` / `tool.result` | `session`, `seq`, `call_id`, `name`, `args` / `result` |
| `agent.navigated` | `session`, `url` (page viewer follows the agent) |
| `approval.request` | `request`, `session`, `site`, `recipe`, `body`, `hash` |
| `page.changed` | `url`, `kind: created\|modified\|deleted`, `by: agent\|app\|external` |
| `worker.status` | `worker`, `ticket`, `session`, `state` |
| `slots` | `used`, `total`, `waiting` |

### HTTP

| method + path | purpose |
|---------------|---------|
| `GET /api/sites` | sites + index summaries |
| `GET /api/pages/{site}/{path}` | raw markdown + parsed frontmatter |
| `PUT /api/pages/{site}/{path}` | write a page (apps) |
| `GET /api/files/{site}/{path}` / `PUT` | non-markdown files (json, png) |
| `GET /api/search?q=` | full text + frontmatter filters |
| `POST /api/sessions/{id}/messages` | app push (`text`, `pages`, `images`) |
| `GET /{site}/` | built micro app |

| `GET /api/sessions/{id}/project/diff` | git status + diff of the active project (changes view on remote clients) |

### Thinking tokens

- The provider streams reasoning separately from content; the server forwards it as `thinking.delta` and stores it in the transcript.
- TUI shows thinking collapsed by default, toggle with a key.
- Thinking from previous turns is **not** sent back to the model (keeps context small; Qwen guidance), only within the current turn's tool loop.

## Remote access

Use the server from another laptop or the phone, over Tailscale. Tested end to end over `https://<machine>.<tailnet>.ts.net`.

- **Exposure:** the server binds `127.0.0.1`; `tailscale serve --bg http://127.0.0.1:7420` makes it HTTPS on the tailnet only. Needs root once: `sudo tailscale set --operator=$USER`. Off again: `tailscale serve --https=443 off`.
- **Pairing:** `composer pair --name phone` makes a one-time code (10 minutes, stored hashed in `state/pairing.json`) and prints a QR code for `https://…/pair?code=…` plus `composer login https://… CODE` for computers. It warns when Tailscale isn't serving yet.
  - Phone: the `/pair` page asks to confirm (a link preview can't spend the code), then sets an HttpOnly, `Secure`, SameSite=Strict cookie for a year and opens `/chat/`.
  - Computer: `composer login <server> <code>` stores the server and token in `<home>/remote.json`; `composer` (TUI) and `composer ask` then use it (`composer logout` undoes it).
- **Devices:** each has its own token, stored only as a hash in `state/devices.json`; `composer devices` lists, `composer devices revoke <name>` cuts one off immediately. The local token (`state/token`) keeps working on the server machine.
- **Auth:** every route needs the local token or a device token (header, `?token=`, or cookie), except `/pair` and `/api/pair` (the code is the credential). The origin check accepts `X-Forwarded-Host` from `tailscale serve`; cookies get `Secure` behind HTTPS.
- **Remote TUI:** connects to the public URL over `wss://`. Page editing is the temp-file round trip through the API (the one editing mechanism); apps open in the client's browser with the public URL; the changes view shows the diff, but opening a project file needs the TUI on the server machine.
- **Phone (`site://chat`):** sessions (workers nested), the chat (tool and thinking rows collapsed, light markdown), sending messages, answering recipe approvals inline, starting a session. Built with `just ui-build` in the chat site (pre-approved by `init`).

## Visual design

Mockups: [tui.html](mockups/tui.html), [apps.html](mockups/apps.html). Tokens: [theme.css](mockups/theme.css) → becomes `templates/sdk/theme.css`; the TUI uses the same values as RGB.

- Dark and minimal: four surfaces (`bg`, `surface`, `raised`, `overlay`), three text levels (`text`, `muted`, `faint`).
- **One accent** (soft blue) for links, focus, selection and Normal mode. State colours (`ok`, `warn`, `err`) only where state matters; `info` (violet) marks the agent, tool rows and thinking.
- Approvals always use `warn`, never the accent.
- Fonts: JetBrains Mono (TUI, code, URLs, ids), Inter (app UI).
- **TUI: no rounded corners** (plain borders everywhere).
- **Chat messages are background bands:** yours use `--you` (bg tinted toward the accent), the agent's use `--surface`, system messages (e.g. a worker's ticket) are bordered with no fill.
- **Apps: corner radius 2px–5px.** `--radius-sm: 2px` for badges, chips, kbd, markers and status dots; `--radius: 5px` for buttons, inputs, cards and panels. No pills, no circles.
- **Shared app shell:** top bar (site / page breadcrumb, session chip, slots, primary action) and a footer with keyboard hints. Every app is keyboard-driven and shows its keys.

## Seed sites

`seed/sites/` in this repo is copied to `~/.composer/sites/` by `composer init` (existing files are never overwritten). These pages are the agent's manual; keep them short — `site://home` is in every system prompt.

| page | content |
|------|---------|
| `home/index.md` | root (`site://home`): what the web is, addressing, site table, how to work |
| `memory/index.md` | layout (`user.md`, `projects/`, `topics/`, `feedback/`), page format, rules |
| `plans/index.md` | plan format (goal, steps → tickets, decisions), status lifecycle |
| `orchestration/index.md` | ticket format, lifecycle, `just new-ticket` |
| `orchestration/justfile` + `templates/ticket.md` | ticket creation with sequential ids |
| `agents/index.md` | agent page format, prompt assembly |
| `agents/{default,orchestrator,worker}.md` | the three built-in agents |

Seed recipes are pre-approved by `composer init`.

## Phase 3: harnesses and attachments

Tickets can also be worked by the official Claude Code CLI (personal use on this machine, under its own
login), and messages and tickets can carry files.

### Harness interface

- `crates/server/src/harness/`: a small trait: **start** (ticket, working dir, MCP config) → handle,
  **send** a message (resume), **events** (a stream of our `ServerMessage`s for the worker session),
  **interrupt**. Each harness parses its own output into those events, so parsing stays contained.
- **Own harness:** what `run_worker` / `worker_registry` do today, moved behind the trait first with no
  change in behaviour.
- **Claude Code harness:** a `claude -p` child process (below). Others (Codex, Aider, …) are further
  implementations: a process that speaks MCP plus an output parser.
- The ticket records `harness:`, `session:` (the harness's own id), `model:` and `exit:`.

### `composer mcp`

- `composer mcp --session <worker> --parent <url>`: a stdio MCP server (`initialize`, `tools/list`,
  `tools/call`, `ping`), the public contract for any harness. It connects to the running server with the
  local token and offers that **running** worker's tools: the ones its agent's `tools:` lists, as a native
  worker gets them, under the same site access rules.
- Protocol: `tool.list` / `tool.call` (replies `tools` / `tool_output`), refused for any session that isn't
  a running worker. The server runs the native tool in the worker session: one implementation.
- The ticket is a page (`site://orchestration/tickets/<id>`), read and written with `open` / `edit`;
  `finish(done|failed, summary)` works as for own workers: the server sets the ticket status and messages
  the orchestrator. After `finish` the run has 60s to exit before it is stopped; a run that exits
  without calling it fails the ticket.
- Later: a permission tool for `--permission-prompt-tool`, so Claude Code's prompts become composer
  approvals (phone included).

### Worktrees

- A ticket whose project is a git checkout (with a commit) runs in its own worktree:
  `state/worktrees/<ticket>` on branch `ticket/<id>`, made from the project's HEAD (uncommitted changes in
  the project are not in it; `spawn_worker` says so). A project that is a subfolder of a repository maps
  to the same subfolder of the worktree. Plain folders: the worker works in the folder itself.
- The worker session's project is the worktree; the ticket gets `branch:`, and the finish message tells
  the orchestrator which branch holds the changes.
- A retried ticket reuses its worktree (or checks out its kept branch again); a folder in the way that
  isn't a worktree of that repository fails the spawn.
- Merging back and cleanup (`git worktree remove`, deleting the branch): by the user for now; later a
  tool or command for it.

### Claude Code harness

Built (milestone 13):

- **Choosing it:** a ticket's `harness: claude-code`, or `spawn_worker(ticket, harness: "claude-code")`.
- **Allowed per project**, default off, in the server's `config.toml` (not in the repo, so a cloned repo
  can't switch it on for itself); the closest configured folder containing the project counts:
  ```toml
  [projects."~/Projects/composer"]
  external_harnesses = ["claude-code"]
  allowed_tools = ["Bash(cargo test:*)"]   # on top of [claude_code] allowed_tools
  ```
  Checked against the project before it becomes a worktree; refused with the snippet to add otherwise.
- **`[claude_code]`:** `command` (default `claude`), `model` (empty: the CLI's default), `max_turns` (60),
  `timeout_minutes` (60), `allowed_tools` (read/edit/write/search, the composer MCP tools, and
  `git status|diff|log|add|commit`).
- **Login:** only an already logged-in CLI is used: `claude auth status --json` (with the same cleared
  environment) must say `loggedIn`. Never read or reuse its token. Not logged in or not installed: the
  spawn fails with the error and the ticket stays as it was; no retry.
- **Environment:** cleared; only `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `TERM`, `LANG`, `LC_*`,
  `TMPDIR`, `XDG_*` and `CLAUDE_CONFIG_DIR` pass. A set `ANTHROPIC_API_KEY` would win over the login and
  bill the API; this also keeps out `ANTHROPIC_*`, `CLAUDE_CODE_USE_*`, the provider key and
  `COMPOSER_*`. Not `--bare` (it ignores the login).
- **Command:** `claude -p <prompt> --output-format stream-json --verbose --permission-mode acceptEdits`
  `--permission-prompts none --max-turns N --mcp-config <composer mcp for this worker> --strict-mcp-config`
  `--append-system-prompt <role> --session-id <uuid> [--model M] --plugin-dir <templates/harness/claude-code>`
  `--allowedTools <list>`, in the worktree (or project folder). Anything outside the list is denied, never
  asked. We choose the session id, so the ticket has it (`harness_session:`) before the first event.
- **Ticket fields:** `harness`, `harness_session`, `model` (from the init event), `exit`.
- **Process:** its own process group; cancel and the timeout stop the group (TERM, KILL after 5s).
- **Output:** every stream-json line goes to `state/workers/<id>.log`. Mapped to worker session events:
  text, thinking, tool calls (composer's tools under their own names) and results (cut at 4000 chars),
  turn start/end and state, so it shows in the orchestration picker, worker cards and the chat.
- **Ends without `finish`:** failed, with what it last said. **Usage limit** (a rejected `rate_limit_event`,
  or a 429 result): the worker is `paused` (new `WorkerState::Paused`), the ticket `status: paused`, and
  Claude Code launches are refused until the reset time; own-harness tickets keep running.
- **Feedback:** a message to a stopped Claude Code worker session resumes the run (`--resume
  <harness_session>`) in the same session and worktree, after the same checks.
- **Skills:** a plugin in `templates/harness/claude-code/` (installed by `init`, merged by `update`, loaded
  with `--plugin-dir`, never written into project repos): `composer-sites`, `composer-tickets`,
  `composer-memory`, `composer-apps`.

Built (13b):

- **Models per project:** `models = ["claude-code:sonnet", "composer"]` in `[projects."…"]`: `"<harness>"`
  allows any of its models, `"<harness>:<model>"` one. The configured model is used when allowed, else the
  first allowed one (`--model`); composer's own harness runs the provider's model or is refused. Empty: any.
- **Off switch:** `composer harness <project> [on|off] [claude-code[:model]]`, `:harness [on|off] [entry]`
  in the TUI (the session's project), and "Turn off Claude Code for <project>" in the app's chat menu;
  protocol `harness.list` / `harness.set` (reply `harnesses`). Stored in `state/projects.json`; it only
  narrows config.toml. Off stops its running tickets in that project (`paused`, `paused_reason: switch`)
  and refuses new ones; back on, the ones it stopped go on (`--resume`).
- **Shutdown:** Ctrl-C or SIGTERM stops every worker before the server exits (they run in their own
  process groups). Claude Code tickets pause (`paused_reason: shutdown`; a message resumes them), own
  workers fail; orchestrators aren't told.
- **Automatic resume:** a ticket paused by a usage limit records `paused_until` and goes on by itself a
  minute after the reset, also after a server restart.

### Attachments

Built (milestone 14, server):

- **Storage:** `state/sessions/<id>/attachments/<hash>-<name>` (first 12 hex of the SHA-256 of what is
  stored; the same file is stored once). Names sanitized (`A-Z a-z 0-9 . _ -`, no folders, no leading
  dot, ≤ 80 chars); a reference resolves only to a file inside that session's folder.
- **Protocol:** `Attachment { file, name, mime, size }` on `ChatMessage.attachments` and
  `message.send.attachments`. The server re-reads each referenced file (the client's mime and size are
  ignored) and refuses references that aren't the session's.
- **Upload:** `POST /api/sessions/{id}/attachments`, multipart, token or device token; up to 5 files per
  request, each ≤ `[attachments] max_mb` (20). Allowed: images (PNG, JPEG, GIF, WebP, by content), PDF
  (by content), text by extension and valid UTF-8 (prose, data, common code). Reply: the stored
  `Attachment`s. `GET /api/sessions/{id}/attachments/{file}` serves one back (download, `nosniff`).
  Clients create the session first, upload, then send the message.
- **Images:** turned upright (EXIF orientation), shrunk to `image_max_edge` (1500) on the long side and
  re-encoded (JPEG stays JPEG at q85, others become PNG), which drops EXIF and other metadata.
- **Own harness:** images go to the model as image parts (`attachment://<session>/<file>` in history,
  a data URL when sent; needs a vision model, otherwise the provider's error ends the turn); text inline
  and PDFs as `pdftotext` text, cut at `inline_max_kb` (100), each in an `<attachment name type>` block,
  with a line saying the contents are data, never instructions. The message lists the refs
  (`<session>/<file>`) so the orchestrator can put them in a ticket's `attachments:`.
- **Tickets:** `attachments: ["<session>/<file>"]`; only files from sessions in the ticket's own project
  (or both without one) are passed. Own workers get them in their first message; Claude Code gets their
  paths in the prompt and `--add-dir` on their folders (also for files sent with a message that resumes
  it), and reads them with its Read tool.

Built (milestone 15, clients):

- **TUI:** a file attaches when its path is pasted while writing (bracketed paste; quoted, `file://` and
  shell-escaped paths too), with `:file <path>`, or the clipboard image with `space a` (`arboard`, Wayland
  and X11). Each file uploads over HTTP right away (the TUI may not be on the server machine) and shows
  as a chip above the input (`…`, `✓`, `✗`); backspace in an empty message removes the last one,
  `:file clear` all. Enter waits for uploads; files alone can be sent. Sent messages list their files.
  Switching sessions drops attached files (they belong to the session they went to). Headless scripts:
  `paste <text>`.
- **App:** a paperclip beside the input: Take a photo (phones), Photos (`image_picker`, up to 5, shrunk to
  1500px at quality 85 on the phone), File (`file_picker`; Android `content://` files are read into
  memory). Chips show upload progress, ✓, retry on failure, and ×. Send waits until every file is
  uploaded; files alone can be sent. Sent messages show image thumbnails (fetched with the token) and
  file chips.

### Milestones

10. ✅ **harness trait** — own worker behind it; cancel stops the worker's process group; a ticket's `harness:` picks one.
11. ✅ **`composer mcp`** — a running worker's tools over MCP, run by the server (`tool.list` / `tool.call`).
12. ✅ **worktrees** — per-ticket worktrees on `ticket/<id>` branches for both harnesses.
13. ✅ **Claude Code harness** — project switch, auth check, cleared environment, process group, log, event
    mapping, failure handling (missing, logged out, usage limit, no `finish`), resume on feedback, skills plugin.
    13b ✅: per-project models, runtime off switch, shutdown, automatic resume after a usage limit.
14. ✅ **attachments (server)** — protocol, upload route, storage, resizing, own harness, Claude Code paths, ticket files.
15. ✅ **attachments (clients)** — app pickers and chips, TUI paths and clipboard.

Later: more harnesses. Following and approvals for Claude Code moved to phase 4.

## Phase 4: finishing tickets, approvals, following Claude Code

What happens after a worker is done (review, merge, clean up), Claude Code asking instead of being
denied, and watching a Claude Code run as closely as an own worker.

### Merging ticket branches

- **Review:** a ticket's changes are its branch against the project's checked-out branch, from their
  merge base, plus what the worker left uncommitted in its worktree (own workers don't commit). The
  worker session's changes view (`gd` in the TUI) shows this instead of "against HEAD", so it is the
  review while the worker runs and after. HTTP `GET /api/tickets/{id}/changes`: the `ProjectDiff`, the
  branch, the target branch, commits ahead, and whether it merges cleanly (`git merge-tree --write-tree`,
  nothing touched) with the conflicting files.
- **Merge** (`ticket.merge`): only when the ticket is `done` and its run has ended. Uncommitted changes in
  the worktree are committed first ("ticket <id>: uncommitted changes"). Refused when it would conflict
  (the conflicting files are listed; send the worker feedback to sort it out) or when the project's
  checkout has changes in the same files; otherwise `git merge --no-ff ticket/<id>` in the project's
  checkout, message `Merge ticket <id>: <title>`. Then the worktree is removed, the branch deleted, and
  the ticket gets `merged: <commit>`; the worker session stays (read-only history).
- **Discard** (`ticket.discard`): removes the worktree and deletes the branch (`-D`) without merging;
  the ticket gets `discarded: true`. Asks for confirmation in clients.
- **Where:** `composer ticket changes|merge|discard <id>`; the TUI's `:merge` / `:discard` in a worker
  session and `m` in its changes view; the app's worker card and worker chat get "Review", a changes
  screen (files, diff) with Merge / Discard.
- **Orchestrator:** a `merge_ticket(ticket)` tool (in the seed orchestrator's `tools:`); every merge it
  asks for is an approval request (below) showing the diff stat, so the user decides, from the phone too.

Built (milestone 16): `crates/server/src/project/ticket_branch.rs` (review through a temporary index, so
nothing in the worktree changes; `merge-tree` for conflicts; a failed `git merge` is aborted), WS
`ticket.changes` / `ticket.merge` / `ticket.discard` (replies `changes`, `merged`), the session diff route
returning a worker's ticket review, a merged or discarded ticket refusing to resume. The app's review
screen opens from a worker's report card (`Changes`) or a worker chat's menu; `integration_test/review_test.dart`.

### Approvals, generalised

- `approval.request` gets a `kind`: `recipe` (as now), `merge` (a ticket merge an agent asked for),
  `tool` (Claude Code wants a tool outside its allowlist), plus a `title` line for clients. Its body is
  what to look at (recipe body, diff stat, the tool input). Additive: missing `kind` means `recipe`.
- Decisions: `approve`, `deny`, and `always` (tool requests only: remember the rule for the project).
- A worker waiting on one is `awaiting approval` (orange in the TUI and the app, a notification on the
  phone), as recipe approvals are now.

### Claude Code approvals (`--permission-prompt-tool`)

- `composer mcp` offers a `permission` tool; Claude Code runs with `--permission-prompt-tool
  mcp__composer__permission` (and `--permission-prompts host`) instead of denying everything outside the
  allowlist. The call becomes a `tool` approval in the worker's session: tool name, its input (the
  command for Bash, the path and diff for edits), and the rule "always" would add (`Bash(<first word>:*)`,
  `WebFetch(domain:<host>)`, else the tool name).
- Approve → `{"behavior":"allow","updatedInput":<input>}`; deny → `{"behavior":"deny","message":"The
  user denied this."}`; always → allow, and the rule is stored per project in `state/tool_rules.json`
  (outside agent reach) and added to `--allowedTools` from the next launch on; within the run, the same
  rule is answered without asking.
- `[claude_code] ask = true` (default); `false` keeps today's deny-without-asking. Unanswered requests
  wait (the run's timeout still applies); cancelling the worker closes them.

Built (milestone 17): `harness/tool_permission.rs` and `harness/tool_rules.rs`. `permission` is listed and
answered by the server only for external harnesses' workers (`tool.list` / `tool.call`); it refuses to
approve itself. Rules are offered only for plain commands (no `;&|$` etc. outside single quotes) and
never for file tools; `git`, `cargo`, `npm` and the like pin their subcommand (`Bash(git push:*)`), and
with a flag first (`git -C x …`) there is no rule. The child gets `MCP_TOOL_TIMEOUT` (the run's timeout),
so a question can wait. Checked with the real CLI: asked, approved from the TUI, then `always` and a
second run that wasn't asked.

### Following a Claude Code run

- **Streaming:** `--include-partial-messages`; text and thinking deltas become `message.delta` /
  `thinking.delta`, so the worker's chat streams like an own worker's.
- **Tool rows that say what happened:** both clients summarise Claude Code's tools (Read/Write/Edit
  path, Bash command, Grep/Glob pattern, WebFetch URL, Task description, TodoWrite progress); Edit,
  MultiEdit and Write show their diff in the details; TodoWrite shows the checklist.
- **Sub-agents:** calls made inside a Task (`parent_tool_use_id`) show under their Task row, indented.
- **Changes as they happen:** after a file tool's result in the session you're looking at, an open
  changes view (TUI `gd`, the app's review screen) refreshes.
- **Run stats:** the result's turns, duration and cost go into the ticket (`turns:`, `duration:`,
  `cost_usd:`, at API prices; a subscription isn't billed per run).

Built (milestone 18): `tool.call` has an optional `parent`. Current CLIs call the sub-agent tool `Agent`
(older: `Task`) and keep to-dos with `TaskCreate` / `TaskUpdate` (older: `TodoWrite`); all are
summarised. Thinking arrives redacted, so only text streams. A sub-agent's own text stays inside it
(its report is the `Agent` call's result). Summaries and details live in `crates/tui/src/tool_text.rs`
and `apps/composer_app/lib/state/tool_text.dart` (the same rules). The TUI's changes view already
refreshed after every tool result; the app's review screen refreshes after a file-changing tool in the
open chat and when the worker stops. Claude Code's system prompt names composer's tool parameters (its
MCP schemas load lazily, and it guessed `open(target)` once).

### Desktop app polish

The desktop (wide) layout of the app: [mockups](mockups/desktop.html) (seven screens), then built once they're agreed.
Four columns: server rail, session list (the phone's Needs you / Running / Recent), chat, and a resizable side
pane with tabs (Page, App, Changes, Worker; Server from the rail); the session list collapses to a strip (ctrl-b). Review, a worker's live run and the server page sit
beside the chat instead of replacing it; ctrl-k palette (sessions, pages, actions), ctrl-n new chat dialog,
the chat's ⋮ menu as on phones, approvals answerable with y / a / n.

Built (milestone 19): `apps/composer_app/lib/ui/wide/` (`WideLayout`, `ServerRail`, `SessionColumn`,
`SessionStrip`, `SidePane`, `WorkerPane`, `CommandPalette`, `ProjectHarnesses`). The side pane's tab and ticket
live in `ServerStatus` (`side`, `sideTicket`), the Worker tab's session as a second attached chat
(`sideChat`; neither chat detaches a session the other shows). Following the agent opens a closed pane on
its page and otherwise leaves the tab you chose; an app stays loaded while another tab is up. The folded
list and the pane's width are kept in `layout.json` beside the server list. Shortcuts go through
`AppShortcuts` (a keyboard handler, so they work after the focused widget went away; plain keys not while
typing): ctrl-k, ctrl-n, ctrl-p, ctrl-b, ctrl-shift-d, alt-↑/↓, ctrl-], esc, y / a / n. The old rail and
server page are gone. Not built from the mockups: the header's project and Claude Code chips (the header
keeps its project label; ⋮ changes both).

### Milestones

16. ✅ **ticket merging** — review diff against the merge base, merge / discard with checks, CLI, TUI, app
    review screen; generalised approvals and the orchestrator's `merge_ticket`.
17. ✅ **Claude Code approvals** — `permission` MCP tool, tool approvals in both clients, `always` rules.
18. ✅ **following Claude Code** — partial messages, tool summaries and diffs, sub-agent nesting, live
    changes, run stats.
19. ✅ **desktop app** — mockups, then the wide layout.

## Open questions

- None blocking.
