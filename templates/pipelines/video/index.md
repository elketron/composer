---
title: Video
type: pipeline
needs_project: false
---
# Video

Short videos from an idea: research → script → storyboard → production, with a look from you at the script
and the storyboard.

Each video is a folder here (`site://__PIPELINE__/<work>/`). The words and pictures stay in the work as
markdown; anything big (Remotion code, footage, renders) goes to a project, the one the work links.

## Stages

| stage | does | gate |
|-------|------|------|
| [01 research](stages/01_research.md) | what to say: facts, audience, the one point | no |
| [02 script](stages/02_script.md) | the script, scene by scene | yes |
| [03 storyboard](stages/03_storyboard.md) | what is on screen per scene | yes |
| [04 production](stages/04_production.md) | builds the video in the linked project | no |

## Layout

| path | holds |
|------|-------|
| `_config/` | `voice.md` (how it sounds), `structure.md` (scenes, timing); a work's own `_config/` file wins |
| `stages/` | the stage contracts |
| `<work>/index.md` | the video: its project, brief and stages |
| `<work>/NN_stage/` | what the stage wrote, and `_stage.md` (its state) |

## Works
