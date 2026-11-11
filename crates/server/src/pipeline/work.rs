use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use composer_core::{Frontmatter, Page, SiteUrl, Web};
use composer_protocol::http::{StageState, WorkSummary};
use serde_json::Value;

use crate::content_hash;
use crate::pipeline::pipeline::Pipeline;
use crate::pipeline::stage_contract::StageContract;
use crate::pipeline::types::FINISHED;
use crate::project::{ProjectMode, ProjectRef};
use crate::server::Server;

/// A piece of work: a folder in a pipeline site (`site://coding/save-system/`) with its brief and
/// state in `index.md` (`type: work`) and a folder per stage holding what the stage wrote. A work
/// from before the state moved to its page still reads its stages' `_stage.md`.
#[derive(Debug, Clone)]
pub struct Work {
    pub pipeline: Pipeline,
    /// The folder's name.
    pub name: String,
    pub dir: PathBuf,
    pub title: String,
    /// Its `project:` as written (`~/Projects/game`, `game`).
    pub project: Option<String>,
}

impl Work {
    pub fn load(pipeline: Pipeline, name: &str) -> Option<Self> {
        let dir = pipeline.dir.join(name);
        let raw = std::fs::read_to_string(dir.join("index.md")).ok()?;
        let page = Page::parse(
            SiteUrl::new(&pipeline.name, &format!("{name}/index.md")).ok()?,
            raw,
        );
        if page.frontmatter.get_str("type").as_deref() != Some("work") {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            title: page.title(),
            project: page
                .frontmatter
                .get_str("project")
                .filter(|p| !p.trim().is_empty()),
            dir,
            pipeline,
        })
    }

    pub fn url(&self) -> String {
        format!("site://{}/{}/index.md", self.pipeline.name, self.name)
    }

    /// How this work may use its project: the stage's `project_mode`, its legacy `worktree:`,
    /// then the work's own `project_mode`, then the pipeline's default.
    pub fn project_mode(&self, contract: Option<&StageContract>) -> ProjectMode {
        if let Some(contract) = contract {
            if let Some(raw) = contract.project_mode.as_deref() {
                return ProjectMode::parse(raw);
            }
            if contract.worktree {
                return ProjectMode::Worktree;
            }
        }
        if let Some(raw) = self.field("project_mode") {
            return ProjectMode::parse(&raw);
        }
        self.pipeline.project_mode
    }

    /// The project page this work names, resolved.
    pub fn project_root(&self, server: &Server) -> Option<ProjectRef> {
        self.project
            .as_deref()
            .and_then(|project| ProjectRef::resolve(server, project).ok())
    }

    /// Points this work's `project/` at `target` (the project root or its worktree) and records
    /// the mode.
    pub fn project_mount(&self, server: &Server, target: &Path, mode: ProjectMode) {
        let link = self.dir.join("project");
        let _ = std::fs::remove_file(&link);
        if let Err(error) = std::os::unix::fs::symlink(target, &link) {
            tracing::warn!("project link {}: {error}", link.display());
            return;
        }
        server.web.set_project_mount(&link, target, mode.as_mount());
    }

    /// A field of the work's own `index.md`.
    fn field(&self, key: &str) -> Option<String> {
        self.frontmatter()?
            .get_str(key)
            .filter(|value| !value.is_empty())
    }

    fn frontmatter(&self) -> Option<Frontmatter> {
        let raw = std::fs::read_to_string(self.dir.join("index.md")).ok()?;
        let url = SiteUrl::new(&self.pipeline.name, &format!("{}/index.md", self.name)).ok()?;
        Some(Page::parse(url, raw).frontmatter)
    }

    /// The stages that finished (`done:` on the work's page).
    pub fn done(&self) -> Vec<String> {
        self.frontmatter()
            .and_then(|fm| {
                fm.get("done").and_then(Value::as_array).map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// The stage the work is on (`stage:` on its page), if any.
    pub fn current(&self) -> Option<String> {
        self.field("stage")
    }

    /// The work's own `status:` (`waiting`, `running`, `review`, `approved`, ...).
    pub fn current_status(&self) -> Option<String> {
        self.field("status")
    }

    /// How many times a stage ran (`attempts:` on the work's page).
    pub fn attempts(&self, stage: &str) -> u32 {
        self.frontmatter()
            .and_then(|fm| fm.get("attempts").cloned())
            .and_then(|value| value.as_object().cloned())
            .and_then(|map| map.get(stage).cloned())
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32
    }

    /// A stage's state: done when it is in `done:`, the work's `status:` while it is the
    /// current stage, else `waiting`. A work from before the state moved to its page still reads
    /// its stage's `_stage.md`.
    pub fn stage_status(&self, stage: &str) -> String {
        if let Some(fm) = self.frontmatter() {
            // The current stage's own status wins: a rerun in progress (or failed) is not `done`
            // just because an earlier run of it was.
            if fm.get_str("stage").as_deref() == Some(stage)
                && let Some(status) = fm.get_str("status").filter(|s| !s.is_empty())
            {
                return status;
            }
            if self.done().iter().any(|done| done == stage) {
                return "done".into();
            }
        }
        self.legacy_stage_field(stage, "status")
            .unwrap_or_else(|| "waiting".into())
    }

    /// A stage's run record: `ticket:`, `summary:`, `session:` and the rest, keyed by stage on
    /// the work's page; `_stage.md` for a work from before.
    pub fn stage_field(&self, stage: &str, key: &str) -> Option<String> {
        if let Some(value) = self
            .frontmatter()
            .and_then(|fm| fm.get(key).cloned())
            .and_then(|value| value.as_object().cloned())
            .and_then(|map| map.get(stage).cloned())
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }
        self.legacy_stage_field(stage, key)
    }

    /// The inputs a stage read last time, from the work's page; `_stage.md` for an older work.
    fn stage_inputs(&self, stage: &str) -> Option<BTreeMap<String, String>> {
        let map = self
            .frontmatter()
            .and_then(|fm| fm.get("inputs").cloned())
            .and_then(|value| value.as_object().cloned())
            .and_then(|map| map.get(stage).cloned())
            .and_then(|value| value.as_object().cloned());
        if let Some(map) = map {
            return Some(
                map.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect(),
            );
        }
        self.legacy_stage_inputs(stage)
    }

    fn legacy_stage_field(&self, stage: &str, key: &str) -> Option<String> {
        let raw = std::fs::read_to_string(self.dir.join(stage).join("_stage.md")).ok()?;
        let url = SiteUrl::new(
            &self.pipeline.name,
            &format!("{}/{stage}/_stage.md", self.name),
        )
        .ok()?;
        Page::parse(url, raw)
            .frontmatter
            .get_str(key)
            .filter(|v| !v.is_empty())
    }

    fn legacy_stage_inputs(&self, stage: &str) -> Option<BTreeMap<String, String>> {
        let raw = std::fs::read_to_string(self.dir.join(stage).join("_stage.md")).ok()?;
        let url = SiteUrl::new(
            &self.pipeline.name,
            &format!("{}/{stage}/_stage.md", self.name),
        )
        .ok()?;
        let page = Page::parse(url, raw);
        let inputs = page.frontmatter.get("inputs")?.as_object()?;
        Some(
            inputs
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect(),
        )
    }

    /// Its stages with their state. A stage that ran is stale when an input it read changed
    /// since (its `inputs:` on the work's page keep their hashes), and so is every stage after
    /// it that ran.
    pub fn stages(&self) -> Vec<StageState> {
        let mut behind = false;
        self.pipeline
            .stages
            .iter()
            .map(|s| {
                let status = self.stage_status(&s.name);
                let ran = matches!(status.as_str(), "done" | "approved" | "review");
                let changed = if ran { self.changed_inputs(s) } else { vec![] };
                let stale = ran && (behind || !changed.is_empty());
                behind |= stale;
                StageState {
                    name: s.name.clone(),
                    ticket: self.stage_field(&s.name, "ticket"),
                    summary: self.stage_field(&s.name, "summary"),
                    outputs: self.outputs(&s.name),
                    status,
                    stale,
                    changed,
                }
            })
            .collect()
    }

    /// What a stage reads now, by page, with each page's hash (empty: not there).
    pub fn input_hashes(&self, contract: &StageContract) -> BTreeMap<String, String> {
        let root = self.pipeline.dir.parent().map(Web::new);
        contract
            .inputs
            .iter()
            .filter_map(|input| self.resolve(&input.path).map(|url| (input, url)))
            .map(|(input, url)| {
                // The brief is the work page's body; its state is frontmatter and must not make
                // every stage that read the brief stale.
                let hash = if input.path.trim() == "brief" {
                    root.as_ref()
                        .and_then(|ws| ws.read_page(&url).ok())
                        .map(|page| content_hash(page.body().as_bytes()))
                        .unwrap_or_default()
                } else {
                    root.as_ref()
                        .and_then(|ws| ws.locate(&url).ok())
                        .and_then(|path| std::fs::read(path).ok())
                        .map(|bytes| content_hash(&bytes))
                        .unwrap_or_default()
                };
                (url.to_string(), hash)
            })
            .collect()
    }

    /// The inputs that differ from what the stage read last time (`inputs:` on the work's page).
    fn changed_inputs(&self, contract: &StageContract) -> Vec<String> {
        let Some(recorded) = self.recorded_inputs(&contract.name) else {
            // Run before inputs were recorded: nothing to compare with.
            return vec![];
        };
        let now = self.input_hashes(contract);
        let mut changed: Vec<String> = now
            .iter()
            .filter(|(url, hash)| recorded.get(*url) != Some(*hash))
            .map(|(url, _)| url.clone())
            .collect();
        changed.extend(
            recorded
                .keys()
                .filter(|url| !now.contains_key(*url))
                .cloned(),
        );
        changed
    }

    fn recorded_inputs(&self, stage: &str) -> Option<BTreeMap<String, String>> {
        self.stage_inputs(stage)
    }

    /// The pages a stage wrote: markdown in its folder (and one level down), not `_stage.md`.
    pub fn outputs(&self, stage: &str) -> Vec<String> {
        let dir = self.dir.join(stage);
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "_stage.md" {
                continue;
            }
            if entry.path().is_dir() {
                for inner in std::fs::read_dir(entry.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let file = inner.file_name().to_string_lossy().to_string();
                    if file.ends_with(".md") && !file.starts_with('.') {
                        found.push(format!("{name}/{file}"));
                    }
                }
            } else if name.ends_with(".md") {
                found.push(name);
            }
        }
        found.sort();
        found
            .into_iter()
            .map(|f| format!("site://{}/{}/{stage}/{f}", self.pipeline.name, self.name))
            .collect()
    }

    /// Where it is: the first stage that is stale or not finished, and its state
    /// (`02 script · review`, `02 script · stale`), or `done`.
    pub fn status(&self, stages: &[StageState]) -> String {
        match stages
            .iter()
            .find(|s| s.stale || !FINISHED.contains(&s.status.as_str()))
        {
            Some(s) => {
                let label = s.name.replacen('_', " ", 1);
                let state = if s.stale { "stale" } else { s.status.as_str() };
                format!("{label} · {state}")
            }
            None => "done".into(),
        }
    }

    /// A stage input's page: `brief` is the work's `index.md`; a `_config/` file is the work's own
    /// when it has one, else the pipeline's; anything else is in the work.
    pub fn resolve(&self, path: &str) -> Option<SiteUrl> {
        let path = path.trim().trim_start_matches("./");
        let site = &self.pipeline.name;
        if path == "brief" {
            return SiteUrl::new(site, &format!("{}/index.md", self.name)).ok();
        }
        if path.starts_with("_config/") && !self.dir.join(path).is_file() {
            return SiteUrl::new(site, path).ok();
        }
        SiteUrl::new(site, &format!("{}/{path}", self.name)).ok()
    }

    pub fn summary(&self) -> WorkSummary {
        let stages = self.stages();
        WorkSummary {
            url: self.url(),
            pipeline: self.pipeline.name.clone(),
            title: self.title.clone(),
            status: Some(self.status(&stages)),
            stages,
            project: self.project.clone(),
        }
    }
}
