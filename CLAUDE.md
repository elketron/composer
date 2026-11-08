# composer

Design and decisions: [docs/plan.md](docs/plan.md) (phase 1; phase 3: harnesses and attachments; phase 4: merging, approvals, following Claude Code; phase 5: pipelines; phase 6: MCP servers; phase 7: entering sites, apps in the chat), [docs/plan-app.md](docs/plan-app.md) (phase 2: the Flutter app). UI mockups: [docs/mockups/](docs/mockups/).

## Code conventions (Rust)

- **A type with an `impl` block gets its own file.** That includes trait impls (`impl Component for ChatPane`). Its impls live in that file. Derives don't count.
- **Types without impl blocks may share a file** (plain data enums/structs, e.g. `action.rs`, `store/types.rs`).
- File name = type name in snake_case (`SessionStore` → `session_store.rs`).

## Protocol changes

`crates/protocol` is the source of truth for Rust, TypeScript (`--features ts`) and Dart. After changing it,
regenerate the Dart models and fixtures: `cargo run -p composer-protocol --features schema --bin dartgen`
(`-- --check` fails when they are stale), then `dart test` in `apps/composer_protocol`.

## Stock sites and templates

`seed/sites` and `templates/` are embedded in the binary: `composer init` installs them, `composer update`
brings an existing home up to date (three-way merge against the copies kept in `state/seed/`). Changing them
reaches users through `update`, so keep each change mergeable: small edits rather than rewrites.

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
wait turn 240
snap chat
```

Use a scratch `COMPOSER_HOME` (`composer init`, set another port in its config.toml) so ~/.composer stays untouched.
Set `EDITOR` to a non-interactive script to exercise the `e` (edit page) round trip headlessly.
Snapshots always force a draw, so they can't catch a missing repaint: for that, run
`scripts/check-tui-redraw.py target/debug/composer <COMPOSER_HOME>` (the real TUI in a pseudo-terminal).

## The Flutter app (`apps/composer_app`)

The phone client (Android; iOS later). The desktop is `apps/desktop` (gpui + CEF), its own Cargo
workspace; the Flutter app has no Linux, Windows or macOS target.

- Same Flux shape as the TUI: widgets dispatch `AppAction`s, `reduce` (pure) returns the next `AppState`
  plus `AppEffect`s, `EffectRunner` does the IO. Same file rule: a class with behaviour (widgets
  included) gets its own file; plain data types share `types.dart` / `actions.dart` / `effects.dart`.
- `flutter analyze` and `flutter test` (reducer, parsing). End to end on an Android emulator against a
  rehearsal server: `scripts/phone-e2e.sh <device> <workspace url> [test...]` pairs each test (a fresh
  install needs its own pairing code) and forwards the server's port with `adb reverse`; it sets up the
  scripted model and `tools` site the tests need. Tests: `pairing_test.dart` pairs through the UI,
  `chat_test.dart` runs a whole turn with a recipe approval, `reader_test.dart` covers pages, an app and
  follow-agent, `review_test.dart` merges a ticket from the palette, `measure_open_test.dart` measures
  opening a chat. A headless emulator:
  `emulator -avd <name> -read-only -no-window -no-audio`.
- `COMPOSER_APP_DATA=<dir>` keeps the server list in that directory (tests, a second instance).
- Icons: edit `assets/icon/*.svg`, then `scripts/app-icons.sh` renders the Android ones.
- Screenshots: `adb exec-out screencap -p > shot.png`.

## Seeing an app

Apps are served at `http://<bind>/<site>/?token=<state/token>&session=<id>` (`:app <site>` in a headless
script shows the URL in the status bar). Open that URL with Playwright to screenshot or drive the app.
The app template and sdk live in `templates/` and are embedded in the binary; `composer init` installs them.
