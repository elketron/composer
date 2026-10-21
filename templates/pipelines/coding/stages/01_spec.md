---
title: Spec
type: stage
gate: true
harness: composer
---
# 01 · Spec

## Inputs

| | file | for |
|---|---|---|
| working | `brief` | what the work is for |
| reference | `_config/conventions.md` | how code is written here |

The project's code is open to read (`project`).

## Process

Read the brief and the parts of the project it touches. Write a spec: the problem, what changes and what
doesn't, the behaviour from the outside, and how to tell it works (checks a reviewer can run). List open
questions instead of guessing.

## Outputs

- `01_spec/spec.md`

## Verify

Every requirement in the brief is in the spec or listed as out of scope.
