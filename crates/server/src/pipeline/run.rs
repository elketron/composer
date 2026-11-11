//! Running a work's stages: each stage is a ticket in `site://orchestration` worked by a stage
//! worker with scoped context; a stage without a gate starts the next one when it is done.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use composer_core::{Page, SiteUrl};
use composer_protocol::{ChangedBy, ServerMessage, WorkerState};
use serde_json::{Map, Value};

use crate::pipeline::pipeline::Pipeline;
use crate::pipeline::stage_contract::StageContract;
use crate::pipeline::types::FINISHED;
use crate::pipeline::work::Work;
use crate::project::ProjectMode;
use crate::server::Server;
use crate::session::Session;
use crate::worker::ticket::{TicketId, set_field, ticket_url};

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
            // The first stage whose needs are done that is stale or not finished.
            let done = work.done();
            let finished = |name: &str| {
                done.iter().any(|d| d == name)
                    || matches!(work.stage_status(name).as_str(), "done" | "approved")
            };
            let states = work.stages();
            let next = stages
                .iter()
                .zip(&states)
                .find(|(contract, state)| {
                    contract.needs.iter().all(|need| finished(need))
                        && (state.stale || !FINISHED.contains(&state.status.as_str()))
                })
                .ok_or_else(|| {
                    format!(
                        "no stage of {} can start: finish or approve the stages before it, or give a stage",
                        work.name
                    )
                })?;
            let (next, state) = next;
            if state.status == "review" && !state.stale {
                return Err(format!(
                    "{} waits for review: approve it (`status: approved` in the work's index.md) or run it again with stage {}",
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
    let work = find_work(server.web.root(), work)?;
    // A gate that is approved and skipped (no page-change watcher): record the user's edits
    // before the next stage reads its output.
    for stage in &work.pipeline.stages {
        let approved = work.current().as_deref() == Some(stage.name.as_str())
            && work.current_status().as_deref() == Some("approved");
        if approved && work.stage_field(&stage.name, "edited").as_deref() != Some("true") {
            crate::pipeline::learning::gate_approved(server, &work, &stage.name);
        }
    }
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
    // A rerun's note is on the work's page when the gate set it; an explicit one wins.
    let note = note
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| work.stage_field(&contract.name, "note"));
    let note = note.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let ticket = new_ticket(server, &work, &contract, note)?;
    // Before the worker starts: one that finishes quickly must not be overwritten.
    let now = chrono::Local::now().to_rfc3339();
    let attempts = (work.attempts(&contract.name) + 1).to_string();
    // What it reads, by hash: when one of them changes, the stage is stale.
    let inputs = serde_json::to_string(&work.input_hashes(&contract)).expect("a map serializes");
    let mut fields = vec![
        ("stage", contract.name.as_str()),
        ("status", "running"),
        ("ticket", ticket.as_str()),
        ("started", now.as_str()),
        // Who hears about it, and whose session the next stage joins after a gate.
        ("session", parent.id()),
        // A rerun's note is in the ticket now.
        ("note", ""),
        ("inputs", inputs.as_str()),
        ("attempts", attempts.as_str()),
    ];
    if let Some(harness) = &contract.harness {
        fields.push(("harness", harness));
    }
    set_stage_fields(server, &work, &contract.name, &fields);
    let worker = match server
        .workers
        .spawn(server, parent, &TicketId::parse(&ticket)?, None, None)
        .await
    {
        Ok((worker, worktree)) => {
            if let Some(worktree) = &worktree {
                work.project_mount(server, &worktree.path, ProjectMode::Worktree);
            }
            worker
        }
        Err(e) => {
            set_stage_fields(
                server,
                &work,
                &contract.name,
                &[("status", "failed"), ("error", &one_line_raw(&e))],
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
    let tickets = server.web.root().join("orchestration/tickets");
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
    let url = ticket_url(&TicketId::parse(&id)?).map_err(|e| e.to_string())?;
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

/// Sets the work's state: `stage`, `status` and `done` on its page; the run record (`ticket`,
/// `inputs`, `summary`, ...) in a map keyed by stage. The page is the only state; there is no
/// `_stage.md` to keep in step.
pub(crate) fn set_stage_fields(server: &Server, work: &Work, stage: &str, fields: &[(&str, &str)]) {
    let Ok(url) = SiteUrl::parse(&work.url()) else {
        return;
    };
    let Ok(page) = server.web.read_page(&url) else {
        return;
    };
    let mut raw = page.raw.clone();
    let mut maps: HashMap<String, Map<String, Value>> = HashMap::new();
    for (key, value) in fields {
        if matches!(*key, "stage" | "status" | "done") {
            raw = set_field(&raw, key, value);
            continue;
        }
        let map = maps.entry((*key).to_string()).or_insert_with(|| {
            page.frontmatter
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        });
        let value = match *key {
            "inputs" => serde_json::from_str(value).unwrap_or(Value::Null),
            "attempts" => serde_json::from_str(value).unwrap_or(Value::String(value.to_string())),
            _ => Value::String(value.to_string()),
        };
        map.insert(stage.to_string(), value);
        raw = set_plain_field(
            &raw,
            key,
            &serde_json::to_string(map).expect("a map serializes"),
        );
    }
    if let Err(e) = server.write_page(&url, &raw, ChangedBy::Agent) {
        tracing::error!("updating {url}: {e}");
    }
}

/// `set_field`, but for a value that is a YAML flow collection: no ` #` is a comment.
fn set_plain_field(raw: &str, key: &str, value: &str) -> String {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return format!("---\n{key}: {value}\n---\n{raw}");
    };
    let Some(end) = rest.find("\n---") else {
        return raw.to_string();
    };
    let (front, body) = rest.split_at(end);
    let mut found = false;
    let lines: Vec<String> = front
        .lines()
        .map(|line| {
            if line.starts_with(&format!("{key}:")) {
                found = true;
                format!("{key}: {value}")
            } else {
                line.to_string()
            }
        })
        .collect();
    let mut front = lines.join("\n");
    if !found {
        front.push_str(&format!("\n{key}: {value}"));
    }
    format!("---\n{front}{body}")
}

/// A stage ticket's worker ended: record it on the work's page. Nothing starts the next stage.
/// Returns what the parent should be told, if anything:
/// `None` for a ticket that isn't a stage's.
pub fn stage_ended(
    server: &Arc<Server>,
    ticket: &Page,
    _parent: &str,
    state: WorkerState,
    summary: &str,
) -> Option<Option<String>> {
    let work = ticket.frontmatter.get_str("work")?;
    let stage = ticket.frontmatter.get_str("stage")?;
    let work = match find_work(server.web.root(), &work) {
        Ok(work) => work,
        Err(e) => return Some(Some(format!("[stage {stage}: {e}]"))),
    };
    let project = work.project_root(server);
    let project_mode = work.project_mode(work.pipeline.stage(&stage));
    if let Some(project) = &project {
        work.project_mount(server, &project.root, project_mode);
    }
    let gate = work.pipeline.stage(&stage).is_some_and(|c| c.gate);
    let status = match state {
        WorkerState::Done if gate => "review",
        WorkerState::Done => "done",
        WorkerState::Paused => "paused",
        _ => "failed",
    };
    let now = chrono::Local::now().to_rfc3339();
    let summary = one_line_raw(summary);
    let mut done = work.done();
    if status == "done" && !done.iter().any(|d| d == &stage) {
        done.push(stage.clone());
    }
    let done = serde_json::to_string(&done).expect("a list serializes");
    set_stage_fields(
        server,
        &work,
        &stage,
        &[
            ("stage", &stage),
            ("status", status),
            ("done", &done),
            ("finished", &now),
            ("summary", &summary),
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
        summary: summary.clone(),
    });
    let label = format!("{}/{}", work.pipeline.name, work.name);
    let folder = format!("site://{label}/{stage}/");
    Some(match status {
        "done" => Some(format!(
            "[work {label}: stage {stage} is done]\n{summary}\n\nIts output is in {folder}; \
             start the next stage when you want it."
        )),
        "review" => Some(format!(
            "[work {label}: stage {stage} waits for review]\n{summary}\n\nIts output is in {folder}. \
             The user approves it (`status: approved` and the stage in `done:` in the work's \
             index.md) or has it run again."
        )),
        other => Some(format!("[work {label}: stage {stage} {other}]\n{summary}")),
    })
}

fn one_line_raw(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn one_line(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    serde_json::to_string(&line).expect("a string serializes")
}

/// The session a work's stages belong to when nobody asked for them in a chat: the one its
/// last stage ran under (`session:` on the work's page), else a new one in the work's project.
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
        .and_then(|p| server.use_project(p).ok());
    let session = server
        .sessions
        .create("default", project)
        .map_err(|e| e.to_string())?;
    let label = format!("work {}/{}", work.pipeline.name, work.name);
    session.update_info(&server.hub, |info| info.preview = Some(label));
    Ok(session)
}

/// Rebuilds every work's `project/` link: after a restart, a work made by hand, or a moved
/// project. The worktree is not active outside a run, so the link points at the project root.
pub fn ensure_project_mounts(server: &Server) {
    let Ok(sites) = server.web.sites() else {
        return;
    };
    for site in sites {
        let dir = server.web.root().join(&site.name);
        let Some(pipeline) = Pipeline::from_dir(&site.name, &dir) else {
            continue;
        };
        for name in pipeline.works() {
            let Some(work) = Work::load(pipeline.clone(), &name) else {
                continue;
            };
            if let Some(project) = work.project_root(server) {
                let mode = work.project_mode(None);
                work.project_mount(server, &project.root, mode);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_work_spec_never_leaves_the_web() {
        // The workspace folder above the web holds a pipeline-looking page and a work: `..` must
        // not reach it (0149 F6: `Pipeline::from_dir` and `Work::load` build site URLs, which
        // refuse it).
        let dir = tempfile::tempdir().unwrap();
        let web = dir.path().join("sites");
        std::fs::create_dir_all(dir.path().join("w")).unwrap();
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(dir.path().join("index.md"), "---\ntype: pipeline\n---\n").unwrap();
        std::fs::write(dir.path().join("w/index.md"), "---\ntype: work\n---\n").unwrap();
        for spec in ["../w", "site://../w", "./w"] {
            assert!(find_work(&web, spec).is_err(), "{spec}");
        }
    }
}
