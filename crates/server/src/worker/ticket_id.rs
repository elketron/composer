use std::fmt;

/// A ticket's id: `0003` (numbers normalized to four digits), or a plain name of letters, digits,
/// `-` and `_` that starts with a letter or digit, at most 64 characters.
///
/// Every path, branch and URL made from a ticket (`state/worktrees/<id>`, `ticket/<id>`,
/// `site://orchestration/tickets/<id>.md`) is made from this type, so an id from a client, an
/// agent or a page (`../../../b/state/worktrees/0002`, `--force`) can never name a folder of
/// another workspace or pass as a git option (ticket 0150).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TicketId(String);

impl TicketId {
    /// `3`, `0003`, `0003.md` or `site://orchestration/tickets/0003` → `0003`; refused unless what
    /// is left is a plain ticket name.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let id = raw
            .trim()
            .trim_start_matches("site://")
            .trim_start_matches("orchestration/")
            .trim_start_matches("tickets/")
            .trim_end_matches(".md");
        if let Ok(n) = id.parse::<u32>() {
            return Ok(Self(format!("{n:04}")));
        }
        let plain = id.len() <= 64
            && id.starts_with(|c: char| c.is_ascii_alphanumeric())
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !plain {
            return Err(format!(
                "`{raw}` is not a ticket id: give its number (`0003`) or the name of a page in \
                 site://orchestration/tickets/"
            ));
        }
        Ok(Self(id.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Its git branch, `ticket/<id>`.
    pub fn branch(&self) -> String {
        format!("ticket/{}", self.0)
    }
}

impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_normalize_and_names_stay_plain() {
        for (raw, id) in [
            ("3", "0003"),
            ("0012.md", "0012"),
            ("site://orchestration/tickets/0007", "0007"),
            (" tickets/fix-login_2 ", "fix-login_2"),
        ] {
            assert_eq!(TicketId::parse(raw).unwrap().as_str(), id);
        }
        for bad in [
            "",
            "..",
            "../../../b/state/worktrees/0002",
            "a/b",
            "-x",
            "--force",
            ".hidden",
            "a b",
            "x\\y",
            &"x".repeat(65),
        ] {
            assert!(TicketId::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(TicketId::parse("12").unwrap().branch(), "ticket/0012");
    }
}
