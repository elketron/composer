//! The stage's tab strip, from the session's desk.

use composer_protocol::{Desk, DeskTab};

// rewrite: phase 2: name the live view after what it shows (a site's `app:` is ignored now).
/// The live view tab's label.
pub const LIVE_VIEW: &str = "Live view";

/// A tab's label in the strip.
pub fn label(tab: &DeskTab) -> String {
    match tab {
        DeskTab::Page { url } => url
            .trim_start_matches("site://")
            .trim_end_matches(".md")
            .to_string(),
        DeskTab::Terminal { .. } => "terminal".into(),
        DeskTab::Changes { project } => format!("changes · {project}"),
        DeskTab::LiveView => LIVE_VIEW.into(),
    }
}

/// The tab shown: the desk's active one, else the first.
pub fn shown(desk: &Desk) -> Option<&str> {
    desk.active
        .as_deref()
        .filter(|id| desk.tabs.iter().any(|t| t.id == *id))
        .or_else(|| desk.tabs.first().map(|t| t.id.as_str()))
}

/// The tab `step` places from the shown one, wrapping (`ctrl+tab` is 1, `ctrl+shift+tab` -1).
pub fn cycled(desk: &Desk, step: isize) -> Option<&str> {
    let count = desk.tabs.len() as isize;
    if count == 0 {
        return None;
    }
    let at = shown(desk)
        .and_then(|id| desk.tabs.iter().position(|t| t.id == id))
        .unwrap_or(0) as isize;
    Some(
        desk.tabs[(at + step).rem_euclid(count) as usize]
            .id
            .as_str(),
    )
}

#[cfg(test)]
mod tests {
    use composer_protocol::DeskEntry;

    use super::*;

    fn desk(ids: &[&str], active: Option<&str>) -> Desk {
        Desk {
            tabs: ids
                .iter()
                .map(|id| DeskEntry {
                    id: id.to_string(),
                    tab: DeskTab::Terminal { id: id.to_string() },
                    unseen: false,
                })
                .collect(),
            active: active.map(str::to_string),
        }
    }

    #[test]
    fn shown_and_cycling() {
        assert_eq!(shown(&desk(&[], None)), None);
        assert_eq!(shown(&desk(&["a", "b"], None)), Some("a"));
        assert_eq!(shown(&desk(&["a", "b"], Some("b"))), Some("b"));
        assert_eq!(shown(&desk(&["a", "b"], Some("gone"))), Some("a"));
        let d = desk(&["a", "b", "c"], Some("c"));
        assert_eq!(cycled(&d, 1), Some("a"));
        assert_eq!(cycled(&d, -1), Some("b"));
        assert_eq!(cycled(&desk(&[], None), 1), None);
    }

    #[test]
    fn labels() {
        let page = DeskTab::Page {
            url: "site://plans/x.md".into(),
        };
        assert_eq!(label(&page), "plans/x");
        assert_eq!(label(&DeskTab::LiveView), "Live view");
    }
}
