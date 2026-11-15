# Composer — Noema desktop and phone studies

Proposal · 2026-11-14 · **12 screens** (6 desktop, 6 phone) · mockups only, not shipped UI.

This surface starts again from the user's three Composer Desktop Refresh
screenshots and supplied Noema theme. The previous twelve-screen study,
including work in progress, is preserved in
[`../next-archive-2026-11-14/`](../next-archive-2026-11-14/README.md).
It is design history, not another current direction.

## Inventory

| Screen | Shows | Content contract |
|---|---|---|
| [01 — Workspace](screens/01-workspace.html) | Native conversation, tools, thread, approval, composer and live stage | [CONTRACT.md](CONTRACT.md) |
| [02 — Sessions](screens/02-sessions.html) | Temporary session switcher above the left transcript | [CONTRACT.md](CONTRACT.md#session-switcher) |
| [03 — Commands](screens/03-commands.html) | Grouped command palette in the same position | [CONTRACT.md](CONTRACT.md#command-palette) |
| [04 — Phone inbox](screens/04-phone-inbox.html) | The app's inbox: approval tile and unread session under Needs you, recent cards, New chat | [PHONE-CONTRACT.md](PHONE-CONTRACT.md#04--inbox) |
| [05 — Phone conversation](screens/05-phone-chat.html) | The app's chat page: site row, small live view and handle, recipe approval, newest first (the app now has the newest at the bottom) | [PHONE-CONTRACT.md](PHONE-CONTRACT.md#05--conversation) |
| [06 — Phone live view](screens/06-phone-live.html) | The live view at its large size; Hide chat (proposal) | [PHONE-CONTRACT.md](PHONE-CONTRACT.md#06--larger-live-view) |
| [07 — Phone site sheet](screens/07-phone-site.html) | The app's site sheet over the chat | [PHONE-CONTRACT.md](PHONE-CONTRACT.md#07--site-sheet) |
| [08 — Phone landscape](screens/08-phone-landscape.html) | Proposal: chat left with a one-row header, live view right at full height, fold to a rail | [PHONE-CONTRACT.md](PHONE-CONTRACT.md#08--landscape-proposal) |
| [09 — Phone chat hidden](screens/09-phone-focus.html) | Proposal: the live view takes the screen; Show chat restores it | [PHONE-CONTRACT.md](PHONE-CONTRACT.md#09--hide-chat-proposal) |
| [10 — Review](screens/10-desktop-review.html) | The same two panes: Hunk on the stage, notes and verdict in the chat | [CONTRACT.md](CONTRACT.md#review) |
| [11 — Home](screens/11-desktop-home.html) | Workspace screen outside the session: sessions, needing you first, with a preview | [CONTRACT.md](CONTRACT.md#workspace-home) |
| [12 — Projects](screens/12-desktop-projects.html) | Projects, their tickets' four stages, sessions and pages | [CONTRACT.md](CONTRACT.md#workspace-home) |

Start at [index.html](index.html). Screen pages are edge-to-edge app mockups;
the index carries the design commentary instead of adding chrome to the app.

## Files and rules

- `tokens.css`: the supplied Noema palette, verbatim values. No coupling to
  the web SDK theme, no Tailwind/build step, no remote resources.
- `base.css`: one shared desktop shell and its components.
- `phone-tokens.css`: the phone app's colour roles (`tokens.dart`), filled with Noema values.
- `phone.css`: the phone as it ships, each block following one Flutter widget; landscape is the proposal.
- `screens/01-workspace.html`: template. Session states 01–03 share one `.workbench`
  shell; only overlay state differs. 10 keeps the same two panes with another session;
  11–12 are workspace screens outside the session (`.home`), with the same title and status bars. `screens/05-phone-chat.html`
  is the phone conversation template; live and sheet states retain its shell.
  Screen-only CSS belongs in a trailing
  style block scoped to the screen's root class.
- `CONTRACT.md`: canonical scenario, copy, layout and honest implementation gaps.
- `PHONE-CONTRACT.md`: what each phone screen shows and which widget it follows.
- `DECISIONS.md`: dated decisions. These are proposals until visually approved.
- `assets/`: local JetBrains Mono and Inter fonts/licenses, and the three original references.
- `prototype.js`: navigation, local filtering and keyboard focus only. No server
  calls; actions that would change real data are explicitly disabled.
- `phone-prototype.js`: Hide/Show chat, the landscape rail, and sheet focus/dismissal;
  never sends messages, approvals or site changes.
- `inventory.js`: scales live screen previews on the inventory; original reference
  images are linked separately so they are never confused with the mockups.
- `qa/`: static consistency/link checks and Chromium rendering/interaction checks.

On desktop, keep the phone's small corners (5/4/3/2px; bars and the stage square), hairlines, restrained indigo and low-contrast
surfaces: no message bubbles or permanent session sidebar. The phone is drawn
from the Flutter app as it ships, with only its colours changed to Noema: Inter prose, mono names, its
cards, tinted user bubbles, 2–5px corners and the newest message at the bottom of the chat. Change it only where a
screen is marked as a proposal (landscape, Hide chat). No gradients or extra decorative navigation. On desktop the stage is the largest region. On
phone the view-only live section sits above its handle and steps between sizes.
In landscape keep chat left and live view right, as on desktop.
Keep 44px touch targets; no desktop modal key hints on phones. Site capabilities
replace the reference's separate workflow label.

To add a screen: extend the contract, copy the workspace shell, reuse the same
scenario, add an inventory entry here and in `index.html`, update the screen
count, record decisions, then run both checks and inspect the screenshots.

## Preview and verify

```sh
python3 -m http.server 8766 --bind 127.0.0.1 --directory docs
# http://127.0.0.1:8766/mockups/next/
python3 docs/mockups/next/qa/check.py
PLAYWRIGHT_MODULE=/usr/lib/node_modules/playwright/index.js CHROME=/usr/bin/chromium node docs/mockups/next/qa/render.mjs
```

Render sizes: 2000×1246 (reference), 1440×1000 and 820×900 (compact desktop,
not a phone layout). Screenshots go to `/tmp/opencode/composer-next/`.
Phone sizes: 390×844, 360×740, and 820×900 (centered preview).
Landscape phone sizes: 844×390, 740×360, and 932×430. The landscape study
also opens as a centered wide canvas on desktop; tap Collapse to compare its
full-width conversation state.
Open them and check the composition; passing automated checks is not approval.
Serve the `docs/` parent so links to the archived study and desktop contract also work.

The desktop screen buttons and Escape navigate between mockup states. The switcher and
palette can be filtered and traversed with the arrow keys. This is not a working
Composer client. Approval, Drive, attachments and send are visual states only.
On phone, open default from the inbox, expand/collapse its live view, and tap the
site row to open the sheet. Close or Escape dismisses it; keyboard focus stays
inside the sheet. Production actions remain disabled.
The live viewer also offers Hide chat / Show chat in portrait and landscape.
Hiding chat preserves the conversation and restores its scroll position; it is
not leaving the session. The editable draft itself is outside this static prototype.
