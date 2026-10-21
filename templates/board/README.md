# The board

The board app of a pipeline site (copied into `site://<pipeline>/ui/` by `new_pipeline`): every work of the
pipeline, its stages with their state, the output of the stage you pick, and the gate.

- **Continue** writes `status: approved` into the stage's `_stage.md`; the server starts the next stage.
- **Rerun…** writes `status: rerun` and your `note`; the server runs the stage again with the note.
- **Stop** writes `status: stopped`; nothing runs after it.
- **Run** starts the next stage that hasn't run (`work.run`); **+ New work** makes one (`work.new`).
- A **stale** stage (dashed) ran before something it read changed: an earlier output, the brief, a `_config`
  file. **Rerun from here** (or **Rerun from NN** at the top) runs it again and goes on through the stages after it.

Build it with the site's `ui-build` action. It is plain page state: the TUI or an agent can do the same by
editing `_stage.md`.
