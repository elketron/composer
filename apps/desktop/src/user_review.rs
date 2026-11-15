//! The coding site's user review (site://coding `_shared/how-to-user-review.md`): the agent opens
//! the run's `review.md`, a TUI page (`type: tui`, `control: hunk`) that starts Hunk on the
//! stage; you leave notes
//! in Hunk, then say `pass` or `fail: <why>` in the chat and the agent writes `check.md`. The
//! chat offers that verdict while a review is open: nothing on the server, only the message.

use composer_client::ChatItem;

/// The ticket whose Hunk review the agent opened in this chat and you haven't given a verdict
/// on since (`""` when its run can't be told), or `None`.
pub fn pending_review(items: &[ChatItem]) -> Option<String> {
    let mut pending = None;
    for item in items {
        match item {
            ChatItem::Tool {
                name,
                summary,
                result: Some(result),
                is_error: false,
                ..
            } if name == "open" && opens_hunk(result) => {
                pending = Some(
                    ticket_in(summary)
                        .or_else(|| ticket_in(result))
                        .unwrap_or_default(),
                );
            }
            ChatItem::User {
                text, source: None, ..
            } if is_verdict(text) => pending = None,
            _ => {}
        }
    }
    pending
}

/// The opened page is a Hunk review: its frontmatter (which `open` answers with) says
/// `type: tui` and `control: hunk`. Not its command line, which each page writes its own way.
fn opens_hunk(result: &str) -> bool {
    let mut lines = result.lines().map(str::trim).skip_while(|l| *l != "---");
    if lines.next().is_none() {
        return false;
    }
    let frontmatter: Vec<&str> = lines.take_while(|l| *l != "---").collect();
    let field = |key: &str| {
        frontmatter.iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == key).then(|| v.trim().trim_matches(['"', '\'']))
        })
    };
    field("type") == Some("tui") && field("control") == Some("hunk")
}

/// `pass`, or `fail…`: what the review stage takes as your word.
fn is_verdict(text: &str) -> bool {
    let first = text.trim_start().to_lowercase();
    first.starts_with("pass") || first.starts_with("fail")
}

/// The `<ticket>` of a run's `works/<ticket>/` in a URL or path.
fn ticket_in(text: &str) -> Option<String> {
    let rest = &text[text.find("works/")? + "works/".len()..];
    let ticket: String = rest
        .chars()
        .take_while(|c| *c != '/' && !c.is_whitespace())
        .collect();
    (!ticket.is_empty()).then_some(ticket)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(url: &str, result: &str) -> ChatItem {
        ChatItem::Tool {
            call_id: "c".into(),
            name: "open".into(),
            summary: url.into(),
            detail: None,
            nested: false,
            result: Some(result.into()),
            is_error: false,
        }
    }

    fn said(text: &str) -> ChatItem {
        ChatItem::User {
            text: text.into(),
            source: None,
            files: vec![],
        }
    }

    #[test]
    fn open_until_a_verdict() {
        // `open` answers with the page's URL and the page; only its frontmatter says Hunk.
        let review = "url: site://coding/works/0014/review.md\n\n---\ntitle: Review 0014\n\
                      type: tui\ncontrol: hunk\ncwd: project\n---\n# Review 0014";
        let mut items = vec![
            said("review it"),
            open("site://coding/works/0014/review.md", review),
        ];
        assert_eq!(pending_review(&items).as_deref(), Some("0014"));
        items.push(said("the second hunk is odd"));
        assert_eq!(
            pending_review(&items).as_deref(),
            Some("0014"),
            "a note, not a verdict"
        );
        items.push(said("Fail: rename the flag"));
        assert_eq!(pending_review(&items), None);
        // Opened by a relative URL: the ticket comes from the canonical one in the answer.
        items.push(open("../../review.md", review));
        assert_eq!(pending_review(&items).as_deref(), Some("0014"));
        items.push(said("pass"));
        assert_eq!(pending_review(&items), None);
    }

    #[test]
    fn other_pages_are_not_reviews() {
        let page = |frontmatter: &str| {
            format!("url: site://coding/works/0014/notes.md\n\n---\n{frontmatter}\n---\n# Notes")
        };
        for result in [
            "url: site://coding/index.md".to_string(),
            page("type: index"),
            page("type: tui\ncontrol: other"),
            // A body that mentions the fields isn't frontmatter.
            "url: site://coding/x.md\n\n# x\ntype: tui\ncontrol: hunk".to_string(),
        ] {
            let items = [open("site://coding/works/0014/notes.md", &result)];
            assert_eq!(pending_review(&items), None, "{result}");
        }
    }
}
