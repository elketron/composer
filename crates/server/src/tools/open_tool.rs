use std::path::{Path, PathBuf};

use async_trait::async_trait;
use base64::Engine;
use composer_protocol::ServerMessage;
use serde_json::{Value, json};

use composer_core::SiteUrl;

use crate::apps::{DataUrl, path_inside};
use crate::attachments::{AttachmentKind, classify};
use crate::tools::args::OpenArgs;
use crate::tools::{CallResult, Tool, ToolContext, expand_home, text_result};

/// Extensions `open` treats as pictures (what it then shows is told by the bytes).
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

pub struct OpenTool;

#[async_trait]
impl Tool for OpenTool {
    fn name(&self) -> &'static str {
        "open"
    }

    fn description(&self) -> &'static str {
        "Open a page and read its full markdown, frontmatter included. \
         `site://memory` opens the site's index.md; `site://plans/x` finds x.md or x/index.md. \
         Relative paths are not resolved here: always pass a full site:// URL. \
         Also opens an image so you see it: a picture in a site (`site://designer/mock.png`), or \
         an image file in your active project (a screenshot you took: `/abs/path.png`, `~/x.png`, \
         or a path relative to the project). And an app's work data, what it shows for a \
         project (read-only): `data://<project>/<app>/<path>` (a file; a folder lists its files), \
         `data://<project>/` (which apps keep data there), `data://@home/<app>/...` (on Home). \
         A `type: tui` page starts its terminal program on the session's desk; its control \
         adapter (for Hunk, the `hunk` tool) drives it. A site folder lists its files, and a \
         page's output starts with the `hash` to pass to `edit`."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "e.g. site://memory/user, or an image: site://designer/mock.png, build/shot.png, or app data: data://game/cad/view.json" } },
            "required": ["url"]
        })
    }

    async fn call_result(&self, context: &ToolContext, args: Value) -> CallResult {
        let raw = args["url"].as_str().unwrap_or_default().to_string();
        if raw.trim().starts_with("data://") {
            return data(context, &raw).unwrap_or_else(|error| text_result(error, true));
        }
        match image_path(context, &raw) {
            Some(Ok(path)) => match image(context, &raw, &path) {
                Ok(result) => result,
                Err(error) => text_result(error, true),
            },
            Some(Err(error)) => text_result(error, true),
            None => match self.call(context, args).await {
                Ok(text) => text_result(text, false),
                Err(error) => text_result(error, true),
            },
        }
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: OpenArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let url = context.url(&args.url)?;
        context.check_read(&url)?;
        let server = &context.server;
        let page = match server.web.read_page(&url) {
            Ok(page) => page,
            Err(error) => {
                if let Some(listing) = directory(context, &url) {
                    return Ok(listing);
                }
                return Err(format!(
                    "{error}. Use `search` to find pages, or `list_sites`."
                ));
            }
        };

        let session = context.session.id().to_string();
        let canonical = page.url.to_string();
        context
            .session
            .emit(&server.hub, |seq| ServerMessage::AgentNavigated {
                session,
                seq,
                url: canonical,
            });

        // A TUI page is a capability, not just text: opening it starts its program on the
        // session's desk. Its control adapter (the `hunk` tool) is how the agent drives it.
        if page.frontmatter.get_str("type").as_deref() == Some("tui") {
            let tui = crate::tui::TuiPage::parse(&page)?;
            let started = tui.open(context, &page)?;
            return Ok(format!("url: {}\n\n{started}\n\n{}", page.url, page.raw));
        }

        let hash = crate::content_hash(page.raw.as_bytes());
        let mut header = format!("url: {}\nhash: {}", page.url, &hash[..12]);
        if let Some(error) = &page.frontmatter_error {
            header.push_str(&format!(
                "\nwarning: invalid frontmatter ({error}); fix it with `edit`"
            ));
        }
        let backlinks = server
            .index
            .read()
            .expect("index lock")
            .backlinks(&page.url);
        if !backlinks.is_empty() {
            let list: Vec<_> = backlinks.iter().map(ToString::to_string).collect();
            header.push_str(&format!("\nlinked from: {}", list.join(", ")));
        }
        Ok(format!("{header}\n\n{}", page.raw))
    }
}

/// A site folder `open` was asked for: list what is in it, so the model can pick a page or file.
fn directory(context: &ToolContext, url: &SiteUrl) -> Option<String> {
    let site = context.server.web.root().join(url.site());
    let path = site.join(url.path());
    if !path.is_dir() || context.server.web.check(&path).is_err() {
        return None;
    }
    let mut entries: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&path).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            entries.push(format!("{name}/"));
        } else if let Ok(meta) = entry.metadata() {
            entries.push(format!("{name} ({} KB)", meta.len().div_ceil(1024)));
        } else {
            entries.push(name);
        }
    }
    entries.sort();
    let total = entries.len();
    entries.truncate(200);
    let mut text = format!(
        "{url}: a folder ({total} entr{}). Open one by its site URL:\n",
        if total == 1 { "y" } else { "ies" }
    );
    text.push_str(&entries.join("\n"));
    if total > entries.len() {
        text.push_str(&format!("\n… {} more", total - entries.len()));
    }
    Some(text)
}

/// Text beyond this is cut when an app's data file is read.
const MAX_DATA_TEXT: usize = 100 * 1024;
/// Files listed at most for a data folder.
const MAX_LISTED: usize = 200;

/// `data://...`: an app's work data (ticket 0171), read-only. A folder lists its files; an image
/// is shown; text is read; anything else is described.
fn data(context: &ToolContext, raw: &str) -> Result<CallResult, String> {
    let url = DataUrl::parse(raw)?;
    let server = &context.server;
    let Some(app) = &url.app else {
        let folder = url.folder(server)?;
        let mut apps: Vec<String> = std::fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|app| SiteUrl::new(app, "").is_ok_and(|s| context.check_read(&s).is_ok()))
            .collect();
        apps.sort();
        let text = match apps.is_empty() {
            true => format!("{url}: no app keeps data here yet"),
            false => format!(
                "{url}: data of {}",
                apps.iter()
                    .map(|a| format!("{url}{a}/"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        return Ok(text_result(text, false));
    };
    let site = SiteUrl::new(app, "").map_err(|e| e.to_string())?;
    context.check_read(&site)?;
    let folder = url.folder(server)?;
    let target = match url.path.is_empty() {
        true => folder.clone(),
        false => path_inside(&folder, &url.path, &format!("the {app} app's data"))?,
    };
    if target.is_dir() {
        return Ok(text_result(listing(&url, &target), false));
    }
    let pictured = target
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if pictured {
        return image(context, raw, &target);
    }
    let bytes = std::fs::read(&target)
        .map_err(|_| format!("{url}: no such file (`open` its folder to list what there is)"))?;
    let text = match String::from_utf8(bytes) {
        Ok(mut text) => {
            if text.len() > MAX_DATA_TEXT {
                let mut end = MAX_DATA_TEXT;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                text.push_str("\n… (cut: the file is over 100 KB)");
            }
            format!("url: {url}\n\n{text}")
        }
        Err(e) => format!(
            "{url}: a binary file ({} KB), not shown",
            e.as_bytes().len().div_ceil(1024)
        ),
    };
    Ok(text_result(text, false))
}

/// The files under `folder` (relative, with sizes), at most [`MAX_LISTED`].
fn listing(url: &DataUrl, folder: &Path) -> String {
    let mut files: Vec<(String, u64)> = walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let relative = e.path().strip_prefix(folder).ok()?.display().to_string();
            Some((relative, e.metadata().map(|m| m.len()).unwrap_or(0)))
        })
        .take(MAX_LISTED + 1)
        .collect();
    files.sort();
    if files.is_empty() {
        return format!("{url}: empty");
    }
    let more = files.len() > MAX_LISTED;
    files.truncate(MAX_LISTED);
    let mut text = format!("{url}:\n");
    for (file, size) in &files {
        text.push_str(&format!("- {file} ({} KB)\n", size.div_ceil(1024)));
    }
    if more {
        text.push_str("… (more)\n");
    }
    text
}

/// The file `raw` names when it is an image: a picture in a site, or an image file in the
/// session's project (its worktree, for a worker). `None`: a page, as always.
fn image_path(context: &ToolContext, raw: &str) -> Option<Result<PathBuf, String>> {
    let raw = raw.trim();
    let pictured = Path::new(raw)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if raw.starts_with('/') || raw.starts_with('~') {
        return Some(project_file(context, raw));
    }
    if !pictured {
        return None;
    }
    // A relative picture: the project's, else a site's (`designer/mock.png`).
    if !raw.starts_with("site://")
        && let Ok(path) = project_file(context, raw)
    {
        return Some(Ok(path));
    }
    let url = match context.url(raw) {
        Ok(url) => url,
        Err(error) => return Some(Err(error)),
    };
    Some(context.check_read(&url).and_then(|()| {
        context
            .server
            .web
            .locate(&url)
            .map_err(|e| format!("{e}. Use `search` to find pages, or `list_sites`."))
    }))
}

/// `raw` in the session's project, which it may not leave.
fn project_file(context: &ToolContext, raw: &str) -> Result<PathBuf, String> {
    let project = context
        .session
        .info()
        .project
        .ok_or("no active project: open a site's picture with a site:// URL")?;
    // Checked again: a session's project may have become another workspace's since it was set.
    let project = PathBuf::from(context.server.see_project(&project)?);
    let path = if raw.starts_with('/') || raw.starts_with('~') {
        expand_home(raw)
    } else {
        project.join(raw)
    };
    let path = path.canonicalize().map_err(|e| format!("{raw}: {e}"))?;
    if !path.starts_with(&project) {
        return Err(format!(
            "{raw} is outside your project ({}): only its files open here",
            project.display()
        ));
    }
    Ok(path)
}

/// The image at `path` for the model: named in the text, the picture beside it.
fn image(context: &ToolContext, raw: &str, path: &Path) -> Result<CallResult, String> {
    let max = context.server.config().attachments.max_mb * 1024 * 1024;
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{raw}: {e}"))?
        .len();
    if size > max {
        return Err(format!("{raw} is larger than {} MB", max / 1024 / 1024));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{raw}: {e}"))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    let Some((AttachmentKind::Image, mime)) = classify(name, &bytes) else {
        return Err(format!(
            "{raw} is not an image (PNG, JPEG, GIF, WebP): read text files with the shell"
        ));
    };
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(CallResult {
        text: format!(
            "{raw}: image ({mime}, {} KB), shown to you",
            size.div_ceil(1024)
        ),
        is_error: false,
        images: vec![format!("data:{mime};base64,{data}")],
    })
}
