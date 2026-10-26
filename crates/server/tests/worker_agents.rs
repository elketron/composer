//! Worker agents: which agents `spawn_worker` may start, and how orchestrators learn of them.

mod support;

use composer_core::SiteUrl;
use composer_protocol::{ChangedBy, SessionInfo, TurnState};
use composer_server::agent::{AgentDefinition, system_prompt};
use support::*;

fn agent_page(name: &str, frontmatter: &str) -> String {
    format!("---\ntitle: {name}\ntype: agent\nname: {name}\n{frontmatter}\n---\nYou are {name}.\n")
}

#[tokio::test]
async fn orchestrators_see_the_worker_agents() {
    let (_dir, server, _p, _addr) = start(vec![]).await;
    let pages = [
        (
            "coder",
            "role: worker\ndescription: writes code\ntools: [write, finish]",
        ),
        // From before `role:`: the finish tool makes it a worker.
        ("legacy", "tools: [open, finish]"),
        // A role other than worker, even with finish: not one.
        ("helper", "role: chat\ntools: [open, finish]"),
        ("chatty", "tools: [open]"),
    ];
    for (name, frontmatter) in pages {
        let url = SiteUrl::new("agents", &format!("{name}.md")).unwrap();
        server
            .write_page(&url, &agent_page(name, frontmatter), ChangedBy::External)
            .unwrap();
    }
    let index = server.index.read().unwrap();
    let workers: Vec<(String, String)> = AgentDefinition::workers(&server.workspace, &index)
        .into_iter()
        .map(|w| (w.name, w.description))
        .collect();
    assert_eq!(
        workers,
        [
            ("coder".to_string(), "writes code".to_string()),
            ("legacy".to_string(), "legacy".to_string()),
            (
                "worker".to_string(),
                "general worker; carries out any ticket".to_string()
            ),
        ]
    );

    let session = SessionInfo {
        id: "ab12".into(),
        agent: "orchestrator".into(),
        project: None,
        ticket: None,
        parent: None,
        worker: None,
        state: TurnState::Idle,
        last_seq: 0,
        context_tokens: 0,
        model: None,
        preview: None,
        updated: String::new(),
    };
    let prompt = |agent: &str| {
        let definition = AgentDefinition::load(&server.workspace, agent).unwrap();
        system_prompt(&definition, &server.workspace, &index, &session)
    };
    let orchestrator = prompt("orchestrator");
    assert!(
        orchestrator
            .contains("- worker agents (`spawn_worker`'s `agent`):\n  - `coder`: writes code\n"),
        "{orchestrator}"
    );
    assert!(!prompt("default").contains("`spawn_worker`'s `agent`"));
}
