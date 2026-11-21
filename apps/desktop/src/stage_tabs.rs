//! The stage's tab strip, from the session's desk.

use composer_protocol::{Desk, DeskTab};

/// The live view tab's label while the session is in no site.
pub const LIVE_VIEW: &str = "Live view";

/// A tab's label in the strip; the live view is named after `site`, the session's, when it has one.
pub fn label(tab: &DeskTab, site: Option<&str>) -> String {
    match tab {
        DeskTab::Page { url } => url
            .trim_start_matches("site://")
            .trim_end_matches(".md")
            .to_string(),
        DeskTab::Terminal { .. } => "terminal".into(),
        DeskTab::Changes { site } => format!("changes · {site}"),
        DeskTab::LiveView => site.map_or(LIVE_VIEW.into(), |site| format!("live · {site}")),
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
        assert_eq!(label(&page, None), "plans/x");
        assert_eq!(label(&DeskTab::LiveView, None), "Live view");
        assert_eq!(label(&DeskTab::LiveView, Some("3d")), "live · 3d");
        let changes = DeskTab::Changes {
            site: "game".into(),
        };
        assert_eq!(label(&changes, Some("3d")), "changes · game");
    }
}
