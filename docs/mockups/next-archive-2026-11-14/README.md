# composer next — platform studies for the shared view

Status: proposal, 2026-11-11. Static HTML + CSS; no build, no framework, no
remote resources.

This surface has two layers and a newer desktop direction:

- **06–09, the platform studies** — the implementation direction. The desktop
  shell is gpui (`apps/desktop`); its revised screens propose cool-blue focus, JetBrains
  Mono, a two-pane workbench and a native overlay. The phone is Flutter
  (`apps/composer_app`), so its screens use Inter, 44px targets, 56px bars and
  bottom sheets.
- **01–05, the single-chrome study** — the first pass, which put one dense web
  chrome on every client. Kept as a reference for the shared-view ideas, not as
  the desktop or phone layout.
- **10–12, the native desktop direction** — compact near-black monospace
  GPUI conversation without the embedded TUI or a permanent session sidebar,
  retaining the CEF/terminal stage and adding floating session and command overlays.

Both layers use the same palette (`tokens.css`, copied from
`templates/sdk/theme.css` where the web SDK uses it): a chat beside the thing
being worked on, with comments anchored to the thing itself. Sites may supply
apps and MCP tools; there is no separate workflow category.

The older boards (`docs/mockups/app.html`, `desktop.html`, `apps.html`,
`tui.html`, `pipelines.html`) remain as design history.

## Platform studies — the implementation direction

| # | screen | shows | source |
|---|---|---|---|
| 06 | [Desktop — the GPUI workbench](screens/06-desktop-gpui.html) | active stage tab, square message bands, resizable two-pane shell | `apps/desktop/src/shell.rs`, `term_pane.rs` |
| 07 | [Desktop — the GPUI picker](screens/07-desktop-gpui-palette.html) | proposed grouped sites, apps, pages and commands over the stage | `apps/desktop/src/picker.rs`, `pick_lists.rs` |
| 08 | [Phone — the touch-first session](screens/08-phone-touch.html) | Inter 15, 44px controls, 52px workflow row, live-view handle, approval in place | `apps/composer_app/lib/theme`, `ui/chat` |
| 09 | [Phone — the workflow sheet](screens/09-phone-workflow-sheet.html) | a native bottom sheet with 56px rows and the consequence of the switch | `ui/chat/workflow_sheet.dart`, `site.set` |
| 10 | [Desktop — native conversation](screens/10-desktop-native-chat.html) | compact monospace transcript, insert-mode composer, bottom status bar, wide CEF/terminal stage | `docs/desktop.md`, `crates/protocol/src/client.rs`, `apps/desktop/src/shell.rs` |
| 11 | [Desktop — floating sessions](screens/11-desktop-session-pane.html) | session switching on demand over the chat without a permanent sidebar | `docs/desktop.md`, `apps/desktop/src/picker.rs`, `pick_lists.rs` |
| 12 | [Desktop — floating commands](screens/12-desktop-commands-pane.html) | grouped session, site and stage commands anchored over the transcript | `docs/desktop.md`, `apps/desktop/src/picker.rs`, `pick_lists.rs` |

## Single-chrome study — kept as a reference

| # | screen | shows | source |
|---|---|---|---|
| 01 | [Desktop — the live shared view](screens/01-desktop-live.html) | a workflow running Blender in wayai, chat beside it, drive and a thread pin | workflows.md, desktop.md |
| 02 | [Desktop — a view type with threads](screens/02-desktop-mockups.html) | the mockups view, two pins, a dragged region, the thread panel | workflows.md (phase 4), `templates/sdk/Threads.svelte` |
| 03 | [Desktop — the project hub](screens/03-desktop-project.html) | sites, works, the worktree preview, agent/app parity | project-hub.md |
| 04 | [Phone — check in, approve, watch](screens/04-phone-session.html) | one session, view-only live view, newest-first chat, approval in place | workflows.md (phase 3), plan-app.md |
| 05 | [TUI — the chat and the hunk beside it](screens/05-tui-workflow.html) | workflow switch, the artifact, notes on a hunk, square chrome | tui-mode.md, plan.md |

The [index](index.html) is the board: platform studies first, the single-chrome
pass after, and the token story underneath.

## The two grammars

`tokens.css` is the palette for the web SDK and the single-chrome study. The
platform studies add their own contracts beside it:

- **`gpui.css`** — from the actual shell: `BG #0c0c0f`, `LINE #25252e`,
  `TEXT #d7d7dd`, `MUTED #85858f`, `ACTIVE_BG #1c1c24`, focus accent
  `#e0a040`; JetBrains Mono 14/18 in the terminal, 12px chrome, `rounded_md`
  (6px) for the picker. Two panes, one 4px splitter, a CEF stage. No web layout
  engine, no blur, no CSS-only effects.
- **`phone.css`** — from Flutter: Inter for the UI and JetBrains Mono for ids
  and URLs; 15px body, 11–12px metadata, 44px touch targets, 56px app bar and
  bottom bar, 52px workflow row, 44px live-view handle, 4px controls and 5px
  cards, native bottom sheets.

- The live view is the application itself. On a paired device it is view-only;
  `drive: you` on the desktop is the local token.
- A thread is one gesture: pin a card, drag a region, note a hunk. The page is
  stored beside the work, so the agent reads it with `open` / `search`.

## View it

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/mockups/next
# http://127.0.0.1:8765/
```

Each screen is also self-contained: open its file directly. Below 1360 px a
small script scales the whole frame to the viewport, so the composition is
never clipped; the page stays readable and the `notes` keep their size.

## Add a screen

1. Copy the closest screen; keep the masthead, `.screen`, `.fit` wrapper, the
   platform frame, `.notes` and `.docfoot` structure.
2. Put the screen's CSS in its own trailing `<style>` under a `.sN` root class.
   Do not edit `base.css`, `gpui.css` or `phone.css` for one screen.
3. Content comes from the contract the inventory names, not invented copy.
   Ids, times and counts must agree with the sibling screens.
4. Link it from `index.html`, add a row above, and run the checks.
5. If the screen settles a decision the contract did not say, write it into the
   contract. Until then, the mockup runs ahead of shipped code.

## Check it

```sh
python3 docs/mockups/next/qa/check.py
```

It checks the shared shell, palette-only screen CSS, local links, that the
colour and radius values in `tokens.css` still match `templates/sdk/theme.css`,
and that the index links every screen. The render pass is the other half:

```sh
PLAYWRIGHT_MODULE=/usr/lib/node_modules/playwright/index.js \
CHROME=/usr/bin/chromium \
node docs/mockups/next/qa/render.mjs
```

It screenshots every screen at 1440, 1100 and 820 into
`/tmp/composer-next/`, and fails on horizontal overflow or a frame that did not
fit. Open the images and look: a check can pass while a screen is sparse, ugly
or off-register with its siblings.

## Decisions recorded here (2026-11-11)

- The desktop shell is **gpui**, not a web app: two panes, the real TUI on the
  left, a CEF stage on the right. Screens 06–07 are the design direction; their
  proposed blue focus replaces the shipped shell's amber accent.
- The phone is **Flutter**: Inter 15px, 44px targets, 56px bars, 5px cards,
  bottom sheets. Screen 08–09 are the implementation direction.
- The single-chrome web shell in 01–05 is a reference, not the desktop or phone
  layout. It stays because the shared-view ideas (workflow chip, thread pins,
  project hub) are still the model.
- The shared view is the **stage**: a real application, a view type, or a diff,
  with the chat beside it.
- Threads are the one gesture across a mockup, a live view and a hunk. The
  anchor is a path; the thread is a page beside the work.

## Desktop refinement (2026-11-13)

- Study 06 keeps the shipped 640px starting chat width and proposes square
  conversation bands and a cool-blue active-tab edge in place of amber. These
  visual refinements are not yet implemented.
- Study 07 proposes clearer picker rows, grouping and a keyboard-hint footer.
  The shipped picker remains a flat list; the proposal uses sites and apps,
  not a special workflow category. The user should review the rendered screens
  before production changes.
- The 2026-11-13 feedback prefers a darker, cooler look over orange. These two
  studies use the existing blue palette accent; warm colours remain for warnings.
- The next desktop is a native conversation, not a TUI in a window. Screen 10
  proposes native messages, floating session controls and a modal multiline composer;
  the right-hand stage remains CEF apps, wayai and terminal tabs. None of the
  native chat is shipped yet; see the contract in `docs/desktop.md`.
- Follow-up feedback: no permanent chat list and no message bubbles. Screen 10
  uses speaker labels and spacing; screen 11 shows the session list as a
  temporary floating pane over the conversation.
- The 2026-11-14 reference favours a near-black, compact monospace workbench:
  subtle green and violet speaker rules, inline tool, thread and approval cards,
  a single bottom status bar, floating session/command overlays over the left
  transcript, and a wider stage. The reference's separate workflow label is
  intentionally omitted in favour of sites and MCP capabilities (`site` + `mcp`).
  Screens 10–12 show message timestamps for visual rhythm even though the
  current chat message protocol does not provide them yet.
