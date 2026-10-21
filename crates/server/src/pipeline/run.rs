//! Running a work's stages: each stage is a ticket in `site://orchestration` worked by a stage
//! worker with scoped context; a stage without a gate starts the next one when it is done.

use std::path::Path;
use std::sync::Arc;

use composer_core::{Page, SiteUrl};
use composer_protocol::{ChangedBy, ServerMessage, WorkerState};

use crate::pipeline::pipeline::Pipeline;
use crate::pipeline::stage_contract::StageContract;
use crate::pipeline::types::FINISHED;
use crate::pipeline::work::Work;
use crate::server::Server;
use crate::session::Session;
use crate::worker::ticket::{set_field, ticket_url};

/// The agent a stage runs as unless its contract names another.
const STAGE_AGENT: &str = "stage";

/// A stage that was started.
pub struct StageStarted {
    pub work: Work,
    pub stage: String,
    pub ticket: String,
    pub worker: String,
}

/// A work by its URL (`site://coding/save-system/index.md`, `site://coding/save-system`) or
/// `coding/save-system`.
pub fn find_work(root: &Path, spec: &str) -> Result<Work, String> {
    let spec = spec
        .trim()
        .trim_start_matches("site://")
        .trim_end_matches('/');
    let spec = spec.strip_suffix("/index.md").unwrap_or(spec);
    let (site, name) = spec
        .split_once('/')
        .filter(|(_, name)| !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| {
            format!("`{spec}` is not a work: give <pipeline>/<work>, e.g. coding/save-system")
        })?;
    let pipeline = Pipeline::from_dir(site, &root.join(site))
        .ok_or_else(|| format!("site://{site} is not a pipeline"))?;
    Work::load(pipeline, name).ok_or_else(|| format!("site://{site}/{name}/ is not a work"))
}

/// The stage to run: `stage` (a name, `02`, or `plan`), else the first one not finished.
fn pick_stage<'a>(work: &'a Work, stage: Option<&str>) -> Result<&'a StageContract, String> {
    let stages = &work.pipeline.stages;
    if stages.is_empty() {
        return Err(format!(
            "site://{} has no stage contracts in stages/",
            work.pipeline.name
        ));
    }
    match stage.map(str::trim).filter(|s| !s.is_empty()) {
        Some(wanted) => stages
            .iter()
            .find(|s| {
                s.name == wanted
                    || s.number() == wanted
                    || s.name.split_once('_').is_some_and(|(_, n)| n == wanted)
            })
            .ok_or_else(|| {
                let names: Vec<&str> = stages.iter().map(|s| s.name.as_str()).collect();
                format!("no stage `{wanted}` (stages: {})", names.join(", "))
            }),
        None => {
            // The first that is stale (rerun from there) or not finished.
            let states = work.stages();
            let next = stages
                .iter()
                .zip(&states)
                .find(|(_, s)| s.stale || !FINISHED.contains(&s.status.as_str()))
                .ok_or_else(|| {
                    format!(
                        "every stage of {} is done; give a stage to run one again",
                        work.name
                    )
                })?;
            let (next, state) = next;
            if state.status == "review" && !state.stale {
                return Err(format!(
                    "{} waits for review: approve it (`status: approved` in its _stage.md) or run it again with stage {}",
                    next.name, next.name
                ));
            }
            Ok(next)
        }
    }
}

/// Starts a stage of `work` as a ticket worked by a stage worker that belongs to `parent`.
pub async fn run_stage(
    server: &Arc<Server>,
    parent: &Arc<Session>,
    work: &str,
    stage: Option<&str>,
    note: Option<&str>,
) -> Result<StageStarted, String> {
    let work = find_work(server.workspace.root(), work)?;
    // Running is what has a worker now (a `running` left by a stopped server is not).
    let tickets: Vec<String> = server
        .workers
        .running()
        .into_iter()
        .map(|(_, h)| h.ticket)
        .collect();
    if let Some(running) = work.pipeline.stages.iter().find(|s| {
        work.stage_field(&s.name, "ticket")
            .is_some_and(|t| tickets.contains(&t))
    }) {
        return Err(format!(
            "{} of {} is running; wait for it",
            running.name, work.name
        ));
    }
    let contract = pick_stage(&work, stage)?.clone();
    if contract.worktree && work.project.is_none() {
        return Err(format!(
            "{} writes code in the work's project, and {} links none (set `project:` in its index.md)",
            contract.name, work.name
        ));
    }
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    let ticket = new_ticket(server, &work, &contract, note)?;
    // Before the worker starts: one that finishes quickly must not be overwritten.
    let now = chrono::Local::now().to_rfc3339();
    let quoted = format!("\"{ticket}\"");
    let session = format!("\"{}\"", parent.id());
    // What it reads, by hash: when one of them changes, the stage is stale.
    let inputs = serde_json::to_string(&work.input_hashes(&contract)).expect("a map serializes");
    let mut fields = vec![
        ("status", "running"),
        ("ticket", quoted.as_str()),
        ("started", now.as_str()),
        // Who hears about it, and whose session the next stage joins after a gate.
        ("session", session.as_str()),
        // A rerun's note is in the ticket now.
        ("note", "\"\""),
        ("inputs", inputs.as_str()),
    ];
    if let Some(harness) = &contract.harness {
        fields.push(("harness", harness));
    }
    set_stage_fields(server, &work, &contract.name, &fields);
    let worker = match server
        .workers
        .spawn(server, parent, &ticket, None, None)
        .await
    {
        Ok((worker, _)) => worker,
        Err(e) => {
            set_stage_fields(
                server,
                &work,
                &contract.name,
                &[("status", "failed"), ("error", &one_line(&e))],
            );
            return Err(e);
        }
    };
    Ok(StageStarted {
        stage: contract.name.clone(),
        work,
        ticket,
        worker,
    })
}

/// Writes the stage's ticket: the work, its stage contract, where the outputs go.
fn new_ticket(
    server: &Server,
    work: &Work,
    contract: &StageContract,
    note: Option<&str>,
) -> Result<String, String> {
    let site = &work.pipeline.name;
    let folder = format!("site://{site}/{}/{}/", work.name, contract.name);
    let title = format!(
        "{site}/{} · {} {}",
        work.name,
        contract.number(),
        contract.title
    );
    let mut fm = format!(
        "---\ntitle: {}\ntype: ticket\nstatus: open\nagent: {}\n",
        serde_json::to_string(&title).expect("a string serializes"),
        contract.agent.as_deref().unwrap_or(STAGE_AGENT)
    );
    if let Some(harness) = &contract.harness {
        fm.push_str(&format!("harness: {harness}\n"));
    }
    if let Some(model) = &contract.model {
        fm.push_str(&format!("model: {model}\n"));
    }
    if let Some(project) = &work.project {
        fm.push_str(&format!("project: {project}\n"));
    }
    if !contract.worktree {
        // Only stages that write code get a worktree; the others read the project.
        fm.push_str("worktree: false\n");
    }
    if let Some(note) = note {
        fm.push_str(&format!("note: {}\n", one_line(note)));
    }
    fm.push_str(&format!(
        "pipeline: site://{site}/stages/{}.md\nwork: {}\nstage: {}\ncreated: {}\n---\n",
        contract.name,
        work.url(),
        contract.name,
        chrono::Local::now().format("%Y-%m-%d")
    ));
    let outputs: Vec<String> = contract
        .outputs
        .iter()
        .map(|o| format!("- [ ] `{o}`"))
        .chain(
            contract
                .verify
                .iter()
                .map(|c| format!("- [ ] verified: {c}")),
        )
        .collect();
    let body = format!(
        "# {title}\n\n## Task\n\nRun stage {} {} of [{}]({}) as [its contract](site://{site}/stages/{}.md) says. \
         Your context holds the pipeline, the work, the contract and its input files. Write the outputs into \
         `{folder}`.\n\n## Done when\n\n{}\n\n## Result\n",
        contract.number(),
        contract.title,
        work.title,
        work.url(),
        contract.name,
        if outputs.is_empty() {
            "- the contract's Outputs are written".to_string()
        } else {
            outputs.join("\n")
        }
    );
    let tickets = server.workspace.root().join("orchestration/tickets");
    std::fs::create_dir_all(&tickets).map_err(|e| e.to_string())?;
    let last = std::fs::read_dir(&tickets)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".md")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    let id = format!("{:04}", last + 1);
    let url = ticket_url(&id).map_err(|e| e.to_string())?;
    let text = fm.replacen(
        "type: ticket\n",
        &format!("type: ticket\nid: \"{id}\"\n"),
        1,
    ) + &body;
    server
        .write_page(&url, &text, ChangedBy::Agent)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Sets fields of a stage's `_stage.md`.
pub(crate) fn set_stage_fields(server: &Server, work: &Work, stage: &str, fields: &[(&str, &str)]) {
    let Ok(url) = SiteUrl::new(
        &work.pipeline.name,
        &format!("{}/{stage}/_stage.md", work.name),
    ) else {
        return;
    };
    let raw = server
        .workspace
        .read_page(&url)
        .map(|p| p.raw)
        .unwrap_or_else(|_| {
            format!("---\ntitle: {stage}\ntype: stage-run\nstage: {stage}\n---\n# {stage}\n")
        });
    let raw = fields
        .iter()
        .fold(raw, |raw, (key, value)| set_field(&raw, key, value));
    if let Err(e) = server.write_page(&url, &raw, ChangedBy::Agent) {
        tracing::error!("updating {url}: {e}");
    }
}

/// A stage ticket's worker ended: record it in the stage's `_stage.md`; a finished stage
/// without a gate starts the next one. Returns what the parent should be told, if anything:
/// `None` for a ticket that isn't a stage's.
pub fn stage_ended(
    server: &Arc<Server>,
    ticket: &Page,
    parent: &str,
    state: WorkerState,
    summary: &str,
) -> Option<Option<String>> {
    let work = ticket.frontmatter.get_str("work")?;
    let stage = ticket.frontmatter.get_str("stage")?;
    let work = match find_work(server.workspace.root(), &work) {
        Ok(work) => work,
        Err(e) => return Some(Some(format!("[stage {stage}: {e}]"))),
    };
    let gate = work.pipeline.stage(&stage).is_some_and(|c| c.gate);
    let status = match state {
        WorkerState::Done if gate => "review",
        WorkerState::Done => "done",
        WorkerState::Paused => "paused",
        _ => "failed",
    };
    let now = chrono::Local::now().to_rfc3339();
    set_stage_fields(
        server,
        &work,
        &stage,
        &[
            ("status", status),
            ("finished", &now),
            ("summary", &one_line(summary)),
            // A fresh run: its output is the agent's again.
            ("edited", "false"),
        ],
    );
    if matches!(status, "done" | "review") {
        // As the agent wrote it, to tell your edits from it at the gate.
        crate::pipeline::learning::keep_agent_output(&work, &stage);
    }
    server.hub.send(ServerMessage::StageEnded {
        work: work.url(),
        title: work.title.clone(),
        stage: stage.clone(),
        status: status.to_string(),
        summary: summary.to_string(),
    });
    let label = format!("{}/{}", work.pipeline.name, work.name);
    let folder = format!("site://{label}/{stage}/");
    let next = work
        .pipeline
        .stages
        .iter()
        .skip_while(|s| s.name != stage)
        .nth(1)
        .map(|s| s.name.clone());
    Some(match (status, next) {
        ("done", Some(next)) => {
            // Not from inside the worker's ending: the next one starts on its own task.
            let server = server.clone();
            let parent = parent.to_string();
            let spec = label.clone();
            tokio::spawn(async move {
                let Ok(parent) = server.sessions.get(&parent) else {
                    return;
                };
                if let Err(e) = run_stage(&server, &parent, &spec, Some(&next), None).await {
                    tracing::warn!("starting {next} of {spec}: {e}");
                }
            });
            None
        }
        ("done", None) => Some(format!(
            "[work {label}: every stage is done]\n{summary}\n\nThe last stage wrote into {folder}."
        )),
        ("review", _) => Some(format!(
            "[work {label}: stage {stage} waits for review]\n{summary}\n\nIts output is in {folder}. \
             The user approves it (`status: approved` in its _stage.md) or has it run again."
        )),
        (other, _) => Some(format!("[work {label}: stage {stage} {other}]\n{summary}")),
    })
}

fn one_line(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    serde_json::to_string(&line).expect("a string serializes")
}

/// The session a work's stages belong to when nobody asked for them in a chat: the one its
/// last stage ran under (`session:` in `_stage.md`), else a new one in the work's project.
pub fn work_session(
    server: &Server,
    work: &Work,
    last: Option<&str>,
) -> Result<Arc<Session>, String> {
    if let Some(session) = last
        .and_then(|id| work.stage_field(id, "session"))
        .and_then(|id| server.sessions.get(&id).ok())
    {
        return Ok(session);
    }
    let project = work
        .project
        .as_deref()
        .and_then(|p| crate::tools::resolve_project(p, &server.config.projects.dir).ok());
    let session = server
        .sessions
        .create("default", project)
        .map_err(|e| e.to_string())?;
    let label = format!("work {}/{}", work.pipeline.name, work.name);
    session.update_info(&server.hub, |info| info.preview = Some(label));
    Ok(session)
}

/// Watches stages' `_stage.md` pages for what the user decided at a gate: `approved` starts
/// the next stage (if it hasn't run), `rerun` runs the stage again with its `note`. Whoever
/// wrote it (the board app, the TUI, an editor, an agent).
pub async fn run_gates(server: Arc<Server>) {
    let mut events = server.hub.subscribe();
    crate::pipeline::learning::ask_open_proposals(&server);
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => return,
        };
        let ServerMessage::PageChanged { url, .. } = &*event else {
            continue;
        };
        if url.ends_with("/_stage.md")
            && let Err(e) = gate_changed(&server, url).await
        {
            tracing::warn!("{url}: {e}");
        }
    }
}

async fn gate_changed(server: &Arc<Server>, url: &str) -> Result<(), String> {
    let url = SiteUrl::parse(url).map_err(|e| e.to_string())?;
    let mut parts = url.path().split('/');
    let (Some(name), Some(stage), Some("_stage.md"), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(());
    };
    let Ok(work) = find_work(server.workspace.root(), &format!("{}/{name}", url.site())) else {
        return Ok(());
    };
    match work.stage_status(stage).as_str() {
        "approved" => {
            // What you changed before approving it (once: the edit is recorded then).
            if work.stage_field(stage, "edited").as_deref() != Some("true") {
                crate::pipeline::learning::gate_approved(server, &work, stage);
            }
            let Some(next) = work
                .pipeline
                .stages
                .iter()
                .skip_while(|s| s.name != stage)
                .nth(1)
                .map(|s| s.name.clone())
            else {
                return Ok(());
            };
            // Only a stage that hasn't run, or whose inputs changed (a rerun going on through
            // the stages after it): approving again (or editing the page) does nothing.
            let fresh = work
                .stages()
                .iter()
                .find(|s| s.name == next)
                .is_some_and(|s| s.status == "waiting" || s.stale);
            if !fresh {
                return Ok(());
            }
            let parent = work_session(server, &work, Some(stage))?;
            let spec = format!("{}/{name}", url.site());
            run_stage(server, &parent, &spec, Some(&next), None)
                .await
                .map(|_| ())
        }
        "rerun" => {
            let note = work.stage_field(stage, "note");
            let parent = work_session(server, &work, Some(stage))?;
            let spec = format!("{}/{name}", url.site());
            run_stage(server, &parent, &spec, Some(stage), note.as_deref())
                .await
                .map(|_| ())
        }
        _ => Ok(()),
    }
}
