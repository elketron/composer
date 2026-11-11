use async_trait::async_trait;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::tools::args::EditArgs;
use crate::tools::{Tool, ToolContext};

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Change part of an existing page: replaces `old_text` with `new_text`. \
         `old_text` must match the page exactly (whitespace included) and occur once, \
         unless `replace_all` is true. Open the page first to copy the exact text; pass the \
         `hash` `open` printed as `if_hash` and a stale page is refused instead of edited. \
         When the text is not found, the nearest block is returned."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "old_text": { "type": "string" },
                "new_text": { "type": "string" },
                "replace_all": { "type": "boolean" }
            },
            "required": ["url", "old_text", "new_text"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: EditArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let url = context.url(&args.url)?;
        context.check_write(&url)?;
        if args.old_text.is_empty() {
            return Err("old_text is empty; use `write` to replace a whole page".into());
        }
        let page = context
            .server
            .web
            .read_page(&url)
            .map_err(|e| e.to_string())?;
        let hash = crate::content_hash(page.raw.as_bytes());
        let short = &hash[..12];
        if let Some(expected) = args
            .if_hash
            .as_deref()
            .map(str::trim)
            .filter(|hash| !hash.is_empty())
            && !hash.starts_with(expected)
        {
            return Err(format!(
                "page changed since you opened it (now {short}); open it again and redo the edit"
            ));
        }
        let count = page.raw.matches(&args.old_text).count();
        let updated = match count {
            0 => {
                let nearest = nearest_hint(&page.raw, &args.old_text);
                return Err(format!(
                    "old_text not found in {} (hash {short}); open the page and copy the exact \
                     text.\n\nNearest text:\n{nearest}",
                    page.url
                ));
            }
            1 => page.raw.replacen(&args.old_text, &args.new_text, 1),
            _ if args.replace_all => page.raw.replace(&args.old_text, &args.new_text),
            n => {
                return Err(format!(
                    "old_text occurs {n} times; include more context or set replace_all"
                ));
            }
        };
        context
            .server
            .write_page(&page.url, &updated, ChangedBy::Agent)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "edited {} ({} replacement{})",
            page.url,
            count,
            if count == 1 { "" } else { "s" }
        ))
    }
}

/// The page around the closest line to `old_text`: a first line still there, else the line
/// sharing the most long words with it. Enough for the model to correct its `old_text`.
fn nearest_hint(raw: &str, old_text: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.is_empty() {
        return "(the page is empty)".into();
    }
    let first = old_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty());
    let at = first
        .and_then(|first| lines.iter().position(|line| line.trim() == first))
        .or_else(|| {
            let words: Vec<&str> = old_text
                .split_whitespace()
                .filter(|word| word.len() > 3)
                .collect();
            let mut best = (0, 0);
            for (i, line) in lines.iter().enumerate() {
                let score = words.iter().filter(|word| line.contains(**word)).count();
                if score > best.1 {
                    best = (i, score);
                }
            }
            Some(best.0).filter(|_| best.1 > 0)
        })
        .unwrap_or(0);
    let count = old_text.lines().count().clamp(1, 20);
    let end = (at + count + 2).min(lines.len());
    let mut hint = lines[at..end].join("\n");
    if hint.len() > 2000 {
        let end = hint.floor_char_boundary(2000);
        hint.truncate(end);
        hint.push_str("\n…");
    }
    hint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_the_nearest_block() {
        let raw = "one\ntwo\nthree\nfour\nfive\n";
        assert!(nearest_hint(raw, "three\nfour").starts_with("three\nfour"));
        assert!(nearest_hint(raw, "three\nchanged").starts_with("three\nfour"));
        assert!(nearest_hint(raw, "unrelated").contains("one"));
    }
}
