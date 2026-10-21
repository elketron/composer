use composer_core::Frontmatter;
use composer_protocol::http::StageSummary;

use crate::pipeline::types::{InputKind, StageInput};

/// A stage's contract (`stages/02_script.md`, `type: stage`): what it reads, does and writes,
/// for every work of the pipeline.
#[derive(Debug, Clone)]
pub struct StageContract {
    /// `02_script`: the file's name, and the stage's folder in each work.
    pub name: String,
    pub title: String,
    pub gate: bool,
    pub harness: Option<String>,
    pub model: Option<String>,
    /// The agent that runs it (default `stage`).
    pub agent: Option<String>,
    pub worktree: bool,
    pub inputs: Vec<StageInput>,
    /// The Outputs section's paths (in backticks).
    pub outputs: Vec<String>,
    /// The Verify section's checks (its list items, or its paragraphs): what the worker checks
    /// against earlier stages before `finish`.
    pub verify: Vec<String>,
}

impl StageContract {
    /// `name` is the file name without `.md`; `raw` the whole page.
    pub fn parse(name: &str, raw: &str) -> Self {
        let (yaml, body) = Frontmatter::split(raw);
        let fm = yaml
            .and_then(|y| Frontmatter::parse(y).ok())
            .unwrap_or_default();
        let flag = |key: &str| fm.get_str(key).is_some_and(|v| v == "true");
        let text = |key: &str| {
            fm.get_str(key)
                .filter(|v| !v.trim().is_empty() && v != "default")
        };
        let title = fm.title().unwrap_or_else(|| {
            name.split_once('_')
                .map_or(name, |(_, rest)| rest)
                .replace(['-', '_'], " ")
        });
        Self {
            name: name.to_string(),
            title,
            gate: flag("gate"),
            harness: text("harness"),
            model: text("model"),
            agent: text("agent"),
            worktree: flag("worktree"),
            inputs: inputs(section(body, "inputs")),
            verify: checks(section(body, "verify")),
            outputs: section(body, "outputs")
                .lines()
                .filter_map(|l| l.trim_start().strip_prefix("- "))
                .filter_map(backticked)
                .collect(),
        }
    }

    /// `01`: the number in front of the name.
    pub fn number(&self) -> &str {
        self.name.split_once('_').map_or("", |(n, _)| n)
    }

    pub fn summary(&self) -> StageSummary {
        StageSummary {
            name: self.name.clone(),
            title: self.title.clone(),
            gate: self.gate,
            harness: self.harness.clone(),
            model: self.model.clone(),
            worktree: self.worktree,
        }
    }
}

/// The text under `## <heading>` (case-insensitive), up to the next heading.
fn section<'a>(body: &'a str, heading: &str) -> &'a str {
    let mut start = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end();
        if let Some(h) = trimmed.strip_prefix("## ") {
            if start.is_some() {
                return &body[start.unwrap_or(0)..offset];
            }
            if h.trim().eq_ignore_ascii_case(heading) {
                start = Some(offset + line.len());
            }
        } else if trimmed.starts_with("# ") && start.is_some() {
            return &body[start.unwrap_or(0)..offset];
        }
        offset += line.len();
    }
    start.map_or("", |s| &body[s..])
}

/// The rows of the Inputs table whose first cell is `working` or `reference`.
fn inputs(section: &str) -> Vec<StageInput> {
    section
        .lines()
        .filter(|l| l.trim_start().starts_with('|'))
        .filter_map(|l| {
            let cells: Vec<&str> = l
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            let kind = match cells.first()?.to_lowercase().as_str() {
                "working" => InputKind::Working,
                "reference" => InputKind::Reference,
                _ => return None,
            };
            let path = cells
                .get(1)
                .map(|c| backticked(c).unwrap_or_else(|| c.to_string()))?;
            Some(StageInput {
                kind,
                path,
                about: cells.get(2).map(|c| c.to_string()).unwrap_or_default(),
            })
        })
        .collect()
}

/// A section's list items, else its paragraphs, each on one line.
fn checks(section: &str) -> Vec<String> {
    let items: Vec<String> = section
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("- "))
        .map(|l| l.trim().to_string())
        .collect();
    if !items.is_empty() {
        return items;
    }
    section
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect()
}

/// The first `code span` in `text`.
fn backticked(text: &str) -> Option<String> {
    let (_, rest) = text.split_once('`')?;
    let (inside, _) = rest.split_once('`')?;
    Some(inside.to_string()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_contract() {
        let raw = "---\ntitle: Script\ntype: stage\ngate: true\nharness: composer\nmodel: default\n---\n\
                   # 02 · Script\n\n## Inputs\n\n| | file | for |\n|---|---|---|\n\
                   | working | `01_research/research.md` | what to say |\n\
                   | reference | `_config/voice.md` | how it sounds |\n\n\
                   ## Process\n\nWrite it.\n\n## Outputs\n\n- `02_script/script.md`\n- notes, no path\n";
        let c = StageContract::parse("02_script", raw);
        assert_eq!((c.title.as_str(), c.number()), ("Script", "02"));
        assert!(c.gate && !c.worktree);
        assert_eq!(c.harness.as_deref(), Some("composer"));
        assert_eq!(c.model, None, "default is no model");
        assert_eq!(c.inputs.len(), 2);
        assert_eq!(c.inputs[0].kind, InputKind::Working);
        assert_eq!(c.inputs[1].path, "_config/voice.md");
        assert_eq!(c.inputs[1].about, "how it sounds");
        assert_eq!(c.outputs, vec!["02_script/script.md".to_string()]);
        assert!(c.verify.is_empty());
        let c = StageContract::parse(
            "02_script",
            "# x\n\n## Verify\n\nEvery claim is in\nresearch.md.\n\nIt fits the length.\n",
        );
        assert_eq!(
            c.verify,
            ["Every claim is in research.md.", "It fits the length."]
        );
    }

    #[test]
    fn falls_back_to_the_name() {
        let c = StageContract::parse("04_write-up", "# Write-up\n");
        assert_eq!(c.title, "write up");
        assert!(!c.gate && c.inputs.is_empty());
    }
}
