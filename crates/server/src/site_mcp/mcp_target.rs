//! How a declared server is reached.

use std::collections::BTreeMap;

use crate::site_mcp::describe::{command_line, environment, word};

/// Exactly one of `bundle`, `command` or `url`. Each becomes an [`crate::McpServerConfig`]
/// ([`crate::site_mcp::resolve`]), so a site's server is of the same kind as one in `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTarget {
    /// An installed `.mcpb`, by name: a home-wide install under `<home>/mcps/`, referenced here.
    Bundle(String),
    /// A child process speaking MCP on stdio.
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    /// A streamable-HTTP server that is already listening.
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

impl McpTarget {
    /// What it would start or reach, in words: everything that decides what runs.
    pub fn describes(&self) -> String {
        match self {
            Self::Bundle(name) => format!("the installed bundle {}", word(name)),
            Self::Stdio { command, args, env } => {
                let env = environment(env)
                    .map(|said| format!(" {said}"))
                    .unwrap_or_default();
                format!("runs {}{env}", command_line(command, args))
            }
            Self::Http { url, headers } => {
                let headers = match headers.is_empty() {
                    true => String::new(),
                    false => format!(
                        " with the headers {}",
                        headers
                            .iter()
                            .map(|(name, value)| format!("{}: {}", word(name), word(value)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                format!("the MCP server at {}{headers}", word(url))
            }
        }
    }
}
