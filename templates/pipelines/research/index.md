---
title: Research
type: pipeline
needs_project: false
---
# Research

Answering a question properly: question → sources → synthesis → write-up, with a look from you at the
question and the synthesis.

Each question is a folder here (`site://__PIPELINE__/<work>/`). Sources are pages (one per source in
`02_sources/`), so they are searchable and linkable like any page.

## Stages

| stage | does | gate |
|-------|------|------|
| [01 question](stages/01_question.md) | sharpens the question: scope, what would answer it | yes |
| [02 sources](stages/02_sources.md) | finds and summarises sources, one page each | no |
| [03 synthesis](stages/03_synthesis.md) | what the sources say together, and where they disagree | yes |
| [04 write-up](stages/04_write-up.md) | the answer, for its reader | no |

## Layout

| path | holds |
|------|-------|
| `_config/` | `sources.md` (what counts as a source), `style.md` (how write-ups read) |
| `stages/` | the stage contracts |
| `<work>/index.md` | the question: its brief and stages |
| `<work>/NN_stage/` | what the stage wrote, and `_stage.md` (its state) |

## Works
