use anyhow::{Context, bail};
use composer_protocol::{ClientCommand, ClientRequest, CommandResult};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use crate::cli::ask::next_reply;

/// `composer app rebuild [--all] [--yes]`: the workspace's apps built with an older sdk, rebuilt.
pub async fn rebuild_apps(
    server_url: &str,
    token: &str,
    all: bool,
    yes: bool,
) -> anyhow::Result<()> {
    note(server_url, token, ClientCommand::AppsRebuild { all, yes }).await
}

/// Sends `command` and prints the note it is answered with.
async fn note(server_url: &str, token: &str, command: ClientCommand) -> anyhow::Result<()> {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/ws?token={token}", base.replacen("http", "ws", 1));
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connecting to {base} (is `composer serve` running?)"))?;
    let json = serde_json::to_string(&ClientRequest { id: 1, command }).expect("serializes");
    socket.send(Message::Text(json.into())).await?;
    match next_reply(&mut socket).await? {
        CommandResult::Note { text } => {
            println!("{text}");
            Ok(())
        }
        other => bail!("unexpected reply: {other:?}"),
    }
}

/// `composer app allow|promote <project> <app>`, through the running server of the workspace:
/// `allow` a project-local app as it is now, or `promote` it into the workspace's sites.
pub async fn local_app(
    server_url: &str,
    token: &str,
    project: String,
    app: String,
    promote: bool,
) -> anyhow::Result<()> {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/ws?token={token}", base.replacen("http", "ws", 1));
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connecting to {base} (is `composer serve` running?)"))?;
    let command = match promote {
        true => ClientCommand::AppPromote { project, app },
        false => ClientCommand::AppAllow { project, app },
    };
    let json = serde_json::to_string(&ClientRequest { id: 1, command }).expect("serializes");
    socket.send(Message::Text(json.into())).await?;
    match next_reply(&mut socket).await? {
        CommandResult::Note { text } => {
            println!("{text}");
            Ok(())
        }
        other => bail!("unexpected reply: {other:?}"),
    }
}
