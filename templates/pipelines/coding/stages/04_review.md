---
title: Review
type: stage
gate: true
harness: composer
---
# 04 · Review

## Inputs

| | file | for |
|---|---|---|
| working | `03_implement/notes.md` | what was done |
| reference | `01_spec/spec.md` | what it should do |
| reference | `_config/review.md` | what to look for |

The ticket's changes are open to read.

## Process

Review the change against the spec and `review.md`. Run its checks. Write the findings, most important
first, and a verdict: merge, or what has to change first. The gate is the merge: Merge in the Changes view,
or Rerun 03 with the findings.

## Outputs

- `04_review/review.md`
