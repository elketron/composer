//! A site's `folder:` counts only once the user approved that folder for the site, the way its
//! `mcp.toml` does: until then the site works as if it named none (no mount into it, terminals
//! and processes start elsewhere, no `always` rule of the folder applies). Setting the site asks
//! the user's clients; a `folder:` naming another folder asks again and the old one stops
//! counting; a run that is not stored can only use an earlier approval.

mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use composer_protocol::{ApprovalDecision, ApprovalKind, RunKind, ServerMessage};
use composer_server::agent::{Entered, FOLDER_APPROVAL, Site, SiteSet, set_site};
use composer_server::provider::{ModelEvent, ModelMessage};
use composer_server::runs::HookConfig;
use composer_server::session::{MemorySink, Session};
use composer_server::tools::ToolContext;
use composer_server::{Config, Server};
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

/// One approval request as a client saw it.
#[derive(Debug, Clone)]
struct Asked {
    kind: ApprovalKind,
    recipe: String,
    args: Vec<String>,
    body: String,
}

/// Answers every approval request: `folder:` ones with what `folder` holds at the time, any
/// other with Approve. Keeps what it was asked.
struct Answerer {
    folder: Arc<Mutex<ApprovalDecision>>,
    asked: Arc<Mutex<Vec<Asked>>>,
}

impl Answerer {
    fn start(server: &Arc<Server>, folder: ApprovalDecision) -> Self {
        let folder = Arc::new(Mutex::new(folder));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let mut events = server.hub.subscribe();
        let (server, answer, seen) = (server.clone(), folder.clone(), asked.clone());
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if let ServerMessage::ApprovalRequest {
                    request,
                    kind,
                    recipe,
                    args,
                    body,
                    ..
                } = &*event
                {
                    seen.lock().unwrap().push(Asked {
                        kind: *kind,
                        recipe: recipe.clone(),
                        args: args.clone(),
                        body: body.clone(),
                    });
                    let decision = match recipe.as_str() {
                        FOLDER_APPROVAL => *answer.lock().unwrap(),
                        _ => ApprovalDecision::Approve,
                    };
                    server.pending_approvals.resolve(request, decision);
                }
            }
        });
        Self { folder, asked }
    }

    fn answer(&self, decision: ApprovalDecision) {
        *self.folder.lock().unwrap() = decision;
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn folder_asks(&self) -> Vec<Asked> {
        self.asked()
            .into_iter()
            .filter(|a| a.recipe == FOLDER_APPROVAL)
            .collect()
    }
}

/// `site://game` naming `folder`.
fn game_site(server: &Server, folder: &Path) {
    let site = server.home.sites().join("game");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("index.md"),
        format!(
            "---\ntitle: Game\nfolder: {}\n---\n# Game\n",
            folder.display()
        ),
    )
    .unwrap();
    server.refresh_mounts();
}

fn project(root: &Path, name: &str) -> PathBuf {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("notes.md"), format!("# {name}\n")).unwrap();
    folder.canonicalize().unwrap()
}

async fn enter(server: &Arc<Server>, session: &Arc<Session>) -> Entered {
    match set_site(
        server,
        &*server.sink(session),
        Some("game"),
        &CancellationToken::new(),
    )
    .await
    .unwrap()
    {
        SiteSet::Entered(entered) => entered,
        other => panic!("{other:?}"),
    }
}

fn context(server: &Arc<Server>, session: &Arc<Session>) -> ToolContext {
    ToolContext {
        server: server.clone(),
        run: server.sink(session),
        kind: RunKind::Agentic,
        only: None,
        site: Site::load(&server.web, "game").ok().map(Arc::new),
        cancel: CancellationToken::new(),
    }
}

/// Where `process_start` with no `cwd` ran its command, from its answer.
async fn process_cwd(server: &Arc<Server>, session: &Arc<Session>) -> String {
    let result = server
        .tools
        .call_value(
            &context(server, session),
            "process_start",
            json!({ "command": "true" }),
        )
        .await;
    assert!(!result.is_error, "{}", result.text);
    let (_, rest) = result.text.split_once("(in ").unwrap();
    rest.split_once(')').unwrap().0.to_string()
}

fn mount_reads(server: &Server) -> bool {
    let url = composer_core::SiteUrl::new("game", "src/notes.md").unwrap();
    server.web.read_page(&url).is_ok()
}

#[tokio::test]
async fn an_unapproved_folder_counts_for_nothing_until_approved() {
    let (_dir, server, _p, _addr) = start_with(Config::defaults(), vec![]).await;
    let projects = tempfile::tempdir().unwrap();
    let one = project(projects.path(), "one");
    game_site(&server, &one);
    std::os::unix::fs::symlink(&one, server.home.sites().join("game/src")).unwrap();
    let answers = Answerer::start(&server, ApprovalDecision::Deny);
    let session = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();

    // Refused: asked once, naming the site and the folder; no mount, no session folder,
    // processes start in the sites folder, not in it.
    let entered = enter(&server, &session).await;
    assert!(!entered.folder.approved);
    assert!(
        entered.notes().contains("not approved"),
        "{}",
        entered.notes()
    );
    let asked = answers.folder_asks();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].kind, ApprovalKind::Recipe);
    assert_eq!(asked[0].args, vec![one.display().to_string()]);
    assert!(
        asked[0].body.contains("site://game") && asked[0].body.contains(&one.display().to_string()),
        "{}",
        asked[0].body
    );
    assert!(!mount_reads(&server));
    assert_eq!(server.session_folder(&session.info()), None);
    let sites = server.home.sites().display().to_string();
    assert_eq!(process_cwd(&server, &session).await, sites);

    // Approved: the folder counts, the mount reads, processes start there.
    answers.answer(ApprovalDecision::Approve);
    let entered = enter(&server, &session).await;
    assert!(entered.folder.approved, "{entered:?}");
    assert_eq!(answers.folder_asks().len(), 2);
    assert!(mount_reads(&server));
    assert_eq!(server.session_folder(&session.info()), Some(one.clone()));
    assert_eq!(
        process_cwd(&server, &session).await,
        one.display().to_string()
    );

    // Remembered: setting the site again asks nothing.
    assert!(enter(&server, &session).await.folder.approved);
    assert_eq!(answers.folder_asks().len(), 2);

    // `folder:` now names another folder: the old one stops counting at once (no mount into
    // it, no session folder), and setting the site asks about the new one.
    let two = project(projects.path(), "two");
    let url = composer_core::SiteUrl::new("game", "index.md").unwrap();
    server
        .write_page(
            &url,
            &format!("---\ntitle: Game\nfolder: {}\n---\n# Game\n", two.display()),
            composer_protocol::ChangedBy::Agent,
        )
        .unwrap();
    assert!(
        !mount_reads(&server),
        "the mount into the old folder still reads"
    );
    assert_eq!(server.session_folder(&session.info()), None);
    answers.answer(ApprovalDecision::Deny);
    let entered = enter(&server, &session).await;
    assert!(!entered.folder.approved);
    let asked = answers.folder_asks();
    assert_eq!(asked.len(), 3);
    assert_eq!(asked[2].args, vec![two.display().to_string()]);
    assert_eq!(server.session_folder(&session.info()), None);

    // A refusal changed nothing kept; approving the second folder forgets the first, so going
    // back to it asks again.
    answers.answer(ApprovalDecision::Approve);
    assert!(enter(&server, &session).await.folder.approved);
    assert_eq!(answers.folder_asks().len(), 4);
    assert_eq!(server.session_folder(&session.info()), Some(two.clone()));
    std::fs::write(
        server.home.sites().join("game/index.md"),
        format!("---\ntitle: Game\nfolder: {}\n---\n# Game\n", one.display()),
    )
    .unwrap();
    assert!(enter(&server, &session).await.folder.approved);
    assert_eq!(answers.folder_asks().len(), 5);
}

#[tokio::test]
async fn always_rules_of_a_folder_need_the_folder_approved() {
    let (_dir, server, _p, _addr) = start_with(Config::defaults(), vec![]).await;
    let projects = tempfile::tempdir().unwrap();
    let one = project(projects.path(), "one");
    game_site(&server, &one);
    // The user said `always` to `true` in that folder, through some other site.
    server
        .tool_rules
        .add(&format!("terminal:{}", one.display()), "Bash(true:*)")
        .unwrap();
    let answers = Answerer::start(&server, ApprovalDecision::Deny);
    let session = server
        .sessions
        .create(RunKind::Agentic, None, Some("game".into()))
        .unwrap();

    // Through a site whose folder is not approved, the rule does not apply: it is asked.
    process_cwd(&server, &session).await;
    let tools: Vec<Asked> = answers
        .asked()
        .into_iter()
        .filter(|a| a.kind == ApprovalKind::Tool)
        .collect();
    assert_eq!(tools.len(), 1, "{tools:?}");

    // Once the folder is approved for the site, it does: nothing more is asked.
    approve_folder(&server, "game");
    assert_eq!(
        process_cwd(&server, &session).await,
        one.display().to_string()
    );
    let tools = answers
        .asked()
        .into_iter()
        .filter(|a| a.kind == ApprovalKind::Tool)
        .count();
    assert_eq!(tools, 1);
}

#[tokio::test]
async fn an_unstored_run_only_uses_an_earlier_approval() {
    let mut config = Config::defaults();
    config.hooks.insert(
        "work".into(),
        HookConfig {
            token_file: String::new(),
            token: "hook-secret".into(),
            kind: RunKind::Agentic,
            site: Some("game".into()),
            folder: None,
            prompt: "{{prompt}}".into(),
            harness: None,
            store: false,
            timeout_secs: 30,
        },
    );
    let reply = || vec![ModelEvent::Content("done".into()), finished(vec![])];
    let (_dir, server, provider, addr) = start_with(config, vec![reply(), reply()]).await;
    let projects = tempfile::tempdir().unwrap();
    let one = project(projects.path(), "one");
    game_site(&server, &one);
    let answers = Answerer::start(&server, ApprovalDecision::Approve);

    // Setting the site in a run that is not stored asks nobody, and the folder does not count.
    let sink = MemorySink::new("go", None);
    let set = set_site(&server, &sink, Some("game"), &CancellationToken::new())
        .await
        .unwrap();
    let SiteSet::Entered(entered) = set else {
        panic!("{set:?}")
    };
    assert!(!entered.folder.approved);
    assert!(answers.asked().is_empty());

    let post = || async {
        reqwest::Client::new()
            .post(format!("http://{addr}/api/hooks/work"))
            .bearer_auth("hook-secret")
            .json(&json!({ "prompt": "go" }))
            .send()
            .await
            .unwrap()
    };
    let system = |n: usize| match &provider.requests()[n].messages[0] {
        ModelMessage::System { content } => content.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(post().await.status(), 200);
    let first = system(0);
    assert!(first.contains("None yet"), "{first}");
    assert!(!first.contains("The site works on"), "{first}");
    assert!(answers.asked().is_empty());

    // Approved earlier (in a client): the next unstored run works in it.
    approve_folder(&server, "game");
    assert_eq!(post().await.status(), 200);
    let second = system(1);
    assert!(
        second.contains(&format!("The site works on {}", one.display())),
        "{second}"
    );
}
