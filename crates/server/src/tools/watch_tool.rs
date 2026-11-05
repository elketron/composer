use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::WatchArgs;
use crate::tools::{Tool, ToolContext};
use crate::watches::FileWatch;

pub struct WatchTool;

#[async_trait]
impl Tool for WatchTool {
    fn name(&self) -> &'static str {
        "watch"
    }

    fn description(&self) -> &'static str {
        "Get a message when files in a site change (created, written, deleted), instead of \
         checking back: e.g. a render or a job report landing. `pattern` is a file's URL, \
         `site://x/dir/*` (files in dir) or `site://x/dir/**` (anything below); a glob only at \
         the end. By default the watch ends after the first message (`once: false` keeps it). \
         Your own writes through write/edit are not reported (files your actions write are). \
         `stop: true` removes a watch; no pattern lists yours."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "e.g. site://video/work/renders/**" },
                "once": { "type": "boolean", "description": "end after the first message (default true)" },
                "stop": { "type": "boolean", "description": "remove this watch" }
            }
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: WatchArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let server = &context.server;
        if server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot watch files; report back with `finish` instead".into());
        }
        let session = context.session.id();
        let Some(raw) = args.pattern else {
            let watches = server.watches.for_session(session);
            if watches.is_empty() {
                return Ok("You watch nothing.".into());
            }
            return Ok(watches
                .iter()
                .map(|w| format!("{}{}", w.pattern, if w.once { " (once)" } else { "" }))
                .collect::<Vec<_>>()
                .join("\n"));
        };
        let pattern = normalize(&raw);
        if args.stop {
            return Ok(if server.watches.remove(session, &pattern) {
                format!("stopped watching {pattern}")
            } else {
                format!("you weren't watching {pattern}")
            });
        }
        // The site part must be a URL you could open.
        let base = pattern
            .strip_suffix("/**")
            .or_else(|| pattern.strip_suffix("/*"))
            .unwrap_or(&pattern)
            .to_string();
        if base.contains('*') {
            return Err(format!(
                "{pattern}: only a trailing /* or /** is supported, e.g. site://x/dir/**"
            ));
        }
        let url = context.url(&base)?;
        context.check_read(&url)?;
        server.watches.add(FileWatch {
            session: session.to_string(),
            pattern: pattern.clone(),
            once: args.once,
        });
        Ok(format!(
            "watching {pattern}{}; changes arrive as a message",
            if args.once {
                " until the first change"
            } else {
                ""
            }
        ))
    }
}

/// `video/renders/**` means `site://video/renders/**`.
fn normalize(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("site://") {
        raw.to_string()
    } else {
        format!("site://{}", raw.trim_start_matches('/'))
    }
}
