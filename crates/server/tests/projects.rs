//! Projects: the list (search, folders outside the projects directory, session counts) and one
//! project's overview (git state, works, ticket branches, memory, switches).

mod support;

use std::path::Path;
use std::process::Command;

use composer_core::SiteUrl;
use composer_protocol::http::ProjectSummary;
use composer_protocol::{ChangedBy, ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use support::*;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[tokio::test]
async fn list_search_and_overview() {
    let projects = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;

    // game: a git project with a ticket worktree; notes: a plain folder; tools: outside the
    // projects directory, known from a session.
    let game = projects.path().join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join("main.gd"), "extends Node\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "."],
        &["commit", "-qm", "init"],
    ] {
        git(&game, args);
    }
    std::fs::write(game.join("wip.gd"), "# wip\n").unwrap();
    let worktree = dir.path().join("state/worktrees/0031");
    git(
        &game,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "ticket/0031",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("save.gd"), "# save\n").unwrap();
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-qm", "save slots"]);
    std::fs::create_dir_all(projects.path().join("notes")).unwrap();
    let tools = elsewhere.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();

    for site in ["coding", "video"] {
        std::fs::create_dir_all(dir.path().join("sites").join(site)).unwrap();
        std::fs::write(
            dir.path().join("sites").join(site).join("index.md"),
            format!("---\ntitle: {site}\ntype: pipeline\n---\n# {site}\n"),
        )
        .unwrap();
    }
    let page = |site: &str, path: &str, text: String| {
        server
            .write_page(&SiteUrl::new(site, path).unwrap(), &text, ChangedBy::Agent)
            .unwrap();
    };
    page(
        "orchestration",
        "tickets/0031.md",
        "---\ntitle: Save slots\ntype: ticket\nstatus: done\n---\n# Save slots\n".into(),
    );
    page(
        "memory",
        "projects/game.md",
        "---\ntitle: game\ntype: memory\n---\n# game\n\nGodot 4.4, C#.\n\nSaves go through SaveService.\n".into(),
    );
    page(
        "coding",
        "game-save-system/index.md",
        format!(
            "---\ntitle: Save system\ntype: work\nproject: {}\nstatus: 03 implement\n---\n# Save system\n",
            game.display()
        ),
    );
    page(
        "video",
        "other/index.md",
        "---\ntitle: Other\ntype: work\nproject: notes\n---\n# Other\n".into(),
    );

    let game_folder = game.canonicalize().unwrap().display().to_string();
    for project in [
        game_folder.clone(),
        tools.canonicalize().unwrap().display().to_string(),
    ] {
        server.sessions.create("default", Some(project)).unwrap();
    }

    // The list: all three; `game` and `tools` have a session; a search narrows it.
    let list = |q: &str| {
        let url = format!("http://{addr}/api/projects?q={q}");
        let token = server.token.clone();
        async move {
            reqwest::Client::new()
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .json::<Vec<ProjectSummary>>()
                .await
                .unwrap()
        }
    };
    let all = list("").await;
    let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"game") && names.contains(&"notes") && names.contains(&"tools"),
        "{names:?}"
    );
    let game_summary = all.iter().find(|p| p.name == "game").unwrap();
    assert_eq!(
        (game_summary.sessions, game_summary.folder.as_str()),
        (1, game_folder.as_str())
    );
    assert!(game_summary.last_used.is_some());
    assert_eq!(all.iter().find(|p| p.name == "notes").unwrap().sessions, 0);
    let found = list("GA").await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "game");
    // The projects directory's own path matches nothing; a path outside it does.
    let dir_name = projects
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(list(&dir_name).await.is_empty(), "{dir_name}");
    let outside = elsewhere
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(list(&outside).await.len(), 1);
    assert!(list("nothing-like-this").await.is_empty());

    // The overview.
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::ProjectOverview {
            project: "game".into(),
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Project { overview },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    assert_eq!(overview.name, "game");
    assert_eq!(overview.folder, game_folder);
    assert_eq!(overview.branch.as_deref(), Some("main"));
    assert_eq!(overview.uncommitted, 1);
    assert_eq!(overview.works.len(), 1, "only the work linking game");
    assert_eq!(
        (
            overview.works[0].pipeline.as_str(),
            overview.works[0].title.as_str(),
            overview.works[0].status.as_deref()
        ),
        ("coding", "Save system", Some("03 implement"))
    );
    assert_eq!(overview.branches.len(), 1);
    let branch = &overview.branches[0];
    assert_eq!(
        (
            branch.ticket.as_str(),
            branch.title.as_str(),
            branch.branch.as_str(),
            branch.commits
        ),
        ("0031", "Save slots", "ticket/0031", 1)
    );
    assert_eq!(branch.status.as_deref(), Some("done"));
    let memory = overview.memory.as_ref().unwrap();
    assert_eq!(memory.url, "site://memory/projects/game.md");
    assert_eq!(
        memory.excerpt,
        "Godot 4.4, C#. Saves go through SaveService."
    );
    assert_eq!(overview.tool_rules, 0);

    // A plain folder: no git, nothing waiting.
    send(
        &mut socket,
        2,
        ClientCommand::ProjectOverview {
            project: "notes".into(),
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Project { overview },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    assert_eq!((overview.branch.clone(), overview.uncommitted), (None, 0));
    assert!(overview.branches.is_empty() && overview.memory.is_none());
    assert_eq!(overview.works.len(), 1, "the work linking notes by name");
}
