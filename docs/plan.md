# composer — plan

Status: draft, iterating. See [idea.md](idea.md) for the original idea.

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

- **Provider:** OpenAI-compatible `/v1/chat/completions` at `ai.ts.elketron.com`, tool calling + streaming + images. Behind a small `Provider` trait.
  - Model: `qwen3.8 flash next` — the only model on the server, so the id is taken from `GET /v1/models` at startup (overridable in config).
  - Context: 256K. Compaction threshold configurable, default well below max (e.g. 160K) since long contexts are slow locally.
  - **Parallelism 2:** ~40–50 t/s with one stream, ~20–30 t/s each with two. The parent server owns a **global slot limiter** (default 2 slots) shared by all sessions, workers and compaction turns.
- **Sessions:** free-form, many per project. A session may have an **active project** (a real code directory outside the workspace), set with `:project <path>` in the TUI or by the agent via `set_project(path)`. Persisted in `state/sessions/`.
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
| `spawn_worker(ticket)` | orchestrator only |
| `new_app(site)` | copy app scaffold into `<site>/ui/` |
| `set_project(path)` | set the session's active project (shown in the TUI) |
| `finish(status, summary)` | worker only: ends the worker, sets ticket status, notifies the orchestrator |

The agent reads raw markdown; the server parses links on the side (back-links, TUI navigation).

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
- The worker process is a server instance without an API: model slots are lent by the parent (`slot.acquire`/`slot.release`), recipe approvals are asked through the parent (`approval.ask`; approval prompts reach every client), and its session events go to the parent (`worker.event`), which re-sequences them into the worker session's transcript. The worker keeps its model history in `state/workers/sessions/<id>/`.
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
| `?` | help |

Commands: `:new [agent]`, `:attach <id>`, `:project <path>`, `:open <url>`, `:app [site]`, `:search <q>`, `:pair`, `:q`.

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

## Open questions

- None blocking. Next: milestone 1.
