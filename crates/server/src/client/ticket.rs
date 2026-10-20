use anyhow::{Context, bail};
use composer_protocol::http::ProjectDiff;
use composer_protocol::{ClientCommand, ClientRequest, CommandResult};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use crate::client::ask::next_reply;

/// What `composer ticket` does with a ticket's branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketAction {
    Changes,
    Merge,
    Discard,
}

/// `composer ticket changes|merge|discard <id>`. Discarding without `yes` only shows what
/// would go.
pub async fn ticket(
    server_url: &str,
    token: &str,
    action: TicketAction,
    ticket: String,
    yes: bool,
) -> anyhow::Result<()> {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/ws?token={token}", base.replacen("http", "ws", 1));
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connecting to {base} (is `composer serve` running?)"))?;
    let mut send = async |id, command| {
        let json = serde_json::to_string(&ClientRequest { id, command }).expect("serializes");
        socket.send(Message::Text(json.into())).await?;
        next_reply(&mut socket).await
    };
    match action {
        TicketAction::Changes => {
            let CommandResult::Changes { changes } =
                send(1, ClientCommand::TicketChanges { ticket }).await?
            else {
                bail!("unexpected reply");
            };
            print_changes(&changes, true);
        }
        TicketAction::Merge => {
            let CommandResult::Merged { commit } = send(
                1,
                ClientCommand::TicketMerge {
                    ticket: ticket.clone(),
                },
            )
            .await?
            else {
                bail!("unexpected reply");
            };
            println!("ticket {ticket} merged as {commit}; its worktree and branch are removed");
        }
        TicketAction::Discard if !yes => {
            let CommandResult::Changes { changes } = send(
                1,
                ClientCommand::TicketChanges {
                    ticket: ticket.clone(),
                },
            )
            .await?
            else {
                bail!("unexpected reply");
            };
            print_changes(&changes, false);
            println!("\nThis would delete all of it. Run again with --yes to discard.");
        }
        TicketAction::Discard => {
            send(
                1,
                ClientCommand::TicketDiscard {
                    ticket: ticket.clone(),
                },
            )
            .await?;
            println!("ticket {ticket}'s worktree and branch are removed");
        }
    }
    Ok(())
}

fn print_changes(changes: &ProjectDiff, diff: bool) {
    if let Some(review) = &changes.ticket {
        let target = review.target.as_deref().unwrap_or("(detached HEAD)");
        println!(
            "{} → {target} · {} commit(s){}",
            review.branch,
            review.commits.len(),
            if review.uncommitted {
                " + uncommitted changes"
            } else {
                ""
            }
        );
        for commit in &review.commits {
            println!("  {commit}");
        }
        if !review.conflicts.is_empty() {
            println!(
                "conflicts with {target} in: {}",
                review.conflicts.join(", ")
            );
        }
    }
    println!();
    for file in &changes.files {
        println!(
            "{} {}  +{} -{}",
            file.status, file.path, file.added, file.removed
        );
    }
    if diff && !changes.diff.is_empty() {
        println!("\n{}", changes.diff.trim_end());
    }
}
