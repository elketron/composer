# composer

Design and decisions: [docs/plan.md](docs/plan.md). UI mockups: [docs/mockups/](docs/mockups/).

## Code conventions (Rust)

- **A type with an `impl` block gets its own file.** That includes trait impls (`impl Component for ChatPane`). Its impls live in that file. Derives don't count.
- **Types without impl blocks may share a file** (plain data enums/structs, e.g. `action.rs`, `store/types.rs`).
- File name = type name in snake_case (`SessionStore` → `session_store.rs`).

## TUI

- Flux: all state changes go through `Action` → stores' `reduce` → `AppState`. Reducers do no IO; they return `Effect`s, which the `EffectRunner` executes.
- Components implement `Component` and hold only local view state; shared state belongs in a store.
- Styling follows `docs/mockups/theme.css`: no rounded corners in the TUI.
