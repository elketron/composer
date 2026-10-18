use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid url `{url}`: {reason}")]
    InvalidUrl { url: String, reason: &'static str },

    #[error("site not found: {0}")]
    SiteNotFound(String),

    #[error("page not found: {0}")]
    PageNotFound(String),

    #[error("invalid frontmatter: {0}")]
    InvalidFrontmatter(String),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, CoreError>;
