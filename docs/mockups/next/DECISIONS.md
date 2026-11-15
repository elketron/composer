# Design decisions

| Date | Decision | Status |
|---|---|---|
| 2026-11-14 | Restart `next/` as a desktop-only, three-state study; preserve the entire previous surface at `next-archive-2026-11-14/`, including uncommitted work. | Requested by user |
| 2026-11-14 | The three supplied screenshots define composition; the supplied Noema tokens define colors. Native chat left, larger live stage right, temporary overlays rather than a session sidebar. | Proposal awaiting visual review |
| 2026-11-14 | Bundle JetBrains Mono and reference images locally. No remote fonts, framework or build step. | Mockup implementation |
| 2026-11-14 | Use sites and MCP capabilities, not the screenshot's older workflow category. Show existing Linux desktop shortcuts rather than suggesting macOS-only bindings ship. | Product consistency |
| 2026-11-14 | Dim the conversation only beneath overlays; keep stage visible. Share one shell and one scenario across the three pages. | Proposal awaiting visual review |
| 2026-11-14 | Native conversation, timestamps, inline thread/approval treatment and schematic Blender canvas are illustrative, not claims about shipped functionality. | Scope boundary |
| 2026-11-14 | Extend Noema to four phone studies: inbox, conversation, expanded live view and site sheet. Share exact desktop tokens and JetBrains Mono, but retain 44px targets, native page navigation and bottom sheets. No Vim key hints on phone. | Proposal awaiting visual review |
| 2026-11-14 | The paired phone's live viewer is view-only. No Drive control, even though the desktop fixture has a local driving token. | Product consistency |
| 2026-11-14 | Add landscape phone conversation: compact top rows, 34/66 live/chat split, inline approval actions and a one-row composer. Collapsing live returns chat to the entire width. Keep type and touch targets, rather than scaling the portrait layout down. | Proposal awaiting visual review |
| 2026-11-14 | Add Hide chat / Show chat to the phone live viewer in portrait and landscape. Focus mode expands the view without leaving the session; restoring chat retains scroll position and must retain drafts in production. Collapsing live always restores chat. | User-requested capability; visual treatment pending review |
| 2026-11-14 | User correction: landscape chat belongs on the left, live on the right. Preserve the current Flutter phone's Inter prose, message/card separation, tinted user bubbles, small radii and newest-first order. Apply Noema colors without transplanting the desktop's mono/rule transcript. Supersedes the earlier phone mono/square and reversed-pane proposals. | Requested by user; revised mockups pending review |
| 2026-11-14 | Remove the redundant Inbox heading from the phone inbox; the selected bottom tab already names the screen. Keep session counts and content unchanged. | Requested by user |
| 2026-11-14 | Desktop prose (message text, composer placeholder, session previews) uses Inter, as the references do; speakers, tools, threads, approvals, paths and chrome stay JetBrains Mono. Same split as the phone. | Fidelity correction; pending review |
| 2026-11-14 | The desktop chat is native and desktop-first, as in the studies; it no longer embeds the TUI. The two-pane session screen (chat left, stage right) does not change. | Requested by user |
| 2026-11-14 | Code review happens on the stage: Hunk in a terminal tab, notes in Hunk, read by the agent with the `hunk` tool. No `:merge`/`:discard` chat commands; merging is `merge_ticket`, approved by the user. Study 10 shows it with an unchanged session screen. | Requested by user; verdict card is a proposal |
| 2026-11-14 | Sessions and projects get workspace screens outside the session (11 home, 12 projects), reached from the workspace name, the palette and the switcher; Esc returns to the session. The switcher stays the quick path. | Proposal awaiting visual review |
| 2026-11-14 | User correction: the phone mockups follow the current app's look, colours included. Drawn from renders of the real Flutter widgets; `phone-tokens.css` copies `tokens.dart` and QA compares them. No Noema on the phone. Supersedes the Noema phone studies. | Requested by user |
| 2026-11-14 | Phone fixtures follow the app: a recipe approval (`3d › export ship.glb`), no threads, an unread session under Needs you, live view above its handle with sizes instead of collapse. | Product consistency |
| 2026-11-14 | Landscape (proposal): the app today stacks the portrait column and leaves the chat a sliver. Chat left with one 48px header row (site chip folded in), live view right at full height with its handle at the bottom; › folds it into a 52px rail. Applies to every chat screen in landscape under 500px high. | Requested by user; pending review |
| 2026-11-14 | Desktop corners follow the phone (tokens.dart): cards and floating panes 5px, controls 4px, labels and keys 3px, status dots 2px. Title bar, status bar, tabs, stage and list rows stay square. Supersedes the square desktop. | Requested by user |
| 2026-11-14 | The phone keeps the app's layout, type and widgets; only its colours change to Noema, through the app's colour roles (`phone-tokens.css`). Supersedes the same-day decision to keep the app's own palette. | Requested by user |
| 2026-11-14 | Implemented: phone colours (Noema roles in `tokens.dart`, filled controls deep indigo), landscape side by side with the rail, Hide chat, the 44px-wide site chip; desktop shell colours and corners, title and status bars, floating grouped palette and switcher, approvals counted in the client store. The native desktop chat, home and projects screens are not yet built. | Shipped in code, pending review |
| 2026-11-15 | Implemented: the native desktop chat (01: transcript, streaming, folded thinking, grouped tool rows, approval cards, modal composer with IME, attachments by picker, drop and paste), the workspace home (11: sessions, 12: projects) and the palette's project, model and compact commands. The window no longer embeds the TUI. The review verdict card (10) stays a proposal. | Shipped in code, pending review |
| 2026-11-15 | The review verdict card (10) ships client-side: while the agent has opened a Hunk review page (frontmatter `type: tui`, `control: hunk`) and no `pass`/`fail` has been said, the chat offers Pass (sends `pass`) and Changes… (`fail: ` in the composer). The coding site's user-review stage takes the message as the verdict; no server change. | Shipped in code, pending review |

## Verification · 2026-11-14

- Static checks: identical workbench shells, local links and assets, all 30 supplied
  color values, token-only styling, inventory and fixture counts pass.
- Chromium: all three screens and the inventory render at 2000×1246, 1440×1000
  and 820×900 without horizontal page/overlay-row overflow or offscreen status bars.
- Filtering, empty state, selection arrows, Enter navigation, Escape and the
  palette shortcut pass. Screenshots are under `/tmp/opencode/composer-next/`.
- Visual inspection caught a ship selection that sat outside its mesh outline,
  a stage drawing positioned too low, and undersized wide-screen metadata. These
  were corrected and the render pass rerun. Inventory previews now render actual
  pages rather than presenting reference images as mockups.
- Awaiting the user's visual review. No production UI change is approved by this pass.

## Phone verification · 2026-11-14

- Static checks cover all seven screens, shared desktop/phone conversation shells,
  local resources, exact Noema values, and four inbox sessions.
- Rendered the four phone states at 390×844, 360×740 and 820×900. Checks cover
  44px control dimensions, pinned composers, visible approval actions, scroll
  bounds, bottom-sheet focus and the absence of Drive controls. Existing desktop
  renders and keyboard tests remain in the same check suite.
- Visual inspection covered the inbox, latest conversation, expanded viewer and
  sheet. Reduced trailing message spacing so the latest-message anchor has a
  cleaner leading edge; corrected the workspace dot's overridden color.
- The first inventory render caught sheet autofocus being blocked inside a
  file-origin iframe. Sheet focus now runs only in a top-level preview, so live
  inventory thumbnails cannot steal focus from the board.
- Awaiting visual review; no Flutter theme changes have been made.

## Landscape and focus verification · 2026-11-14

- Added screens 08 (interactive landscape comparison) and 09 (portrait focus).
  The shared phone conversation shell and exact palette still pass static QA.
- Rendered actual rotated chat/live pages and both new states at 844×390,
  740×360 and 932×430, plus the existing portrait/desktop regression set.
- Checked 44px targets, full-width collapsed chat, separate live/chat bounds,
  visible approval controls, pinned composers, and the absence of Drive.
- Exercised Hide/Show chat in portrait and landscape, unchanged message DOM,
  exact scroll restoration, and collapsing live while chat is hidden. The static
  composer does not validate editable draft persistence; that remains a stated
  implementation requirement.
- The render pass caught a selector-precedence issue that kept landscape focus
  mode in its old column. Corrected it so the view fills the released space;
  reran all checks and inspected expanded, collapsed and focus screenshots.
- Mockup-only, awaiting visual review. No Flutter/GPUI implementation changes.

## Phone-style correction verification · 2026-11-14

- Compared the mockups with current Flutter `UserBubble`, `AgentText`,
  `StepsView`, `ApprovalCard`, `SessionCard`, theme tokens and `chat_order.dart`.
  Restored their typography and separation instead of the desktop transcript.
- Bundled Inter with its license. Phone prose/UI uses Inter, technical fields
  use JetBrains Mono, and the shared Noema color values are unchanged.
- Phone cards/user messages regain 4–5px radii; approval retains its warning
  border and separated approve/deny footer. Phone history is newest-first.
- Landscape renders and assertions now explicitly require chat/composer to the
  left of the live view. Existing collapse, focus and exact scroll-restoration
  tests still pass in both orientations.
- Reran the complete static/render/interaction suite and visually inspected
  portrait chat, inbox, short-screen live chat and landscape. Desktop markup
  and production app code are unchanged. Revised phone study awaits review.
