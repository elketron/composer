# The desktop app (Linux)

One window per workspace: the chat on the left is your composer TUI (the server starts it for
the window), the stage on the right shows the session's desk as tabs: apps, pages, a project's
changes, terminals. The agent puts things there (`show_app`, `show_page`); you open them too.
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
| `ctrl+shift+p` | palette: commands, sessions, apps, pages, launcher entries, attach a file |
| `ctrl+shift+s` | switch session (the chat and the stage follow) |
| `ctrl+shift+w` | switch workspace (a window each) |
| `ctrl+alt+1..9` | the nth workspace's window |
| `ctrl+shift+c` / `ctrl+shift+v` | copy / paste in a terminal (select by dragging; middle click pastes the selection) |

In the TUI, Enter is a new line while you type; send with Escape, then Enter.

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
| a page stays blank | `COMPOSER_CEF_GPU=1` tries the GPU (software rendering is the default) |
| slow to start | `COMPOSER_DESKTOP_TRACE=1` prints where the time goes |
| a page misbehaves | `COMPOSER_CEF_DEBUG_PORT=9229`, then Chromium's DevTools on that port; `COMPOSER_CEF_LOG=1` for Chromium's log |

Windows (with the server in WSL) comes later: see [wsl.md](wsl.md).
