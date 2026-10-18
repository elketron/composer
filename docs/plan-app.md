# composer app — plan (phase 2)

Status: draft. Phase 1 (TUI, server, micro apps, remote access) is done; see [plan.md](plan.md).

## Goal

A desktop and mobile app (Flutter) that is a graphical client of one or more composer servers:
the chat, the pages, and the micro apps in an embedded browser. No terminal is assumed.

- **Platforms, in order:** Linux desktop (home, Hyprland/Wayland), Android, Windows (work PC; its server runs in WSL). macOS builds where it costs nothing extra.
- **Several servers:** home computer and work PC, switchable, all kept connected.
- **Same look:** the tokens from `templates/sdk/theme.css` (dark, one accent, radius 2–5px, Inter + JetBrains Mono).
- **First version is chat-first:** servers, sessions, chat, approvals, reading pages, micro apps. Editing pages, the changes view and a command palette come after.
- **Input on desktop:** conventional (text fields, clicking) plus shortcuts mirroring the TUI where natural (ctrl-enter sends, ctrl-k palette later).
- **Notifications** without a push service: the app keeps its connections open (desktop; Android via a foreground service) and shows local notifications.
- **The phone web app (`site://chat`) stays** as the no-install fallback.
- **Distribution:** installable packages built by GitHub Actions on release tags.

## Architecture

```
apps/composer_protocol/       Dart package generated from crates/protocol (do not edit)
apps/composer_app/            Flutter app
  lib/flux/                   Store: action → reduce → (state, effects), as in the TUI
  lib/state/                  AppState, actions, effects, the reducer (pure, unit-tested)
  lib/servers/                effects: server store, pairing, one connection per server
  lib/ui/  lib/theme/         widgets (read state, dispatch actions); tokens + theme
  (later: lib/chat/  lib/pages/  lib/apps/)
```

### Servers and connections

- A server is `{name, url, device token}`; the token lives in platform secure storage (libsecret on Linux, Keystore on Android, Credential Manager on Windows). On a Linux desktop without a Secret Service (e.g. KWallet with its Secret Service API off) the list goes to a 0600 file instead, as `composer login` does, and the switcher says so.
- **Adding one:** scan the QR from `composer pair` (Android camera) or enter URL + code (desktop) → `POST /api/pair` (exists).
- **One WebSocket per server, always open** (reconnecting, `since`-replay on attach as the TUI does). Approval requests and worker / turn events from every server feed one notification stream, wherever you are in the app.
- New server endpoint: `GET /api/info` → server name (hostname), version, model, slots; shown in the server switcher.

### Protocol in Dart

Rust stays the source of truth. `crates/protocol` gets a `schema` feature (`schemars`) that writes JSON Schema; a script generates Dart models (`json_serializable`) into `lib/protocol/`. CI fails if the generated files are out of date, so Rust, TypeScript and Dart can't drift.

### Embedded browser (micro apps)

One `AppView` interface, a backend per platform:

| platform | backend |
|----------|---------|
| Android, Windows, macOS | `flutter_inappwebview` (in-window, stable) |
| Linux | `flutter_inappwebview` 6.2 beta (WPE WebKit as a Flutter texture), with the patched `apps/vendor/flutter_inappwebview_linux` (WPE 2.54 build fix; frames imported on Flutter's EGL display). Fallbacks: software rendering, then the system browser. See [the spike](spikes/webview.md). |

Apps open as `https://<server>/<site>/?token=<device token>&session=<id>`: the existing cookie flow signs the webview in, so apps, `send()` and live updates work unchanged. Cookie stores are per host, so servers don't mix.

### Replacing the terminal

| TUI | app |
|-----|-----|
| nvim edit round trip | in-app markdown editor, same `PUT` with hash check (later) |
| `gd` changes view | native diff viewer on `/project/diff`; "view file" instead of opening an editor (later) |
| `f` link hints | tappable / clickable links in rendered markdown |
| pickers, `:` commands | session/site lists now; command palette (ctrl-k) later |
| approval popup | native sheet / dialog, also as a notification |

## The work PC: server in WSL

The work PC runs `composer serve` inside WSL2, so the server stays Linux-only; only the app runs natively on Windows.

- **Tailscale:** either on the Windows host (WSL2 forwards `localhost`, so `tailscale serve --bg http://127.0.0.1:7420` on Windows reaches the server in WSL), or inside WSL as its own tailnet node. Both work; the host install is simpler if Tailscale is already there.
- **Opening apps from the TUI in WSL:** `xdg-open` has no Windows browser; set `BROWSER=wslview` (from `wslu`), which `composer` already honours.
- **Projects:** keep checkouts inside the WSL filesystem (not `/mnt/c`), or git (changes view) and file watching get slow.
- Nothing else changes: `just`, `bun`, `git`, `nvim` are the Linux ones.

## CI/CD (GitHub Actions)

- **`ci.yml`** on push / PR:
  - Rust: `fmt --check`, `clippy -D warnings`, tests on `ubuntu-latest` (the server runs on Linux, incl. WSL).
  - Templates: build the app template and the seed apps with bun (catches sdk breakage).
  - Flutter: `analyze`, `test`, generated-protocol up-to-date check.
- **`release.yml`** on `v*` tags → a GitHub release with:
  - `composer` binary: Linux x86_64 (tar.gz; also what WSL uses).
  - App: Android APK (signed with a keystore from repository secrets), Linux AppImage, Windows installer (Inno Setup .exe; MSIX later if signing is set up).

## Milestones

1. ✅ **Webview spike** — in-window `flutter_inappwebview` works on Linux (with two patches) and Android: cookie sign-in, live WebSocket, `send()`, input, Flutter drawing on top. [Findings](spikes/webview.md).
2. ✅ **Protocol in Dart + `/api/info`** — `apps/composer_protocol` (plain Dart package) generated by `dartgen` from the protocol's JSON Schema: classes, enums, sealed classes for tagged enums (with an `…Unknown` variant), plus Rust-written fixtures that the Dart tests round-trip; `--check` fails when stale. `GET /api/info` → name (`server.name` or hostname), version, model, slots.
3. ✅ **App shell + servers** — theme and bundled fonts, Flux store, server list (keyring or 0600 file), pairing by QR (Android) or by pasting the link / `composer login` line / address + code, one reconnecting WebSocket per server (backoff to 30s, retried on resume), switcher (rail on desktop with ctrl-1…9, sheet on phones), server page with info and sessions. Verified on Linux (integration test pairing against a real server in headless Weston) and the Android emulator.
4. ✅ **Chat** — session tree (workers nested; in the desktop rail, the page on phones), chat (markdown, collapsible tool/thinking/compaction rows, streaming with a caret, replay on attach and `since` catch-up on reconnect), composer (ctrl-enter sends; queues while busy), stop, approvals from every server (a bar on every screen, a sheet, inline cards), failed commands as snackbars. Also "New session" (default agent), pulled forward from milestone 8. Verified by `integration_test/chat_test.dart` (a real turn with an approval) and on the Android emulator.
5. ✅ **Pages + apps** — a reader per server (beside the chat on desktop, its own screen on phones): markdown with links resolved like `SiteUrl::join`, frontmatter, backlinks, site images, back/forward, reload when the page changes on disk, follow-agent (live navigation of the open session only, not replays). Site list on the server page; `site://` links and `open` tool rows in the chat open the reader. Micro apps in `AppView` (`flutter_inappwebview`, patched Linux backend), signed in through `?token=` with the open session. Verified by `integration_test/reader_test.dart` and on the Android emulator; Windows not tried yet.
6. **Notifications** — approvals, worker finished, turn done; desktop notifications, Android foreground service.
7. **CI/CD + WSL notes** — `ci.yml`, `release.yml`, packaging; the WSL setup documented and tried on the work PC.
8. **Later** — page editor, diff viewer, command palette, new session / compact / project from the app.

## Open questions

- Report the two Linux plugin fixes upstream (they'd let us drop the vendored copy).
- Release signing: Android keystore now; Windows code signing (and MSIX) only if SmartScreen warnings bother you.
