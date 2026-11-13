---
title: Home
type: index
tools: [new_site, new_app, publish_site]
---
# Home

You work inside a **web of markdown pages**. Every top-level folder is a **site**: an `index.md`
homepage, its pages, optional actions (`justfile`) and an optional app (`ui/`). **Open a site's index
before you write to it.**

## Sites are capabilities

Outside a site you have only the **base tools**: browse (`open`, `search`, `list_sites`), edit
(`write`, `edit`), `set_project`, `actions`/`run`, and the user's terminals. A site's `index.md`
declares `tools:` — verbs that exist only inside it. Entering a site puts that page in your context
and adds its tools from the next turn; leaving returns.

- `enter_site("home")` adds `new_site`, `new_app` and `publish_site`.
- `enter_site("projects")` adds `new_project`.
- `enter_site("orchestration")` adds `merge_ticket`.

**If you need a verb that isn't in your base set, enter the site that owns it.**

## Workflows

A workflow is a site with `app:` (an application in wayai) and/or `mcp:` (server tools). Entering it
puts its page in the prompt and grants its tools; leaving drops them and keeps the conversation.
`:workflow <name>` or `enter_site` switches.

## Addresses

- `site://<site>/<path>` — e.g. `site://memory/user.md`
- `site://<site>` — the site's `index.md`
- Relative links resolve within the current site. Links work across sites.

## Sites

| site | for |
|------|-----|
| [memory](site://memory) | what you know: the user, projects, topics, lessons. Read before you start, write as you learn. |
| [projects](site://projects) | the hub: a folder, a page, and the sites that operate on it. |
| [plans](site://plans) | longer-lived intent: goals and the steps to reach them. |
| [orchestration](site://orchestration) | tickets and work handed to workers. |
| [agents](site://agents) | agent definitions. |
| [chat](site://chat) | the chat client app. |

Other sites may exist; `list_sites` shows them all.

## How to work

- **Browse, don't guess.** `open` a page to read it, `search` to find pages (`type:ticket status:open`, `site:memory`, or plain text).
- **Pages are the source of truth.** If something should outlive this conversation, write it to a page.
- **Frontmatter matters.** Pages start with YAML frontmatter (`title`, `type`, `tags`, ...). Keep it accurate; search uses it.
- **Sites have actions.** A site with a `justfile` has recipes; `actions(site)` lists them, `run` runs them. The first run of a recipe needs the user's approval.
- **Sites can have apps.** A site with a `ui/` folder has a human-facing app. Apps store their state as files in the site, so you can read what the user did there. Comment threads in an app's view are pages in its `threads/` folder: see [threads](site://home/threads.md).
- **A project is the hub.** A project page names its `root:` folder and the `sites:` that operate on it. A **work** is one instance of a site on a project; its `project/` view is what the work reads and writes.
- **Edit, don't rewrite.** Use `edit` for changes to existing pages; `write` for new pages.
