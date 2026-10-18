use clap::{Parser, Subcommand};
use composer_server::client::{AskOptions, ask};
use composer_server::{Config, Home, Server, init_home};

#[derive(Parser)]
#[command(name = "composer", about = "Agents browsing a web of markdown sites")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
        #[arg(long, default_value = "default")]
        agent: String,
        /// Continue an existing session.
        #[arg(long, short)]
        session: Option<String>,
        /// Active project directory for a new session.
        #[arg(long, short)]
        project: Option<String>,
        /// Also print the model's reasoning.
        #[arg(long, short = 't')]
        thinking: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "composer_server=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let home = Home::from_env();
    match Cli::parse().command {
        Command::Init => {
            let created = init_home(&home)?;
            println!("composer home: {}", home.root().display());
            if created.is_empty() {
                println!("already initialized, nothing created");
            }
            for path in created {
                println!("  created {path}");
            }
        }
        Command::Serve { bind } => {
            let server = Server::start(home.clone()).await?;
            let bind = bind.unwrap_or_else(|| server.config.server.bind.clone());
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!("listening on http://{bind}");
            server.serve(listener).await?;
        }
        Command::Ask {
            text,
            agent,
            session,
            project,
            thinking,
        } => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                anyhow::bail!("nothing to ask");
            }
            let _ = Config::load(&home.config_file())?;
            ask(
                &home,
                AskOptions {
                    text,
                    agent,
                    session,
                    project,
                    show_thinking: thinking,
                },
            )
            .await?;
        }
    }
    Ok(())
}
