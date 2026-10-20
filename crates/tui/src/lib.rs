//! The composer terminal UI.
//!
//! Flux: keys, server events and effect results become `Action`s; `AppState::reduce`
//! applies them and returns `Effect`s; components render from the state.

// Components and stores are built once with `new()`; a `Default` would add nothing.
#![allow(clippy::new_without_default)]

mod action;
mod api_client;
mod app;
mod commands;
mod completion;
pub mod components;
mod diff_parse;
mod dispatcher;
mod editor;
mod effect;
mod effect_runner;
mod headless;
mod keymap;
pub mod markdown;
mod options;
pub mod snapshot;
pub mod store;
mod terminal;
mod theme;
mod tool_text;
mod ws_link;

pub use action::Action;
pub use app::App;
pub use effect::Effect;
pub use headless::{parse_keys, run_script};
pub use options::TuiOptions;
pub use store::types::Startup;

/// Runs the TUI until `:q`.
pub async fn run(options: TuiOptions) -> anyhow::Result<()> {
    let terminal = crate::terminal::init();
    let result = App::new(&options).run(terminal).await;
    crate::terminal::restore();
    result
}
