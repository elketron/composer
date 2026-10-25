//! Learning from edits: a stage's output as the agent wrote it is kept (`<stage>/.agent/`); at
//! its gate the difference is your edit. When works of a pipeline keep getting the same kind of
//! edit at a stage, a model proposes changing the source (the stage contract or a `_config`
//! page) instead, as an approval.

use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::{ApprovalDecision, ApprovalKind, ChangedBy};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::pipeline::pipeline::Pipeline;
use crate::pipeline::run::{set_stage_fields, work_session};
use crate::pipeline::work::Work;
use crate::provider::{ModelEvent, ModelMessage, ModelRequest};
use crate::server::Server;

/// Where a stage's untouched output is kept, in its folder.
const KEPT: &str = ".agent";

/// One edit is cut after this many characters in the model's prompt.
const EDIT_MAX: usize = 6_000;

/// A stage ended with output: keep it as written, to tell your edits from it later.
pub fn keep_agent_output(work: &Work, stage: &str) {
    let dir = work.dir.join(stage);
    let kept = dir.join(KEPT);
    let _ = std::fs::remove_dir_all(&kept);
    for url in work.outputs(stage) {
        let Some(rel) = url.split(&format!("/{stage}/")).nth(1) else {
            continue;
        };
        let (from, to) = (dir.join(rel), kept.join(rel));
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(&from, &to) {
            tracing::warn!("keeping {}: {e}", from.display());
        }
    }
}

/// Your edit of a stage's output: a diff of what the agent wrote against what is there now.
/// `None` without kept output or without changes.
pub fn edit_diff(work: &Work, stage: &str) -> Option<String> {
    let dir = work.dir.join(stage);
    let kept = dir.join(KEPT);
    let mut files: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&kept).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().is_dir() {
            for inner in std::fs::read_dir(entry.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                files.push(format!("{name}/{}", inner.file_name().to_string_lossy()));
            }
        } else if name.ends_with(".md") {
            files.push(name);
        }
    }
    files.sort();
    let diff: String = files
        .iter()
        .map(|rel| {
            let now = std::fs::read_to_string(dir.join(rel)).unwrap_or_default();
            let then = std::fs::read_to_string(kept.join(rel)).unwrap_or_default();
            text_diff(&then, &now, rel)
        })
        .collect();
    (!diff.trim().is_empty()).then_some(diff)
}

/// A gate was approved: record your edit (if any) and, when enough works of the pipeline have
/// been edited at this stage, ask for an amendment.
pub fn gate_approved(server: &Arc<Server>, work: &Work, stage: &str) {
    let Some(diff) = edit_diff(work, stage) else {
        return;
    };
    let path = work.dir.join(stage).join(KEPT).join("edit.diff");
    if let Err(e) = std::fs::write(&path, &diff) {
        tracing::warn!("{}: {e}", path.display());
        return;
    }
    set_stage_fields(server, work, stage, &[("edited", "true")]);
    let threshold = server.config().pipelines.learn_after as usize;
    if threshold == 0 {
        return;
    }
    let edited = edited_works(&work.pipeline, stage);
    if edited.len() >= threshold {
        // Counted once: they wait for this proposal, whatever comes of it.
        for other in &edited {
            set_stage_fields(server, other, stage, &[("learned", "\"proposed\"")]);
        }
        let server = server.clone();
        let pipeline = work.pipeline.clone();
        let stage = stage.to_string();
        tokio::spawn(async move {
            if let Err(e) = propose(&server, &pipeline, &stage, &edited).await {
                tracing::warn!("proposing an amendment for {}/{stage}: {e}", pipeline.name);
            }
        });
    }
}

/// Works of the pipeline with an edit at `stage` that no proposal has looked at yet.
fn edited_works(pipeline: &Pipeline, stage: &str) -> Vec<Work> {
    pipeline
        .works()
        .into_iter()
        .filter_map(|name| Work::load(pipeline.clone(), &name))
        .filter(|w| {
            w.stage_field(stage, "edited").as_deref() == Some("true")
                && w.stage_field(stage, "learned").is_none()
                && w.dir.join(stage).join(KEPT).join("edit.diff").is_file()
        })
        .collect()
}

/// What the model answers.
#[derive(Debug, Deserialize)]
struct Answer {
    /// The kind of edit that recurs, in a sentence; `null`: nothing worth changing.
    pattern: Option<String>,
    /// `stages/02_script.md` or `_config/<file>.md`.
    target: Option<String>,
    /// The whole new page.
    text: Option<String>,
}

/// Asks the model what the edits have in common and what to change at the source, writes the
/// proposal page and asks the user to approve it.
async fn propose(
    server: &Arc<Server>,
    pipeline: &Pipeline,
    stage: &str,
    works: &[Work],
) -> Result<(), String> {
    let site = &pipeline.name;
    let contract_path = format!("stages/{stage}.md");
    let read = |rel: &str| std::fs::read_to_string(pipeline.dir.join(rel)).unwrap_or_default();
    let mut prompt = format!(
        "The user ran stage {stage} of the `{site}` pipeline in several works and edited what the stage \
         wrote each time, before approving it. Find the kind of edit that recurs, and change the source so \
         the stage gets it right from now on: its contract, or one of the shared `_config` pages it reads. \
         Change as little as possible; keep the page's structure and frontmatter.\n\n\
         Answer with one JSON object and nothing else: {{\"pattern\": \"<the recurring edit, one sentence>\", \
         \"target\": \"{contract_path}\" or \"_config/<file>.md\", \"text\": \"<the whole new page>\"}}. \
         If the edits have nothing in common worth changing, answer {{\"pattern\": null}}.\n\n\
         <page path=\"{contract_path}\">\n{}\n</page>\n",
        read(&contract_path)
    );
    if let Some(contract) = pipeline.stage(stage) {
        for input in contract
            .inputs
            .iter()
            .filter(|i| i.path.starts_with("_config/"))
        {
            prompt.push_str(&format!(
                "\n<page path=\"{}\">\n{}\n</page>\n",
                input.path,
                read(&input.path)
            ));
        }
    }
    for work in works {
        let diff = std::fs::read_to_string(work.dir.join(stage).join(KEPT).join("edit.diff"))
            .unwrap_or_default();
        let diff = match diff.char_indices().nth(EDIT_MAX) {
            Some((i, _)) => format!("{}\n(cut)", &diff[..i]),
            None => diff,
        };
        prompt.push_str(&format!(
            "\n<edit work=\"{}\">\n{diff}\n</edit>\n",
            work.name
        ));
    }

    let reply = complete(server, prompt).await?;
    let json = reply
        .find('{')
        .zip(reply.rfind('}'))
        .map(|(a, b)| &reply[a..=b])
        .ok_or("the model didn't answer with JSON")?;
    let answer: Answer = serde_json::from_str(json).map_err(|e| format!("its answer: {e}"))?;
    let (Some(pattern), Some(target), Some(text)) = (answer.pattern, answer.target, answer.text)
    else {
        tracing::info!("no recurring edit at {site}/{stage}");
        return Ok(());
    };
    let target = target.trim().trim_start_matches('/').to_string();
    let allowed = target == contract_path
        || (target.starts_with("_config/") && target.ends_with(".md") && !target.contains(".."));
    if !allowed {
        return Err(format!(
            "it proposed changing {target}, not the contract or a _config page"
        ));
    }
    let before = read(&target);
    let diff = text_diff(&before, &text, &target);
    if diff.trim().is_empty() {
        return Ok(());
    }

    // The proposal is a page, so it outlasts a restart; its text is kept beside it.
    let id = format!("{stage}-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let names: Vec<&str> = works.iter().map(|w| w.name.as_str()).collect();
    let page = format!(
        "---\ntitle: {}\ntype: proposal\nstatus: open\nstage: {stage}\ntarget: {target}\nworks: [{}]\n---\n\
         # Amend {target}\n\n{pattern}\n\nLearned from your edits in {}.\n\n## Change\n\n```diff\n{diff}\n```\n",
        serde_json::to_string(&format!("Amend {target}")).expect("a string serializes"),
        names.join(", "),
        names.join(", "),
    );
    let url = SiteUrl::new(site, &format!("_proposals/{id}.md")).map_err(|e| e.to_string())?;
    let text_path = pipeline.dir.join("_proposals").join(format!(".{id}.text"));
    std::fs::create_dir_all(pipeline.dir.join("_proposals")).map_err(|e| e.to_string())?;
    std::fs::write(&text_path, &text).map_err(|e| e.to_string())?;
    server
        .write_page(&url, &page, ChangedBy::Agent)
        .map_err(|e| e.to_string())?;
    ask(server, url, works.first()).await
}

/// Asks the user to approve an open proposal; applies or dismisses it.
async fn ask(server: &Arc<Server>, url: SiteUrl, work: Option<&Work>) -> Result<(), String> {
    let page = server
        .workspace
        .read_page(&url)
        .map_err(|e| e.to_string())?;
    let fm = &page.frontmatter;
    let target = fm.get_str("target").ok_or("the proposal names no target")?;
    let stage = fm.get_str("stage").unwrap_or_default();
    let site = url.site().to_string();
    let pipeline_dir = server.workspace.root().join(&site);
    let id = url
        .path()
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".md"))
        .unwrap_or_default()
        .to_string();
    let text = std::fs::read_to_string(pipeline_dir.join("_proposals").join(format!(".{id}.text")))
        .map_err(|e| format!("the proposed text: {e}"))?;
    let session = match work {
        Some(work) => work_session(server, work, Some(&stage))?,
        None => {
            let pipeline = Pipeline::from_dir(&site, &pipeline_dir).ok_or("not a pipeline")?;
            let name = pipeline
                .works()
                .into_iter()
                .next()
                .ok_or("the pipeline has no works")?;
            let work = Work::load(pipeline, &name).ok_or("no work")?;
            work_session(server, &work, None)?
        }
    };
    let pattern = page
        .body()
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or_default()
        .to_string();
    let diff = text_diff(
        &std::fs::read_to_string(pipeline_dir.join(&target)).unwrap_or_default(),
        &text,
        &target,
    );
    let decision = server
        .ask_approval(
            &session,
            ApprovalKind::Amendment,
            format!("{site}: amend {target} · {pattern}"),
            diff,
            None,
            &CancellationToken::new(),
        )
        .await;
    let status = match decision {
        Some(ApprovalDecision::Approve | ApprovalDecision::Always) => {
            let target_url = SiteUrl::new(&site, &target).map_err(|e| e.to_string())?;
            server
                .write_page(&target_url, &text, ChangedBy::Agent)
                .map_err(|e| e.to_string())?;
            "approved"
        }
        Some(ApprovalDecision::Deny) => "dismissed",
        None => return Ok(()),
    };
    let raw = crate::worker::ticket::set_field(&page.raw, "status", status);
    server
        .write_page(&url, &raw, ChangedBy::Agent)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// At startup: proposals still open are asked again (an approval doesn't outlast the server).
pub fn ask_open_proposals(server: &Arc<Server>) {
    let open: Vec<SiteUrl> = {
        let index = server.index.read().expect("index lock");
        index
            .entries()
            .filter(|e| e.url.path().starts_with("_proposals/"))
            .filter(|e| e.frontmatter.get_str("type").as_deref() == Some("proposal"))
            .filter(|e| e.frontmatter.get_str("status").as_deref() == Some("open"))
            .map(|e| e.url.clone())
            .collect()
    };
    for url in open {
        let server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = ask(&server, url.clone(), None).await {
                tracing::warn!("{url}: {e}");
            }
        });
    }
}

/// One completion without tools, holding a model slot for it.
async fn complete(server: &Arc<Server>, prompt: String) -> Result<String, String> {
    let _slot = server.slots.acquire().await?;
    let mut stream = server
        .provider
        .stream(ModelRequest {
            messages: vec![ModelMessage::User {
                content: prompt,
                images: vec![],
            }],
            tools: vec![],
        })
        .await?;
    let mut content = String::new();
    while let Some(event) = stream.recv().await {
        match event? {
            ModelEvent::Content(text) => content.push_str(&text),
            ModelEvent::Finished { .. } => break,
            ModelEvent::Thinking(_) => {}
        }
    }
    Ok(content)
}

/// A unified diff of a page's text before and after, named `path`; empty when they're equal.
fn text_diff(before: &str, after: &str, path: &str) -> String {
    if before == after {
        return String::new();
    }
    let dir = std::env::temp_dir().join(format!("composer-diff-{:016x}", rand::random::<u64>()));
    let (a, b) = (dir.join("a"), dir.join("b"));
    let written = std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&a, before))
        .and_then(|_| std::fs::write(&b, after));
    let output = written.ok().and_then(|_| {
        std::process::Command::new("git")
            .args(["diff", "--no-index", "--no-color", "-U2", "--"])
            .arg(&a)
            .arg(&b)
            .output()
            .ok()
    });
    let _ = std::fs::remove_dir_all(&dir);
    let Some(output) = output else {
        return String::new();
    };
    let hunks: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip_while(|l| !l.starts_with("@@"))
        .map(str::to_string)
        .collect();
    if hunks.is_empty() {
        return String::new();
    }
    format!("--- a/{path}\n+++ b/{path}\n{}\n", hunks.join("\n"))
}
