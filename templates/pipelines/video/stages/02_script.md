---
title: Script
type: stage
gate: true
harness: composer
---
# 02 · Script

## Inputs

| | file | for |
|---|---|---|
| working | `01_research/research.md` | what to say (this work) |
| reference | `_config/voice.md` | how it sounds |
| reference | `_config/structure.md` | scenes, timing |

## Process

Write the script from the research, scene by scene, following `structure.md`. Match `voice.md`.

## Outputs

- `02_script/script.md`

## Verify

Every claim in the script is in `research.md`. Scenes add up to the length in `structure.md`.
