# Chat and artifacts

Status: idea, 2026-11-12. Not built.

The two poles:

- **Grok bot**: the conversation is the product. Output is text, ephemeral.
- **Claude artifacts**: the conversation has a live object beside it. The object
  is the point; the chat is how you steer it.

Composer already has the second half in **sites**: pages, micro apps, workflows,
threads, a shared view, and now a portable pack. What it does not have is the
first half: a site is a place you open, not something that can appear in the
conversation like an artifact.

The mesh is not a new artifact system. It is one rule:

> **A site is the artifact; the chat is its control surface.**

## One object, three levels of presence

| level | where | for |
|---|---|---|
| **card** | in the chat transcript | the agent just made or changed something; keep talking |
| **stage** | beside the chat | cowork: point, comment, watch, drive |
| **focus** | full screen, chat as a sheet | the artifact is the work; the chat is an aside |

Today the desktop is stage-first and the phone is a session screen. A card is
the missing first level. It makes the artifact as light as a Grok output while
keeping the site as the durable object.

```text
┌ chat ─────────────────────────────────────────────┐
│ default                                           │
│                                                   │
│ I made a dashboard for the release.               │
│ ┌ site · release-dashboard ─────────────── v2 ─┐  │
│ │ [ live preview / thumbnail ]                  │  │
│ │ draft · 3 comments                            │  │
│ │ [open]  [comment]  [save as site…]            │  │
│ └───────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────┘
```

- The card is a **link**, not a copy. The site is still the source of truth.
- **Open** puts it on the stage (desktop) or full screen (phone).
- **Comment** opens the same `Threads` panel anchored to the site.
- **Save as site** promotes a draft into a real site.
- The card survives in history; if a draft expires it says so and links the
  last saved version, if any.

## Drafts and saved sites

Not every artifact deserves a site. A chat should be able to make something in
ten seconds, iterate, and throw it away.

```text
site://drafts/<session>/<artifact>     hidden, session-scoped
site://<name>                          a saved site, in the workspace
```

- **Drafts** are created by `write`/`edit` under the drafts namespace. They are
  not in Home, not workflows, not searchable by default, and expire with the
  session (or after a few days).
- **Save as site** (or `promote`) copies the draft into
  `workspaces/<ws>/sites/<name>/`, adds the site to the workspace's index, and
  records the draft as its provenance. The user approves the name and the
  promotion; the agent does not silently create durable sites.
- A saved site can be opened, packed, installed, given a workflow, or made a
  project's app. A draft cannot.
- The draft still works like a site: the same `site://` addressing, the same
  SDK, the same server APIs, the same threads. The only difference is lifetime
  and visibility.

This keeps the chat fast without turning every sketch into site sprawl.

## The card is a protocol part

The agent already writes sites and can put pages/apps on the desk. The missing
piece is a message that says **"this is the artifact"**, not just "here is a
path".

A message can carry a site card:

```ts
type SiteCard = {
  url: string;            // site://drafts/... or site://...
  title: string;
  kind: "page" | "app" | "workflow" | "diff" | "view";
  mode: "draft" | "saved";
  version?: string;       // later: artifact versions
  openThreads?: number;
  preview?: string;       // a thumbnail or a render hint
};
```

- The tool can be `show_artifact` (a thin wrapper over what `show_app` and
  `show_page` already do), or a new message part on those tools. The important
  part is the **card**, not another tool.
- The card is rendered by every client in its own grammar:
  - **desktop (gpui)**: a flat rounded card in the transcript, with a live
    preview when the pane is cheap enough; open → the stage tab.
  - **phone (Flutter)**: a card with a thumbnail; tap → full-screen site, with
    the chat as a sheet.
  - **TUI**: a bordered card with the title, URL, mode and open count; `enter`
    opens it on the stage or in the browser.
- The same card is the entry to the **threads** panel. A thread is already the
  cowork primitive; the card is how a thread gets discovered from the chat.

## Why this is not just two panels

A chat beside a site is already the desktop today. The mesh is about the
**transition** between the two:

- The agent can **produce** an artifact in a turn and show it as a card without
  leaving the conversation.
- The user can **expand** the card into the stage, or **collapse** the stage
  back to the card, without losing the conversation.
- The artifact can **grow up**: draft → saved site → workflow → packed site →
  another PC. The same object, not a new format.
- The chat can **follow** the artifact: when the agent changes it, the card
  updates and the stage can follow; when the user comments, the thread is
  attached to the thing.
- The artifact has **threads**, not a separate comments system; the chat is the
  conversation, the threads are the anchored parts of it.

```text
chat-only (Grok)        cowork (Claude artifacts)       focus
┌──────────────┐        ┌──────────┬──────────────┐     ┌──────────────┐
│ chat + card  │  expand│ chat     │ site + thread│     │ site         │
│              │ ──────▶│          │              │ ───▶│ chat sheet   │
└──────────────┘        └──────────┴──────────────┘     └──────────────┘
```

## What already exists

- **Sites**: `site://` addressing, pages, `ui/` micro apps, `index.md`
  instructions, `justfile` actions, workflows, MCP.
- **The shared shell**: the SDK's `Threads` panel, pins, resolve, send to agent.
- **The stage / desk**: apps, pages, diffs, terminals, live view; the desktop
  and phone already show them.
- **Packs**: a site can be put on GitHub and installed elsewhere.
- **The agent's tools**: `write`, `edit`, `show_app`, `show_page`, `desk`,
  `enter_site`, `run`, `open`.

What is new is only the **card**, the **draft namespace**, and **promote**. It
is a thin layer, not a new client or a second artifact engine.

## The rule set

1. **The site is the artifact.** No parallel artifact storage, no separate
   preview document. A card points at a site.
2. **Drafts are ephemeral; saved sites are durable.** The agent may create
   drafts freely; saving is a user action.
3. **Cards are links.** They do not copy content and do not fork the site.
4. **Threads are the comments.** A card shows the open count; opening it opens
   the same thread panel.
5. **The chat never silently publishes.** Promote, pack and install are explicit
   user actions, with the same approval language as the rest of composer.
6. **The same object in every client.** The desktop card, the phone card and the
   TUI card are representations of one site; no client has a private artifact.

## Phases

1. **Card for existing sites.** Render a site card for `show_app`/`show_page`
   and `site://` links in a message; open/comment works in desktop, phone and
   TUI. No drafts yet.
2. **Drafts and promote.** `site://drafts/<session>/<artifact>`, hidden from
   Home, expiry, `save as site`, provenance. The card shows draft/saved.
3. **Focus and live previews.** Expand/collapse between chat, stage and focus;
   a keyboard chord for each; phone sheet. Preview only when cheap; thumbnail
   otherwise.
4. **Versions and sharing.** Draft versions and diff; promote a version; the
   site pack/install flow from [site-packs.md](site-packs.md).

## Open questions

1. **Draft scope.** Per session, per workspace, or per project? A draft made in
   one chat may be useful in another; a workspace-scoped draft with a short TTL
   is the compromise.
2. **Draft expiry.** Delete with the session, after N days, or never until the
   user cleans up? The card in old history needs a clear expired state.
3. **Auto-card.** Does every `write` to a draft produce a card, or only an
   explicit `show_artifact`? Auto-card is magical but noisy.
4. **Live preview vs thumbnail.** A live site in a chat card is a webview per
   card; the desktop can afford one, the phone maybe not. Start with a
   thumbnail and a single live card at a time?
5. **Versions.** Sites have no per-page history today. Is a draft version store
   enough, or should saved sites get the same?
6. **Threads on drafts.** A draft can have threads; when promoted, do the
   threads move with it? They are in the site folder, so yes, if the draft's
   `threads/` is copied.
7. **Multi-user.** The workspace is single-user. A shared/packed site is the
   first multi-user surface; do cards need a "who is here" state?
8. **The TUI card.** A terminal cannot render a live artifact; a bordered card
   with a URL and a key is enough, or should the TUI keep the artifact on the
   stage only?

## Done when

A user can ask for a dashboard in a chat, the agent builds it as a draft and
shows a card, the user opens it beside the chat, comments on a chart, the agent
answers in the thread, the user saves it as a site, packs it, and installs it on
another PC — without any of those steps leaving the "one object" model.
