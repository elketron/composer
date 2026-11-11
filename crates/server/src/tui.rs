//! A `type: tui` page: a capability whose UI is a terminal program. The page declares the
//! command and where it runs; the terminal is the human's view. The agent drives the program's
//! own control surface (for hunk, the `hunk` tool), not its keys.

use std::path::PathBuf;

use composer_core::Page;

use crate::tools::{ToolContext, expand_home};

/// The supported control adapters. A page with one of these can start its command directly: the
/// command is the adapter's own binary, not a shell string.
const CONTROLS: [&str; 1] = ["hunk"];

/// What a `type: tui` page says.
#[derive(Debug, Clone)]
pub struct TuiPage {
    /// The program and its arguments, already split (`hunk diff`).
    pub command: Vec<String>,
    /// `project` (default), `site`, `home`, or a path.
    pub cwd: String,
    pub control: String,
    pub columns: u16,
    pub rows: u16,
}

impl TuiPage {
    pub fn parse(page: &Page) -> Result<Self, String> {
        let control = page.frontmatter.get_str("control").unwrap_or_default();
        if !CONTROLS.contains(&control.as_str()) {
            return Err(format!(
                "a TUI page needs `control:` one of {} (this one has `{control}`)",
                CONTROLS.join(", ")
            ));
        }
        let raw = page
            .frontmatter
            .get_str("command")
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| "a TUI page needs `command:`".to_string())?;
        let command: Vec<String> = raw.split_whitespace().map(str::to_string).collect();
        // No shell: the command is the adapter's program and plain arguments.
        if command.first().map(String::as_str) != Some(&control)
            || command
                .iter()
                .any(|arg| arg.contains([';', '|', '&', '$', '`', '<', '>', '\n', '\r']))
        {
            return Err(format!(
                "a `control: {control}` page runs `{control}` and plain arguments, not a shell command"
            ));
        }
        if !matches!(
            command.get(1).map(String::as_str),
            Some("diff" | "show" | "status" | "log")
        ) {
            return Err("a Hunk page runs `hunk diff`, `show`, `status` or `log`".into());
        }
        // Hunk's extensions run code from a path: a page may review diffs, not load extensions.
        if command.iter().any(|arg| {
            arg == "--extension" || arg.starts_with("--extension=") || arg == "--enable-extension"
        }) {
            return Err("a Hunk page may not load extensions".into());
        }
        let number = |key: &str, default: u16| {
            page.frontmatter
                .get(key)
                .and_then(|v| v.as_u64())
                .map_or(default, |n| (n as u16).clamp(20, 500))
        };
        Ok(Self {
            command,
            cwd: page
                .frontmatter
                .get_str("cwd")
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| "project".into()),
            control,
            columns: number("cols", 120),
            rows: number("rows", 40),
        })
    }

    /// Where the program runs.
    pub fn resolve_cwd(&self, context: &ToolContext, page: &Page) -> Result<PathBuf, String> {
        let site = context.server.web.root().join(page.url.site());
        let raw = self.cwd.trim();
        let path = match raw {
            "project" => {
                // A page inside a work (`<work>/review.md`) runs in that work's `project/`
                // (the worktree while a stage runs); a site-level page uses the session's.
                if let Some(work) =
                    page.url.path().split('/').next().filter(|w| {
                        page.url.path().contains('/') && !w.is_empty() && *w != "index.md"
                    })
                {
                    let link = site.join(work).join("project");
                    if link.is_dir() {
                        return link
                            .canonicalize()
                            .map_err(|e| format!("{}: {e}", link.display()));
                    }
                }
                context
                    .session
                    .info()
                    .project
                    .and_then(|project| context.server.see_project(&project).ok())
                    .map(PathBuf::from)
                    .ok_or_else(|| {
                        "this TUI runs in the project, and no project is active; set one, or open it from a work"
                            .to_string()
                    })?
            }
            "site" => site.clone(),
            "home" => context.server.home.root().to_path_buf(),
            other => {
                let expanded = expand_home(other);
                if expanded.is_absolute() {
                    expanded
                } else if expanded
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
                {
                    return Err(format!("`cwd: {other}` may not go up from the site"));
                } else {
                    site.join(expanded)
                }
            }
        };
        if !path.is_dir() {
            return Err(format!("{} is not a folder", path.display()));
        }
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        // A page may run in the site, the workspace, the session's project, or one of its
        // worktrees; not in some other folder the page names.
        let site = site.canonicalize().unwrap_or(site);
        let project = context
            .session
            .info()
            .project
            .and_then(|project| context.server.see_project(&project).ok())
            .map(PathBuf::from);
        let worktrees = context.server.home.root().join("state/worktrees");
        let allowed = path.starts_with(&site)
            || path.as_path() == context.server.home.root()
            || project
                .as_ref()
                .is_some_and(|project| path.starts_with(project))
            || path.starts_with(&worktrees);
        if !allowed {
            return Err(format!(
                "{} is outside this workspace; a TUI page runs in the site, the project, or a worktree",
                path.display()
            ));
        }
        Ok(path)
    }

    /// Starts the program in a terminal on the session's desk. The terminal is a client (like the
    /// desktop TUI), so it runs outside the agent's sandbox; the page's control is one of
    /// [`CONTROLS`], not an arbitrary shell.
    pub fn open(&self, context: &ToolContext, page: &Page) -> Result<String, String> {
        let cwd = self.resolve_cwd(context, page)?;
        let id = context.server.terminals.open_tui(
            &context.server,
            &context.session,
            &self.command,
            &cwd,
            self.columns,
            self.rows,
        )?;
        Ok(format!(
            "started `{}` in terminal {id} (in {}). The user sees it; the `{}` tool reads and \
             drives its live session. Don't send keys to the TUI.",
            self.command.join(" "),
            cwd.display(),
            self.control
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use composer_core::SiteUrl;

    fn page(frontmatter: &str) -> Page {
        Page::parse(
            SiteUrl::new("coding", "review.md").unwrap(),
            format!("---\n{frontmatter}\n---\n# Review\n"),
        )
    }

    #[test]
    fn parses_a_hunk_page() {
        let p = TuiPage::parse(&page(
            "title: Review\ntype: tui\ncommand: hunk diff\ncontrol: hunk\ncwd: project",
        ))
        .unwrap();
        assert_eq!(p.command, ["hunk", "diff"]);
        assert_eq!((p.cwd.as_str(), p.control.as_str()), ("project", "hunk"));
        assert_eq!((p.columns, p.rows), (120, 40));
    }

    #[test]
    fn refuses_a_shell_or_an_unknown_control() {
        assert!(
            TuiPage::parse(&page("type: tui\ncommand: hunk; rm -rf /\ncontrol: hunk")).is_err()
        );
        assert!(TuiPage::parse(&page("type: tui\ncommand: lazygit\ncontrol: git")).is_err());
        assert!(TuiPage::parse(&page("type: tui\ncommand: hunk diff")).is_err());
    }
}
