//! A site's `mcp.toml`, read: the servers it declares and what did not read.
//!
//! The parse is pure — the file's bytes and the path they came from in, the servers and the
//! errors out — so the format is testable without a server, and the caller decides what a broken
//! file costs. Setting the site decides that it costs nothing but a note.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::site_mcp::{McpTarget, Parsed, SiteMcpServer};

/// The largest `mcp.toml` read at all: the whole file is what the user is asked to approve, so
/// one that cannot be shown whole is not something they can approve.
pub const MAX_BYTES: usize = 64 * 1024;

/// The one table an `mcp.toml` has: `[servers.<name>]`, one per server.
const SERVERS_TABLE: &str = "servers";

/// Reads `bytes`, which came from `path` (only its file name is quoted back).
///
/// A file that does not parse is one error naming the line it broke on. An entry that does not
/// read is one error naming the server and its line, and the other entries are still returned: a
/// typo in a file an agent can write must not cost a site the servers it did declare.
pub fn parse(bytes: &[u8], path: &Path) -> Parsed {
    let file = file_name(path);
    if bytes.len() > MAX_BYTES {
        let why = format!(
            "{file} is {size} KiB, more than the {limit} KiB that is read, so it cannot be shown \
             whole to the one who approves it: nothing in it is used",
            size = bytes.len().div_ceil(1024),
            limit = MAX_BYTES / 1024,
        );
        return Parsed::nothing_read(vec![why]);
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Parsed::nothing_read(vec![format!("{file} is not text")]);
    };
    let doc = match toml::from_str::<toml::Value>(text) {
        Ok(toml::Value::Table(doc)) => doc,
        Ok(_) => return Parsed::nothing_read(vec![format!("{file} is not a table of servers")]),
        Err(e) => {
            return Parsed::nothing_read(vec![broke(
                &file,
                text,
                e.span(),
                &one_said(&e.to_string()),
            )]);
        }
    };
    let mut errors: Vec<String> = doc
        .keys()
        .filter(|key| key.as_str() != SERVERS_TABLE)
        .map(|key| {
            format!("{file}: there is no [{key}] in an mcp.toml (only [servers.<name>] tables)")
        })
        .collect();
    let servers = match doc.get(SERVERS_TABLE) {
        None => return Parsed::nothing_read(errors),
        Some(toml::Value::Table(servers)) => servers.clone(),
        Some(other) => {
            errors.push(format!(
                "{file}: `[servers]` holds [servers.<name>] tables, not {}",
                kind_of(other)
            ));
            return Parsed::nothing_read(errors);
        }
    };
    let mut parsed = Parsed {
        servers: Vec::new(),
        errors,
    };
    let at = declared_at(text);
    for (name, entry) in servers {
        match read_entry(&at, &file, &name, &entry) {
            Ok(server) => parsed.servers.push(server),
            Err(error) => parsed.errors.push(error),
        }
    }
    parsed
}

/// One `[servers.<name>]` table as the file writes it, before it is checked. Every field is
/// optional here, so that a table with too little or too much in it is an error naming the
/// server rather than a file that does not read at all.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct Declared {
    bundle: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    url: Option<String>,
    headers: BTreeMap<String, String>,
    allow: Vec<String>,
    read_only: Vec<String>,
}

fn read_entry(
    at: &Positions,
    file: &str,
    name: &str,
    value: &toml::Value,
) -> Result<SiteMcpServer, String> {
    let line = line_of(at, name);
    if !valid_name(name) {
        return Err(about(
            file,
            name,
            line,
            "is not a name: letters, digits, `-` and `_`, no `__`",
        ));
    }
    let declared = Declared::deserialize(value.clone())
        .map_err(|e| about(file, name, line, &field_said(&e)))?;
    let target = target(file, name, line, &declared)?;
    Ok(SiteMcpServer {
        name: name.to_string(),
        target,
        allow: declared.allow,
        read_only: declared.read_only,
    })
}

/// A server is exactly one of a bundle, a command or a url; `args`/`env` go with a command and
/// `headers` with a url.
fn target(file: &str, name: &str, line: usize, declared: &Declared) -> Result<McpTarget, String> {
    let bundle = word(&declared.bundle);
    let command = word(&declared.command);
    let url = word(&declared.url);
    let named = [&bundle, &command, &url]
        .iter()
        .filter(|v| v.is_some())
        .count();
    let refuse = |why: &str| Err(about(file, name, line, why));
    if named == 0 {
        return refuse("names no server: give it a `bundle`, a `command` or a `url`");
    }
    if named > 1 {
        return refuse(
            "names more than one server: a `bundle`, a `command` or a `url`, one of them",
        );
    }
    let with_command = !declared.args.is_empty() || !declared.env.is_empty();
    let with_url = !declared.headers.is_empty();
    if let Some(bundle) = bundle {
        if with_command || with_url {
            return refuse(
                "names a bundle, which brings its own command: no `args`, `env` or `headers`",
            );
        }
        return Ok(McpTarget::Bundle(bundle));
    }
    if let Some(command) = command {
        if with_url {
            return refuse("runs a command: `headers` go with a `url`");
        }
        return Ok(McpTarget::Stdio {
            command,
            args: declared.args.clone(),
            env: declared.env.clone(),
        });
    }
    let url = url.expect("one of the three is named");
    if with_command {
        return refuse("names a url: `args` and `env` go with a `command`");
    }
    if !url.contains("://") {
        return refuse(&format!(
            "has a url with no scheme ({url}); write it as http://127.0.0.1:5174/mcp"
        ));
    }
    Ok(McpTarget::Http {
        url,
        headers: declared.headers.clone(),
    })
}

/// What serde said about a table, said about the server instead.
fn field_said(error: &toml::de::Error) -> String {
    let said = error.to_string();
    match said.strip_prefix("unknown field ") {
        Some(rest) => match rest.split('`').nth(1).filter(|n| !n.is_empty()) {
            Some(named) => format!("has a field `{named}` that a server does not have"),
            None => "has a field a server does not have".to_string(),
        },
        None => format!("is not something I can read: {}", one_said(&said)),
    }
}

fn word(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A server name is the first half of its tools' names: the rule `[mcp.<name>]` follows too.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("__")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn kind_of(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "a string",
        toml::Value::Integer(_) | toml::Value::Float(_) => "a number",
        toml::Value::Boolean(_) => "a yes or no",
        toml::Value::Datetime(_) => "a date",
        toml::Value::Array(_) => "an array",
        toml::Value::Table(_) => "a table",
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Where each server is declared, found in one pass over the text: its name to the line its key
/// is on. `toml::Value` keeps no positions, so the keys are read as text: a `[servers.x]` header,
/// an inline entry under `[servers]`, a dotted key, a quoted `[servers."q x"]`.
fn declared_at(text: &str) -> Positions {
    let mut at = Positions::default();
    let mut table: Vec<String> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, is_header) = match line.strip_prefix('[') {
            Some(rest) => (rest.split(']').next().unwrap_or_default(), true),
            None => (line.split('=').next().unwrap_or_default(), false),
        };
        let Some(path) = key_path(key.trim()) else {
            continue;
        };
        let path = if is_header {
            table = path.clone();
            path
        } else {
            table.iter().chain(path.iter()).cloned().collect()
        };
        if path.len() >= 2 && path[0] == SERVERS_TABLE {
            at.entry(path[1].clone()).or_insert(index + 1);
        }
    }
    at
}

/// Where server keys are written: the name to the 1-counted line it starts on.
type Positions = BTreeMap<String, usize>;

/// Where a server is named, or 0 when no key says it.
fn line_of(at: &Positions, name: &str) -> usize {
    at.get(name).copied().unwrap_or_default()
}

/// A key path as it is written — `servers."q x".command` — as its parts, or `None` when the text
/// is not a key.
fn key_path(text: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut at = 0;
    loop {
        at = skip_space(&chars, at);
        let mut part = String::new();
        match chars.get(at) {
            Some(quote @ ('"' | '\'')) => {
                at += 1;
                loop {
                    match chars.get(at)? {
                        c if *c == *quote => {
                            at += 1;
                            break;
                        }
                        '\\' => {
                            at += 1;
                            part.push(*chars.get(at)?);
                            at += 1;
                        }
                        c => {
                            part.push(*c);
                            at += 1;
                        }
                    }
                }
            }
            Some(c) if !matches!(c, '.' | '=' | '[' | ']' | '#') => {
                while chars.get(at).is_some_and(|c| *c != '.') {
                    part.push(*chars.get(at)?);
                    at += 1;
                }
            }
            _ => return None,
        }
        if part.trim().is_empty() {
            return None;
        }
        parts.push(part.trim().to_string());
        at = skip_space(&chars, at);
        match chars.get(at) {
            None => return Some(parts),
            Some('.') => at += 1,
            _ => return None,
        }
    }
}

fn skip_space(chars: &[char], mut at: usize) -> usize {
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    at
}

/// `<file> line <n>: server `<name>` <why>`, leaving out the line when it is not known.
fn about(file: &str, name: &str, line: usize, why: &str) -> String {
    let line = match line {
        0 => String::new(),
        line => format!("line {line}: "),
    };
    format!("{file} {line}server `{}` {why}", name.escape_debug())
}

/// One sentence: toml answers a bad file with a header and a code frame around the line; what is
/// kept is the reason, since the line is ours to give.
fn one_said(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_frame(line) && !line.starts_with("TOML parse error"))
        .collect();
    match kept.split_first() {
        Some((reason, [])) => (*reason).to_string(),
        Some((reason, where_)) => format!("{reason} ({})", where_.join(" ")),
        None => text.trim().to_string(),
    }
}

fn is_frame(line: &str) -> bool {
    line.starts_with(['|', '^']) || line.contains(" | ")
}

/// The same for a file that does not parse at all: toml gives a byte range, the note a line.
fn broke(file: &str, text: &str, span: Option<std::ops::Range<usize>>, why: &str) -> String {
    let line = span.map_or(0, |span| 1 + text[..span.start].matches('\n').count());
    let line = match line {
        0 => String::new(),
        line => format!("line {line}: "),
    };
    format!("{file} {line}{why}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Parsed {
        parse(text.as_bytes(), Path::new("/sites/lab/mcp.toml"))
    }

    const ALL_THREE: &str = r#"
[servers.blender]
bundle = "blender"
allow = ["get_*"]
read_only = ["get_*"]

[servers.github]
command = "github-mcp-server"
args = ["stdio"]
env = { TOKEN = "${GITHUB_TOKEN}" }

[servers.docs]
url = "https://example.com/mcp"
headers = { Authorization = "Bearer x" }
"#;

    #[test]
    fn a_server_is_a_bundle_a_command_or_a_url() {
        let parsed = read(ALL_THREE);
        assert_eq!(parsed.errors, Vec::<String>::new());
        let by_name = |name: &str| parsed.servers.iter().find(|s| s.name == name).unwrap();
        assert_eq!(
            by_name("blender"),
            &SiteMcpServer {
                name: "blender".into(),
                target: McpTarget::Bundle("blender".into()),
                allow: vec!["get_*".into()],
                read_only: vec!["get_*".into()],
            }
        );
        assert_eq!(
            by_name("github").target,
            McpTarget::Stdio {
                command: "github-mcp-server".into(),
                args: vec!["stdio".into()],
                env: [("TOKEN".to_string(), "${GITHUB_TOKEN}".to_string())].into(),
            }
        );
        assert_eq!(
            by_name("docs").target,
            McpTarget::Http {
                url: "https://example.com/mcp".into(),
                headers: [("Authorization".to_string(), "Bearer x".to_string())].into(),
            }
        );
        assert!(by_name("github").read_only.is_empty(), "none by default");
    }

    /// Each entry that does not read is one error naming its server and line; the rest stand.
    #[test]
    fn each_bad_entry_is_its_own_error() {
        let cases = [
            ("[servers.a]\nallow = [\"*\"]\n", "names no server"),
            (
                "[servers.a]\nbundle = \"b\"\nurl = \"http://x/y\"\n",
                "more than one",
            ),
            (
                "[servers.a]\ncommand = \"c\"\nurl = \"http://x/y\"\n",
                "more than one",
            ),
            (
                "[servers.a]\nbundle = \"b\"\nargs = [\"x\"]\n",
                "names a bundle",
            ),
            (
                "[servers.a]\ncommand = \"c\"\nheaders = { A = \"b\" }\n",
                "`headers` go with a `url`",
            ),
            (
                "[servers.a]\nurl = \"http://x/y\"\nenv = { A = \"b\" }\n",
                "go with a `command`",
            ),
            ("[servers.a]\nurl = \"127.0.0.1:5\"\n", "no scheme"),
            ("[servers.a]\ncommand = \"c\"\nfoo = 1\n", "field `foo`"),
            ("[servers.a]\ncommand = 1\n", "in `command`"),
            ("[servers.\"a__b\"]\ncommand = \"c\"\n", "is not a name"),
            (
                "[servers.\"my server\"]\ncommand = \"c\"\n",
                "is not a name",
            ),
        ];
        for (text, why) in cases {
            let parsed = read(&format!("[servers.good]\ncommand = \"ok\"\n\n{text}"));
            assert_eq!(parsed.servers.len(), 1, "{text}: {:?}", parsed.servers);
            assert_eq!(parsed.errors.len(), 1, "{text}: {:?}", parsed.errors);
            let error = &parsed.errors[0];
            assert!(error.contains(why), "{text}: {error}");
            assert!(error.contains("line 4"), "{text}: {error}");
            assert!(error.starts_with("mcp.toml line 4: server `"), "{error}");
            assert!(!error.contains('\n'), "{error}");
        }
    }

    #[test]
    fn the_line_is_named_whatever_way_a_server_is_written() {
        for (text, line) in [
            ("[servers]\nb = { command = 1 }\n", "line 2"),
            ("servers.c.command = 1\n", "line 1"),
            ("# a comment\n[servers.\"q x\"]\n", "line 2"),
        ] {
            let parsed = read(text);
            assert_eq!(parsed.errors.len(), 1, "{text}: {:?}", parsed.errors);
            assert!(
                parsed.errors[0].contains(line),
                "{text}: {:?}",
                parsed.errors
            );
        }
    }

    #[test]
    fn a_file_that_does_not_parse_says_where_in_one_line() {
        let parsed = read("[servers.a]\ncommand = \"x\"\n[servers.a]\n");
        assert!(parsed.servers.is_empty());
        let error = &parsed.errors[0];
        assert!(error.starts_with("mcp.toml line 3"), "{error}");
        assert!(!error.contains('\n'), "{error}");
    }

    #[test]
    fn only_server_tables_are_understood() {
        let parsed = read("[apps.x]\ncommand = \"x\"\n[servers.good]\ncommand = \"x\"\n");
        assert_eq!(parsed.servers.len(), 1);
        assert!(parsed.errors[0].contains("[apps]"), "{:?}", parsed.errors);
        let parsed = read("servers = 1\n");
        assert!(
            parsed.errors[0].contains("not a number"),
            "{:?}",
            parsed.errors
        );
    }

    #[test]
    fn an_empty_file_declares_nothing_and_says_nothing() {
        for text in ["", "# only a comment\n", "[servers]\n"] {
            let parsed = read(text);
            assert_eq!(parsed, Parsed::default(), "{text:?}");
        }
    }

    #[test]
    fn a_file_too_big_to_show_is_refused() {
        let text = format!(
            "{}[servers.big]\ncommand = \"x\"\n",
            "# padding\n".repeat(8000)
        );
        let parsed = read(&text);
        assert!(parsed.servers.is_empty());
        assert!(
            parsed.errors[0].contains("cannot be shown whole"),
            "{:?}",
            parsed.errors
        );
        let parsed = parse(&[0xff, 0xfe], Path::new("mcp.toml"));
        assert_eq!(parsed.errors, ["mcp.toml is not text"]);
    }

    #[test]
    fn what_the_user_reads_names_each_server_and_what_it_would_do() {
        let parsed = read(ALL_THREE);
        let said: Vec<String> = parsed.servers.iter().map(|s| s.describes()).collect();
        assert_eq!(
            said,
            [
                "blender: the installed bundle blender; tools get_* run without asking; tools \
                 get_* are offered to workflow runs",
                "docs: the MCP server at https://example.com/mcp with the headers \
                 Authorization: \"Bearer x\"",
                "github: runs `github-mcp-server stdio` in the environment \
                 TOKEN=\"${GITHUB_TOKEN}\"",
            ]
        );
    }
}
