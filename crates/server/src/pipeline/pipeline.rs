use std::path::{Path, PathBuf};

use composer_core::{Page, SiteUrl};
use composer_protocol::http::PipelineSummary;
use serde_json::Value;

use crate::pipeline::stage_contract::StageContract;
use crate::project::ProjectMode;

/// A pipeline: a folder with an `index.md` of `type: pipeline` and its stage contracts in
/// `stages/`. A site (`sites/coding/`) or a template (`templates/pipelines/coding/`).
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// The site's name, or the template's kind.
    pub name: String,
    pub dir: PathBuf,
    pub title: String,
    pub about: String,
    /// A work must link a project.
    pub needs_project: bool,
    /// The default project access for its works and stages.
    pub project_mode: ProjectMode,
    /// In order (by file name).
    pub stages: Vec<StageContract>,
}

impl Pipeline {
    /// `None` unless `dir/index.md` says `type: pipeline`.
    pub fn from_dir(name: &str, dir: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(dir.join("index.md")).ok()?;
        let page = Page::parse(SiteUrl::new(name, "index.md").ok()?, raw);
        if page.frontmatter.get_str("type").as_deref() != Some("pipeline") {
            return None;
        }
        let stages = match page.frontmatter.get("stages").and_then(Value::as_array) {
            // The graph is declared on the pipeline's page: `id`, `file`, `needs`, `gate`.
            Some(graph) => graph
                .iter()
                .filter_map(|entry| {
                    let id = entry.get("id").and_then(Value::as_str)?;
                    let file = entry.get("file").and_then(Value::as_str)?;
                    let raw = std::fs::read_to_string(dir.join(file)).ok()?;
                    let mut contract = StageContract::parse(id, &raw);
                    contract.needs = strings(entry.get("needs"));
                    if let Some(gate) = entry.get("gate").and_then(Value::as_bool) {
                        contract.gate = gate;
                    }
                    contract.repeat = entry
                        .get("repeat")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    Some(contract)
                })
                .collect(),
            // Before the graph: every `NN_name.md` in `stages/`, each needing the one before.
            None => {
                let mut files: Vec<(String, PathBuf)> = std::fs::read_dir(dir.join("stages"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| {
                        let file = e.file_name().into_string().ok()?;
                        let stem = file.strip_suffix(".md")?.to_string();
                        // `NN_name.md`: a number first.
                        stem.split_once('_')
                            .filter(|(n, rest)| {
                                !n.is_empty()
                                    && n.chars().all(|c| c.is_ascii_digit())
                                    && !rest.is_empty()
                            })
                            .map(|_| (stem.clone(), e.path()))
                    })
                    .collect();
                files.sort();
                let mut previous: Option<String> = None;
                files
                    .into_iter()
                    .filter_map(|(stem, path)| {
                        let raw = std::fs::read_to_string(path).ok()?;
                        let mut contract = StageContract::parse(&stem, &raw);
                        contract.needs = previous.take().into_iter().collect();
                        previous = Some(stem);
                        Some(contract)
                    })
                    .collect()
            }
        };
        Some(Self {
            name: name.to_string(),
            dir: dir.to_path_buf(),
            title: page.title(),
            about: page.description(),
            needs_project: page.frontmatter.get_str("needs_project").as_deref() == Some("true"),
            project_mode: ProjectMode::parse(
                page.frontmatter
                    .get_str("project_mode")
                    .as_deref()
                    .unwrap_or(""),
            ),
            stages,
        })
    }

    /// The works in it: subfolders with an `index.md` of `type: work`, by name.
    pub fn works(&self) -> Vec<String> {
        let mut works: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| {
                std::fs::read_to_string(self.dir.join(name).join("index.md"))
                    .ok()
                    .and_then(|raw| {
                        let url = SiteUrl::new(&self.name, &format!("{name}/index.md")).ok()?;
                        Page::parse(url, raw).frontmatter.get_str("type")
                    })
                    .as_deref()
                    == Some("work")
            })
            .collect();
        works.sort();
        works
    }

    pub fn stage(&self, name: &str) -> Option<&StageContract> {
        self.stages.iter().find(|s| s.name == name)
    }

    pub fn summary(&self) -> PipelineSummary {
        PipelineSummary {
            site: self.name.clone(),
            title: self.title.clone(),
            about: self.about.clone(),
            stages: self.stages.iter().map(StageContract::summary).collect(),
            needs_project: self.needs_project,
            works: self.works().len() as u32,
        }
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
