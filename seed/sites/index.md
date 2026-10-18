---
title: Home
type: index
---
# Home

You work inside a **web of markdown pages**. Every top-level folder is a **site**; every site has an `index.md` homepage that explains what the site is for and its conventions. **Open a site's index before you write to it.**

## Addresses

- `site://<site>/<path>` — e.g. `site://memory/projects/composer.md`
- `site://<site>` — the site's `index.md`
- Relative links resolve within the current site. Links work across sites.

## Sites

| site | for |
|------|-----|
| [memory](site://memory) | what you know: the user, projects, topics, lessons. Read before you start, write as you learn. |
| [plans](site://plans) | longer-lived intent: goals and the steps to reach them. |
| [orchestration](site://orchestration) | tickets: units of work handed to worker agents. |
| [agents](site://agents) | agent definitions (yours included). |

Other sites may exist; `list_sites` shows them all.

## How to work

- **Browse, don't guess.** `open` a page to read it, `search` to find pages (`type:ticket status:open`, `site:memory`, or plain text).
- **Pages are the source of truth.** If something should outlive this conversation, write it to a page.
- **Frontmatter matters.** Pages start with YAML frontmatter (`title`, `type`, `tags`, ...). Keep it accurate; search uses it.
- **Sites have actions.** A site with a `justfile` has recipes; `actions(site)` lists them, `run` runs them. The first run of a recipe needs the user's approval.
- **Sites can have apps.** A site with a `ui/` folder has a human-facing app. Apps store their state as files in the site, so you can read what the user did there. Messages from an app arrive in your chat marked with the app's site.
- **Edit, don't rewrite.** Use `edit` for changes to existing pages; `write` for new pages.
