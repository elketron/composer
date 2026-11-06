use anyhow::{Context, bail};
use composer_protocol::{ClientCommand, ClientRequest, CommandResult};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use crate::client::ask::next_reply;

/// What `composer library` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryAction {
    List,
    Install {
        site: String,
        name: Option<String>,
    },
    Update {
        dry_run: bool,
    },
    /// Without `yes`, shows what would be published; with it, publishes exactly that.
    Publish {
        site: String,
        name: Option<String>,
        yes: bool,
    },
}

/// `composer library list|install|update`, through the running server of the workspace.
pub async fn library(server_url: &str, token: &str, action: LibraryAction) -> anyhow::Result<()> {
    let base = server_url.trim_end_matches('/');
    let url = format!("{}/ws?token={token}", base.replacen("http", "ws", 1));
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connecting to {base} (is `composer serve` running?)"))?;
    let mut send = async |id, command| -> anyhow::Result<CommandResult> {
        let json = serde_json::to_string(&ClientRequest { id, command }).expect("serializes");
        socket.send(Message::Text(json.into())).await?;
        next_reply(&mut socket).await
    };
    let command = match action {
        LibraryAction::List => ClientCommand::LibraryList,
        LibraryAction::Install { site, name } => ClientCommand::LibraryInstall { site, name },
        LibraryAction::Update { dry_run } => ClientCommand::LibraryUpdate { dry_run },
        LibraryAction::Publish { site, name, yes } => {
            let plan = ClientCommand::LibraryPublish {
                site: site.clone(),
                name: name.clone(),
                approved: None,
            };
            let CommandResult::LibraryPublishPlan { plan } = send(1, plan).await? else {
                bail!("unexpected reply");
            };
            print!("{}", plan.text);
            if !yes {
                println!("\nNothing was published. Run again with --yes to publish this.");
                return Ok(());
            }
            ClientCommand::LibraryPublish {
                site,
                name,
                approved: Some(plan.digest),
            }
        }
    };
    match send(2, command).await? {
        CommandResult::Library { sites } => {
            if sites.is_empty() {
                println!("The library is empty.");
            }
            for site in sites {
                let installed: Vec<String> = site
                    .installed
                    .iter()
                    .map(|i| match i.behind {
                        true => format!("{} (update available)", i.site),
                        false => i.site.clone(),
                    })
                    .collect();
                let here = match installed.is_empty() {
                    true => "not installed".to_string(),
                    false => format!("installed as {}", installed.join(", ")),
                };
                println!("{:<20} {}  {here}", site.site, site.commit);
            }
        }
        CommandResult::LibraryInstalled {
            site,
            from,
            commit,
            files,
            has_ui,
        } => {
            println!(
                "installed site://{site} from the library's {from} @ {commit} ({files} files)"
            );
            if has_ui {
                println!("It has an app: build it with its `ui-build` action.");
            }
        }
        CommandResult::LibraryUpdated { changes } => {
            if changes.is_empty() {
                println!("Every installed site is up to date.");
            }
            for change in &changes {
                println!("  {}  {}", change.path, change.outcome);
            }
            let reviews: Vec<&String> = changes.iter().filter_map(|c| c.review.as_ref()).collect();
            if !reviews.is_empty() {
                println!("\nThe library's versions of the files kept as yours:");
                for review in reviews {
                    println!("  {review}");
                }
                println!("Take what you want from them, then delete them.");
            }
        }
        CommandResult::LibraryPublished { site, name, commit } => {
            println!("\npublished site://{site} to the library as `{name}` @ {commit}");
        }
        _ => bail!("unexpected reply"),
    }
    Ok(())
}
