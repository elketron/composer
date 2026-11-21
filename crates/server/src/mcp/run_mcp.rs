use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::init::local_token;
use crate::mcp::{McpOptions, McpServer, ServerLink};

/// Serves MCP on stdin/stdout until stdin closes (the harness exits) or the server goes away.
pub async fn run_mcp(home: Home, options: McpOptions) -> Result<()> {
    let token = local_token(&home)?;
    let link = ServerLink::open(&options.server_url, &token, &options.session)
        .await
        .map_err(ServerError::Invalid)?;
    // Requests are handled concurrently (a tool may wait for an approval); replies and
    // notifications are written one line at a time.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let server = Arc::new(McpServer::new(link.clone(), tx.clone()));
    let following = server.clone();
    let follower = tokio::spawn(async move { following.follow_changes().await });
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if stdout
                .write_all(format!("{line}\n").as_bytes())
                .await
                .is_err()
                || stdout.flush().await.is_err()
            {
                break;
            }
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else { break };
                let (server, tx) = (server.clone(), tx.clone());
                tokio::spawn(async move {
                    if let Some(reply) = server.handle(&line).await {
                        let _ = tx.send(reply);
                    }
                });
            }
            _ = link.closed() => {
                tracing::warn!("lost the server; stopping");
                break;
            }
        }
    }
    follower.abort();
    drop(tx);
    drop(server);
    let _ = writer.await;
    Ok(())
}
