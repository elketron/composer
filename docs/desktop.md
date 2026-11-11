# The desktop app (Linux)

One window per workspace: the chat on the left is your composer TUI (the server starts it for
the window), the stage on the right shows the session's desk as tabs: apps, pages, a project's
changes, terminals, and the live view of the workflow's application. The agent puts things there
(`show_app`, `show_page`, `desk`); you open them too.
It talks to a composer server, never runs one: start `composer serve` first.

## Install

From a release: unpack `composer-desktop-<version>-linux-x86_64.tar.gz`, run `./install.sh`
(installs to `~/.local/lib/composer-desktop`, links `~/.local/bin/composer-desktop`, adds the
menu entry). Keep the folder's files together: the Chromium runtime (CEF) is beside the binary.

From source:

```sh
cd apps/desktop
scripts/fetch-cef.sh          # the CEF runtime, once: ~/.local/share/composer/cef (1.5 GB)
cargo run --release           # or scripts/package-desktop.sh <version> from the repo root
```

Run `scripts/fetch-cef.sh` before the first build: without it the `cef` crate downloads its own
copy into the build folder.

## Run

```sh
composer-desktop                  # the `default` workspace
composer-desktop --workspace lab  # another one (or ctrl+shift+w once it runs)
```

It finds the server the way the CLI does: a paired server (`composer login`), else the home's
(`$COMPOSER_HOME` or `~/.composer`): its `[server] bind` and the workspace's token.

## Keys

The desktop takes only these; everything else goes to the focused pane (the TUI keeps its
keymap, nvim its own).

| Key | |
|---|---|
| `ctrl+1` / `ctrl+2` | focus the chat / the stage |
| `ctrl+tab` / `ctrl+shift+tab` | next / previous stage tab |
| `ctrl+shift+p` | palette: commands, sessions, apps, pages, workflows, launcher entries, attach a file |
| `ctrl+shift+s` | switch session (the chat and the stage follow) |
| `ctrl+shift+w` | switch workspace (a window each) |
| `ctrl+alt+1..9` | the nth workspace's window |
| `ctrl+shift+c` / `ctrl+shift+v` | copy / paste in a terminal (select by dragging; middle click pastes the selection) |

In the TUI, Enter is a new line while you type; send with Escape, then Enter.

## What a pane may do

A browser pane loads only the workspace server's origin (scheme, host and port compared exactly, no
userinfo), `about:blank`, and in subframes `about:srcdoc` and same-origin `blob:`. Every other
main-frame or subframe navigation is cancelled, whether a click, a script, a meta refresh or a redirect;
a link the user clicked to http(s) or mail opens in their own browser instead. Downloads, camera,
microphone and other permission prompts, client certificates, HTTP auth, OS protocol handlers and
JavaScript dialogs are refused or dismissed. The token is not in a pane's URL: before a view loads,
the desktop stores the server's `composer_token` cookie in CEF for the workspace's path.

## The live view and Drive

Entering a workflow whose application runs (the palette's `workflow: <name>`, `:workflow` in the TUI,
or the agent's `enter_site`) opens a `live · <application>` tab on the stage: wayai's full viewer
page through the workspace (`<workspace>/view`), with its toolbar and annotations. When the window
speaks for the machine's own user (the workspace's local token, which the desktop reads from the
home), the page also has a **Drive** button: click it and the mouse, wheel and keyboard go to the
application until you click Stop driving. A window on a paired server (`composer login`) has no
Drive button: only the local token may drive. Leaving a workflow keeps the tab; `leave workflow` in
the palette does it, and `workflow: <name>` switches. Each answers with a notification saying what
happened (the application started or was already running, or why not).

The tab's pane passes on everything Drive needs: all three buttons (press, move and release
separately, also when a drag leaves the pane), the wheel, modifier keys held on their own (shift-drag,
ctrl-click), and key repeats marked as repeats. Hovering the stage gives it the keyboard like the
other tabs. What cannot reach the application: the desktop's own keys above (`ctrl+1`, `ctrl+2`,
`ctrl+tab`, `ctrl+shift+tab`, `ctrl+shift+p`, `ctrl+shift+w`, `ctrl+shift+s`, `ctrl+alt+1..9`; the
application still sees the bare modifiers of such a chord pressed and released); anything the window
manager takes first (the super key, `alt+tab`); and keys outside a US layout, the numpad, caps lock,
menu, print screen, pause and `f13` and up, which the pane does not translate.

## What it does for you

- **Follow agent** (top right): what the agent opens comes to the front; off, it arrives as a
  tab with a dot.
- **Terminals** (`+`): a shell in the session's project, sandboxed like the agents; they live on
  the server (closing the desktop doesn't end them; idle ones end after a day and reopen where
  they were).
- **Links** to the workspace's pages open as tabs; links elsewhere open in your browser.
- **Files** dropped on the chat become attachments; dropped on a terminal, their paths.
- **Notifications** when the agent finishes or an approval waits while the window is behind.
- The workspace's colour (a stripe and its name) says which one you're in: set it with
  `[desktop] color = "#rrggbb"` in the workspace's `config.toml`.

## Troubleshooting

| | |
|---|---|
| `no token in …/state/token` | the server hasn't run that workspace yet: `composer serve` |
| `CEF didn't start` | its messages are above; from source, run `scripts/fetch-cef.sh` |
| `cargo run` dies with SIGTRAP at start-up | a half-downloaded `.cef`: run `scripts/fetch-cef.sh` (the app now warns and falls back instead) |
| a page stays blank | `COMPOSER_CEF_GPU=1` tries the GPU (software rendering is the default) |
| slow to start | `COMPOSER_DESKTOP_TRACE=1` prints where the time goes |
| a page misbehaves | `COMPOSER_CEF_DEBUG_PORT=9229`, then Chromium's DevTools on that port; `COMPOSER_CEF_LOG=1` for Chromium's log |
| exits with SIGKILL / "Forced quit" | check `journalctl -u systemd-oomd --since today` and `journalctl --user --since today --grep='oom-kill'`; the Nov 8 hidden-pane frame leak is fixed by coalescing pending frames and reporting hidden tabs/windows to CEF. GPU-adapter warnings alone do not diagnose the kill. |

### Frame-retention regression test

From `apps/desktop`, `cargo test --all-targets` covers the frame mailbox, image/atlas retirement,
popup clearing, hidden CEF callbacks and tab/window visibility. The regression initially failed with
1,000 images retained after 1,000 unpainted frames. The replacement holds at most the latest pending
frame and the last presented image per view, and creates images only in the canvas paint callback.
A full-HD test feeds 300 frames (~2.3 GiB of pixels) without drawing and pins retained pixels to
two frames (15.8 MiB). The CEF callback's own latest-frame mailbox is also bounded to one frame.

For an actual CEF + GPUI soak, without connecting to a server or touching a Composer home:

```sh
cd apps/desktop
COMPOSER_CEF_GPU=1 cargo run --example frame_soak
```

This opens an animated canvas for four 15-second phases: drawn normally, not drawn while CEF is
deliberately allowed to paint, hidden via `was_hidden`, then drawn again. It exits after a minute.
Watch process-tree PSS/RSS (not just one renderer); it should plateau rather than retain every
frame. On Nov 8 the fixed GPU-enabled run exited normally, with sampled process-tree PSS around
491–521 MiB and no swap. This is a short targeted regression check, not proof that every source of
long-running memory growth is eliminated. Unset `COMPOSER_CEF_GPU` to exercise software compositing.

Windows (with the server in WSL) comes later: see [wsl.md](wsl.md).
