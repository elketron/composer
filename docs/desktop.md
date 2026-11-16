# The desktop app

One window per workspace: the chat on the left is a native conversation of the shown session,
the stage on the right shows the session's desk as tabs: apps, pages, a project's
changes, terminals, and the live view of a site's application. The agent puts things there
(`show_app`, `show_page`, `desk`); you open them too.
It talks to a composer server, never runs one: start `composer serve` first.

The proposed visual refinements in [mockups/next/](mockups/next/) restart the desktop
study from the user's three references and supplied Noema theme: near-black surfaces,
warm gray monospace type, restrained indigo, a compact transcript and floating pickers. The shell follows them
(2026-11-14): Noema colours and the phone's small corners, the title and status bars, and the
floating, grouped palette and session switcher. Since 2026-11-15 the chat is native (below).

### The native chat

The desktop gets a native, desktop-first GPUI conversation, approvals and composer on
the left; it no longer embeds the TUI as the chat. The session screen stays two panes, chat
left and stage right, and does not change shape. Code review is not a chat command: it runs
on the stage as Hunk in a terminal tab (see [tui-mode.md](tui-mode.md)), and merging is the
agent's `merge_ticket`, which you approve. Sessions and projects get workspace screens of their
own outside the session (studies 10–12). Session switching and
command palette open in floating panes over the left transcript, not a permanent
chat sidebar. Present messages as a compact, near-black monospace transcript
with subtle speaker labels and vertical rules, not chat bubbles. Group tool
activity inline and show thread callouts and approvals where they arise, with a
single window status bar along the bottom. This visual direction follows the
user's reference (the fresh `01`–`03` studies); the workbench remains resizable, and the
reference's separate "workflow" label is not part of the model. The study
illustrates a 34.7% chat pane to give the stage ~2/3 of a wide window; the shipped
shell still starts at 640px. Keep the existing resizable right-hand stage: CEF
app and wayai views, plus terminal tabs, continue to belong to the attached
session's desk. Sites remain sites; their apps and MCP servers are optional
capabilities.

The native composer retains the TUI's modal interaction: `i` enters insert mode,
Enter inserts a newline, Escape returns to normal mode, and Enter there sends.
An explicit send control is also shown for pointer use, and `ctrl+Enter` sends from either
mode. In NORMAL, `h` `l` `0` `$` `x` `a` `A` `I` work as in vim, `g` / `G` go to the top / end
of the transcript, `ctrl+c` stops a running turn (as does Stop).

What ships (2026-11-15): the chat opens on the session's last 60 messages
(`session.history`), attaches since the last of them, and streams what follows; "Earlier
messages" fetches older ones. `composer-client`'s `Chat` reduces the transcript (the TUI's
rules: tool calls, thinking, compaction, cancelled turns) and `Store` keeps every waiting
approval. Thinking folds to one line, consecutive tool calls share a box whose rows open to the
call's changes and result, and approvals are cards (Deny, Always when the tool offers a rule,
Allow; "show" opens what is asked). Files from Attach, dropped on the chat, or pasted
(`ctrl+v`: an image or copied files; text goes into the message) upload to the session and go
with the next message. Typing in INSERT is text input, so an input method's composition works
(underlined until it commits). The window starts on the latest of your sessions (a
new one when there is none); the switcher and "New session" (same agent and project) change
it. The window no longer asks for a TUI (`window.hello`): another client can't move it to a
session (`window.session`).

Markdown in the chat (2026-11-15): agent messages and your own render — headings, paragraphs,
lists (bullet, ordered from their own number, tasks), block quotes, tables, thematic breaks,
fenced code, emphasis, strong, strikethrough, inline code and links. Your own messages rendering
it is a desktop choice: the phone shows them plain (only its agent text is markdown). A table is
aligned mono text, so a link or image in a cell shows as its words and isn't clickable. A pure parser
(`apps/desktop/src/markdown.rs`, pulldown-cmark with tables, strikethrough and task lists) hands
the renderer blocks with styled runs as byte ranges on character boundaries; GPUI does the
wrapping, so the model never wraps text itself. An unclosed marker (mid-stream) stays literal
text, and an open fence is code so far. Type follows the mockups scale: the body stays Inter at
the size it had, headings step up (h1 +4px, h2 +3, h3 +2, h4–h6 +1), all of them SemiBold; strong
is SemiBold and emphasis the italic face (Inter's italic faces are bundled now, so the transcript
never falls back to a system font); line height is unchanged. A quote is a 2px left rule in
`--noema-border-2` with a 12px indent; lists hang-indent per level with the TUI's markers
(`• `, `N. `, `[x] `, `[ ] `; the marker dim, a checked box green); a table is mono with padded
columns and a SemiBold header; a rule is a full-width dim line; code keeps the block look it
had. Images stay text: the alt and where it points. Links are live: a `site://` address opens as
a stage tab, an outside address goes to the OS, and what cannot resolve says so in the link rail
— a message has no page of its own, so a relative link only resolves when the session belongs to
a site.

### The workspace home

`ctrl+shift+h`, the workspace's name in the title bar, the palette's Sessions overview and
Projects, or the switcher's "All sessions…" open the home (studies 11, 12) in place of the chat
and the stage; Esc, or "back to the session", returns. **sessions**: NEEDS YOU (an approval
waits) above RECENT, each with its state, last words, site/project and age; the selected one's
waiting approvals (answerable there), last messages and stage tabs, and Open session.
**projects** (`GET /api/projects`): sessions, running ones and approvals per project; the
selected one's branch, sites, harnesses, uncommitted files, ticket branches, pages and
sessions, and New session here. Keys: `↑↓`/`j k` move, Enter opens (a project: its latest
session, else a new one there), `1` / `2` / Tab switch tabs, `n` new session.
### The user review (site://coding)

The coding site's user review runs on the stage: the agent opens the run's `review.md`, a
`type: tui` page that starts `hunk diff <base>` in a terminal tab; you leave notes in Hunk
(`c`), the agent reads them with the `hunk` tool. Your verdict is a chat message, `pass` or
`fail: <why>`, from which the agent writes `check.md`; the merge is `merge_ticket`, approved
like any approval. While such a review is open (the agent opened a page whose frontmatter says `type: tui` and
`control: hunk`, and you haven't said `pass` or `fail` since), the chat shows a verdict card: **Pass** sends `pass`,
**Changes…** starts `fail: ` in the composer for the why. Nothing on the server: it is only
the message. A verdict you send goes into the chat's outbox (`outbox.rs`) at once: the card
shows it given (`✓ pass`, no buttons) until the message comes back in the transcript, which
during a running turn is when that turn ends, so a second click sends nothing. If the send
fails, the entry goes and the card offers its buttons again.

The 2026-11-14 reset preserves the previous studies, including the phone and TUI
explorations, in [mockups/next-archive-2026-11-14/](mockups/next-archive-2026-11-14/).
Only the new desktop study is current. Its [content contract](mockups/next/CONTRACT.md)
distinguishes fixture timestamps, threads and schematic live-view art from shipped
behavior; its [decision log](mockups/next/DECISIONS.md) records the reset. The
mockup is pending visual review, not authorization to change production UI.

## Install

From a release: unpack `composer-desktop-<version>-linux-x86_64.tar.gz`, run `./install.sh`
(installs to `~/.local/lib/composer-desktop`, links `~/.local/bin/composer-desktop`, adds the
menu entry). Keep the folder's files together: the Chromium runtime (CEF) is beside the binary.

On Windows, unpack `composer-desktop-<version>-windows-x86_64.zip` and run
`composer_desktop.exe`. The server still runs on Linux (for example in WSL); see the archive's
README for pointing `COMPOSER_HOME` at its workspace. Keep the CEF files together.

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

The desktop takes only these; everything else goes to the focused pane (the chat its modal
keys, nvim its own).

| Key | |
|---|---|
| `ctrl+1` / `ctrl+2` | focus the chat / the stage |
| `ctrl+tab` / `ctrl+shift+tab` | next / previous stage tab |
| `ctrl+shift+p` | palette: commands (new session, set project, model, compact, attach a file, follow), sessions, apps, pages, sites, launcher entries |
| `ctrl+shift+s` | switch session (the chat and the stage follow) |
| `ctrl+shift+h` | the workspace home: its sessions and projects |
| `ctrl+shift+w` | switch workspace (a window each) |
| `ctrl+alt+1..9` | the nth workspace's window |
| `ctrl+shift+c` / `ctrl+shift+v` | copy / paste in a terminal (select by dragging; middle click pastes the selection) |

In the chat, Enter is a new line while you type (INSERT); send with Escape, then Enter, or
`ctrl+Enter`. `ctrl+v` pastes into the message.

## What a pane may do

A browser pane loads only the workspace server's origin (scheme, host and port compared exactly, no
userinfo), `about:blank`, and in subframes `about:srcdoc` and same-origin `blob:`. Every other
main-frame or subframe navigation is cancelled, whether a click, a script, a meta refresh or a redirect;
a link the user clicked to http(s) or mail opens in their own browser instead. Downloads, camera,
microphone and other permission prompts, client certificates, HTTP auth, OS protocol handlers and
JavaScript dialogs are refused or dismissed. The token is not in a pane's URL: before a view loads,
the desktop stores the server's `composer_token` cookie in CEF for the workspace's path.

## The live view and Drive

Entering a site whose application runs (the palette's `site: <name>`, `:site` in the TUI,
or the agent's `enter_site`) opens a `live · <application>` tab on the stage: wayai's full viewer
page through the workspace (`<workspace>/view`), with its toolbar and annotations. When the window
speaks for the machine's own user (the workspace's local token, which the desktop reads from the
home), the page also has a **Drive** button: click it and the mouse, wheel and keyboard go to the
application until you click Stop driving. A window on a paired server (`composer login`) has no
Drive button: only the local token may drive. Leaving a site keeps the tab; `leave site` in
the palette does it, and `site: <name>` switches. Each answers with a notification saying what
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
- The workspace's colour (the dot before its name in the title bar) says which one you're in:
  set it with `[desktop] color = "#rrggbb"` in the workspace's `config.toml`.
- **The bars:** the title bar shows the workspace, the shown session and project, the link and
  follow, with Sessions and Commands buttons; the status bar shows what is live, how many
  approvals wait, the workers, and the palette and session keys.
- **The palette and session switcher** float over the chat (it dims; the stage doesn't), grouped
  while nothing is typed, sessions with what they last said and `! waiting` when one asks.

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

For the server in WSL, see [wsl.md](wsl.md).
