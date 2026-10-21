---
title: Production
type: stage
gate: false
harness: claude-code
worktree: true
---
# 04 · Production

## Inputs

| | file | for |
|---|---|---|
| working | `03_storyboard/storyboard.md` | what to build |
| reference | `02_script/script.md` | the words |

## Process

Build the video in the linked project (Remotion or whatever it uses), scene by scene from the storyboard,
in a ticket worktree. Render a preview.

## Outputs

- commits on the ticket's branch
- `04_production/notes.md`: how to render it, where the preview is, what differs from the storyboard
