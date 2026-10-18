use std::io::Write;

use anyhow::{Context, bail};
use composer_protocol::{ClientCommand, ClientRequest, CommandResult, Role, ServerMessage};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::config::Config;
use crate::home::Home;
use crate::init::local_token;

/// Options for `composer ask`.
#[derive(Debug, Clone)]
pub struct AskOptions {
    pub text: String,
    pub agent: String,
    /// Continue this session instead of creating one.
    pub session: Option<String>,
    pub project: Option<String>,
    pub show_thinking: bool,
}

/// Sends one message over the WebSocket API and prints the turn as it streams.
pub async fn ask(home: &Home, options: AskOptions) -> anyhow::Result<()> {
    let config = Config::load(&home.config_file())?;
    let token = local_token(home)?;
    let url = format!("ws://{}/ws?token={token}", config.server.bind);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| {
            format!(
                "connecting to {} (is `composer serve` running?)",
                config.server.bind
            )
        })?;

    let mut next_id = 0;
    let mut request = |command| {
        next_id += 1;
        Message::Text(
            serde_json::to_string(&ClientRequest {
                id: next_id,
                command,
            })
            .expect("serializes")
            .into(),
        )
    };

    let session = match options.session {
        Some(id) => id,
        None => {
            socket
                .send(request(ClientCommand::SessionCreate {
                    agent: options.agent,
                    project: options.project,
                }))
                .await?;
            match next_reply(&mut socket).await? {
                CommandResult::Session { session } => session.id,
                other => bail!("unexpected reply: {other:?}"),
            }
        }
    };
    eprintln!("\x1b[2msession {session}\x1b[0m");
    socket
        .send(request(ClientCommand::SessionAttach {
            session: session.clone(),
            since: Some(u64::MAX),
        }))
        .await?;
    next_reply(&mut socket).await?;
    socket
        .send(request(ClientCommand::MessageSend {
            session: session.clone(),
            text: options.text,
            pages: vec![],
            images: vec![],
        }))
        .await?;

    let mut thinking_open = false;
    let mut out = std::io::stdout();
    while let Some(message) = socket.next().await {
        let Message::Text(text) = message? else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<ServerMessage>(&text) else {
            continue;
        };
        match event {
            ServerMessage::ThinkingDelta { text, .. } if options.show_thinking => {
                if !thinking_open {
                    print!("\x1b[2;3m");
                    thinking_open = true;
                }
                print!("{text}");
            }
            ServerMessage::MessageDelta { text, .. } => {
                if thinking_open {
                    print!("\x1b[0m\n\n");
                    thinking_open = false;
                }
                print!("{text}");
            }
            ServerMessage::MessageDone { message, .. } => {
                if thinking_open {
                    println!("\x1b[0m");
                    thinking_open = false;
                }
                if message.role == Role::System {
                    println!("\x1b[33m{}\x1b[0m", message.content);
                } else if message.role == Role::Assistant && !message.content.is_empty() {
                    println!();
                }
            }
            ServerMessage::ToolCallStarted { name, args, .. } => {
                println!(
                    "\x1b[35m▸ {name}\x1b[0m \x1b[2m{}\x1b[0m",
                    one_line(&args.to_string(), 100)
                );
            }
            ServerMessage::ToolResult {
                result, is_error, ..
            } => {
                let color = if is_error { "31" } else { "2" };
                println!("  \x1b[{color}m{}\x1b[0m", one_line(&result, 100));
            }
            ServerMessage::TurnDone { cancelled, .. } => {
                if cancelled {
                    println!("\x1b[33m(cancelled)\x1b[0m");
                }
                break;
            }
            ServerMessage::Error { message, .. } => eprintln!("\x1b[31merror: {message}\x1b[0m"),
            _ => {}
        }
        out.flush()?;
    }
    eprintln!("\x1b[2mcontinue with: composer ask --session {session} \"...\"\x1b[0m");
    Ok(())
}

async fn next_reply<S>(socket: &mut S) -> anyhow::Result<CommandResult>
where
    S: futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(message) = socket.next().await {
        let Message::Text(text) = message? else {
            continue;
        };
        match serde_json::from_str::<ServerMessage>(&text) {
            Ok(ServerMessage::Ok { result, .. }) => return Ok(result),
            Ok(ServerMessage::Error {
                id: Some(_),
                message,
            }) => bail!("{message}"),
            _ => {}
        }
    }
    bail!("connection closed")
}

fn one_line(text: &str, max: usize) -> String {
    let line = text.replace('\n', " ⏎ ");
    if line.chars().count() > max {
        format!("{}…", line.chars().take(max).collect::<String>())
    } else {
        line
    }
}
