---
title: Projects
type: index
tools: [new_project]
---
# Projects

The things you work on: a folder on disk and a page here. A project is the **hub**: sites
(coding, video, 3D, orchestration, ...) operate on it, and a **work** is one instance of a site on
a project. This page is the entry point; `new_project` here creates the folder, the page, its index
entry and its memory page.

## The project page

One page per project: `site://projects/<name>.md`.

```markdown
---
title: composer
type: project
root: ~/Projects/composer   # the folder; absolute, ~, or relative to [projects] dir
sites: [coding, orchestration]
tags: [rust, tui]
---
# composer

The project's memory: what it is, decisions, current state. Link plans and memory.
```

- `root:` is the one folder; every spelling (a name, a path, the page) resolves to it.
- `sites:` lists the sites that apply to this project; their apps appear on the project's hub.
- The body is the project's living note. Keep it current; don't duplicate [memory](site://memory).
- The old `site://memory/projects/<name>.md` page still reads as a fallback, but new projects
  live here.

## Works

A work is one run of a site on a project — a coding run, a video edit, a stage of a pipeline. It
lives in that site (`site://orchestration/<work>/`, a pipeline's folder), not here.

Wherever the work runs, it sees the project through its own `project/` link:

| mode | `project/` is | writes |
|------|----------------|--------|
| `read` | the project folder | refused |
| `write` | the project folder | the project |
| `worktree` | a git worktree on the work's branch | the worktree |

Reads always resolve through the link. Writes follow the mode, so a work can't damage the project
by accident. Apps read and write the same view with `projectFile(path, work)`.

## The hub app

A project's page is also a hub: a `ui/` app in this site lists its sites and works, with each
site's own app. The Flutter client's project screen uses the same data (`Project.page`, `.sites`).

## Projects

| project | root | sites |
|---------|------|-------|
