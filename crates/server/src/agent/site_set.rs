use crate::agent::Entered;

/// What setting a session's site did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteSet {
    Entered(Entered),
    /// Left this site.
    Left(String),
    /// Asked to leave, but it was in none.
    NotInSite,
}

impl SiteSet {
    /// What happened, in a sentence for the user or the model.
    pub fn text(&self) -> String {
        match self {
            Self::Entered(entered) => {
                let mut text = format!("Entered {} ({}).", entered.site, entered.title);
                let notes = entered.notes();
                if !notes.is_empty() {
                    text.push(' ');
                    text.push_str(&notes);
                }
                text
            }
            Self::Left(site) => format!("Left site {site}."),
            Self::NotInSite => "The session was not in a site.".into(),
        }
    }
}
