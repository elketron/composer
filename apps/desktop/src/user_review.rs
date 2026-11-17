//! The coding site's user review (site://coding `_shared/how-to-user-review.md`): the agent opens
//! the run's `review.md`, a TUI page (`type: tui`, `control: hunk`) that starts Hunk on the
//! stage; you leave notes
//! in Hunk, then say `pass` or `fail: <why>` in the chat and the agent writes `check.md`. The
//! chat offers that verdict while a review is open: nothing on the server, only the message.

use composer_client::ChatItem;

use crate::outbox::Outbox;

/// A Hunk review the agent opened in this chat.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    /// The call id of the `open` that started it: what the outbox keys its verdict by.
    pub call_id: String,
    /// The review's ticket (`""` when its run can't be told).
    pub ticket: String,
}

/// A verdict to send, and the review it answers (the outbox's key for it).
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub text: String,
    pub review: String,
}

/// The verdict card the chat shows.
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictCard {
    pub ticket: String,
    /// The verdict you gave this review, sent and not back in the transcript yet (the chat's
    /// outbox): the card shows it as given and offers nothing more to click.
    pub sent: Option<String>,
}

/// The card while a review is open, with this review's verdict if the outbox holds one.
pub fn verdict_card(items: &[ChatItem], session: &str, outbox: &Outbox) -> Option<VerdictCard> {
    pending_review(items).map(|review| VerdictCard {
        sent: outbox.queued(session, &review.call_id).map(str::to_string),
        ticket: review.ticket,
    })
}

/// Pass clicked on the card: the message to send, or `None` when there is nothing to answer or
/// this review's verdict is already on its way (a second click).
pub fn on_pass(items: &[ChatItem], session: &str, outbox: &mut Outbox) -> Option<Verdict> {
    let review = pending_review(items)?;
    outbox
        .queue(session, &review.call_id, "pass")
        .then(|| Verdict {
            text: "pass".into(),
            review: review.call_id,
        })
}

/// A message you send from the composer: when it is the verdict on the open review (Changes…
/// starts `fail: `) and that review has none on its way, it goes into the outbox too. Returns
/// the review it answers then, so a failed send can take it out again. A second verdict while
/// one is on its way is still sent (you wrote it), but neither adds an entry nor moves the card.
pub fn on_send(
    items: &[ChatItem],
    session: &str,
    text: &str,
    outbox: &mut Outbox,
) -> Option<String> {
    if !is_verdict(text) {
        return None;
    }
    let review = pending_review(items)?;
    outbox
        .queue(session, &review.call_id, text)
        .then_some(review.call_id)
}

/// The outbox after the transcript changed: an entry whose review is no longer the open one
/// (your verdict came back, or another review was opened since) goes.
pub fn sync_outbox(items: &[ChatItem], session: &str, outbox: &mut Outbox) -> bool {
    let open = pending_review(items).map(|r| r.call_id);
    outbox.sync(session, open.as_deref())
}

/// The Hunk review the agent opened in this chat and you haven't given a verdict on since, or
/// `None`.
pub fn pending_review(items: &[ChatItem]) -> Option<Review> {
    let mut pending = None;
    for item in items {
        match item {
            ChatItem::Tool {
                call_id,
                name,
                summary,
                result: Some(result),
                is_error: false,
                ..
            } if name == "open" && opens_hunk(result) => {
                pending = Some(Review {
                    call_id: call_id.clone(),
                    ticket: ticket_in(summary)
                        .or_else(|| ticket_in(result))
                        .unwrap_or_default(),
                });
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

/// `pass`, or `fail: <why>`: what the review stage takes as your word. The word itself, ended
/// by the end, a space or a `:` (or `.`/`!` after pass), so `failed to build` or `passing`
/// is not one.
fn is_verdict(text: &str) -> bool {
    let text = text.trim_start().to_lowercase();
    let ends_word = |rest: &str, extra: &[char]| {
        rest.chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == ':' || extra.contains(&c))
    };
    if let Some(rest) = text.strip_prefix("pass") {
        ends_word(rest, &['.', '!'])
    } else if let Some(rest) = text.strip_prefix("fail") {
        ends_word(rest, &[])
    } else {
        false
    }
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

    const REVIEW: &str = "url: site://coding/works/0014/review.md\n\n---\ntitle: Review 0014\n\
                          type: tui\ncontrol: hunk\ncwd: project\n---\n# Review 0014";

    fn open(url: &str, result: &str) -> ChatItem {
        open_as("c", url, result)
    }

    fn open_as(call_id: &str, url: &str, result: &str) -> ChatItem {
        ChatItem::Tool {
            call_id: call_id.into(),
            name: "open".into(),
            summary: url.into(),
            detail: None,
            nested: false,
            result: Some(result.into()),
            is_error: false,
        }
    }

    fn review(call_id: &str) -> ChatItem {
        open_as(call_id, "site://coding/works/0014/review.md", REVIEW)
    }

    fn said(text: &str) -> ChatItem {
        ChatItem::User {
            text: text.into(),
            source: None,
            files: vec![],
        }
    }

    fn ticket(items: &[ChatItem]) -> Option<String> {
        pending_review(items).map(|r| r.ticket)
    }

    fn sent(items: &[ChatItem], outbox: &Outbox) -> Option<String> {
        verdict_card(items, "s", outbox).and_then(|c| c.sent)
    }

    #[test]
    fn open_until_a_verdict() {
        // `open` answers with the page's URL and the page; only its frontmatter says Hunk.
        let mut items = vec![
            said("review it"),
            open("site://coding/works/0014/review.md", REVIEW),
        ];
        assert_eq!(ticket(&items).as_deref(), Some("0014"));
        items.push(said("the second hunk is odd"));
        assert_eq!(
            ticket(&items).as_deref(),
            Some("0014"),
            "a note, not a verdict"
        );
        items.push(said("Fail: rename the flag"));
        assert_eq!(ticket(&items), None);
        // Opened by a relative URL: the ticket comes from the canonical one in the answer.
        items.push(open("../../review.md", REVIEW));
        assert_eq!(ticket(&items).as_deref(), Some("0014"));
        items.push(said("pass"));
        assert_eq!(ticket(&items), None);
    }

    #[test]
    fn only_the_word_is_a_verdict() {
        for text in [
            "pass",
            "Pass.",
            " PASS",
            "pass: fine",
            "fail",
            "fail: why",
            "Fail it",
        ] {
            assert!(is_verdict(text), "{text}");
        }
        for text in [
            "passing thought",
            "passed",
            "failed to build earlier",
            "fails",
            "ok",
        ] {
            assert!(!is_verdict(text), "{text}");
        }
    }

    /// Two clicks on Pass: one message, and the card shows it given before it comes back.
    #[test]
    fn pass_clicked_twice_sends_once() {
        let mut items = vec![review("c1")];
        let mut outbox = Outbox::default();
        assert_eq!(
            verdict_card(&items, "s", &outbox),
            Some(VerdictCard {
                ticket: "0014".into(),
                sent: None
            })
        );
        let sends: Vec<_> = (0..2).map(|_| on_pass(&items, "s", &mut outbox)).collect();
        let pass = Verdict {
            text: "pass".into(),
            review: "c1".into(),
        };
        assert_eq!(sends, [Some(pass), None], "two clicks, one message");
        assert_eq!(
            sent(&items, &outbox).as_deref(),
            Some("pass"),
            "decided before the message comes back"
        );
        // It comes back: the card goes and so does the entry.
        items.push(said("pass"));
        sync_outbox(&items, "s", &mut outbox);
        assert_eq!(verdict_card(&items, "s", &outbox), None);
        assert_eq!(outbox.queued("s", "c1"), None);
    }

    #[test]
    fn a_failed_send_reopens_the_card() {
        let items = vec![review("c1")];
        let mut outbox = Outbox::default();
        assert!(on_pass(&items, "s", &mut outbox).is_some());
        outbox.failed("s", "c1");
        assert_eq!(sent(&items, &outbox), None);
        assert!(
            on_pass(&items, "s", &mut outbox).is_some(),
            "Pass works again"
        );
    }

    #[test]
    fn a_fail_verdict_behaves_as_before() {
        let mut items = vec![review("c1")];
        let mut outbox = Outbox::default();
        assert_eq!(
            on_send(&items, "s", "looks odd", &mut outbox),
            None,
            "a note"
        );
        assert_eq!(sent(&items, &outbox), None);
        // Changes… writes `fail: ` in the composer; sending it is a verdict like Pass.
        assert_eq!(
            on_send(&items, "s", "fail: rename the flag", &mut outbox).as_deref(),
            Some("c1")
        );
        assert_eq!(
            on_pass(&items, "s", &mut outbox),
            None,
            "no Pass after a fail"
        );
        // A second verdict typed meanwhile is sent, but takes no entry and leaves the card.
        assert_eq!(
            on_send(&items, "s", "fail: and the docs", &mut outbox),
            None
        );
        assert_eq!(
            sent(&items, &outbox).as_deref(),
            Some("fail: rename the flag")
        );
        items.push(said("fail: rename the flag"));
        sync_outbox(&items, "s", &mut outbox);
        assert_eq!(ticket(&items), None);
        assert_eq!(verdict_card(&items, "s", &outbox), None);
        assert_eq!(outbox.queued("s", "c1"), None, "drained");
    }

    /// The echo of a verdict can miss the chat (sent from another session's view, detached):
    /// the next review starts without it.
    #[test]
    fn a_verdict_does_not_carry_over_to_the_next_review() {
        let mut items = vec![review("c1")];
        let mut outbox = Outbox::default();
        on_send(&items, "s", "fail: x", &mut outbox);
        items.push(review("c2"));
        assert_eq!(
            sent(&items, &outbox),
            None,
            "the new review has no verdict yet"
        );
        assert_eq!(
            on_pass(&items, "s", &mut outbox)
                .map(|v| v.review)
                .as_deref(),
            Some("c2")
        );
        sync_outbox(&items, "s", &mut outbox);
        assert_eq!(outbox.queued("s", "c1"), None, "the old entry goes");
        assert_eq!(outbox.queued("s", "c2"), Some("pass"));
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
