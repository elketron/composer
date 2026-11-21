# composer

What composer is: [README.md](README.md). The method its workflows follow: [docs/icm.md](docs/icm.md).
Terminals and `type: tui` pages: [docs/tui-mode.md](docs/tui-mode.md). UI boards:
[docs/mockups/next/](docs/mockups/next/) (desktop and phone) and
[docs/mockups/theme.css](docs/mockups/theme.css) (the TUI's colours).

## The code

The root Cargo workspace (`crates/*`):

- `core`: the web model. Sites, `site://` URLs, pages, frontmatter, links, the page index, mounts.
- `protocol`: the messages between the server and its clients (WS `/ws` and HTTP bodies). The
  source of truth for Rust, TypeScript (`--features ts`) and Dart.
- `server`: the harness. Sessions and the run loop (`session/`), run settings and the system prompt
  (`agent/`), tools (`tools/`), webhooks and spawned runs (`runs/`), outside agents over ACP
  (`harness/`), MCP clients for `[mcp.*]` and sites' `mcp.toml` (`mcp_clients/`, `site_mcp/`),
  composer's own MCP server (`mcp/`), recipes and approvals (`actions/`), terminals and background
  processes (`terminals/`), pairing (`devices/`), the API (`api/`), config.
- `client`: what a client needs and nothing of the server: finding the server, a reconnecting
  connection, a store of what the server says. The desktop links only this.
- `term`: a terminal emulator model without a UI (wezterm-term), used by the TUI's terminal pane.
- `tui`: the terminal UI.
- `cli`: the `composer` binary: `init`, `serve`, the TUI, `ask`, `pair`, `devices`, `login`,
  `logout`, and the hidden `mcp` and `script`.

Outside it:

- `apps/desktop`: the desktop (gpui + CEF), its own Cargo workspace. Needs the CEF runtime:
  `apps/desktop/scripts/fetch-cef.sh`.
- `apps/composer_app`: the phone app (Flutter, Android only).
- `apps/composer_protocol`: the Dart models generated from `crates/protocol`.
- `seed/sites`: the stock sites, embedded in the binary; `composer init` installs them. Their links
  are checked by `crates/core/tests/seed_web.rs`.

## Checks

Before a merge, run what CI runs: `scripts/ci.sh` (all four jobs) or `scripts/ci.sh rust protocol`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace                                        # some tests run real recipes and need `just`
cargo test -q -p composer-protocol --features ts              # TypeScript bindings
cargo run -q -p composer-protocol --features schema --bin dartgen -- --check
(cd apps/composer_protocol && dart analyze --fatal-infos && dart test)
(cd apps/composer_app && flutter analyze --fatal-infos && flutter test)
(cd apps/desktop && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test)
```

## Code conventions (Rust)

- **A type with an `impl` block gets its own file.** That includes trait impls (`impl Component for ChatPane`). Its impls live in that file. Derives don't count.
- **Types without impl blocks may share a file** (plain data enums/structs, e.g. `action.rs`, `store/types.rs`).
- File name = type name in snake_case (`SessionStore` → `session_store.rs`).

## Protocol changes

After changing `crates/protocol`, regenerate the Dart models and fixtures:
`cargo run -p composer-protocol --features schema --bin dartgen` (`-- --check` fails when they are
stale), then `dart test` in `apps/composer_protocol`, and update the desktop, the TUI and the
Flutter app to match.

## Stock sites

`seed/sites` is what `composer init` installs into a new home; it never overwrites an existing
file, and nothing updates an existing home. `site://home/index.md` is the agent's main
instructions when a session is in no site, so keep it in step with the tools in
`crates/server/src/tools/`.

## TUI

- Flux: all state changes go through `Action` → stores' `reduce` → `AppState`. Reducers do no IO; they return `Effect`s, which the `EffectRunner` executes.
- Components implement `Component` and hold only local view state; shared state belongs in a store.
- Styling follows `docs/mockups/theme.css`: no rounded corners in the TUI.

## Seeing the TUI (headless snapshots)

`composer script <file> --out <dir> [--new]` drives the real TUI against a running server
with an in-memory terminal and writes `<name>.html` (colours) + `<name>.txt` per `snap`.
Screenshot the HTML with Playwright to look at it. Script commands (see `crates/tui/src/headless.rs`):

```text
size 140x40
wait connected                 # also: session, idle, page, turn [secs], text <substring>
keys i hello<Esc><Enter>       # vim notation; Enter is a newline in Insert, sends in Normal
paste /tmp/photo.png           # a bracketed paste: a file's path becomes an attachment (a drop arrives as one)
sleep 2
wait turn 240
snap chat
```

Use a scratch `COMPOSER_HOME` (`composer init`, set another port in its config.toml) so ~/.composer stays untouched.
Set `EDITOR` to a non-interactive script to exercise the `e` (edit page) round trip headlessly.
Snapshots always force a draw, so they can't catch a missing repaint: for that, run
`scripts/check-tui-redraw.py target/debug/composer <COMPOSER_HOME>` (the real TUI in a pseudo-terminal).

## The Flutter app (`apps/composer_app`)

The phone client. It has an Android target only: no iOS, Linux, Windows or macOS.

- Same Flux shape as the TUI: widgets dispatch `AppAction`s, `reduce` (pure) returns the next `AppState`
  plus `AppEffect`s, `EffectRunner` does the IO. Same file rule: a class with behaviour (widgets
  included) gets its own file; plain data types share `types.dart` / `actions.dart` / `effects.dart`.
- `flutter analyze` and `flutter test` (reducer, parsing).
- End to end on an Android emulator against a test server: `scripts/phone-e2e.sh <device> <server url>
  [test...]` pairs each test (a fresh install needs its own pairing code) and forwards the server's
  port with `adb reverse`. Its header says what the test home needs. Tests in
  `integration_test/`: `pairing` (pairs and sees the server online), `chat` (a whole turn with a
  recipe approval), `reader` (a site from the Pages tab, following the agent), `site` (the site
  chip, the live view and the Sites tab), `measure_open` (times opening chats). `SHOT_PAUSE=4`
  makes tests stop at their `shot`s for screenshots. A headless emulator:
  `emulator -avd <name> -read-only -no-window -no-audio`.
- Icons: edit `assets/icon/*.svg`, then `scripts/app-icons.sh` renders the Android ones.
- Screenshots: `adb exec-out screencap -p > shot.png`.

## The desktop (`apps/desktop`)

gpui + CEF: the chat on the left, the stage on the right (pages, terminals, the live view). Build
and test inside `apps/desktop` (`cargo build`, `cargo test`); `scripts/package-desktop.sh` and
`scripts/package-desktop-windows.ps1` make the release archives.
