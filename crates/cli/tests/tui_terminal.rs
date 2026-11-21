//! The TUI's second pane shows a server terminal (`:term`): its output drawn by the terminal
//! model, keys forwarded to it while it has the focus, ctrl-space back to the chat.

mod support;

use composer_protocol::RunKind;
use composer_server::{Home, Server, init_home};
use composer_tui::{Startup, TuiOptions, run_script};
use support::fake_model;

#[tokio::test]
async fn a_terminal_tab_renders_its_output_and_takes_keys() {
    // SAFETY: before the server starts; the only test in this binary.
    unsafe { std::env::set_var("SHELL", "/bin/sh") };
    let base_url = fake_model().await;
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    std::fs::write(
        home.config_file(),
        format!("[provider]\nbase_url = \"{base_url}\"\n"),
    )
    .unwrap();
    let server = Server::start(home).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });

    let options = TuiOptions {
        server_url: format!("http://{addr}"),
        token: server.token.clone(),
        startup: Startup {
            session: None,
            new: true,
            kind: RunKind::Agentic,
            site: None,
        },
        context_limit: 100_000,
    };
    let out = tempfile::tempdir().unwrap();
    let script = "\
size 140x30
wait connected
wait session
keys :term<Enter>
wait text term \u{b7} 10
keys echo typed-$((6*7))<Enter>
wait text typed-42 10
snap terminal
keys <C-Space>
keys :ps<Enter>
wait text no background processes 10
snap chat
";
    let snaps = run_script(options, script, out.path()).await.unwrap();
    assert_eq!(snaps.len(), 2);
    let terminal = std::fs::read_to_string(out.path().join("terminal.txt")).unwrap();
    // The shell echoed what was typed and ran it: keys went to it, its output was drawn.
    assert!(terminal.contains("echo typed-$((6*7))"), "{terminal}");
    assert!(terminal.contains("typed-42"), "{terminal}");
    assert!(
        terminal.contains("ctrl-space: back to the chat"),
        "{terminal}"
    );
    let chat = std::fs::read_to_string(out.path().join("chat.txt")).unwrap();
    assert!(!chat.contains("ctrl-space: back to the chat"), "{chat}");
}
