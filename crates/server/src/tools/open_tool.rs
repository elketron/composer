use std::path::{Path, PathBuf};

use async_trait::async_trait;
use base64::Engine;
use composer_protocol::ServerMessage;
use serde_json::{Value, json};

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
         or a path relative to the project)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "e.g. site://memory/user, or an image: site://designer/mock.png, build/shot.png" } },
            "required": ["url"]
        })
    }

    async fn call_result(&self, context: &ToolContext, args: Value) -> CallResult {
        let raw = args["url"].as_str().unwrap_or_default().to_string();
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
        let page = server
            .workspace
            .read_page(&url)
            .map_err(|e| format!("{e}. Use `search` to find pages, or `list_sites`."))?;

        let session = context.session.id().to_string();
        let canonical = page.url.to_string();
        context
            .session
            .emit(&server.hub, |seq| ServerMessage::AgentNavigated {
                session,
                seq,
                url: canonical,
            });

        let mut header = format!("url: {}", page.url);
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
            .workspace
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
    let project = expand_home(&project)
        .canonicalize()
        .map_err(|e| format!("the project folder: {e}"))?;
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
