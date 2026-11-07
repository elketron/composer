use composer_core::SiteUrl;
use composer_protocol::{Desk, DeskBy, DeskEntry, DeskTab};

use crate::apps::ProjectName;

/// A session's desk with what can be done to it (plan phase 5, ticket 0173). A tab's id comes
/// from what it shows, so opening the same thing again focuses it instead of doubling it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionDesk(Desk);

impl SessionDesk {
    pub fn new(desk: Desk) -> Self {
        Self(desk)
    }

    pub fn desk(&self) -> &Desk {
        &self.0
    }

    pub fn into_desk(self) -> Desk {
        self.0
    }

    /// The id of a tab showing `tab`; refused when it names nothing a tab can show.
    pub fn key(tab: &DeskTab) -> Result<String, String> {
        Ok(match tab {
            DeskTab::App { site, project } => {
                SiteUrl::new(site, "").map_err(|_| format!("`{site}` isn't a site name"))?;
                match project {
                    Some(project) => format!("app:{site}@{}", ProjectName::new(project)?),
                    None => format!("app:{site}"),
                }
            }
            DeskTab::Page { url } => {
                let url = url.trim();
                let path = url.strip_prefix("site://").unwrap_or(url);
                let (site, rest) = path.split_once('/').unwrap_or((path, ""));
                let page = SiteUrl::new(site, rest).map_err(|_| format!("`{url}` isn't a page"))?;
                format!("page:{page}")
            }
            DeskTab::Terminal { id } => {
                let plain = !id.is_empty()
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
                if !plain {
                    return Err(format!("`{id}` isn't a terminal id"));
                }
                format!("terminal:{id}")
            }
            DeskTab::Changes { project } => format!("changes:{}", ProjectName::new(project)?),
        })
    }

    /// Opens `tab` (or finds it open) and, with `focus`, shows it. One the agent opens without
    /// focusing is `unseen` until the user shows it. Its id.
    pub fn open(&mut self, tab: DeskTab, focus: bool, by: DeskBy) -> Result<String, String> {
        let id = Self::key(&tab)?;
        if !self.0.tabs.iter().any(|t| t.id == id) {
            self.0.tabs.push(DeskEntry {
                id: id.clone(),
                tab,
                unseen: by == DeskBy::Agent && !focus,
            });
        }
        if focus {
            self.focus(&id)?;
        }
        Ok(id)
    }

    /// Shows `id`; it is seen now.
    pub fn focus(&mut self, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?;
        entry.unseen = false;
        self.0.active = Some(id.to_string());
        Ok(())
    }

    /// Closes `id`; when it was shown, its neighbour is (the next one, else the one before).
    pub fn close(&mut self, id: &str) -> Result<(), String> {
        let at = self.index(id)?;
        self.0.tabs.remove(at);
        if self.0.active.as_deref() == Some(id) {
            let next = self.0.tabs.get(at).or_else(|| self.0.tabs.last());
            self.0.active = next.map(|t| t.id.clone());
        }
        Ok(())
    }

    /// Moves `id` to position `to` (past the end: last).
    pub fn move_to(&mut self, id: &str, to: usize) -> Result<(), String> {
        let at = self.index(id)?;
        let entry = self.0.tabs.remove(at);
        let to = to.min(self.0.tabs.len());
        self.0.tabs.insert(to, entry);
        Ok(())
    }

    fn index(&self, id: &str) -> Result<usize, String> {
        self.0
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| format!("no tab `{id}` on this desk (`desk.get` lists them)"))
    }

    fn entry(&mut self, id: &str) -> Result<&mut DeskEntry, String> {
        let at = self.index(id)?;
        Ok(&mut self.0.tabs[at])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(site: &str, project: Option<&str>) -> DeskTab {
        DeskTab::App {
            site: site.into(),
            project: project.map(str::to_string),
        }
    }

    fn ids(desk: &SessionDesk) -> Vec<String> {
        desk.desk().tabs.iter().map(|t| t.id.clone()).collect()
    }

    #[test]
    fn open_focus_close_move() {
        let mut desk = SessionDesk::default();
        let cad = desk
            .open(app("cad", Some("game")), true, DeskBy::User)
            .unwrap();
        assert_eq!(cad, "app:cad@game");
        let page = desk
            .open(
                DeskTab::Page {
                    url: "site://plans/x".into(),
                },
                true,
                DeskBy::User,
            )
            .unwrap();
        assert_eq!(page, "page:site://plans/x");
        // The same app again: focused, not doubled.
        desk.open(app("cad", Some("game")), true, DeskBy::User)
            .unwrap();
        assert_eq!(ids(&desk), [cad.clone(), page.clone()]);
        assert_eq!(desk.desk().active.as_deref(), Some(cad.as_str()));
        // The agent's unfocused open is unseen until shown.
        let term = desk
            .open(DeskTab::Terminal { id: "t1".into() }, false, DeskBy::Agent)
            .unwrap();
        assert!(desk.desk().tabs[2].unseen);
        desk.focus(&term).unwrap();
        assert!(!desk.desk().tabs[2].unseen);
        desk.move_to(&term, 0).unwrap();
        assert_eq!(ids(&desk), [term.clone(), cad.clone(), page.clone()]);
        // Closing the shown tab shows its neighbour.
        desk.close(&term).unwrap();
        assert_eq!(desk.desk().active.as_deref(), Some(cad.as_str()));
        desk.close(&page).unwrap();
        desk.close(&cad).unwrap();
        assert_eq!(desk.desk(), &Desk::default());
        assert!(desk.close("app:nope").unwrap_err().contains("no tab"));
    }

    #[test]
    fn tabs_name_real_things() {
        for bad in [
            app("../x", None),
            app("cad", Some("a/b")),
            DeskTab::Terminal { id: "../x".into() },
            DeskTab::Changes { project: "".into() },
        ] {
            assert!(SessionDesk::key(&bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            SessionDesk::key(&app("calendar", None)).unwrap(),
            "app:calendar"
        );
    }
}
