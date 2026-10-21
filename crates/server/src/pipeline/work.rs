use std::path::PathBuf;

use composer_core::{Page, SiteUrl};
use composer_protocol::http::{StageState, WorkSummary};

use crate::pipeline::pipeline::Pipeline;
use crate::pipeline::types::FINISHED;

/// A piece of work: a folder in a pipeline site (`site://coding/save-system/`) with its brief in
/// `index.md` (`type: work`) and a folder per stage holding what the stage wrote and its
/// `_stage.md`.
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

    /// A stage's state: its `_stage.md`'s `status:`, `waiting` without one.
    pub fn stage_status(&self, stage: &str) -> String {
        std::fs::read_to_string(self.dir.join(stage).join("_stage.md"))
            .ok()
            .and_then(|raw| {
                let url = SiteUrl::new(
                    &self.pipeline.name,
                    &format!("{}/{stage}/_stage.md", self.name),
                )
                .ok()?;
                Page::parse(url, raw).frontmatter.get_str("status")
            })
            .unwrap_or_else(|| "waiting".into())
    }

    pub fn stages(&self) -> Vec<StageState> {
        self.pipeline
            .stages
            .iter()
            .map(|s| StageState {
                name: s.name.clone(),
                status: self.stage_status(&s.name),
            })
            .collect()
    }

    /// Where it is: the first stage not finished and its state (`02 script · review`), or `done`.
    pub fn status(&self, stages: &[StageState]) -> String {
        match stages
            .iter()
            .find(|s| !FINISHED.contains(&s.status.as_str()))
        {
            Some(s) => {
                let label = s.name.replacen('_', " ", 1);
                format!("{label} · {}", s.status)
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
