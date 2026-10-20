use anyhow::{Context, bail};
use composer_protocol::{ClientCommand, ClientRequest, CommandResult};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use crate::client::ask::next_reply;

/// `composer harness <project> [on|off <entry>]`: shows a project's external harnesses, or
/// turns one (or one of its models) on or off.
pub async fn harness(
    server_url: &str,
    token: &str,
    project: String,
    change: Option<(String, bool)>,
) -> anyhow::Result<()> {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/ws?token={token}", base.replacen("http", "ws", 1));
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connecting to {base} (is `composer serve` running?)"))?;
    let command = match change {
        Some((entry, on)) => ClientCommand::HarnessSet { project, entry, on },
        None => ClientCommand::HarnessList { project },
    };
    let json = serde_json::to_string(&ClientRequest { id: 1, command }).expect("serializes");
    socket.send(Message::Text(json.into())).await?;
    let CommandResult::Harnesses { project, harnesses } = next_reply(&mut socket).await? else {
        bail!("unexpected reply");
    };
    println!("{project}");
    for h in harnesses {
        let state = match (h.allowed, h.on) {
            (false, _) => "not allowed in config.toml",
            (true, true) => "on",
            (true, false) => "off",
        };
        println!("  {:<24} {state}", h.entry);
    }
    Ok(())
}
