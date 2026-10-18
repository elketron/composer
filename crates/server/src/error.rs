use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Core(#[from] composer_core::CoreError),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("config: {0}")]
    Config(String),

    #[error("provider: {0}")]
    Provider(String),

    #[error("agent `{0}` not found in site://agents")]
    AgentNotFound(String),

    #[error("session `{0}` not found")]
    SessionNotFound(String),

    #[error("{0}")]
    Invalid(String),
}

impl ServerError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, ServerError>;
