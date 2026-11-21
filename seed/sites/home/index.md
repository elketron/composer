---
title: Home
type: index
tools: [new_site]
---
# Home

You work in a **web of markdown pages**. Every top-level folder is a **site**: an `index.md`
homepage, its pages, and optionally actions (`justfile`), MCP servers (`mcp.toml`) and a folder
outside the web it works on (`folder:`). **Open a site's index before you write to it.**

## Addresses

- `site://<site>/<path>`, e.g. `site://memory/user.md`
- `site://<site>` is the site's `index.md`; `site://plans/x` finds `x.md` or `x/index.md`.
- Relative links resolve within the current site. Links work across sites.

## Sites

| site | for |
|------|-----|
| [memory](site://memory) | what you know: the user, topics, feedback. Read before you start, write as you learn. |
| [plans](site://plans) | longer-lived intent: goals and the steps to reach them. |
| [icm](site://icm) | how to build a workflow as folders: the ICM method and its templates. |

Other sites may exist; `list_sites` shows them all.

## Setting a site

`set_site("<name>")` puts the site's `index.md` in your context from the next turn, in place of
this page. It adds the tools its `tools:` lists (Home's is `new_site`) and the tools of the MCP
servers in its `mcp.toml`, once the user approved that file. `set_site(null)` leaves the site and
keeps the conversation.

A site's frontmatter can name `folder: ~/Projects/x`, a folder outside the web the site works on.
It counts once the user approved that folder for the site (`set_site` asks them; changing
`folder:` asks again). Then terminals and background processes start there, recipes get it as
`COMPOSER_FOLDER`, and the changes view shows its diff. Until then the site works as if it named no
folder; the `# Folder` section of your prompt says which.

## Kinds of run

Every run is one of three kinds. The `# Environment` section of your prompt says which. A run is
started by the user, by a webhook (`[hooks.<name>]` in `config.toml`, called at
`POST /api/hooks/<name>`), or by another run with `spawn_run`. A webhook's run is kept as a
session only when the hook says `store = true`; a single shot or workflow run from `spawn_run` is
not kept.

| kind | tools | ends |
|------|-------|------|
| **single shot** | none: one model call, the workflow folder's instructions as the whole prompt | with the answer |
| **workflow** | `open`, `search`, `list_sites`, `set_site`, `finish`, and MCP tools marked `read_only` | when you call `finish(value)` |
| **agentic** | every tool below; not `finish` | when you stop; the user follows it as a session |

A **workflow folder** is a folder inside a site with its own `CONTEXT.md` (or `index.md`): the
instructions for one kind of job. A webhook or `spawn_run` names it with `site` and `folder`.

## Tools in an agentic run

- **Pages:** `open`, `search` (`type:plan status:active`, `site:memory`, plain words), `list_sites`,
  `write` (new pages, whole pages), `edit` (part of a page).
- **Sites:** `set_site`, and `new_site` while in Home.
- **Actions:** `actions(site)` lists a site's `justfile` recipes, `run` runs one in the site's
  folder in the web. The first run of a recipe, and the first after it changes, waits for the
  user's approval.
- **Other runs:** `spawn_run(kind, prompt, site?, folder?)`. A single shot or workflow run returns
  its result at once. An agentic run starts a new session and returns its id; its last answer is
  posted to you when it ends (`wait: true` waits for it instead). `harness` runs it on an outside
  agent.
- **Terminals:** the user opens terminals on the session's stage. `desk` lists them,
  `terminal_read` reads one, `terminal_send` types a command into one (the user approves it).
- **Background processes:** `process_start(command)` runs a long command (a dev server, a watcher)
  in its own terminal; you get a message with its exit code when it ends. `process_list` and
  `process_stop` manage them. The user approves each command, or says always for it.
- **The stage:** `show_page` puts a page next to the chat; `desk` with `changes: true` shows the
  changes in the site's folder.
- **Waiting:** `watch` sends you a message when files in a site change.
- **MCP:** servers in `config.toml` are there in every session; a site's `mcp.toml`
  (`[servers.<name>]`) adds its own. Calls outside a server's `allow` list ask the user first.

## How to work

- **Browse, don't guess.** `open` a page to read it, `search` to find pages.
- **Pages are the source of truth.** What should outlive this conversation goes into a page.
- **Frontmatter matters.** Pages start with YAML frontmatter (`title`, `type`, `tags`, ...). Keep
  it accurate; search uses it.
- **Edit, don't rewrite.** Use `edit` for changes to existing pages, `write` for new ones.
- **A project is a site.** Give it `folder:` for its code, keep its notes in its pages, and put
  its workflows in folders inside it.
