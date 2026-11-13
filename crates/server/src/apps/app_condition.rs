use std::fmt;

use crate::apps::{FileGlob, ProjectFacts};

/// One entry of an app's `when`: the app attaches to a project that is a git repository, lists a
/// site, holds a matching file, or carries a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCondition {
    Git,
    Site(String),
    Files(FileGlob),
    Tag(String),
}

impl AppCondition {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let wrong = || {
            format!(
                "`when` entries are `git`, `site:<site>`, `files:<glob>` or `tag:<tag>`; \
                 got `{text}`"
            )
        };
        if text == "git" {
            return Ok(Self::Git);
        }
        let (kind, value) = text.split_once(':').ok_or_else(wrong)?;
        let value = value.trim();
        if value.is_empty() {
            return Err(wrong());
        }
        match kind.trim() {
            "site" => Ok(Self::Site(value.to_string())),
            "files" => FileGlob::new(value).map(Self::Files),
            "tag" => Ok(Self::Tag(value.to_string())),
            _ => Err(wrong()),
        }
    }

    pub fn matches(&self, facts: &ProjectFacts) -> bool {
        match self {
            Self::Git => facts.is_git(),
            Self::Site(site) => facts.has_site(site),
            Self::Files(glob) => facts.has_file(glob),
            Self::Tag(tag) => facts.has_tag(tag),
        }
    }
}

impl fmt::Display for AppCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git => f.write_str("git"),
            Self::Site(site) => write!(f, "site:{site}"),
            Self::Files(glob) => write!(f, "files:{glob}"),
            Self::Tag(tag) => write!(f, "tag:{tag}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn each_kind_parses_and_matches() {
        let facts = ProjectFacts::new(
            true,
            ["cad".to_string()],
            ["game".to_string()],
            vec![PathBuf::from("parts/arm.FCStd")],
        );
        for (text, hit) in [
            ("git", true),
            ("site:cad", true),
            ("site:video", false),
            ("files:*.FCStd", true),
            ("files:*.blend", false),
            ("tag:game", true),
            ("tag: web", false),
        ] {
            let condition = AppCondition::parse(text).unwrap();
            assert_eq!(condition.matches(&facts), hit, "{text}");
        }
        assert_eq!(
            AppCondition::parse(" files:*.FCStd ").unwrap().to_string(),
            "files:*.FCStd"
        );
        for bad in ["svn", "site:", "pipeline:cad", "files:/abs", "colour:red"] {
            assert!(AppCondition::parse(bad).is_err(), "{bad}");
        }
    }
}
