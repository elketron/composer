use std::path::PathBuf;

use clap::{Parser, Subcommand};
use composer_server::client::{AskOptions, ask};
use composer_server::{Config, Home, Server, init_home, local_token};
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
    /// Active project directory for a new session.
    #[arg(long, short, global = true)]
    project: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Create ~/.composer (or $COMPOSER_HOME): seed sites, config, token. Never overwrites.
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
            composer_tui::run(tui_options(
                &home,
                &cli.agent,
                cli.session,
                cli.new,
                cli.project,
            )?)
            .await?
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
            let bind = bind.unwrap_or_else(|| server.config.server.bind.clone());
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!("listening on http://{bind}");
            server.serve(listener).await?;
        }
        Some(Command::Ask { text, thinking }) => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                anyhow::bail!("nothing to ask");
            }
            let options = AskOptions {
                text,
                agent: cli.agent,
                session: cli.session,
                project: cli.project,
                show_thinking: thinking,
            };
            ask(&home, options).await?;
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

fn tui_options(
    home: &Home,
    agent: &str,
    session: Option<String>,
    new: bool,
    project: Option<String>,
) -> anyhow::Result<TuiOptions> {
    let config = Config::load(&home.config_file())?;
    Ok(TuiOptions {
        server_url: format!("http://{}", config.server.bind),
        token: local_token(home)?,
        startup: Startup {
            session,
            new,
            agent: agent.to_string(),
            project,
        },
        context_limit: config.agent.compaction_threshold,
    })
}
