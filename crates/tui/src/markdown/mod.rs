//! Markdown → styled, wrapped ratatui lines, with link positions for hint mode.

mod renderer;
mod types;
mod wrap;

pub use renderer::render;
pub use types::{Rendered, RenderedLink};
