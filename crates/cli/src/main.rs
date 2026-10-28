mod remote;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use composer_server::client::{AskOptions, ask};
use composer_server::mcp::{McpOptions, run_mcp};
use composer_server::worker::{WorkerOptions, run_worker};
use composer_server::{
    Config, Home, Server, UpdateOutcome, init_home, pending_updates, update_home,
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
}

#[derive(Subcommand)]
enum Command {
    /// Create ~/.composer (or $COMPOSER_HOME): seed sites, config, token. Never overwrites.
    Init,
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
enum DevicesAction {
    /// Revoke a device: its token stops working.
    Revoke { name: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cli = Cli::parse();
    let home = Home::from_env();
    cli.project = cli.project.map(project_arg);
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
            let options = tui_options(&home, &cli.agent, cli.session, cli.new, cli.project)?;
            composer_tui::run(options).await?
        }
        Some(Command::Init) => {
            let created = init_home(&home).await?;
            println!("composer home: {}", home.root().display());
            if created.is_empty() {
                println!("already initialized, nothing created");
            }
            for path in created {
                println!("  created {path}");
            }
        }
        Some(Command::Update { dry_run }) => update(&home, dry_run).await?,
        Some(Command::Serve { bind }) => {
            let pending = pending_updates(&home);
            if pending > 0 {
                tracing::info!(
                    "{pending} stock files have newer versions: stop the server and run `composer update`"
                );
            }
            let server = Server::start(home.clone()).await?;
            let bind = bind.unwrap_or_else(|| server.config().server.bind.clone());
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!("listening on http://{bind}");
            server.serve(listener).await?;
        }
        Some(Command::Ask { text, thinking }) => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                anyhow::bail!("nothing to ask");
            }
            let (server_url, token) = remote::client(&home)?;
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
            let (server_url, token) = remote::client(&home)?;
            let change = state.map(|s| (entry, s == "on"));
            composer_server::client::harness(&server_url, &token, project, change).await?;
        }
        Some(Command::Ticket {
            action,
            ticket,
            yes,
        }) => {
            use composer_server::client::TicketAction;
            let action = match action.as_str() {
                "merge" => TicketAction::Merge,
                "discard" => TicketAction::Discard,
                _ => TicketAction::Changes,
            };
            let (server_url, token) = remote::client(&home)?;
            composer_server::client::ticket(&server_url, &token, action, ticket, yes).await?;
        }
        Some(Command::Pair { name }) => remote::pair(&home, &name)?,
        Some(Command::Devices { action }) => {
            let revoke = action.map(|DevicesAction::Revoke { name }| name);
            remote::devices(&home, revoke)?;
        }
        Some(Command::Login { server, code, name }) => {
            remote::login(&home, &server, &code, name).await?
        }
        Some(Command::Logout) => remote::logout(&home)?,
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
            run_worker(home, options).await?;
        }
        Some(Command::Mcp { session, parent }) => {
            let options = McpOptions {
                session,
                parent_url: parent,
            };
            run_mcp(home, options).await?;
        }
        Some(Command::Script { file, out, new }) => {
            let script = std::fs::read_to_string(&file)?;
            let options = tui_options(&home, &cli.agent, cli.session, new, cli.project)?;
            for snapshot in composer_tui::run_script(options, &script, &out).await? {
                println!("{}", snapshot.display());
            }
        }
    }
    Ok(())
}

async fn update(home: &Home, dry_run: bool) -> anyhow::Result<()> {
    if !home.config_file().exists() {
        anyhow::bail!(
            "{} is not set up yet: run `composer init`",
            home.root().display()
        );
    }
    let bind = Config::load(&home.config_file())?.server.bind;
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
        println!(
            "\nThe new versions of the files kept as yours are in {}:",
            home.updates().display()
        );
        for (yours, new) in review {
            println!("  diff {} {}", yours.display(), new.display());
        }
        println!("Take what you want from them, then delete them; they are not reported again.");
    }
    Ok(())
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
    home: &Home,
    agent: &str,
    session: Option<String>,
    new: bool,
    project: Option<String>,
) -> anyhow::Result<TuiOptions> {
    let config = Config::load(&home.config_file())?;
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
        },
        context_limit: config.agent.compaction_threshold,
    })
}
