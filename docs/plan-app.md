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
apps/composer_app/            Flutter app (this repo, next to crates/)
  lib/protocol/               generated from crates/protocol (do not edit)
  lib/servers/                server list, pairing, one connection per server
  lib/chat/  lib/pages/  lib/apps/  lib/theme/
```

### Servers and connections

- A server is `{name, url, device token}`; the token lives in platform secure storage (libsecret on Linux, Keystore on Android, Credential Manager on Windows).
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
| Linux | `flutter_inappwebview` 6.2 beta (WPE WebKit as a Flutter texture) **if the spike shows it holds up**; else a separate window (`desktop_webview_window`), which tiles fine on Hyprland; else the system browser |

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

1. **Webview spike** — `flutter_inappwebview` 6.2 beta on Linux (Hyprland/Wayland) vs a separate window, and on Android; with the real ticket board and chat app: cookie sign-in, `send()`, live updates, input. Decide the Linux backend.
2. **Protocol in Dart + `/api/info`** — schema export, Dart codegen, up-to-date check.
3. **App shell + servers** — theme, server list in secure storage, pairing (QR on Android, code on desktop), a connection per server, switcher.
4. **Chat** — session tree (workers nested), chat (markdown, collapsed tool/thinking rows, streaming), composer, cancel, approval sheet.
5. **Pages + apps** — page reader with links and follow-agent, apps in the `AppView`.
6. **Notifications** — approvals, worker finished, turn done; desktop notifications, Android foreground service.
7. **CI/CD + WSL notes** — `ci.yml`, `release.yml`, packaging; the WSL setup documented and tried on the work PC.
8. **Later** — page editor, diff viewer, command palette, new session / compact / project from the app.

## Open questions

- Linux webview backend: decided by milestone 1.
- Release signing: Android keystore now; Windows code signing (and MSIX) only if SmartScreen warnings bother you.
