mod remote;
mod workspaces;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use composer_client::cli::{AskOptions, ask};
use composer_server::harness::ClaudeToken;
use composer_server::mcp::{McpOptions, run_mcp};
use composer_server::worker::{WorkerOptions, run_worker};
use composer_server::{
    Config, Home, Host, UpdateOutcome, WorkspaceHome, WorkspaceName, init_home, init_workspace,
    migrate_home, pending_updates, update_home,
};
use composer_tui::{Startup, TuiOptions};

#[derive(Parser)]
#[command(name = "composer", about = "Agents browsing a web of markdown sites")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Session to open (default: the most recent one).
    #[arg(long, short, global = true)]
    session: Option<String>,
    /// Start a new session instead of resuming.
    #[arg(long, short = 'n')]
    new: bool,
    /// Agent for new sessions.
    #[arg(long, default_value = "default", global = true)]
    agent: String,
    /// Active project for a new session: a name in the projects directory (`[projects] dir`,
    /// default ~/Projects), `.` for this directory, or a path.
    #[arg(long, short, global = true)]
    project: Option<String>,
    /// Workspace to work in (default: $COMPOSER_WORKSPACE, else `default`).
    #[arg(long, short = 'w', global = true)]
    workspace: Option<String>,
    /// The desktop window this TUI is the chat of (the server starts it so): its stage follows
    /// the session shown here.
    #[arg(long)]
    window: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Create ~/.composer (or $COMPOSER_HOME): global config, the site library, templates and
    /// the `default` workspace; with `--workspace <name>`, that workspace too. Never overwrites.
    Init,
    /// Move a home from before workspaces into workspaces/default/ (backs it up first). Stop the
    /// server first.
    Migrate,
    /// Bring the stock sites and templates up to this version. Your changes are kept: files
    /// you changed are merged, and what can't be merged is left for you in state/updates/.
    /// Stop the server first.
    Update {
        /// Only list what would change.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the server.
    Serve {
        /// Overrides `server.bind` from config.toml.
        #[arg(long)]
        bind: Option<String>,
    },
    /// Send a message to an agent and stream the answer (needs a running server).
    Ask {
        text: Vec<String>,
        /// Also print the model's reasoning.
        #[arg(long, short = 't')]
        thinking: bool,
    },
    /// Pair a device (phone, laptop) for remote access: prints a QR code and a login command.
    Pair {
        /// The device's name, used to revoke it later.
        #[arg(long, default_value = "phone")]
        name: String,
    },
    /// List paired devices, or revoke one.
    Devices {
        #[command(subcommand)]
        action: Option<DevicesAction>,
    },
    /// Connect this machine to a remote composer server, with a code from `composer pair`.
    Login {
        /// e.g. https://my-pc.tailnet.ts.net
        server: String,
        code: String,
        /// This device's name (default: the hostname).
        #[arg(long)]
        name: Option<String>,
    },
    /// Forget the remote server; use the local one again.
    Logout,
    /// Show a project's external harnesses (Claude Code), or turn one on or off for it.
    ///
    /// `composer harness composer off` stops Claude Code tickets in ~/Projects/composer;
    /// `on` lets them go on. A model alone: `composer harness composer off claude-code:opus`.
    Harness {
        /// A name in the projects dir, a path, or `.` (this folder).
        project: String,
        #[arg(value_parser = ["on", "off"])]
        state: Option<String>,
        /// `claude-code` or `claude-code:<model>`.
        #[arg(default_value = "claude-code")]
        entry: String,
    },
    /// A ticket's branch: its changes against the project's branch, merging it, or discarding it.
    ///
    /// `composer ticket changes 7` shows the commits, files and diff; `merge` merges the branch
    /// into the project's checked-out branch and removes the worktree; `discard --yes` deletes them.
    Ticket {
        #[arg(value_parser = ["changes", "merge", "discard"])]
        action: String,
        ticket: String,
        /// Needed to discard.
        #[arg(long)]
        yes: bool,
    },
    /// The site library (~/.composer/sites): list it, install a site into this workspace, or
    /// bring the installed sites up to it (needs a running server).
    ///
    /// `composer library install cad --as my-cad`; `composer library update --dry-run`.
    Library {
        #[command(subcommand)]
        action: LibraryCommand,
    },
    /// A project's own apps (`<project>/.composer/apps/<app>`): allow one as it is now, or promote
    /// it into this workspace's sites (needs a running server).
    ///
    /// `composer app allow mygame viewer`; `composer app promote mygame viewer`.
    App {
        #[command(subcommand)]
        action: AppCommand,
    },
    /// Workspaces of this home: make a new one (optionally from a template), list them, or list
    /// the templates (global/workspace-templates/<name>.toml).
    ///
    /// `composer workspace new printing --template 3d`.
    Workspace {
        #[command(subcommand)]
        action: WorkspaceCommand,
    },
    /// Run a worker on a ticket (started by the server for `spawn_worker`).
    #[command(hide = true)]
    Worker {
        #[arg(long)]
        session: String,
        #[arg(long)]
        ticket: String,
        #[arg(long)]
        parent: String,
        /// `<session>/<file>`: a ticket attachment for the first message (repeatable).
        #[arg(long = "attachment")]
        attachments: Vec<String>,
        /// Go on with the session's history (after a pause or a restart), told this.
        #[arg(long)]
        resume: Option<String>,
    },
    /// Store the Claude Code login sandboxed runs use: run `claude setup-token`, then this, and
    /// paste the token it printed (read from stdin; kept in the home's state/, owner-only).
    ClaudeToken,
    /// Serve a running worker's tools over MCP on stdin/stdout (started for external harnesses).
    #[command(hide = true)]
    Mcp {
        #[arg(long)]
        session: String,
        #[arg(long)]
        parent: String,
    },
    /// Drive the TUI headlessly from a script and save snapshots (for development).
    #[command(hide = true)]
    Script {
        file: PathBuf,
        #[arg(long, default_value = "snapshots")]
        out: PathBuf,
        #[arg(long)]
        new: bool,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommand {
    /// Make a workspace: the stock sites from the library, then the template's sites and config.
    /// A running server serves it right away.
    New {
        name: String,
        #[arg(long)]
        template: Option<String>,
    },
    /// The workspaces and what they installed from the library.
    List,
    /// The workspace templates.
    Templates,
}

#[derive(Subcommand)]
enum AppCommand {
    /// Allow a project-local app as it is now (look at it first: it comes with the project); a
    /// change to its index.md or justfile turns it off again.
    Allow { project: String, app: String },
    /// Copy a project-local app into this workspace's sites; the workspace's copy wins after.
    Promote { project: String, app: String },
    /// Rebuild the apps built with an older sdk (or not built) with their `ui-build`, in the
    /// sandbox. Shows what would run; `--yes` runs it.
    Rebuild {
        /// Every app, not only the outdated ones.
        #[arg(long)]
        all: bool,
        /// Run what was shown.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// The library's sites, and which this workspace installed.
    List,
    /// Copy a library site into this workspace.
    Install {
        site: String,
        /// Install it under another name.
        #[arg(long = "as")]
        name: Option<String>,
    },
    /// Bring this workspace's installed sites up to the library (your changes are merged; what
    /// can't be is left for you in state/updates/).
    Update {
        /// Only list what would change.
        #[arg(long)]
        dry_run: bool,
    },
    /// Publish a site of this workspace to the library: its definition only (never works,
    /// tickets or other pages). Shows what would go; `--yes` publishes it.
    Publish {
        site: String,
        /// Its name in the library.
        #[arg(long = "as")]
        name: Option<String>,
        /// Publish what was shown.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum DevicesAction {
    /// Revoke a device: its token stops working.
    Revoke { name: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cli = Cli::parse();
    let home = Home::from_env();
    cli.project = cli.project.map(project_arg);
    let workspace = workspace_home(&home, cli.workspace.as_deref())?;
    let tui_command = cli.command.is_none();
    if !tui_command {
        // Logging would garble the TUI, so only the other commands log to stderr.
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "composer_server=info".into()),
            )
            .with_writer(std::io::stderr)
            .init();
    }

    match cli.command {
        None => {
            let options = tui_options(
                &workspace,
                &cli.agent,
                cli.session,
                cli.new,
                cli.project,
                cli.window,
            )?;
            composer_tui::run(options).await?
        }
        Some(Command::Init) => {
            let mut created = init_home(&home).await?;
            // `init --workspace <name>`: that workspace too (the home's first one is `default`).
            if cli.workspace.is_some() && !home.is_single_workspace() {
                created.extend(init_workspace(&workspace).await?);
            }
            println!("composer home: {}", home.root().display());
            if created.is_empty() {
                println!("already initialized, nothing created");
            }
            for path in created {
                println!("  created {path}");
            }
        }
        Some(Command::Migrate) => {
            let report = migrate_home(&home).await?;
            println!("backed up to {}", report.backup.display());
            println!("moved into {}", report.workspace.display());
            for (path, repaired) in &report.worktrees {
                let state = if *repaired {
                    "repaired"
                } else {
                    "NOT repaired"
                };
                println!("  worktree {} {state}", path.display());
            }
            for warning in &report.warnings {
                println!("warning: {warning}");
            }
        }
        Some(Command::Update { dry_run }) => update(&home, dry_run).await?,
        Some(Command::ClaudeToken) => claude_token(&home)?,
        Some(Command::Serve { bind }) => {
            let pending = pending_updates(&home);
            if pending > 0 {
                tracing::info!(
                    "{pending} stock files have newer versions: stop the server and run `composer update`"
                );
            }
            let host = Host::start(home.clone()).await?;
            let bind = bind.unwrap_or_else(|| host.bind());
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!("listening on http://{bind}");
            host.serve(listener).await?;
        }
        Some(Command::Ask { text, thinking }) => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                anyhow::bail!("nothing to ask");
            }
            let (server_url, token) = remote::client(&workspace)?;
            let options = AskOptions {
                text,
                agent: cli.agent,
                session: cli.session,
                project: cli.project,
                show_thinking: thinking,
                server_url,
                token,
            };
            ask(options).await?;
        }
        Some(Command::Harness {
            project,
            state,
            entry,
        }) => {
            let project = if project == "." {
                std::env::current_dir()?.display().to_string()
            } else {
                project
            };
            let (server_url, token) = remote::client(&workspace)?;
            let change = state.map(|s| (entry, s == "on"));
            composer_client::cli::harness(&server_url, &token, project, change).await?;
        }
        Some(Command::Workspace { action }) => match action {
            WorkspaceCommand::New { name, template } => {
                workspaces::new(&home, &name, template).await?
            }
            WorkspaceCommand::List => workspaces::list(&home)?,
            WorkspaceCommand::Templates => workspaces::templates(&home)?,
        },
        Some(Command::App { action }) => {
            let (server_url, token) = remote::client(&workspace)?;
            let (project, app, promote) = match action {
                AppCommand::Allow { project, app } => (project, app, false),
                AppCommand::Promote { project, app } => (project, app, true),
                AppCommand::Rebuild { all, yes } => {
                    composer_client::cli::rebuild_apps(&server_url, &token, all, yes).await?;
                    return Ok(());
                }
            };
            composer_client::cli::local_app(&server_url, &token, project, app, promote).await?;
        }
        Some(Command::Library { action }) => {
            use composer_client::cli::LibraryAction;
            let action = match action {
                LibraryCommand::List => LibraryAction::List,
                LibraryCommand::Install { site, name } => LibraryAction::Install { site, name },
                LibraryCommand::Update { dry_run } => LibraryAction::Update { dry_run },
                LibraryCommand::Publish { site, name, yes } => {
                    LibraryAction::Publish { site, name, yes }
                }
            };
            let (server_url, token) = remote::client(&workspace)?;
            composer_client::cli::library(&server_url, &token, action).await?;
        }
        Some(Command::Ticket {
            action,
            ticket,
            yes,
        }) => {
            use composer_client::cli::TicketAction;
            let action = match action.as_str() {
                "merge" => TicketAction::Merge,
                "discard" => TicketAction::Discard,
                _ => TicketAction::Changes,
            };
            let (server_url, token) = remote::client(&workspace)?;
            composer_client::cli::ticket(&server_url, &token, action, ticket, yes).await?;
        }
        Some(Command::Pair { name }) => remote::pair(&workspace, &name)?,
        Some(Command::Devices { action }) => {
            let revoke = action.map(|DevicesAction::Revoke { name }| name);
            remote::devices(&workspace, revoke)?;
        }
        Some(Command::Login { server, code, name }) => {
            remote::login(&workspace, &server, &code, name).await?
        }
        Some(Command::Logout) => remote::logout(&workspace)?,
        Some(Command::Worker {
            session,
            ticket,
            parent,
            attachments,
            resume,
        }) => {
            // `--agent` and `--project` are the global flags.
            let options = WorkerOptions {
                session,
                ticket,
                agent: cli.agent,
                project: cli.project,
                parent_url: parent,
                attachments,
                resume,
            };
            run_worker(workspace, options).await?;
        }
        Some(Command::Mcp { session, parent }) => {
            let options = McpOptions {
                session,
                parent_url: parent,
            };
            run_mcp(workspace, options).await?;
        }
        Some(Command::Script { file, out, new }) => {
            let script = std::fs::read_to_string(&file)?;
            let options = tui_options(
                &workspace,
                &cli.agent,
                cli.session,
                new,
                cli.project,
                cli.window.clone(),
            )?;
            for snapshot in composer_tui::run_script(options, &script, &out).await? {
                println!("{}", snapshot.display());
            }
        }
    }
    Ok(())
}

async fn update(home: &Home, dry_run: bool) -> anyhow::Result<()> {
    if home.is_single_workspace() {
        anyhow::bail!(
            "{} is from before workspaces: run `composer migrate` first",
            home.root().display()
        );
    }
    if !home.global_config_file().exists() {
        anyhow::bail!(
            "{} is not set up yet: run `composer init`",
            home.root().display()
        );
    }
    let bind = Config::load(&home.global_config_file())?.server.bind;
    if !dry_run && std::net::TcpStream::connect(&bind).is_ok() {
        // It keeps recipe approvals in memory and would write over the ones added here.
        anyhow::bail!("a server is running on {bind}: stop it, update, then start it again");
    }
    let items = update_home(home, dry_run).await?;
    if items.is_empty() {
        println!("up to date");
        return Ok(());
    }
    let mut review = Vec::new();
    for item in &items {
        let (what, path) = match &item.outcome {
            UpdateOutcome::Added => ("added", None),
            UpdateOutcome::Replaced => ("updated", None),
            UpdateOutcome::Removed => ("removed (gone from the library)", None),
            UpdateOutcome::Merged => ("merged with your changes", None),
            UpdateOutcome::Conflict { review } => {
                ("conflicts with your changes, kept yours", Some(review))
            }
            UpdateOutcome::Review { review } => ("differs from yours, kept yours", Some(review)),
        };
        println!("  {}  {what}", item.path);
        review.extend(path.map(|p| (home.root().join(&item.path), p)));
    }
    if dry_run {
        println!("(dry run: nothing changed)");
    } else if !review.is_empty() {
        println!("\nThe new versions of the files kept as yours:");
        for (yours, new) in review {
            println!("  diff {} {}", yours.display(), new.display());
        }
        println!("Take what you want from them, then delete them; they are not reported again.");
    }
    Ok(())
}

/// The workspace the client commands (and a server-started worker) act in: the one a parent
/// process passed down, else `--workspace`, `$COMPOSER_WORKSPACE`, then `default`. A home from
/// before workspaces is its own (only) workspace until it is migrated.
fn workspace_home(home: &Home, flag: Option<&str>) -> anyhow::Result<WorkspaceHome> {
    if let Some(workspace) = WorkspaceHome::from_env()? {
        return Ok(workspace);
    }
    if home.is_single_workspace() {
        return Ok(WorkspaceHome::new(home.root()));
    }
    let name = flag
        .map(str::to_string)
        .or_else(|| std::env::var(WorkspaceHome::ENV_NAME).ok());
    let name = match name {
        Some(name) => WorkspaceName::new(&name)?,
        None => WorkspaceName::default_workspace(),
    };
    Ok(home.workspace(&name))
}

/// `.`, `./x` and `../x` are this machine's current directory: made absolute here. Names and
/// other paths go to the server as they are (a name is a folder in its projects directory).
fn project_arg(project: String) -> String {
    let relative = project == "."
        || project == ".."
        || project.starts_with("./")
        || project.starts_with("../");
    match std::env::current_dir() {
        Ok(cwd) if relative => cwd.join(&project).display().to_string(),
        _ => project,
    }
}

/// Connects to the remote server from `composer login`, or the local one.
fn tui_options(
    home: &WorkspaceHome,
    agent: &str,
    session: Option<String>,
    new: bool,
    project: Option<String>,
    window: Option<String>,
) -> anyhow::Result<TuiOptions> {
    let config = home.load_config()?;
    let (server_url, token) = remote::client(home)?;
    Ok(TuiOptions {
        server_url,
        token,
        startup: Startup {
            session,
            new,
            agent: agent.to_string(),
            project,
            projects_dir: Some(config.projects.dir.clone()),
            window,
        },
        context_limit: config.agent.compaction_threshold,
    })
}

/// `composer claude-token`: reads a `claude setup-token` token from stdin and stores it.
fn claude_token(home: &Home) -> anyhow::Result<()> {
    use std::io::{IsTerminal, Read};
    if std::io::stdin().is_terminal() {
        eprintln!("Paste the token `claude setup-token` printed, then press Enter and Ctrl-D:");
    }
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw)?;
    let token = ClaudeToken::new(&raw).map_err(anyhow::Error::msg)?;
    let file = token.save(home).map_err(anyhow::Error::msg)?;
    println!(
        "stored in {} (sandboxed Claude Code runs log in with it)",
        file.display()
    );
    Ok(())
}
