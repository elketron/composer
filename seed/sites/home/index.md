---
title: Home
type: index
tools: [new_site, new_app, new_pipeline, publish_site]
---
# Home

You work inside a **web of markdown pages**. Every top-level folder is a **site**: an `index.md` homepage, its pages, optional actions (`justfile`) and an optional app (`ui/`). **Open a site's index before you write to it.**

## Sites are capabilities

Outside a site you have only the **base tools**: browse (`open`, `search`, `list_sites`), edit (`write`, `edit`), `set_project`, `actions`/`run`, `spawn_worker`/`continue_worker`/`stop_worker`/`answer_approval`, the desk (`show_app`, `show_page`, `desk`, `selection`), the user's terminals (`terminal_read`, `terminal_send`), `watch`.

A site's `index.md` declares `tools:` — verbs that exist only inside it. `enter_site("projects")` puts that page in your context in place of this one and adds its tools from your next turn; `leave_site` returns.

- `enter_site("home")` adds `new_site`, `new_app`, `new_pipeline` and `publish_site`.
- `enter_site("projects")` adds `new_project`.
- `enter_site("orchestration")` adds `new_work`, `run_stage` and `merge_ticket`.

**If you need a verb that isn't in your base set, enter the site that owns it** instead of looking for a general command.

Agent pages ([agents](site://agents)) set the base `tools:`; a site adds the rest. Creating a capability means adding a site tool or a `just` action, not growing the harness.

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
| [orchestration](site://orchestration) | tickets and pipeline work handed to worker agents. |
| [agents](site://agents) | agent definitions (yours included) and their base tools. |
| [chat](site://chat) | the chat client app. |

Other sites may exist; `list_sites` shows them all.

## How to work

- **Browse, don't guess.** `open` a page to read it, `search` to find pages (`type:ticket status:open`, `site:memory`, or plain text).
- **Pages are the source of truth.** If something should outlive this conversation, write it to a page.
- **Frontmatter matters.** Pages start with YAML frontmatter (`title`, `type`, `tags`, ...). Keep it accurate; search uses it.
- **Sites have actions.** A site with a `justfile` has recipes; `actions(site)` lists them, `run` runs them. The first run of a recipe needs the user's approval.
- **Sites can have apps.** A site with a `ui/` folder has a human-facing app. Apps store their state as files in the site, so you can read what the user did there. Messages from an app arrive in your chat marked with the app's site. Comment threads in an app's view are pages in its `threads/` folder: see [threads](site://home/threads.md).
- **A project is the hub.** A project page names its `root:` folder and the `sites:` that operate on it. A **work** is one instance of a site on a project; its `project/` view is what the work reads and writes. Start at [projects](site://projects).
- **Some sites are pipelines** (`type: pipeline`): staged work, a folder per piece of work, each stage with a contract in `stages/`. `new_pipeline` makes one from a template; [orchestration](site://orchestration) starts and merges work.
- **Edit, don't rewrite.** Use `edit` for changes to existing pages; `write` for new pages.
