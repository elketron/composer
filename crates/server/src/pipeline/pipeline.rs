use std::path::{Path, PathBuf};

use composer_core::{Page, SiteUrl};
use composer_protocol::http::PipelineSummary;

use crate::pipeline::stage_contract::StageContract;

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
                        !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && !rest.is_empty()
                    })
                    .map(|_| (stem.clone(), e.path()))
            })
            .collect();
        files.sort();
        let stages = files
            .into_iter()
            .filter_map(|(stem, path)| {
                let raw = std::fs::read_to_string(path).ok()?;
                Some(StageContract::parse(&stem, &raw))
            })
            .collect();
        Some(Self {
            name: name.to_string(),
            dir: dir.to_path_buf(),
            title: page.title(),
            about: page.description(),
            needs_project: page.frontmatter.get_str("needs_project").as_deref() == Some("true"),
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
