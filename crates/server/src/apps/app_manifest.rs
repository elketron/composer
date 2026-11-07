use serde_json::{Map, Value};

use crate::apps::{AppCondition, AppRail, AppScope, FileGlob};

/// The keys an `app:` block may have.
const KEYS: [&str; 7] = [
    "scope", "when", "rail", "tile", "opens", "on_open", "client",
];

/// A site's `app:` block in its `index.md` (decision 2026-11-07): where the app shows, which
/// projects it attaches to (`when`, any one), its rail entry, whether it has a Home tile, the file
/// types it opens, and the recipes of its own justfile run when a project opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppManifest {
    pub scope: AppScope,
    pub when: Vec<AppCondition>,
    pub rail: AppRail,
    pub tile: bool,
    pub opens: Vec<FileGlob>,
    pub on_open: Vec<String>,
    /// It asks to be a full client of the workspace (the chat app: sessions, approvals). Only the
    /// user's `[apps] clients` makes it one (ticket 0170); until then it is listed as asking.
    pub client: bool,
}

impl AppManifest {
    /// A site with a `ui/` and no `app:` block: a global app, as every app was before manifests.
    pub fn plain() -> Self {
        Self {
            scope: AppScope::Global,
            when: Vec::new(),
            rail: AppRail {
                icon: None,
                label: None,
            },
            tile: false,
            opens: Vec::new(),
            on_open: Vec::new(),
            client: false,
        }
    }

    /// The site's manifest from its index's frontmatter: `None` when it isn't an app (no `app:`
    /// block and no `ui/`). A block that doesn't parse says what is wrong and what to write.
    pub fn of_site(app: Option<&Value>, has_ui: bool) -> Result<Option<Self>, String> {
        match app {
            None | Some(Value::Null) => Ok(has_ui.then(Self::plain)),
            Some(Value::Object(block)) => Self::parse(block).map(Some),
            Some(_) => Err(
                "`app:` is a block of keys (scope, when, rail, tile, opens, on_open)".to_string(),
            ),
        }
    }

    fn parse(block: &Map<String, Value>) -> Result<Self, String> {
        if let Some(unknown) = block.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!(
                "`app:` has no `{unknown}`; its keys are {}",
                KEYS.join(", ")
            ));
        }
        let scope = match block.get("scope") {
            None => AppScope::Global,
            Some(Value::String(s)) => AppScope::parse(s)?,
            Some(_) => return Err("`scope` is `project`, `global` or `both`".into()),
        };
        let when = strings(block, "when")?
            .iter()
            .map(|c| AppCondition::parse(c))
            .collect::<Result<Vec<_>, _>>()?;
        if scope == AppScope::Global && !when.is_empty() {
            return Err(
                "a `global` app doesn't attach to projects: drop `when`, or use `scope: project` \
                 (or `both`)"
                    .into(),
            );
        }
        let opens = strings(block, "opens")?
            .iter()
            .map(|g| FileGlob::new(g))
            .collect::<Result<Vec<_>, _>>()?;
        let on_open = strings(block, "on_open")?;
        if let Some(bad) = on_open.iter().find(|r| !is_recipe(r)) {
            return Err(format!(
                "`on_open` lists recipes of the site's justfile; `{bad}` isn't a recipe name"
            ));
        }
        let flag = |key: &str| match block.get(key) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("`{key}` is `true` or `false`")),
        };
        let (tile, client) = (flag("tile")?, flag("client")?);
        Ok(Self {
            scope,
            when,
            rail: rail(block.get("rail"))?,
            tile,
            opens,
            on_open,
            client,
        })
    }
}

/// `key`'s list of strings (one string counts as a list of one); missing: empty.
fn strings(block: &Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    match block.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(s)) => Ok(vec![s.clone()]),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("`{key}` is a list of strings"))
            })
            .collect(),
        Some(_) => Err(format!("`{key}` is a list of strings")),
    }
}

fn rail(value: Option<&Value>) -> Result<AppRail, String> {
    let wrong = || "`rail` is `{ icon: <name>, label: <text> }`".to_string();
    let block = match value {
        None | Some(Value::Null) => return Ok(AppManifest::plain().rail),
        Some(Value::Object(block)) => block,
        Some(_) => return Err(wrong()),
    };
    let text = |key: &str| match block.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(wrong()),
    };
    if block.keys().any(|k| k != "icon" && k != "label") {
        return Err(wrong());
    }
    Ok(AppRail {
        icon: text("icon")?,
        label: text("label")?,
    })
}

fn is_recipe(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<Option<AppManifest>, String> {
        let frontmatter = composer_core::Frontmatter::parse(yaml).unwrap();
        AppManifest::of_site(frontmatter.get("app"), false)
    }

    #[test]
    fn a_full_block() {
        let m = parse(
            "app:\n  scope: project\n  when: [\"pipeline:cad\", \"files:*.FCStd\"]\n  rail: { icon: cube, label: CAD }\n  tile: true\n  opens: [\"*.FCStd\", \"*.step\"]\n  on_open: [refresh]\n",
        )
        .unwrap()
        .unwrap();
        assert_eq!(m.scope, AppScope::Project);
        assert_eq!(m.when.len(), 2);
        assert_eq!(m.rail.icon.as_deref(), Some("cube"));
        assert_eq!(m.rail.label.as_deref(), Some("CAD"));
        assert!(m.tile);
        assert_eq!(m.opens.len(), 2);
        assert_eq!(m.on_open, ["refresh"]);
    }

    #[test]
    fn defaults_and_plain_apps() {
        let m = parse("app:\n  tile: true\n").unwrap().unwrap();
        assert_eq!((m.scope, m.when.len(), m.tile), (AppScope::Global, 0, true));
        assert_eq!(parse("title: x\n").unwrap(), None, "no app, no ui");
        assert_eq!(
            AppManifest::of_site(None, true).unwrap(),
            Some(AppManifest::plain()),
            "a ui without a block is a global app"
        );
        let one = parse("app:\n  scope: both\n  when: git\n")
            .unwrap()
            .unwrap();
        assert_eq!(one.when, [AppCondition::Git]);
    }

    #[test]
    fn errors_say_what_to_write() {
        for (yaml, says) in [
            ("app: yes\n", "block of keys"),
            ("app:\n  colour: red\n", "has no `colour`"),
            (
                "app:\n  scope: everywhere\n",
                "`project`, `global` or `both`",
            ),
            ("app:\n  when: [git]\n", "drop `when`"),
            (
                "app:\n  scope: project\n  when: [svn]\n",
                "`when` entries are",
            ),
            ("app:\n  opens: [/etc/passwd]\n", "isn't a file pattern"),
            ("app:\n  on_open: [\"rm -rf\"]\n", "isn't a recipe name"),
            ("app:\n  tile: maybe\n", "`tile` is"),
            ("app:\n  rail: cube\n", "`rail` is"),
            ("app:\n  rail: { colour: red }\n", "`rail` is"),
            ("app:\n  client: sure\n", "`client` is"),
        ] {
            let error = parse(yaml).unwrap_err();
            assert!(error.contains(says), "{yaml}: {error}");
        }
    }
}
