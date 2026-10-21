---
title: Plan
type: stage
gate: true
harness: composer
---
# 02 · Plan

## Inputs

| | file | for |
|---|---|---|
| working | `01_spec/spec.md` | what to build |
| reference | `_config/conventions.md` | how code is written here |

## Process

Turn the spec into steps in order: the files each step changes, the tests it adds, and how to check it.
Keep each step small enough to review on its own.

## Outputs

- `02_plan/plan.md`

## Verify

Every check in the spec is covered by a step's test.
