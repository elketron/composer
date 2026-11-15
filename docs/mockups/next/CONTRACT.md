# Desktop reference contract

Proposal · 2026-11-14. Applies to the six desktop screens in this surface (01–03, 10–12);
the four phone screens extend it in [PHONE-CONTRACT.md](PHONE-CONTRACT.md).
Product context: [desktop.md](../../desktop.md). The native conversation is
not implemented in the shipped GPUI shell yet.

## Visual source

The user's three “Composer Desktop Refresh Lovable” screenshots, captured
2026-11-14 at 07:12:29, 07:12:49 and 07:12:36, are stored in `assets/` as
`reference-workspace.png`, `reference-sessions.png`, `reference-commands.png`.
The supplied Noema token values are the palette authority, not the archived
blue web/phone studies and not `templates/sdk/theme.css`.

## Shared shell

- Full-window, near-black desktop; 44px title bar and 26px bottom status bar.
- Warm gray text, subtle one-pixel borders, indigo focus. Corners are the phone's: cards and
  floating panes 5px, controls 4px, labels and keys 3px, status dots 2px; bars, tabs, the stage
  and list rows stay square.
- JetBrains Mono throughout, bundled locally for offline rendering. Chrome is
  11–12px; the conversation is 12–16px depending on window width.
- Chat occupies 34.7% of the workspace at wide sizes. The proposed production
  splitter is resizable; these static states use a fixed proportion. At compact
  desktop widths the chat grows to 46% for legibility. No mobile UI is implied.
- Title: workspace `demo`, agent `default`, session `#a3f1`, connected,
  following agent. Sessions and Commands buttons on the right.
- Chat header: `default #a3f1`, `slots 1/2`, site `3d`, project `crashed-ship`,
  capability `mcp blender`. No separate workflow selector/category.
- Stage tabs: `live · blender` (active), `mockups`, `terminal`, `+`; CEF label.
- Viewer breadcrumb: `wayai / live / blender`, Drive, `drive: you`. The viewport
  is a schematic Blender stand-in, not a screenshot of a running application:
  dotted grid, selected top bevel, XYZ legend, small bevel readout.
- A single bottom bar: `live · blender`, `1 waiting`, keyboard hints and
  `composer · gpui mockup`. Do not introduce another app sidebar.

## Canonical conversation

Synthetic fixture copied from the user's reference, not actual session data.
Message times are illustrative: the current protocol does not supply these
timestamps. Tool durations, slots, session previews and thread data are fixtures.

1. Event: `entered 3d · blender already running · drive: you`.
2. `you 13:56`: “What's ready?”
3. `default 13:56`: “0014 is ready to merge; the ship bevel is the only open thread.”
4. `you 14:02`: “Bevel the top edge: 0.03, three segments.” / “Keep the bottom
   sharp — it sits on the grid.”
5. `default 14:02`: “On it — top edges only; the base stays sharp.” Two completed
   tools: `open site://3d/crashed-ship/models/ship.glb` (0.2s) and
   `blender__execute_blender_code bevel(0.03, 3)` (2.1s).
6. Continuation: “Bevel is in. I left a thread on the top edge.” A single thread
   at `crashed-ship / top bevel`: “you: three segments reads softer from the
   default camera”.
7. Continuation: “Want me to export a fresh ship.glb for the site?” Inline
   `approve write` request for `site://3d/crashed-ship/models/ship.glb`, `1 file`,
   Deny and Allow. This is the **one** waiting item.

Composer stays pinned below the independently scrollable transcript: NORMAL,
`i` insert, `o` new line, Enter send, “Press i to write the review reply”,
Attach, Context, `site://3d`, Send. These hints follow the reference. The actual
modal behavior (i, Escape, Enter) is defined in desktop.md; this mockup does not
implement editing, sending or approvals.

## Session switcher

Float above the left transcript, 54px from the left window edge and 116px from
the top at the reference size, with width equal to the chat pane minus 108px.
Dim only the chat underneath, not the active stage or the floating pane. A 5px-cornered
panel, search field, four rows, keyboard footer. No persistent chat list.

| Agent/session | Status | Site/project | Preview |
|---|---|---|---|
| default #a3f1 | shown | 3d/crashed-ship | Bevel is in. I left a thread on the top edge. |
| review-0014 #9c02 | waiting | coding/composer | 4 files, 2 hunks need a reply. |
| plan-phase-8 #71be | idle | coding/composer | Drafted tui-mode.md, waiting on you. |
| ship-textures #e4d9 | idle | 3d/crashed-ship | Baked 3 maps · roughness looks off. |

The current row is selected. Only the current session can be opened in this
prototype; other rows are fixtures, not implemented conversation states.

## Command palette

Same anchor, width, scrim and search treatment as the switcher, with groups:

- SESSION: Switch session; New session.
- SITE: `:site coding` — leave 3d, keep the conversation;
  `:site 3d` — blender · mcp tools.
- STAGE: Open terminal tab; Open app in stage (`site://…`); Toggle split.

Toggle split is initially selected, matching the reference. Arrow keys change
selection; typing filters. Only Switch session navigates to a real mockup state;
all production actions are disabled and labelled as preview-only.

The reference's macOS shortcuts are replaced with the shipped Linux desktop's
`Ctrl Shift S` (sessions) and `Ctrl Shift P` (palette). New session/terminal/split
do not advertise shortcuts the production app does not currently register.

## Review

Screen 10. The session screen unchanged: same two panes, other session. Code review is not a
command in the chat and not a screen of its own: it is Hunk (`hunk diff <base>`, see
[tui-mode.md](../../tui-mode.md)) running in a terminal tab on the stage, opened from the run's
review page. Notes live in Hunk; the agent reads and answers them with the `hunk` tool.

- Session `review-0014 #9c02`, site `coding`, project `composer`, tool `hunk`, ticket `0014`.
- Stage tabs: `review · hunk` (active), `review.md`, `terminal`, `+`.
- The diff is a schematic Hunk stand-in (file list, unified diff, two notes: one resolved, one
  waiting on the user, key hints), not a capture of Hunk's UI.
- Conversation: the agent opened the review; the user left two notes; the agent answered and
  asks for a reply in Hunk or a verdict. The **one** waiting item is the verdict card
  (Changes / Pass). The verdict is a chat message (`pass`, `fail: <why>`) the coding site's
  user-review stage turns into `check.md`; the card only sends it (shipped 2026-11-15, no server
  change). The merge is the agent's `merge_ticket`, which the user approves as a normal approval.

## Workspace home

Screens 11 and 12 are outside the session: no chat and no stage. Reached from the workspace
name in the title bar (every session state links it), the palette's WORKSPACE group and the
switcher's “All sessions”. Esc or the title bar's `↩ default` returns to the last shown session.
Tabs: `sessions 4`, `projects 3`. Same title bar and status bar as the session screen.

- **Sessions (11):** NEEDS YOU (`default` approve write, `review-0014` review waiting) above
  RECENT (`plan-phase-8`, `ship-textures`), from the switcher fixture. The selected session's
  preview on the right: site/project, its waiting item, its last messages and its stage tabs,
  and Open session. `default` opens 01, `review-0014` opens 10; the others are preview-only.
- **Projects (12):** `composer` (coding, 0014 in review), `crashed-ship` (3d, 1 approval),
  `wayai` (coding, no sessions). The selected project: its site, tools and harness; tickets with
  their four stages (code, agent review, user review, merge) and a link to the review; its
  sessions; its pages; New session here and Terminal (disabled).

## Scope

No GPUI, server, TUI or Flutter behavior changes. No promise that the reference's
thread UI, timestamp data or native composer already ships. Stage art is inline
SVG so it remains inspectable and offline; it must not introduce a second palette.
