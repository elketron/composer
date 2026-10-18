use crate::store::types::{Flash, FlashLevel, Mode, Pane};

/// Mode, focus, layout and display toggles.
#[derive(Debug)]
pub struct UiStore {
    pub mode: Mode,
    pub focus: Pane,
    pub maximized: Option<Pane>,
    pub picker_open: bool,
    pub show_thinking: bool,
    pub show_tool_details: bool,
    pub flash: Option<Flash>,
    pub quit: bool,
}

impl UiStore {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            focus: Pane::Chat,
            maximized: None,
            picker_open: false,
            show_thinking: false,
            show_tool_details: false,
            flash: None,
            quit: false,
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        match mode {
            Mode::Insert => self.focus = Pane::Chat,
            Mode::Hint => self.focus = Pane::Page,
            Mode::Normal | Mode::Command => {}
        }
        if self.maximized.is_some_and(|p| p != self.focus) {
            self.maximized = Some(self.focus);
        }
    }

    pub fn flash(&mut self, text: impl Into<String>, level: FlashLevel) {
        self.flash = Some(Flash {
            text: text.into(),
            level,
            at: std::time::Instant::now(),
        });
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Pane::Chat => Pane::Page,
            Pane::Page => Pane::Chat,
        };
        if self.maximized.is_some() {
            self.maximized = Some(self.focus);
        }
    }

    pub fn toggle_maximize(&mut self) {
        self.maximized = match self.maximized {
            Some(_) => None,
            None => Some(self.focus),
        };
    }
}
