//! The stage's tab strip, from the session's desk.

use composer_protocol::http::SiteSummary;
use composer_protocol::{Desk, DeskTab};

/// The application the live view shows, as far as the client can know: the `app:` of the site the
/// session is in, from the sites listing. `None` outside a workflow, or when the site names none.
pub fn live_app<'a>(site: Option<&str>, sites: &'a [SiteSummary]) -> Option<&'a str> {
    let site = site?;
    sites
        .iter()
        .find(|s| s.name == site)
        .and_then(|s| s.app.as_deref())
}

/// A tab's label in the strip. `live_app` names the application of the live view tab.
pub fn label(tab: &DeskTab, live_app: Option<&str>) -> String {
    match tab {
        DeskTab::App { site, project } => match project {
            Some(project) => format!("{site} · {project}"),
            None => site.clone(),
        },
        DeskTab::Page { url } => url
            .trim_start_matches("site://")
            .trim_end_matches(".md")
            .to_string(),
        DeskTab::Terminal { .. } => "terminal".into(),
        DeskTab::Changes { project } => format!("changes · {project}"),
        DeskTab::LiveView => match live_app {
            Some(app) => format!("live · {app}"),
            None => "live view".into(),
        },
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
        let app = DeskTab::App {
            site: "orchestration".into(),
            project: Some("composer".into()),
        };
        assert_eq!(label(&app, None), "orchestration · composer");
        let page = DeskTab::Page {
            url: "site://plans/x.md".into(),
        };
        assert_eq!(label(&page, None), "plans/x");
        assert_eq!(label(&DeskTab::LiveView, None), "live view");
        assert_eq!(label(&DeskTab::LiveView, Some("blender")), "live · blender");
    }

    fn site(name: &str, app: Option<&str>) -> SiteSummary {
        serde_json::from_value(serde_json::json!({
            "name": name, "title": name, "has_justfile": false, "has_ui": false, "app": app
        }))
        .unwrap()
    }

    #[test]
    fn the_live_view_is_named_after_the_sessions_application() {
        let sites = [site("3d", Some("blender")), site("notes", None)];
        assert_eq!(live_app(Some("3d"), &sites), Some("blender"));
        assert_eq!(live_app(Some("notes"), &sites), None, "no application");
        assert_eq!(live_app(Some("gone"), &sites), None, "not in the listing");
        assert_eq!(live_app(None, &sites), None, "not in a workflow");
    }
}
