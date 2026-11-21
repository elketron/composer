//! `open` on a site folder lists what is in it, and a page open prints the hash `edit` can use.

mod support;

use composer_server::agent::AgentDefinition;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn a_folder_lists_and_a_page_shows_its_hash() {
    let (dir, server, _provider, _addr) = start(vec![]).await;
    write(
        &dir.path().join("sites/flow/index.md"),
        "---\ntitle: Flow\ntype: index\n---\n# Flow\n",
    );
    write(
        &dir.path().join("sites/flow/notes/a.md"),
        "---\ntitle: A\n---\n# A\n",
    );
    write(
        &dir.path().join("sites/flow/notes/b.md"),
        "---\ntitle: B\n---\n# B\n",
    );
    let session = server.sessions.create("default", None).unwrap();
    let context = ToolContext {
        server: server.clone(),
        session,
        agent: std::sync::Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: None,
        cancel: CancellationToken::new(),
    };
    let call = |url: &str| {
        let (server, context) = (server.clone(), &context);
        let url = url.to_string();
        async move {
            server
                .tools
                .call_value(context, "open", json!({ "url": url }))
                .await
        }
    };

    let folder = call("site://flow/notes").await;
    assert!(!folder.is_error, "{}", folder.text);
    assert!(
        folder.text.contains("a.md") && folder.text.contains("b.md"),
        "{}",
        folder.text
    );

    let page = call("site://flow/notes/a").await;
    assert!(!page.is_error, "{}", page.text);
    assert!(page.text.contains("hash: "), "{}", page.text);
    let hash = page
        .text
        .lines()
        .find_map(|line| line.strip_prefix("hash: "))
        .unwrap();
    assert!(!hash.is_empty());

    let edit = server
        .tools
        .call_value(
            &context,
            "edit",
            json!({
                "url": "site://flow/notes/a",
                "old_text": "# A\n",
                "new_text": "# A2\n",
                "if_hash": hash,
            }),
        )
        .await;
    assert!(!edit.is_error, "{}", edit.text);

    let stale = server
        .tools
        .call_value(
            &context,
            "edit",
            json!({
                "url": "site://flow/notes/a",
                "old_text": "# A2\n",
                "new_text": "# A3\n",
                "if_hash": hash,
            }),
        )
        .await;
    assert!(
        stale.is_error && stale.text.contains("changed since"),
        "{}",
        stale.text
    );

    let missing = server
        .tools
        .call_value(
            &context,
            "edit",
            json!({
                "url": "site://flow/notes/a",
                "old_text": "# not here\n",
                "new_text": "# no\n",
            }),
        )
        .await;
    assert!(
        missing.is_error && missing.text.contains("Nearest text:"),
        "{}",
        missing.text
    );
}
