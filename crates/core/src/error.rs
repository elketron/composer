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

    #[error(
        "{path} is reached through a symlink that points outside the web and its allowed projects (the projects dir in config.toml); move the symlink or its target inside an allowed project, or copy the file inside the site"
    )]
    MountRefused { path: PathBuf },

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, CoreError>;
