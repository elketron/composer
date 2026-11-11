//! Hunk: a live review TUI with a control API. The TUI belongs to the user; the agent reads the
//! live review and the user's notes, replies, and moves the view through `hunk session *`. The
//! terminal's keys are not the agent's interface.

use async_trait::async_trait;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::project::ProjectPath;
use crate::tools::{Tool, ToolContext, expand_home};

pub struct HunkTool;

#[async_trait]
impl Tool for HunkTool {
    fn name(&self) -> &'static str {
        "hunk"
    }

    fn description(&self) -> &'static str {
        "Work with a live Hunk review (a terminal diff TUI). The user reviews in Hunk; this reads \
         the live session and its notes and writes back through Hunk's own CLI. Actions: \
         `sessions` (list live reviews), `review` (files, hunks, counts), `notes` (the user's \
         inline notes), `reply` (answer a note by id), `comment` (add an agent note at a file and \
         line), `navigate` (move the user's view), `export` (write a review page). Never send \
         keys to the Hunk terminal."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["sessions", "review", "notes", "reply", "comment", "navigate", "export"],
                    "description": "what to do"
                },
                "repo": { "type": "string", "description": "the project or worktree Hunk reviews; default: the session's project" },
                "note": { "type": "string", "description": "a note id from `notes` (for reply/navigate)" },
                "text": { "type": "string", "description": "the reply or note text" },
                "file": { "type": "string", "description": "a path as Hunk shows it, e.g. src/main.rs" },
                "line": { "type": "integer", "description": "the line in the diff (new side by default)" },
                "side": { "type": "string", "enum": ["new", "old"], "description": "which side `line` is on (default new)" },
                "url": { "type": "string", "description": "for `export`: the site:// page to write" },
                "all": { "type": "boolean", "description": "for `notes`: include agent notes too (default: the user's)" },
                "focus": { "type": "boolean", "description": "for `comment`: move the user's view to it" }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        match args["action"].as_str().unwrap_or_default() {
            "sessions" => sessions(),
            "review" => {
                let repo = repo(context, &args)?;
                let result = hunk(&[
                    "session".into(),
                    "review".into(),
                    "--repo".into(),
                    repo,
                    "--include-notes".into(),
                    "--json".into(),
                ])?;
                Ok(review_text(&result["review"]))
            }
            "notes" => {
                let repo = repo(context, &args)?;
                let all = args["all"].as_bool().unwrap_or(false);
                let result = hunk(&[
                    "session".into(),
                    "comment".into(),
                    "list".into(),
                    "--repo".into(),
                    repo,
                    "--type".into(),
                    if all { "all" } else { "user" }.into(),
                    "--json".into(),
                ])?;
                Ok(notes_text(&result["comments"]))
            }
            "reply" => {
                let repo = repo(context, &args)?;
                let note = required(&args, "note")?;
                let text = required(&args, "text")?;
                let result = hunk(&[
                    "session".into(),
                    "comment".into(),
                    "add".into(),
                    "--repo".into(),
                    repo,
                    "--reply-to".into(),
                    note.clone(),
                    "--summary".into(),
                    text,
                    "--author".into(),
                    "agent".into(),
                    "--json".into(),
                ])?;
                Ok(format!(
                    "replied to note {} (new note {})",
                    note,
                    result["result"]["commentId"].as_str().unwrap_or("?")
                ))
            }
            "comment" => {
                let repo = repo(context, &args)?;
                let file = required(&args, "file")?;
                let text = required(&args, "text")?;
                let line = args["line"]
                    .as_u64()
                    .ok_or_else(|| "`comment` needs `line`".to_string())?;
                let old = args["side"].as_str() == Some("old");
                let mut call = vec![
                    "session".into(),
                    "comment".into(),
                    "add".into(),
                    "--repo".into(),
                    repo,
                    "--file".into(),
                    file.clone(),
                    if old { "--old-line" } else { "--new-line" }.into(),
                    line.to_string(),
                    "--summary".into(),
                    text,
                    "--author".into(),
                    "agent".into(),
                ];
                if args["focus"].as_bool().unwrap_or(false) {
                    call.push("--focus".into());
                }
                call.push("--json".into());
                let result = hunk(&call)?;
                Ok(format!(
                    "added note {} at {}:{}",
                    result["result"]["commentId"].as_str().unwrap_or("?"),
                    file,
                    line
                ))
            }
            "navigate" => {
                let repo = repo(context, &args)?;
                if let Some(note) = args["note"].as_str().filter(|n| !n.trim().is_empty()) {
                    hunk(&[
                        "session".into(),
                        "navigate".into(),
                        "--repo".into(),
                        repo,
                        "--comment".into(),
                        note.into(),
                        "--json".into(),
                    ])?;
                    return Ok(format!("moved the user's view to note {note}"));
                }
                let file = required(&args, "file")?;
                let line = args["line"]
                    .as_u64()
                    .ok_or_else(|| "`navigate` needs `note`, or `file` and `line`".to_string())?;
                let old = args["side"].as_str() == Some("old");
                hunk(&[
                    "session".into(),
                    "navigate".into(),
                    "--repo".into(),
                    repo,
                    "--file".into(),
                    file.clone(),
                    if old { "--old-line" } else { "--new-line" }.into(),
                    line.to_string(),
                    "--json".into(),
                ])?;
                Ok(format!("moved the user's view to {file}:{line}"))
            }
            "export" => {
                let repo = repo(context, &args)?;
                let url = context.url(&required(&args, "url")?)?;
                context.check_write(&url)?;
                let result = hunk(&[
                    "session".into(),
                    "review".into(),
                    "--repo".into(),
                    repo,
                    "--include-notes".into(),
                    "--json".into(),
                ])?;
                let review = &result["review"];
                let page = export_markdown(review);
                context
                    .server
                    .write_page(&url, &page, ChangedBy::Agent)
                    .map_err(|e| e.to_string())?;
                Ok(format!(
                    "wrote the review to {url} ({} files, {} notes)",
                    review["files"].as_array().map_or(0, Vec::len),
                    review["reviewNotes"].as_array().map_or(0, Vec::len)
                ))
            }
            other => Err(format!(
                "unknown hunk action `{other}` (sessions, review, notes, reply, comment, navigate, export)"
            )),
        }
    }
}

fn required(args: &Value, key: &str) -> Result<String, String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("`hunk` needs `{key}` for this action"))
}

/// The repo to ask Hunk about: an explicit one, the session's project, or the only live review.
fn repo(context: &ToolContext, args: &Value) -> Result<String, String> {
    let explicit = args["repo"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| ProjectPath::existing(&expand_home(value)))
        .transpose()?
        .map(|path| context.server.see_project(&path.to_string()))
        .transpose()?;
    let preferred = explicit.or_else(|| {
        context
            .session
            .info()
            .project
            .and_then(|project| context.server.see_project(&project).ok())
    });
    let sessions = hunk(&["session".into(), "list".into(), "--json".into()])
        .ok()
        .and_then(|value| value["sessions"].as_array().cloned())
        .unwrap_or_default();
    if let Some(preferred) = preferred {
        if sessions
            .iter()
            .any(|session| same_path(session["repoRoot"].as_str(), &preferred))
        {
            return Ok(preferred);
        }
        // A review started at a worktree while the session still points at the project.
        if let [only] = sessions.as_slice()
            && let Some(root) = only["repoRoot"].as_str()
        {
            return Ok(root.to_string());
        }
        return Ok(preferred);
    }
    match sessions.as_slice() {
        [only] => only["repoRoot"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "hunk's live session has no repo".into()),
        [] => Err("no live hunk session; open a `type: tui` review page first".into()),
        _ => Err("several hunk reviews are live; give `repo`".into()),
    }
}

fn same_path(a: Option<&str>, b: &str) -> bool {
    let canonical = |value: &str| std::fs::canonicalize(value).ok();
    matches!((a.and_then(canonical), canonical(b)), (Some(a), Some(b)) if a == b)
}

/// `hunk ... --json` on the host: Hunk's daemon is the user's, next to the TUI.
#[allow(
    clippy::disallowed_methods,
    reason = "hunk is the user's host tool with a local daemon; it is not an agent shell command"
)]
fn hunk(args: &[String]) -> Result<Value, String> {
    let bin = std::env::var("HUNK_BIN").unwrap_or_else(|_| "hunk".into());
    let output = std::process::Command::new(&bin)
        .args(args)
        .output()
        .map_err(|e| format!("running {bin}: {e} (is hunk installed and on PATH?)"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        return Err(format!(
            "hunk {}: {}",
            args.join(" "),
            if message.is_empty() {
                "it failed"
            } else {
                message
            }
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("hunk returned no JSON: {e} (hunk --version may be too old)"))
}

fn sessions() -> Result<String, String> {
    let value = hunk(&["session".into(), "list".into(), "--json".into()])?;
    let sessions = value["sessions"].as_array().cloned().unwrap_or_default();
    if sessions.is_empty() {
        return Ok("no live hunk review; open a `type: tui` review page first".into());
    }
    let mut out = format!("{} live hunk review(s):\n", sessions.len());
    for session in &sessions {
        let selected = session["snapshot"]["state"]["selectedFilePath"]
            .as_str()
            .unwrap_or("?");
        out.push_str(&format!(
            "- {} — {} ({} files, at {}) in {}\n",
            session["sessionId"].as_str().unwrap_or("?"),
            session["title"].as_str().unwrap_or("review"),
            session["fileCount"].as_u64().unwrap_or(0),
            selected,
            session["repoRoot"].as_str().unwrap_or("?")
        ));
    }
    Ok(out.trim_end().to_string())
}

fn review_text(review: &Value) -> String {
    let title = review["title"].as_str().unwrap_or("review");
    let repo = review["repoRoot"].as_str().unwrap_or("?");
    let mut out = format!("hunk review: {title}\nrepo: {repo}\n");
    if let Some(file) = review["selectedFile"]["path"].as_str() {
        out.push_str(&format!("selected: {file}"));
        if let Some(hunk) = review["selectedHunk"]["header"].as_str() {
            out.push_str(&format!(" {hunk}"));
        }
        out.push('\n');
    }
    let files = review["files"].as_array().cloned().unwrap_or_default();
    out.push_str(&format!("files ({}):\n", files.len()));
    for file in &files {
        out.push_str(&format!(
            "- {} (+{} -{}, {} hunk{})\n",
            file["path"].as_str().unwrap_or("?"),
            file["additions"].as_u64().unwrap_or(0),
            file["deletions"].as_u64().unwrap_or(0),
            file["hunkCount"].as_u64().unwrap_or(0),
            if file["hunkCount"].as_u64().unwrap_or(0) == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    let notes = review["reviewNotes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let user = notes
        .iter()
        .filter(|note| note["source"].as_str() == Some("user"))
        .count();
    out.push_str(&format!(
        "notes: {} ({} user, {} agent)",
        notes.len(),
        user,
        notes.len() - user
    ));
    out
}

fn notes_text(comments: &Value) -> String {
    let comments = comments.as_array().cloned().unwrap_or_default();
    if comments.is_empty() {
        return "no notes".into();
    }
    let mut out = format!("{} note(s):\n", comments.len());
    for note in &comments {
        out.push_str(&format!(
            "- {} {}\n",
            note["noteId"].as_str().unwrap_or("?"),
            note_summary(note)
        ));
    }
    out.trim_end().to_string()
}

fn note_summary(note: &Value) -> String {
    let source = note["source"].as_str().unwrap_or("?");
    let file = note["filePath"].as_str().unwrap_or("?");
    let line = note["line"]
        .as_u64()
        .or_else(|| note["newRange"][0].as_u64())
        .or_else(|| note["oldRange"][0].as_u64());
    let side = if note["side"].as_str() == Some("old") || note["newRange"][0].is_null() {
        "old"
    } else {
        "new"
    };
    let anchor = match line {
        Some(line) => format!("{file}:{line} ({side})"),
        None => file.to_string(),
    };
    let body = note["body"]
        .as_str()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    format!("{source} `{anchor}` — {body}")
}

fn export_markdown(review: &Value) -> String {
    let title = review["title"].as_str().unwrap_or("Review");
    let repo = review["repoRoot"].as_str().unwrap_or("");
    let notes = review["reviewNotes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let files = review["files"].as_array().cloned().unwrap_or_default();
    let mut text = format!(
        "---\ntitle: {}\ntype: review\nrepo: {}\nexported: {}\nnotes: {}\n---\n\n# {}\n\n",
        serde_json::to_string(title).expect("a string serializes"),
        serde_json::to_string(repo).expect("a string serializes"),
        serde_json::to_string(&chrono::Local::now().to_rfc3339()).expect("a string serializes"),
        notes.len(),
        title
    );
    text.push_str(&format!("`{repo}`\n\n## Files\n\n"));
    if files.is_empty() {
        text.push_str("No files.\n");
    } else {
        text.push_str("| file | + | − | hunks |\n|---|---|---|---|\n");
        for file in &files {
            let path = file["path"].as_str().unwrap_or("?").replace('|', "\\|");
            text.push_str(&format!(
                "| `{path}` | {} | {} | {} |\n",
                file["additions"].as_u64().unwrap_or(0),
                file["deletions"].as_u64().unwrap_or(0),
                file["hunkCount"].as_u64().unwrap_or(0)
            ));
        }
    }
    text.push_str("\n## Notes\n\n");
    if notes.is_empty() {
        text.push_str("No notes.\n");
    } else {
        for note in &notes {
            text.push_str(&format!("- {}\n", note_summary(note)));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review() -> Value {
        json!({
            "review": {
                "title": "save working tree",
                "repoRoot": "/p/save",
                "selectedFile": { "path": "src/save.rs" },
                "selectedHunk": { "header": "@@ -1,2 +1,3 @@" },
                "files": [
                    { "path": "src/save.rs", "additions": 3, "deletions": 1, "hunkCount": 1 }
                ],
                "reviewNotes": [
                    { "noteId": "user:1", "source": "user", "filePath": "src/save.rs", "newRange": [12, 12], "body": "This can race." },
                    { "noteId": "mcp:2", "source": "agent", "filePath": "src/save.rs", "newRange": [13, 13], "body": "Fixed in the next commit." }
                ]
            }
        })
    }

    #[test]
    fn reads_a_live_review() {
        let text = review_text(&review()["review"]);
        assert!(
            text.contains("selected: src/save.rs @@ -1,2 +1,3 @@"),
            "{text}"
        );
        assert!(text.contains("notes: 2 (1 user, 1 agent)"), "{text}");
    }

    #[test]
    fn lists_notes_with_the_anchor() {
        let text = notes_text(&review()["review"]["reviewNotes"]);
        assert!(
            text.contains("user:1 user `src/save.rs:12 (new)` — This can race."),
            "{text}"
        );
    }

    #[test]
    fn exports_a_review_page() {
        let page = export_markdown(&review()["review"]);
        assert!(page.contains("type: review"), "{page}");
        assert!(page.contains("| `src/save.rs` | 3 | 1 | 1 |"), "{page}");
        assert!(page.contains("user `src/save.rs:12 (new)`"), "{page}");
    }
}
