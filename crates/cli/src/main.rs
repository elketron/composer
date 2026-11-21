mod kind;
mod remote;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use composer_client::cli::{AskOptions, ask};
use composer_protocol::RunKind;
use composer_server::mcp::{McpOptions, run_mcp};
use composer_server::{Home, Server, init_home};
use composer_tui::{Startup, TuiOptions};
use kind::Kind;

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
    /// Run kind of new sessions.
    #[arg(long, value_enum, default_value_t = Kind::Agentic, global = true)]
    kind: Kind,
    /// The site a new session starts in (its `folder:` is where terminals start).
    #[arg(long, global = true)]
    site: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Create ~/.composer (or $COMPOSER_HOME): its config, the stock sites and its state.
    /// Never overwrites.
    Init,
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
    /// Serve composer's tools for a session over MCP on stdin/stdout, for any harness (needs a
    /// running server). The list follows the session's site.
    #[command(hide = true)]
    Mcp {
        #[arg(long)]
        session: String,
        /// The running server's address (default: `server.bind` from config.toml).
        #[arg(long)]
        server: Option<String>,
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
    let cli = Cli::parse();
    let home = Home::from_env();
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
            let options = tui_options(&home, cli.kind.into(), cli.session, cli.new, cli.site)?;
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
        Some(Command::Serve { bind }) => {
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
                kind: cli.kind.into(),
                session: cli.session,
                site: cli.site,
                show_thinking: thinking,
                server_url,
                token,
            };
            ask(options).await?;
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
        Some(Command::Mcp { session, server }) => {
            let server_url = match server {
                Some(server) => server,
                None => format!("http://{}", home.load_config()?.server.bind),
            };
            let options = McpOptions {
                session,
                server_url,
            };
            run_mcp(home, options).await?;
        }
        Some(Command::Script { file, out, new }) => {
            let script = std::fs::read_to_string(&file)?;
            let options = tui_options(&home, cli.kind.into(), cli.session, new, cli.site)?;
            for snapshot in composer_tui::run_script(options, &script, &out).await? {
                println!("{}", snapshot.display());
            }
        }
    }
    Ok(())
}

/// Connects to the remote server from `composer login`, or the local one.
fn tui_options(
    home: &Home,
    kind: RunKind,
    session: Option<String>,
    new: bool,
    site: Option<String>,
) -> anyhow::Result<TuiOptions> {
    let config = home.load_config()?;
    let (server_url, token) = remote::client(home)?;
    Ok(TuiOptions {
        server_url,
        token,
        startup: Startup {
            session,
            new,
            kind,
            site,
        },
        context_limit: config.agent.compaction_threshold,
    })
}
