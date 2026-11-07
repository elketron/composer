/// What an app adds to a project's rail: an icon (a name the clients map to a glyph) and a label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRail {
    pub icon: Option<String>,
    pub label: Option<String>,
}
