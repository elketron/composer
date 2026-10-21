# Pipeline templates

Each folder is a kind of pipeline. `new_pipeline(kind)` (or `pipeline.new`, New pipeline in the app,
`:pipeline new` in the TUI) copies one into `sites/<name>/`, with `__PIPELINE__` replaced by the site's name.
Every pipeline also gets the board app (`templates/board/`) in its `ui/`. Add your own kind by adding a
folder here with the same shape:

```text
<kind>/
  index.md            type: pipeline (title, what it is for, needs_project), its stages in order
  _config/*.md        shared rules every work reads (a work's own _config/ file wins)
  stages/NN_name.md   type: stage: the contract (Inputs, Process, Outputs, Verify)
```

A stage contract's frontmatter: `gate: true` waits for you after the stage; `harness:` (`composer`,
`claude-code`), `model:`, `agent:` (default `stage`); `worktree: true` for a stage that writes code in the
work's project (in a ticket worktree). Its Inputs table names files relative to the work (`01_spec/spec.md`,
`brief` for the work's brief) or the pipeline (`_config/conventions.md`), each marked `working` (input to
transform) or `reference` (rules to follow).
