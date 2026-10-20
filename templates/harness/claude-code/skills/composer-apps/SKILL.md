---
name: composer-apps
description: Use when a ticket asks to build or change a composer micro app (a site with a Svelte UI). Explains where apps live and how they are built.
---

# Composer micro apps

A micro app is a site with a small Svelte 5 UI, served by composer at `/<site>/`. Its source is
in the site's `ui/` folder (`ui/src/App.svelte`), next to the site's pages; it imports composer's
theme, components and server client from `$sdk`.

- Read the site's `index.md` first (`open site://<site>`): it lists the app's pages and actions.
- Build with the site's `ui-build` action through the composer `run` tool, not with your own
  commands; an action needs the user's approval the first time.
- Keep the app keyboard-driven and in composer's style.
- Data belongs in the site's pages and files, not inside the app: the app shows and edits them.
- The site's folder is not your working folder. If the ticket needs changes to app source you
  can't reach, say so in the ticket's Result instead of guessing.
