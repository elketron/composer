//! Tool argument shapes, as sent by the model.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OpenArgs {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchArgs {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WriteArgs {
    pub url: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct EditArgs {
    pub url: String,
    pub old_text: String,
    pub new_text: String,
    #[serde(default)]
    pub replace_all: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetProjectArgs {
    /// Empty or missing clears the active project.
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActionsArgs {
    pub site: String,
}

#[derive(Debug, Deserialize)]
pub struct RunArgs {
    pub site: String,
    pub recipe: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewAppArgs {
    pub site: String,
}

#[derive(Debug, Deserialize)]
pub struct SpawnWorkerArgs {
    pub ticket: String,
}

#[derive(Debug, Deserialize)]
pub struct FinishArgs {
    /// `done` or `failed`.
    pub status: String,
    pub summary: String,
}
