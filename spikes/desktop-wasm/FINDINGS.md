# Does the desktop compile to wasm?

Spike for [0206](site://orchestration/tickets/0206.md), 2026-11-16, branch `ticket/0206`.
Question from the plan: the Windows `.exe` needs a signing cert, which is more work than it is
worth — could a Windows user instead open a page served by the composer server?

**Short answer: no, the desktop does not compile to wasm, and it should not be made to.** CEF has no
wasm build and gpui's Linux backend is X11/Wayland, so the app as written cannot. But the answer is
more useful than that: **gpui-pre already ships a browser backend** (`gpui-pre-web`: a canvas
renderer, IME, glyph cache and event plumbing over `web-sys`) and it type-checks for
`wasm32-unknown-unknown`, and **the reusable half of the desktop and all of `composer-client`'s
chat logic already compile for a browser target today** — 2,967 lines of the desktop's own code and
1,274 lines of `composer-client` built into a working `.wasm` (`p-link`, 1.2 MB). The Windows answer
is still not a browser client: **ship the native build unsigned (route C)**. It is what CI already
does. If a browser client is ever wanted, it is route B — a thin Svelte shell served from the
server, reusing the Rust logic through wasm-bindgen for the parts that matter — and this spike says
exactly what it could reuse and what it would have to write.

> **Provenance note (orchestrator, 2026-11-16).** This report was written in a worker worktree that
> composer removed before anything could be committed — git is unreachable from a worker sandbox
> (0203's mount needs a rebuilt, restarted server), so the worker mirrored its output into the coding
> site and asked to have it committed. That happened: `scripts/` is carried with this file, and every
> claim below is the worker's, unedited. What did **not** survive the worktree is the raw
> `logs/*.txt` and the 28 `probe*/` crates; regenerate them with the commands under "how to re-run
> it". Two load-bearing claims were re-checked against the repo afterwards and hold: no Windows
> signing step exists (`grep -rniE "codesign|signtool|authenticode" .github scripts docs` → no
> output, and the only signing in `.github/workflows/release.yml` is the Android keystore at line 67),
> and `gpui-pre-web-0.3.11` is on disk at 7,004 lines (`find
> ~/.cargo/registry/src/*/gpui-pre-web-0.3.11/src -name '*.rs' | xargs wc -l` → 7004 total).

Every claim below names the command that produced it. The raw logs were in `logs/` (regenerate them; see the note above); the scripts that
made the probes are in `scripts/`; the throwaway crates are in `probe/`, `probe2/`, `probe3/`.
No production file was touched.

## Build attempts

Machine: cargo/rustc 1.96.0 (`/usr/bin`), targets installed
`wasm32-unknown-unknown, x86_64-pc-windows-msvc, x86_64-unknown-linux-gnu`
(`rustup target list --installed`), toolchains `stable` (default) and `nightly`
(`rustup toolchain list`). Run from a worker worktree
(`state/worktrees/0206`), whose `apps/desktop/target/` started empty.

### 1. Baseline — the desktop for the host target: PASSES

```
cd apps/desktop && cargo check                    # logs/desktop-host.txt
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 23s
EXIT=0
```

So the environment is not the problem: the whole dependency graph type-checks on Linux —
615 name@version entries, 575 distinct crates (`cargo tree -e no-dev --prefix none |
awk 'NF{print $1" "$2}' | sort -u | wc -l` → 615; `… | awk 'NF{print $1}' | sort -u | wc -l` →
575; `grep -c '^name = ' Cargo.lock` → 823 packages locked). The list is in
`logs/trees/deps-all.txt`.

It does need the CEF **runtime files**, but only to *link*, not to check: the build script of
`cef-dll-sys` ran, found `CEF_PATH=.cef` missing (`apps/desktop/.cargo/config.toml` points it at a
sibling `.cef`, which `scripts/fetch-cef.sh` normally links to `~/.local/share/composer/cef/156.0.15`),
and downloaded its own copy — `logs/desktop-host.txt`:

```
CEF_PATH does not exist, downloading archive to: …/apps/desktop/.cef/156.0.15
Using downloaded CEF path: …/cef_linux_x86_64
```

`~/.local/share/composer/cef/156.0.15` exists on this machine (`ls -la`, in the same log) — a fresh
checkout without `scripts/fetch-cef.sh` costs a 1.5 GB download into `apps/desktop/.cef`, which is
what that script exists to avoid.

### 2. The desktop for the browser: FAILS, 8 crates + 1 build script

```
cd apps/desktop && cargo check --target wasm32-unknown-unknown --keep-going   # logs/desktop-wasm.txt
  EXIT=101 after ~2 minutes, 462 units checked
```

`--keep-going` matters: the first attempt without it stopped at the first blocker, and the summary of
this run is `logs/desktop-wasm.summary.txt`
(`bash scripts/analyze.sh logs/desktop-wasm.txt`):

```
crates that failed: filedescriptor, getrandom, mac_address, mio, notify-rust, open, rustls, wasm_thread
build-script failure: error: failed to run custom build command for `cef-dll-sys v156.3.1+156.0.15`
  stderr: Error: Unsupported target triplet: wasm32-unknown-unknown
```

`composer-desktop` itself was never reached: `cef`, `gpui-pre-platform` and `tattoy-wezterm-term`
all failed below it. This is the ticket's answer to "does the desktop compile to wasm": **no**, and
the two hard stops are CEF (no wasm binaries exist — the build script refuses the triplet) and the
native windowing in gpui's Linux backend. What is surprising is that the *toolkit* is not one of
them — see Blocker inventory and route A.

### 3. `composer-protocol` for the browser: PASSES

```
cargo check -p composer-protocol --target wasm32-unknown-unknown    → Finished in 7.40s, exit 0
```

The wire types are pure serde: a browser client can share them exactly as the phone does.

### 4. `composer-client` for the browser: FAILS, three crates, all transport

```
cargo check -p composer-client --target wasm32-unknown-unknown --keep-going   # logs/client-wasm.txt
  error: could not compile `getrandom` (lib) due to 1 previous error
  error: could not compile `mio` (lib) due to 48 previous errors
  error: could not compile `rustls` (lib) due to 7 previous errors
  EXIT=101
```

The three messages, verbatim from that log:

- `getrandom 0.4.3`: *"The wasm32/64-unknown-unknown are not supported by default; you may need to
  enable the `wasm_js` crate feature."*
- `mio 1.2.4`: *"This wasm target is unsupported by mio. If using Tokio, disable the net feature."*
- `rustls 0.23.46`: `no associated function or constant named 'now' found for struct 'UnixTime'`
  (in `rustls-pki-types`, whose `now()` needs a clock the wasm target has no way to ask).

So `composer-client` fails **entirely because of its transport** — `tokio`'s `net` feature (via
`tokio = { features = ["full"] }` and `tokio-tungstenite` in `crates/client/Cargo.toml`) and the
TLS stack under `reqwest`. Nothing in the reducer or the store does. Section "Portable core" proves
that by compiling them.

## Blocker inventory

Every dependency of `apps/desktop` (615 name@version entries, `logs/trees/deps-all.txt`) was
classified by building one tiny crate per suspect
(`scripts/make-probes.sh`, `scripts/make-probes2.sh`, one `cargo check -p … --target
wasm32-unknown-unknown` per probe — separate calls on purpose, because `--workspace` unifies
features across members and hides a crate's own verdict behind another's). Matrices:
`logs/matrix-probe*.txt`. "who pulls it" is `cargo tree -e no-dev -i <crate>`, in
`logs/trees/tree-*.txt`.

### Hard stops — no browser build exists

"Who pulls it" is the reverse tree **for the browser target**
(`cargo tree -e no-dev --target wasm32-unknown-unknown -i <crate>`, all of them in
`logs/trees/wasm-reverse.txt`) — which matters, because the native windowing crates are *not* in
that graph at all: for `target_family = "wasm"` the platform crate picks `gpui-pre-web` instead of
`gpui-pre-linux` (`logs/gpui-web.txt` shows the wasm tree: `gpui-pre-platform → gpui-pre-web`, no
`-linux`, no `x11rb`, no `atspi`).

| crate | who pulls it, on the browser target | the error (from the probe logs) |
|---|---|---|
| `cef` / `cef-dll-sys` | `apps/desktop` directly | `Error: Unsupported target triplet: wasm32-unknown-unknown` from the build script (`logs/per-probe2/p-cef.txt`) — CEF ships Windows/macOS/Linux binaries only. Its own build-dependencies (`download-cef` → `ureq` → `rustls`) are why `rustls` appears twice in the wasm tree |
| `tattoy-wezterm-term` | `apps/desktop` directly | `error[E0432]: unresolved import 'internal'` in `mac_address 1.2.1`, reached through `wezterm-blob-leases` from `tattoy-termwiz`, `-cell`, `-surface` and `-term` (`logs/trees/wasm-reverse.txt`); `mac_address` reads host network-interface data. Adding `uuid/js` and getrandom's wasm features does not help (`logs/per-probe3/p-wezterm-term-js.txt`): the emulator's screen model is fine, its blob-lease/MAC path is not. `filedescriptor` fails the same way, through `tattoy-termwiz` |
| `open`, `notify-rust` | `apps/desktop` directly (both are in its `Cargo.toml`) | `compile_error!("open is not supported on this platform")` (`logs/per-probe/p-open.txt`); notify-rust fails in the same graph. Both are trivially wrong for a browser — `window.open` and the Notifications API are the stand-ins — but they are direct dependencies, so the crate as written does not build |
| `wasm_thread` (via `gpui-pre-scheduler`) | `gpui-pre` → `gpui-pre-scheduler`, and `gpui-pre-web` too | `error[E0554]` on stable only; passes on nightly — see the end of this section, so it is a toolchain problem, not a hard stop |

### Fixable with configuration, not code

| crate | what it takes | proven by |
|---|---|---|
| `tokio` | drop `net` (and `rt-multi-thread`): `default-features = false, features = ["sync","macros","io-util","rt","time"]` | `p-tokio-full` FAIL (mio), `p-tokio-net` FAIL (mio), `p-tokio-async` FAIL — *"Only features sync,macros,io-util,rt,time are supported on wasm."*, `p-tokio-wasm` **PASS** |
| `getrandom` | its `wasm_js` (0.4) / `js` (0.2) feature | `p-getrandom-wasmjs` **PASS** |
| `reqwest` | nothing — for `cfg(target_family = "wasm")` it *is* the browser fetch | `p-reqwest-default` (json, stream, form, multipart) **PASS**; `cargo tree -p p-reqwest-default --target wasm32-unknown-unknown \| grep -c 'rustls\|hyper'` → **0**, the same tree for the host → **20**. hyper, tokio-net and rustls all disappear |
| `rustls` | a ring provider that builds without C, plus a clock; a browser needs neither | `p-rustls` FAIL (`aws-lc-sys` build script), `p-rustls-ring` FAIL (getrandom `js`); both are moot once reqwest uses fetch |

### Plain Rust — fine as they are

`p-pulldown-cmark`, `p-image`, `p-toml`, `p-core` (`composer-core`), `p-client-logic`,
`p-client-endpoint`, `composer-client-lite`, `p-wasm-websocket` (the `web-sys::WebSocket` +
`wasm-bindgen-futures` stand-in), `p-desktop-logic`: all **PASS**. `p-wezterm-surface` (the cell
grid the terminal pane draws from, without the emulator) **PASS** — so the terminal's *data model*
is portable even though the emulator crate is not.

### The one that changes the shape of the question: gpui has a browser backend

`gpui-pre-platform 0.3.11` selects its backend by target
(`grep -A4 'target\.' ~/.cargo/registry/src/*/gpui-pre-platform-0.3.11/Cargo.toml`, in
`logs/gpui-web.txt`):

```toml
[target.'cfg(target_family = "wasm")'.dependencies.gpui_web]
package = "gpui-pre-web"
```

`gpui-pre-web` is not a stub: 7,004 lines across 14 files
(`find ~/.cargo/registry/src/*/gpui-pre-web-0.3.11/src -name '*.rs' | xargs wc -l`) — `window.rs`
(1,054) drawing into a `web_sys::HtmlCanvasElement`, `events.rs` (1,481) over `PointerEvent`,
`KeyboardEvent`, `ResizeObserver`, `ime_mirror.rs` (618) for IME, `glyph_cache.rs`,
`text_system.rs`, `canvas_fallback.rs`, `http_client.rs` over `fetch`, `dispatcher.rs` on
`js_sys::Atomics`. So route A's premise — "does anything upstream support a wasm backend" — is
**yes, the same gpui we already use**.

What stops it today is a toolchain line, not an architecture:

```
cargo check -p p-gpui-platform --target wasm32-unknown-unknown            → FAIL
  error[E0554]: `#![feature]` may not be used on the stable release channel
    --> wasm_thread-0.3.3/src/lib.rs:1  #![cfg_attr(target_arch = "wasm32", feature(stdarch_wasm_atomic_wait))]
cargo +nightly check -p p-gpui-platform --target wasm32-unknown-unknown   → PASS
  (Checking gpui-pre-web v0.3.11 / Checking gpui-pre-platform v0.3.11 / Finished)   # logs/nightly/
```

`gpui-pre` core alone also passes on stable (`p-gpui-core`). Caveats stated plainly: "type-checks
on nightly" is not "renders in a browser" — nothing was run, `wasm-pack` exists here but
`wasm-bindgen` CLI and `trunk` do not (`command -v wasm-bindgen wasm-pack trunk`), and
`wasm_thread` 0.3.3 is the only version published (`cargo search wasm_thread`), so a browser build
of gpui means a nightly toolchain (or a patch to that one line) plus whatever the app code needs on
top. None of composer's own gpui code was ever checked against it, because CEF fails in the same
crate graph first.

## Portable core

The claim to test: `composer-client` breaks only at its transport, and the desktop's non-drawing
logic is reusable. Both probes include the real files verbatim with `#[path]` — no copies, no edits
(`scripts/make-probes3.sh`), so a PASS is about the repo's code, not about a paraphrase.

| probe | what it is | result |
|---|---|---|
| `composer-protocol` | the wire types, as a workspace member | **PASS** |
| `p-core` | `composer-core` (pages, frontmatter, site URLs) as a dependency | **PASS** |
| `p-client-logic` | `chat.rs`, `chat_item.rs`, `tool_text.rs`, `store.rs`, `terminal_view.rs`, `window_binding.rs` — 1,095 lines — with no transport at all | **PASS** |
| `p-client-endpoint` | `endpoint.rs` (179 lines) alone: `std::fs` + `reqwest` + `toml`, no sockets | **PASS** — it *compiles*; at runtime `Endpoint::discover(home, ws)` has no home to read (see below) |
| `p-client-connection` | `connection.rs` as it stands: `tokio::sync` + `tokio_tungstenite::tungstenite::Message` in its public types | **FAIL** (mio) |
| `composer-client-lite` | the same client modules as a real crate, i.e. what a wasm client links against | **PASS** |
| `p-desktop-logic` | 17 of the desktop's own modules, verbatim, against `composer-client-lite` renamed to `composer_client` | **PASS** |
| `p-link` | a `cdylib` with `#[wasm_bindgen]` exports that call `composer_client::tool_summary`, the desktop's `markdown::blocks` and `view_urls::page_for`, with `composer-protocol` and `composer-core` in the graph | **PASS — it links**: `cargo build -p p-link --target wasm32-unknown-unknown --release` → `target/wasm32-unknown-unknown/release/p_link.wasm`, 1,220,947 bytes (`logs/link-release.txt`) |

**What breaks in `composer-client`, named exactly** (`logs/client-wasm.txt`, and the probes above):

- `tokio` with `features = ["full"]` → `net` → `mio`. The reducer and the store need only `sync`
  and `time`; `p-tokio-wasm` proves that spec builds.
- `tokio-tungstenite` (and `tungstenite::Message` leaking through `Connection`) → tokio net +
  rustls. In a browser the stand-in is `web_sys::WebSocket` + `wasm_bindgen_futures`
  (`p-wasm-websocket` PASS): the same shape — a bounded pair of `futures::Stream`/`Sink`,
  reconnect with backoff, requests paired with replies — with the browser owning the socket, TLS
  and the clock. `Connection`'s 189 lines would become a `cfg`-split module (native: tungstenite;
  wasm: web-sys) behind the same API, so nothing above it changes.
- `reqwest` with rustls → disappears on wasm by itself (0 rustls/hyper in the tree): `Endpoint`'s
  http calls need no change.
- `Endpoint::discover(home, workspace)` reads `remote.json`, `global/config.toml`,
  `workspaces/<ws>/state/token` with `std::fs` (`grep -n 'std::fs' crates/client/src/endpoint.rs`).
  That cannot work in a page: there is no home, and the token must not be in a URL
  (`docs/desktop.md`: *"The token is not in a pane's URL"*). The browser's stand-in already exists
  server-side: `POST /api/web-session` returns `{cookie_name, value, path}` for the workspace
  (`crates/server/src/api/http_routes.rs`, in `logs/browser-client.txt`) and the API middleware
  accepts that cookie (`crates/server/src/api/auth.rs`). So a wasm client's `Endpoint` is
  "same origin as the page + the session cookie", and `discover` is ~10 lines of cfg.

**What would stand in for what**

| desktop / client piece | in a browser |
|---|---|
| `Connection` (tungstenite) | `web_sys::WebSocket` + `wasm-bindgen-futures`; the reducer above it unchanged |
| `reqwest` http | `reqwest`'s own wasm backend (fetch) — already used by the passing probes |
| `Endpoint::discover` + token file | page origin + `POST /api/web-session` cookie, or the `/pair?code=` device flow |
| `tokio` runtime | the browser's; tokio `{rt, sync, time, io-util, macros}` still compiles if a crate wants it |
| `wezterm-term` terminal pane | a JS terminal renderer over the server's PTY stream (the terminals are already server-owned: plan phase 5). `tattoy-wezterm-surface` passes for wasm, so even the cell-grid model could stay in Rust — but that is a second project |
| CEF panes | same-origin `<iframe>`s: the URLs are already computed by `view_urls.rs`, which is in the passing probe |
| `open`, `notify-rust` | `window.open`, the Notifications API |
| `std::fs` | `fetch`/the API, or nothing |

## Desktop module split

`scripts/module-split.sh` produced `logs/modules.txt` (per module: `wc -l`, plus the crate roots in
its `use` statements); `scripts/counts.sh` produced `logs/counts.txt` (the exact sums below).

12,006 lines in 35 modules. Split by what a module *names anywhere in its text*
(`grep -qE '(^|[^a-z_])(gpui|cef|wezterm|tokio|…)::'`):

| bucket | lines | notes |
|---|---|---|
| **compiles for a browser today, proven** (`p-desktop-logic`: 17 modules) | **2,967** | listed below |
| never names a UI framework, but pinned to something native (7 modules) | 1,151 | `pick_lists` 484 (imports `crate::picker`, which is gpui), `home_lists` 281 (its lists feed that picker), `key_map` 153 + `term_config` 37 (`use tattoy_wezterm_term`), `notice` 116 (`notify_rust`), `terminal_link` 77 (`composer_client::Connection`), `main` 3 |
| bound to gpui (`use gpui`) | 7,691 | `shell` 1,842, `chat_pane` 1,628, `home_view` 1,302, `term_pane` 620, `picker` 411, … |
| bound to CEF | 1,518 | `web_pane` 1,321, `cef_runtime` 197, `browser_frame` 153 |
| bound to the terminal emulator | 620 | `term_pane` (overlaps the gpui row) |

4,118 of the 12,006 lines are not written against gpui or CEF at all; 2,967 of those build for a
browser as they stand.
(The framework rows overlap on purpose — `term_pane` names both gpui and the emulator, `web_pane`
both gpui and CEF — so they are not meant to sum to 12,006.)

The 17 modules that PASSED, by size (`logs/counts.txt`):

```
markdown 780   view_urls 478   draft 272   dom_keys 184   grid 175   uploads 147
browser_tabs 132   stage_tabs 122   overlay_frame 101   prose 98   matcher 67
workspaces 66   link_state 55   theme 52   chat_width 50   project_binding 53   user_review 135
```

**That is the reusable part, and the answer to the ticket's question is specific about it.** The
three the ticket named — `markdown.rs`, `view_urls.rs`, `key_map.rs` — split three ways:
`markdown.rs` (780) and `view_urls.rs` (478) are portable as they stand and are the valuable ones
(a page/chat renderer that needs no new browser code, and the URL rules for every pane, including
`same_origin`, `navigation`, `page_for`); `key_map.rs` is 153 lines welded to
`tattoy_wezterm_term::KeyCode` and only makes sense once a terminal emulator exists in the browser,
and `matcher.rs` (67) / `prose.rs` (98) are portable but small. `composer-client`'s `Chat` is
portable *without its transport* (359 + 65 + 225 lines: `p-client-logic`).

What a browser shell still has to write: the drawing. `shell.rs` (1,842), `chat_pane.rs` (1,628),
`home_view.rs` (1,302), `term_pane.rs` (620), `picker.rs` (411) are gpui; `web_pane.rs` (1,321) +
`cef_runtime.rs` (197) are CEF and are *replaced*, not ported — a browser already has CEF's job.
The chat pane's logic is in `Chat` (reused) and its drawing is the part to redo; its markdown model
(`markdown.rs`, `prose.rs`) is already done.

## Server feature flag

What `--features web` on the server would mean — described, not built. No crate in the repo declares
cargo features today (`grep -c '^\[features\]' crates/{server,cli,client}/Cargo.toml
apps/desktop/Cargo.toml` → `0` for each, `logs/signing-and-tools.txt`): this would be the
workspace's first feature, and it only needs to add **static serving**:

- One route: `GET /w/<ws>/client/` → `index.html`, and `GET /w/<ws>/client/{*asset}` → the hashed
  assets, next to the existing `GET /w/<ws>/pair` (`crates/server/src/api/pair_routes.rs`) and
  inside the workspace router (`crates/server/src/api/mod.rs`). The bundle would live beside the
  other built UIs and be served the same way `app_routes.rs` serves `<site>/ui/dist`: a folder of
  files with a fallback to `index.html` (`fn pick`, `crates/server/src/api/app_routes.rs`), or
  compiled in with `include_dir` the way stock site files already are (`include_dir` is a workspace
  dependency).
- Nothing else new. **Auth for browsers is already built**: `api/auth.rs` mints and checks the
  `composer_token` (per-workspace; port-suffixed at the root) cookie, `POST /api/web-session` hands
  it to a page, `GET /pair?code=` + `POST /pair` set a device session for a browser that isn't the
  local machine, and the Origin rule is one function (`Audience::Workspace`). The desktop already
  relies on exactly this for its panes: `view_urls.rs::auth_cookie` stores the server's cookie
  rather than putting a token in a URL.
- The pages and apps the shell would embed need no new routes either: `/w/<ws>/view/<site>/…` and
  `/w/<ws>/view/changes/<project>` already exist (`api/mod.rs`), and app URLs come from
  `open_app` (`crates/server/src/apps/app_open.rs`, `<base>/a/<token>/`). Two corrections to the
  ticket's premises, both from the code: apps are now served under **`/a/<token>/`**, not
  `/w/<ws>/p/<project>/<app>/` — `/p/…` and `/h/…` survive only as redirect routes
  (`api/app_api.rs`, `.route("/p/{project}/{app}", get(old_url))`) — and an app page is served
  **sandboxed with an opaque origin** (`Audience::App`: *"its page is sandboxed (an opaque origin,
  `Origin: null`), and its token is in the path, not in a cookie"*). So a page-iframe is same-origin
  with the shell and an app-iframe is not: fine for embedding, but the shell cannot reach into an
  app's document, which is deliberate.
- The real cost is the **build**, not the route: a wasm client needs a JS glue generator and a
  bundler, and neither CI nor this machine has the usual ones —
  `command -v wasm-bindgen wasm-pack trunk` → `wasm-bindgen: not installed`,
  `wasm-pack: ~/.cargo/bin/wasm-pack`, `trunk: not installed` (`logs/signing-and-tools.txt`);
  `node`, `npm` and `bun` are installed, so a bundler is one `bun add` away, but no job in
  `scripts/ci.sh` or `.github/workflows/release.yml` builds wasm today. A release would then carry
  a `client/` bundle — built in CI, or, like site UIs, built on demand.

For contrast, how today's UIs reach a browser: site app UIs are read from `<site>/ui/dist` on disk
and served at `<base>/<site>/…` (`api/app_routes.rs`); the desktop's panes are pointed at
`/w/<ws>/view/…` and `/w/<ws>/a/<token>/…` (`apps/desktop/src/view_urls.rs`). A web client would be
served exactly like a site app — same-origin, at a workspace route, holding the workspace cookie —
which is why the flag is small and the bundle is the work.

## Routes compared

Rough cost, one line each first, then the reasoning. "Days" are focused work-days; "weeks" are
calendar.

| | what it is | cost | verdict |
|---|---|---|---|
| **A** | gpui in the browser: the same Rust UI, wasm-backed | weeks, and a nightly toolchain | no |
| **B** | a thin browser shell served behind a server flag, reusing the Rust logic where it compiles | days for a pilot, weeks for parity | the real option, but not now |
| **C** | ship the native Windows build **unsigned** | ~0 code; one paragraph of docs | **recommended** |

**A — gpui in the browser.** Cheaper than expected on the toolkit and dearer than expected
everywhere else. The toolkit: `gpui-pre-web` exists, 7,004 lines, canvas + IME + glyph cache +
fetch, and `cargo +nightly check -p p-gpui-platform --target wasm32-unknown-unknown` passes. The
costs: a nightly toolchain for every web build (`wasm_thread 0.3.3` needs one unstable feature;
stable fails with `E0554`), a bundler and glue that CI doesn't have, 7,691 lines of composer's own
gpui UI (`logs/modules.txt`: the modules with `use gpui`) never compiled for that target and written
against native windows, and — the decisive part —
CEF disappears by definition, so the app and page panes become DOM iframes anyway. You would end up
with two rendering stacks inside one browser tab (a gpui canvas for the chrome and real iframes for
content) and re-verify input, IME and clipboard through two layers, to get a browser version of an
app whose value is native terminals and native Chromium. Nothing here says the browser is a bad
place for composer's UI; it says *this* is the expensive way to get there.

**B — a thin browser shell.** One page at `/w/<ws>/client/`: chat on the left, tabs on the right,
apps and pages as iframes using `view_urls.rs`'s rules, terminals through a JS terminal renderer
over the server's PTY stream. Its Rust parts are already proven to compile and to link into a
`.wasm`: `composer-protocol`, `composer-core`, `composer-client` minus transport, the markdown and
page model, `view_urls`. Its own code is a Svelte shell like every other composer app (the user's
rule: mockup first, then app code) plus a small `wasm-bindgen` shim over the reducer, or no Rust at
all — the phone is a second client today with the protocol reimplemented in Dart, which is the
counter-example to "the reducer must be shared". Cost: a pilot (chat read over `/ws` with the web
session cookie, one app iframe, one page iframe, one terminal tab) is days of shell work plus the
CI/bundle plumbing; reaching what the desktop does (palette, workspace picker, file drops,
notifications, the native review pane) is weeks, and some of it is what a browser does better.
This is the option to pick when there is a *reason* to be in a browser — and note the reason does
not need a cert: a browser client answers "a machine that can't install an .exe", not "a build
nobody signed".

**C — unsigned native.** Zero code, because **that is what CI does today**. `desktop-windows` runs
`scripts/package-desktop-windows.ps1`, whose last step is `Compress-Archive -Path $out
-DestinationPath "$out.zip"` (`logs/signing-and-tools.txt` quotes the whole script), and the
repository has no Windows signing step at all:
`grep -rniE "codesign|signtool|authenticode" .github scripts docs` → **no output**, and the only
signing step anywhere in CI is the Android keystore (`release.yml:67`). The `.exe` has always
shipped unsigned; nothing breaks by deciding not to sign it.

What unsigned costs the user, concretely: Windows tags a downloaded file with
mark-of-the-web, and unzipping propagates it, so the first launch of an unfamiliar unsigned
`.exe` gets SmartScreen's *"Windows protected your PC"* with **More info → Run app anyway** — one
extra click per build, because the prompt is keyed to the file's hash and every release has a new
one; a signed build with reputation skips it, and an EV certificate removes it outright.
There is no per-launch cost after the override, no admin prompt — the app is not installed, it is
unpacked and run (`docs/wsl.md` §5: "Unpack `composer-desktop-<version>-windows-x86_64.zip` from a
release … `.\composer_desktop.exe`"), so nothing asks for elevation — and antivirus may scan the
~100 MB CEF payload on first run. All of this is a documentation problem plus a support-load
judgement, not a technical one. And the honest counter-argument belongs in the same paragraph: an
OV/EV certificate is an annual fee and a one-time pipeline step, and it is cheaper to buy than to
build a browser client. This is a product decision, so it is stated below as a recommendation, not
as a finding.

One thing route C does not buy: `docs/wsl.md` §5 ends with *"This setup has not yet been tested on
a real PC."* Whichever route is chosen, the Windows/WSL run itself ([0188](site://orchestration/tickets/0188))
is still the open item.

## Recommendation

For the Windows user today: **do not sign anything and do not build a browser client — ship the
native build unsigned, which is what already happens**, and make the one thing that actually
smooths it a sentence in `docs/wsl.md` §5 and the release notes: *"this `.exe` is unsigned, so
SmartScreen says 'Windows protected your PC' — click **More info**, then **Run anyway**; you'll see
it once per new build, and if it bothers you the fix is a signing certificate, not a different
app."* The evidence for it being free is that there is nothing to change (the Windows job
already produces `composer-desktop-<version>-windows-x86_64.zip` and nothing in the repo signs it),
and the evidence for it being cheap is that the alternative costs weeks: route A needs a nightly
toolchain plus 7,691 lines of gpui-bound UI re-verified against a backend nobody has run in a
browser here, and route B needs a new shell; the spike proves both are *possible* —
`p_link.wasm` links, 1.2 MB of reducer, page model and URL rules — and that neither is what's needed
to let someone on a work PC use composer this month. Keep the browser path in the plan as the answer
to a different question
("a machine that can't install an .exe", or "the desktop inside a shared/kiosk browser"), and if
that question gets asked, **the next ticket is a browser-shell pilot**: one page at
`GET /w/<ws>/client/` behind `--features web`, holding the `composer_token` cookie from
`POST /api/web-session`, with (1) the chat reading `/ws` and rendering with the reused
`markdown.rs` model through a small wasm-bindgen shim, (2) one app iframe from `open_app`'s URL and
one page iframe from `view_urls`'s rules, and (3) one terminal tab driven by a JS renderer over the
server's existing PTY stream — the four things that decide whether a browser can be a real client,
none of which is the desktop's Rust.

## Not verified here

- Nothing was *run*: this is `cargo check`/`cargo build` evidence about compilability and linking,
  not about behaviour. `gpui-pre-web` type-checks on nightly; it was never shown a browser, and
  `p_link.wasm` was never loaded by one (no JS glue was generated here — `wasm-bindgen` CLI is not
  installed, see the recipe at the end).
- `std::fs` in a `wasm32-unknown-unknown` page failing at runtime is documented std behaviour
  (the probes only prove it *compiles*); nothing here demonstrated it.
- SmartScreen's exact wording/behaviour per build is the standard documented Windows
  mark-of-the-web + application-reputation behaviour; it was not observed on a Windows PC here — the
  Windows/WSL run in [0188](site://orchestration/tickets/0188) is still held.

## What is on this branch, and how to re-run it

Only `spikes/desktop-wasm/`: this file, `scripts/` (the scripts that produced every log),
`logs/` (the raw cargo output, matrices, dependency trees, greps) and `probe*/` (28 throwaway
crates: one per suspect dependency, plus `composer-client-lite`, `p-desktop-logic` and the `p-link`
browser bundle). No production file changed; no guarded experiment was added to `apps/desktop` or
`crates/`, because the split could be proven with `#[path]` includes instead. `probe*/target/` was
deleted to keep the worktree small; re-running regenerates it.

```sh
cd spikes/desktop-wasm
bash scripts/make-probes.sh  $PWD/probe   && bash scripts/run-probes.sh $PWD/probe   logs/per-probe
bash scripts/make-probes2.sh $PWD/probe2  && bash scripts/run-probes.sh $PWD/probe2  logs/per-probe2
bash scripts/make-probes3.sh $PWD/probe3 && bash scripts/make-link-test.sh $PWD \
  && (cd probe3 && cargo build -p p-link --target wasm32-unknown-unknown --release)
```

To see the bundle in a browser (the user's step — no page exists yet, and `wasm-bindgen` CLI is not
installed here):

```sh
cargo install wasm-bindgen-cli          # or: cargo install trunk
cd spikes/desktop-wasm/probe3
cargo build -p p-link --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web --out-name p_link \
  target/wasm32-unknown-unknown/release/p_link.wasm
printf 'import init, {tool_summary} from "./p_link.js";\nawait init();\nconsole.log(tool_summary("open", `{"url":"site://home"}`));\n' > web/main.js
printf '<script type="module" src="./main.js"></script>\n' > web/index.html
python3 -m http.server -d web 8123      # then open http://127.0.0.1:8123/ and read the console
```
