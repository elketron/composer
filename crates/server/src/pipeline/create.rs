//! Listing pipelines and their templates, and making pipelines and works.

use std::path::Path;

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_protocol::http::WorkSummary;

use crate::pipeline::pipeline::Pipeline;
use crate::pipeline::work::Work;
use crate::server::Server;
use crate::tools::{add_to_home, resolve_project, tilde};

/// Replaced by the pipeline site's name in every template file.
const PLACEHOLDER: &str = "__PIPELINE__";

/// Folder names a work can't have: the pipeline's own.
const RESERVED: [&str; 4] = ["_config", "stages", "ui", "index"];

/// The pipeline sites, by name.
pub fn pipelines(server: &Server) -> Vec<Pipeline> {
    let root = server.workspace.root();
    let mut found: Vec<Pipeline> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            Pipeline::from_dir(&name, &e.path())
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// One pipeline site.
pub fn pipeline(server: &Server, site: &str) -> Result<Pipeline, String> {
    let site = site
        .trim()
        .trim_start_matches("site://")
        .trim_end_matches('/');
    SiteUrl::new(site, "").map_err(|e| e.to_string())?;
    Pipeline::from_dir(site, &server.workspace.root().join(site)).ok_or_else(|| {
        let names: Vec<String> = pipelines(server).into_iter().map(|p| p.name).collect();
        format!(
            "site://{site} is not a pipeline (pipelines: {})",
            if names.is_empty() {
                "none yet; new_pipeline makes one".into()
            } else {
                names.join(", ")
            }
        )
    })
}

/// The templates in `templates/pipelines/`, by kind.
pub fn templates(server: &Server) -> Vec<Pipeline> {
    let root = server.home.templates().join("pipelines");
    let mut found: Vec<Pipeline> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let kind = e.file_name().into_string().ok()?;
            Pipeline::from_dir(&kind, &e.path())
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Makes the site `name` (default: the kind) from `templates/pipelines/<kind>/` and lists it on
/// Home. Never touches an existing site.
pub fn new_pipeline(
    server: &Server,
    kind: &str,
    name: Option<&str>,
    by: ChangedBy,
) -> Result<Pipeline, String> {
    let kind = kind.trim();
    let template = templates(server)
        .into_iter()
        .find(|t| t.name == kind)
        .ok_or_else(|| {
            let kinds: Vec<String> = templates(server).into_iter().map(|t| t.name).collect();
            format!(
                "no pipeline template `{kind}` (templates: {})",
                kinds.join(", ")
            )
        })?;
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(kind);
    check_name(name)?;
    SiteUrl::new(name, "").map_err(|e| e.to_string())?;
    let dir = server.workspace.root().join(name);
    if dir.exists() {
        return Err(format!("site://{name} already exists"));
    }
    std::fs::create_dir(&dir).map_err(|e| format!("creating site://{name}: {e}"))?;
    copy(server, &template.dir, Path::new(""), name, by)?;
    let pipeline = Pipeline::from_dir(name, &dir).ok_or("the template has no pipeline index")?;
    // Not listed on Home is no reason to fail: the site is there.
    let _ = add_to_home(server, name, &pipeline.about, by);
    Ok(pipeline)
}

/// Copies a template folder into the site through the server, so its pages are indexed.
fn copy(server: &Server, from: &Path, rel: &Path, site: &str, by: ChangedBy) -> Result<(), String> {
    for entry in std::fs::read_dir(from.join(rel))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let name = entry.file_name();
        if name == "node_modules" || name == "dist" {
            continue;
        }
        let rel = rel.join(&name);
        if entry.path().is_dir() {
            copy(server, from, &rel, site, by)?;
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|e| e.to_string())?;
        let bytes = match String::from_utf8(bytes) {
            Ok(text) => text.replace(PLACEHOLDER, site).into_bytes(),
            Err(binary) => binary.into_bytes(),
        };
        let url = SiteUrl::new(site, &rel.to_string_lossy()).map_err(|e| e.to_string())?;
        server
            .write_file(&url, &bytes, by)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// What a new work is made from.
pub struct NewWork<'a> {
    pub pipeline: &'a str,
    pub name: &'a str,
    pub title: Option<&'a str>,
    /// A name in the projects directory or a path.
    pub project: Option<&'a str>,
    pub brief: Option<&'a str>,
}

/// Makes the work's folder: its `index.md` (project, brief, stages) and a folder per stage with
/// its `_stage.md` (`status: waiting`). Lists it under the pipeline's Works.
pub fn new_work(server: &Server, new: NewWork, by: ChangedBy) -> Result<Work, String> {
    let pipeline = pipeline(server, new.pipeline)?;
    let name = new.name.trim();
    check_name(name)?;
    if RESERVED.contains(&name) || name.starts_with('_') {
        return Err(format!("`{name}` is the pipeline's own; pick another name"));
    }
    let site = pipeline.name.clone();
    let dir = pipeline.dir.join(name);
    if dir.exists() {
        return Err(format!("site://{site}/{name}/ already exists"));
    }
    let project = match new.project.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Some(tilde(Path::new(&resolve_project(
            p,
            &server.config.projects.dir,
        )?))),
        None if pipeline.needs_project => {
            return Err(format!(
                "a {site} work links a project: give `project` (a name in the projects directory or a path)"
            ));
        }
        None => None,
    };
    let title = new
        .title
        .map(one_line)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| name.to_string());
    let brief = new
        .brief
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or(
            "(What this piece of work is for: write the brief here before the first stage runs.)",
        );
    let today = chrono::Local::now().format("%Y-%m-%d");

    let mut fm = format!(
        "---\ntitle: {}\ntype: work\npipeline: {site}\n",
        serde_json::to_string(&title).expect("a string serializes")
    );
    if let Some(project) = &project {
        fm.push_str(&format!("project: {project}\n"));
    }
    fm.push_str(&format!("created: {today}\n---\n"));
    // Each stage's state is in its `_stage.md`, not here.
    let mut stages = String::from("| stage | gate |\n|-------|------|\n");
    for s in &pipeline.stages {
        stages.push_str(&format!(
            "| [{} {}]({}/_stage.md) | {} |\n",
            s.number(),
            s.title,
            s.name,
            if s.gate { "yes" } else { "no" }
        ));
    }
    let index = format!("{fm}# {title}\n\n## Brief\n\n{brief}\n\n## Stages\n\n{stages}");
    let url =
        |path: &str| SiteUrl::new(&site, &format!("{name}/{path}")).map_err(|e| e.to_string());
    server
        .write_page(&url("index.md")?, &index, by)
        .map_err(|e| e.to_string())?;
    for s in &pipeline.stages {
        let mut text = format!(
            "---\ntitle: {}\ntype: stage-run\nstage: {}\nstatus: waiting\n",
            serde_json::to_string(&format!("{} {}", s.number(), s.title))
                .expect("a string serializes"),
            s.name
        );
        if let Some(harness) = &s.harness {
            text.push_str(&format!("harness: {harness}\n"));
        }
        if let Some(model) = &s.model {
            text.push_str(&format!("model: {model}\n"));
        }
        text.push_str(&format!(
            "---\n# {} · {}\n\nWhat [the stage](../../stages/{}.md) wrote goes in this folder.\n",
            s.number(),
            s.title,
            s.name
        ));
        server
            .write_page(&url(&format!("{}/_stage.md", s.name))?, &text, by)
            .map_err(|e| e.to_string())?;
    }

    // The pipeline's list of works, when it keeps one.
    let index_url = SiteUrl::new(&site, "").expect("valid");
    if let Ok(page) = server.workspace.read_page(&index_url) {
        let item = match &project {
            Some(p) => format!("[{title}]({name}/index.md) · `{p}`"),
            None => format!("[{title}]({name}/index.md)"),
        };
        if let Some(text) = crate::tools::append_list_item(&page.raw, "Works", &item) {
            let _ = server.write_page(&index_url, &text, by);
        }
    }
    Work::load(pipeline, name).ok_or_else(|| format!("site://{site}/{name}/ was not written"))
}

/// The work a `type: work` page is the index of, if it is in a pipeline; its summary.
pub fn work_summary(server: &Server, url: &SiteUrl) -> Option<WorkSummary> {
    let name = url.path().strip_suffix("/index.md")?;
    if name.contains('/') {
        return None;
    }
    // Without stage contracts there is nothing to say where it is: its own `status:` does.
    let pipeline = Pipeline::from_dir(url.site(), &server.workspace.root().join(url.site()))
        .filter(|p| !p.stages.is_empty())?;
    Work::load(pipeline, name).map(|w| w.summary())
}

/// A folder name: letters, digits, `-`, `_`, not starting with `.` or `-`.
fn check_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "`{name}` is not a folder name: use letters, digits, `-` or `_`"
        ))
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
