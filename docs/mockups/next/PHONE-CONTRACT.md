# Phone — the app as it ships, plus landscape

Proposal · 2026-11-14. Screens 04–09. The phone keeps its current look: these screens are drawn
from `apps/composer_app` itself; only the colours change, to the desktop's Noema palette. Only
landscape (08) and Hide chat (06, 09) are proposals; everything else is what the app does today.

## Sources

- **Colours:** [`phone-tokens.css`](phone-tokens.css) keeps the app's colour roles from
  `apps/composer_app/lib/theme/tokens.dart` (bg, surface, raised, overlay, you, border, text,
  muted, faint, accent, ok, warn, err, info) and fills each with a Noema value; `qa/check.py`
  checks every role is there and points at `tokens.css`. Links and the site chip take the light
  indigo; filled controls (Approve, New chat, send) the deep indigo with bright text, as the
  desktop's Allow. The demo workspace's strip is the same violet as its desktop dot.
  phone.css itself only uses the roles.
- **Type:** Inter for UI and prose, JetBrains Mono for names, ids, commands and labels, both
  bundled. Sizes and spacing follow the widgets; each block in [`phone.css`](phone.css) names
  the widget it follows (`SessionCard`, `ApprovalTile`, `ApprovalCard`, `UserBubble`, `StepsView`,
  `MessageComposer`, `SiteRow`, `LiveViewHandle`, `PhoneTabs`, the site sheet).
- **Checked against renders of the real widgets** (theme, fonts, reducer, real fixtures),
  2026-11-14. Corners: cards and sheets 5, controls 4, labels 3, dots 2; the live view and the
  bottom bar are square.

## Fixture

Same sessions as the desktop. Approvals on the phone are what the server asks today: a
**recipe** run (`3d › export ship.glb` with its command), not the desktop study's
“approve write”. There are no threads on the phone.

| Session | Card | Where |
|---|---|---|
| review-0014 #9c02 | unread, `new · 6m` (accent) | Needs you |
| default #a3f1 | `needs approval` (warn dot with halo) | Recent; its approval tile is in Needs you |
| plan-phase-8 #71be | `3h` | Recent |
| ship-textures #e4d9 | `5h` | Recent |

## 04 — Inbox

App bar: the workspace switcher (`● demo`, `ts`, ⌄) and the workspace's 2px strip. NEEDS YOU
with its count `2`: the approval tile (`default #a3f1 wants to run`, `3d › export ship.glb`,
Approve · Deny · View) and the unread review-0014 card. RECENT: the other three cards (agent,
id, state or age, one-line preview, project chip and site tag). New chat floats bottom right.
Tabs: `inbox · 2`, sites, pages, server. The default card and View open 05.

## 05 — Conversation

App bar: back, `default`, the activity line `needs approval · #a3f1`, the menu; the strip.
Site row: the `3d` chip (opens 07), the project `crashed-ship`, the model at the right. The live
view of blender at its small size, nothing over the picture, then the handle:
`● blender · live`, the grip, `view only` (a tap steps the size: 06). Newest first: the approval
card (APPROVAL · #a3f1, Run recipe?, the command, the note, approve | deny), the agent's reply,
`▸ 2 steps`, the earlier messages with your own as right-aligned bubbles. The composer:
attach, `message the agent`, send.

## 06 — Larger live view

05 with the live view at its large size, capped so the chat keeps its room. The handle collapses
it again (05).

## 07 — Site sheet

05 under the Material barrier, with the site sheet: drag handle, `Site`, `3d 3D` checked
(`app blender · mcp blender`), `coding Coding`, `Leave the site` in red. Tapping the barrier
or Esc closes it.

## 08 — Landscape (proposal)

**Today the app stacks the portrait column in landscape:** app bar, site row, live view and
handle take nearly all the height and the chat gets a sliver. The proposal:

- Chat on the left (min 330px, 44%), live view on the right at full height, as on desktop.
- One 48px row on top of the chat: back, title and activity line, the site chip, the menu
  (the site row folds into it; project and model live in the sheet and the menu).
- The handle sits at the bottom of the live view: `● blender · live`, `view only`,
  **Hide chat**, and **›**, which folds the live view into a 52px rail
  (`blender · live` vertical, ‹ brings it back). Folded, the chat takes the width.
- Everything keeps its portrait size: 15px prose, 44px targets, the same cards.

The same layout applies to every chat screen once the phone is wider than tall and under
500px high (05, 06, 09 included).

## 09 — Hide chat (proposal)

The live view takes the whole body; the handle says **Show chat**, which brings the conversation
back at the same scroll position (in production: and the same draft). Not leaving the session.
In landscape the chat column goes and the view takes the full width; folding the view while the
chat is hidden brings the chat back.

## Seen in the app while drawing these

- The site chip was a 44px-high target but only as wide as its label (37px for `3d`):
  `SiteRow` set a minimum height only. Fixed in the app (44×44 minimum).

## Status

Implemented in `apps/composer_app` (2026-11-15): the Noema colours (`tokens.dart`, with
`accentFill` and `onAccent` for filled controls), landscape (08) and Hide chat (09), as drawn.
In the mockups, approve, deny, send, attach, the menu and the other tabs are visual. Blender is
a schematic picture inside the live view, coloured as content.
