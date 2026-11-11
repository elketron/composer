# The board

The board app of a pipeline site (copied into `site://<pipeline>/ui/` by `new_pipeline`): every work of the
pipeline, its stages with their state, the output of the stage you pick, and the gate.

- **Continue** writes `status: approved` and the stage in `done:` on the work's `index.md`.
- **Rerun…** writes `status: rerun` and your `note`; **Run** runs the stage again with the note.
- **Stop** writes `status: stopped`; nothing runs after it.
- **Run** starts the next ready stage (`work.run`); **+ New work** makes one (`work.new`).
- A **stale** stage (dashed) ran before something it read changed: an earlier output, the brief, a `_config`
  file. **Rerun from here** (or **Rerun from NN** at the top) runs it again; the next stage is another Run.

Build it with the site's `ui-build` action. It is plain page state: the TUI or an agent can do the same by
editing the work's `index.md`. Nothing on the server decides what runs next.
