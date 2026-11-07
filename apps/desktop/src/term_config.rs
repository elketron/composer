//! The terminal model's settings and its clipboard hook (OSC 52 writes from the program).

use std::sync::Mutex;

use tattoy_wezterm_term as wt;
use wt::color::ColorPalette;

/// Lines kept in a pane's own history.
pub const HISTORY: usize = 5000;

#[derive(Debug)]
pub struct Config;
impl wt::TerminalConfiguration for Config {
    fn color_palette(&self) -> ColorPalette {
        ColorPalette::default()
    }
    fn enable_kitty_keyboard(&self) -> bool {
        true
    }
    fn scrollback_size(&self) -> usize {
        HISTORY
    }
}

/// OSC 52 from the program (nvim yanking) lands here; the pane moves it to the OS clipboard.
#[derive(Default)]
pub struct Osc52(pub Mutex<Option<String>>);
impl wt::Clipboard for Osc52 {
    fn set_contents(
        &self,
        _sel: wt::ClipboardSelection,
        data: Option<String>,
    ) -> anyhow::Result<()> {
        *self.0.lock().expect("osc52 lock") = data;
        Ok(())
    }
}
