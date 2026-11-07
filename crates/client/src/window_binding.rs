/// The desktop window a client is, and the terminal its TUI runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowBinding {
    pub window: String,
    pub terminal: String,
}

impl WindowBinding {
    pub fn is(&self, window: &str) -> bool {
        self.window == window
    }
}
