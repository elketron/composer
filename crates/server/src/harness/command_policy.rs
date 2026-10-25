//! Which shell commands of a Claude Code worker run without asking. A command is split into
//! its parts (`a; b && c | d`); it runs when every part is allowed: by a rule (the allowlist,
//! the project's, or an earlier `always`), as a read-only command whose paths stay inside the
//! worker's folders, or as a build or test in its folder. Anything the split can't see through
//! (`$(…)`, backticks, `$VAR`, subshells, heredocs, writing to a file, `&`) asks.

use std::path::{Component, Path, PathBuf};

use crate::harness::tool_rules::SUBCOMMAND_TOOLS;

/// What the policy needs to know about the run.
pub struct CommandContext<'a> {
    /// Where the command starts: the worktree, or the project folder.
    pub folder: &'a Path,
    /// Folders read-only commands may touch: the folder, the project, the read_dirs, /tmp.
    pub read_roots: &'a [PathBuf],
    /// Claude Code permission rules (`Bash(cargo test:*)`): the allowlist, the project's and
    /// what the user said `always` to.
    pub rules: &'a [String],
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Allowed,
    /// Ask the user. `rules`: what `always` would add (a rule per part that wasn't allowed);
    /// empty when some part has no safe rule.
    Ask { rules: Vec<String> },
}

/// A word after quote removal; `tilde`: it started with an unquoted `~`.
#[derive(Debug, Clone, PartialEq)]
struct Word {
    text: String,
    tilde: bool,
}

/// One simple command of a chain or pipeline.
#[derive(Debug, Default, PartialEq)]
struct Segment {
    words: Vec<Word>,
    /// Files it reads with `<`.
    inputs: Vec<Word>,
}

/// Read-only programs whose every argument is checked as a path (patterns and counts resolve
/// inside the current folder, which is fine; one that looks like an outside path asks).
const READ_ONLY: &[&str] = &[
    "cat", "ls", "head", "tail", "wc", "grep", "egrep", "fgrep", "rg", "tree", "file", "du",
    "df", "stat", "which", "pwd", "cut", "tr", "diff", "cmp", "jq", "realpath", "dirname",
    "basename", "readlink", "nl", "column", "true", "false", "sort", "uniq", "find", "sed",
];

/// Programs whose arguments are text, not paths.
const TEXT_ONLY: &[&str] = &["echo", "printf"];

/// git subcommands that only read.
const GIT_READ: &[&str] = &[
    "status", "log", "diff", "show", "blame", "rev-parse", "ls-files", "grep", "describe",
    "shortlog", "branch",
];

/// `find` predicates that run, delete or write.
const FIND_ACTIONS: &[&str] = &[
    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprint0", "-fprintf", "-fls",
];

/// Builds and tests, run in the worker's folder: `(program, subcommands)`, empty for any.
const BUILDS: &[(&str, &[&str])] = &[
    (
        "cargo",
        &[
            "build", "check", "test", "clippy", "run", "bench", "doc", "tree", "metadata", "fmt",
            "nextest",
        ],
    ),
    ("flutter", &["test", "analyze", "build"]),
    ("dart", &["test", "analyze", "run", "format"]),
    ("npm", &["test", "run"]),
    ("pnpm", &["test", "run"]),
    ("yarn", &["test", "run"]),
    ("bun", &["test", "run"]),
    ("go", &["build", "test", "vet", "run"]),
    ("dotnet", &["build", "test", "run"]),
    ("pytest", &[]),
    ("just", &[]),
    ("make", &[]),
];

/// Environment a prefix assignment must not set: it changes what runs.
const UNSAFE_ENV: &[&str] = &["PATH", "LD_PRELOAD", "LD_LIBRARY_PATH", "BASH_ENV", "ENV"];

pub fn check_command(command: &str, context: &CommandContext) -> Verdict {
    let Some(segments) = split(command) else {
        return Verdict::Ask { rules: vec![] };
    };
    let mut cwd = context.folder.to_path_buf();
    let mut rules = Vec::new();
    let mut every_part_has_a_rule = true;
    for segment in &segments {
        let Some(words) = strip_prefixes(&segment.words) else {
            every_part_has_a_rule = false;
            continue;
        };
        if words.is_empty() {
            continue;
        }
        if words[0].text == "cd" {
            cwd = match words.get(1) {
                Some(dir) => resolve(dir, &cwd),
                None => home(),
            };
            continue;
        }
        let inputs_inside = segment
            .inputs
            .iter()
            .all(|w| inside(&resolve(w, &cwd), context.read_roots));
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        let allowed = inputs_inside
            && (context.rules.iter().any(|r| rule_matches(r, &texts))
                || read_only(words, &cwd, context.read_roots)
                || build(words, &cwd, context.folder));
        if allowed {
            continue;
        }
        // A reader that asked went outside the folders: a rule for it would let it read
        // anything, so allow it once.
        let reader = READ_ONLY.contains(&texts[0])
            || (texts[0] == "git" && texts.get(1).is_some_and(|s| GIT_READ.contains(s)));
        match rule_for_words(&texts) {
            Some(rule) if inputs_inside && !reader => {
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
            _ => every_part_has_a_rule = false,
        }
    }
    if rules.is_empty() && every_part_has_a_rule {
        Verdict::Allowed
    } else if every_part_has_a_rule {
        Verdict::Ask { rules }
    } else {
        Verdict::Ask { rules: vec![] }
    }
}

/// The rule `always` adds for one plain command: `Bash(<program>:*)`, with the subcommand for
/// tools whose first word says little (`Bash(git push:*)`).
pub fn rule_for_words(words: &[&str]) -> Option<String> {
    let first = *words.first()?;
    let prefix = if SUBCOMMAND_TOOLS.contains(&first) {
        // `git -C x push`: no subcommand to pin, and `git:*` would allow all of git.
        let sub = words.get(1).filter(|s| !s.starts_with('-'))?;
        format!("{first} {sub}")
    } else {
        first.to_string()
    };
    Some(format!("Bash({prefix}:*)"))
}

/// Whether a Claude Code rule allows the command `words`: `Bash(prefix:*)`, `Bash(prefix *)`
/// or `Bash(exact command)`.
fn rule_matches(rule: &str, words: &[&str]) -> bool {
    let Some(pattern) = rule.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')) else {
        return false;
    };
    let (prefix, open) = match pattern
        .strip_suffix(":*")
        .or_else(|| pattern.strip_suffix(" *"))
    {
        Some(prefix) => (prefix, true),
        None => (pattern, false),
    };
    let prefix: Vec<&str> = prefix.split_whitespace().collect();
    if prefix.is_empty() {
        return false;
    }
    if open {
        words.starts_with(&prefix)
    } else {
        words == prefix.as_slice()
    }
}

/// Drops `VAR=value` assignments and `time` / `timeout N` / `nice` in front of the program;
/// `None` when an assignment changes what runs.
fn strip_prefixes(words: &[Word]) -> Option<&[Word]> {
    let mut rest = words;
    loop {
        let Some(first) = rest.first() else {
            return Some(rest);
        };
        let text = first.text.as_str();
        if let Some((name, _)) = text.split_once('=')
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            if UNSAFE_ENV.contains(&name) || name.starts_with("LD_") {
                return None;
            }
            rest = &rest[1..];
        } else if matches!(text, "time" | "nice") {
            rest = &rest[1..];
        } else if text == "/usr/bin/time" {
            rest = &rest[1..];
            while rest.first().is_some_and(|w| w.text.starts_with('-')) {
                rest = &rest[1..];
            }
        } else if text == "timeout" {
            rest = &rest[1..];
            while rest.first().is_some_and(|w| w.text.starts_with('-')) {
                rest = &rest[1..];
            }
            rest = rest.get(1..)?;
        } else {
            return Some(rest);
        }
    }
}

/// A read-only program with every path inside `roots`.
fn read_only(words: &[Word], cwd: &Path, roots: &[PathBuf]) -> bool {
    let program = words[0].text.as_str();
    let args = &words[1..];
    if TEXT_ONLY.contains(&program) {
        return true;
    }
    let paths_inside = |args: &[Word]| {
        args.iter().all(|arg| match arg.text.split_once('=') {
            // `--file=/x`: the value is the path.
            Some((flag, value)) if flag.starts_with('-') => {
                let value = Word {
                    text: value.to_string(),
                    tilde: value.starts_with('~'),
                };
                inside(&resolve(&value, cwd), roots)
            }
            _ if arg.text.starts_with('-') => true,
            _ => inside(&resolve(arg, cwd), roots),
        })
    };
    let flag = |names: &[&str]| {
        args.iter()
            .any(|a| names.iter().any(|n| a.text == *n || a.text.starts_with(&format!("{n}="))))
    };
    match program {
        "git" => {
            // `git -C <dir> <sub>`: the folder is a path too.
            let mut rest = args;
            let mut dirs = Vec::new();
            while let [c, dir, tail @ ..] = rest
                && c.text == "-C"
            {
                dirs.push(dir.clone());
                rest = tail;
            }
            let Some((sub, sub_args)) = rest.split_first() else {
                return false;
            };
            if !GIT_READ.contains(&sub.text.as_str()) || flag(&["--output", "-o"]) {
                return false;
            }
            // `git branch x` makes one; only listing flags.
            if sub.text == "branch" && sub_args.iter().any(|a| !a.text.starts_with('-')) {
                return false;
            }
            if sub.text == "branch"
                && flag(&["-d", "-D", "-m", "-M", "-c", "-C", "--delete", "--move", "--copy"])
            {
                return false;
            }
            paths_inside(&dirs) && paths_inside(sub_args)
        }
        _ if !READ_ONLY.contains(&program) => false,
        "find" => !flag(FIND_ACTIONS) && paths_inside(args),
        "rg" => !flag(&["--pre"]) && paths_inside(args),
        "sort" => !flag(&["-o", "--output"]) && paths_inside(args),
        // `uniq in out` writes `out`.
        "uniq" => args.iter().filter(|a| !a.text.starts_with('-')).count() <= 1 && paths_inside(args),
        "sed" => sed_reads_only(args) && {
            // The script isn't a path.
            let script_given = args.iter().any(|a| a.text == "-e" || a.text == "--expression");
            let mut skipped = script_given;
            let files: Vec<Word> = args
                .iter()
                .filter(|a| {
                    if a.text.starts_with('-') {
                        return false;
                    }
                    if !skipped {
                        skipped = true;
                        return false;
                    }
                    true
                })
                .cloned()
                .collect();
            paths_inside(&files)
        },
        _ => paths_inside(args),
    }
}

/// sed that only prints: no `-i`, and every script is addresses with `p`, `d`, `q`, `=` or
/// an `s///` without the `w` and `e` flags.
fn sed_reads_only(args: &[Word]) -> bool {
    if args
        .iter()
        .any(|a| a.text.starts_with("-i") || a.text.starts_with("--in-place") || a.text == "-s")
    {
        return false;
    }
    let mut scripts = Vec::new();
    let mut iter = args.iter();
    let mut explicit = false;
    while let Some(arg) = iter.next() {
        if arg.text == "-e" || arg.text == "--expression" {
            explicit = true;
            match iter.next() {
                Some(script) => scripts.push(script.text.as_str()),
                None => return false,
            }
        } else if arg.text == "-f" || arg.text.starts_with("--file") {
            return false;
        }
    }
    if !explicit {
        match args.iter().find(|a| !a.text.starts_with('-')) {
            Some(script) => scripts.push(script.text.as_str()),
            None => return false,
        }
    }
    scripts.iter().all(|s| sed_script_prints(s))
}

fn sed_script_prints(script: &str) -> bool {
    let chars: Vec<char> = script.chars().collect();
    let mut i = 0;
    let skip_regex = |i: &mut usize, delim: char| -> bool {
        while *i < chars.len() {
            match chars[*i] {
                '\\' => *i += 2,
                c if c == delim => {
                    *i += 1;
                    return true;
                }
                _ => *i += 1,
            }
        }
        false
    };
    let address = |i: &mut usize| -> bool {
        match chars.get(*i) {
            Some(c) if c.is_ascii_digit() => {
                while chars.get(*i).is_some_and(|c| c.is_ascii_digit()) {
                    *i += 1;
                }
                true
            }
            Some('$') => {
                *i += 1;
                true
            }
            Some('/') => {
                *i += 1;
                skip_regex(i, '/')
            }
            _ => true,
        }
    };
    while i < chars.len() {
        while chars.get(i).is_some_and(|c| c.is_whitespace() || *c == ';') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        if !address(&mut i) {
            return false;
        }
        if chars.get(i) == Some(&',') {
            i += 1;
            if !address(&mut i) {
                return false;
            }
        }
        if chars.get(i) == Some(&'!') {
            i += 1;
        }
        match chars.get(i) {
            Some('p' | 'd' | 'q' | '=') => i += 1,
            Some('s') => {
                let Some(&delim) = chars.get(i + 1) else {
                    return false;
                };
                if delim.is_alphanumeric() || delim == '\\' || delim == '\n' {
                    return false;
                }
                i += 2;
                if !skip_regex(&mut i, delim) || !skip_regex(&mut i, delim) {
                    return false;
                }
                while chars.get(i).is_some_and(|c| matches!(c, 'g' | 'p' | 'i' | 'I' | 'm' | 'M') || c.is_ascii_digit()) {
                    i += 1;
                }
            }
            _ => return false,
        }
    }
    true
}

/// A build or test, run inside the worker's folder; or a program built there
/// (`./target/release/probe`).
fn build(words: &[Word], cwd: &Path, folder: &Path) -> bool {
    if !inside(cwd, &[folder.to_path_buf()]) {
        return false;
    }
    let program = words[0].text.as_str();
    if program.contains('/') {
        return inside(&resolve(&words[0], cwd), &[folder.to_path_buf()]);
    }
    let program = match program {
        "python" | "python3" if words.get(1).is_some_and(|w| w.text == "-m") => {
            return words.get(2).is_some_and(|w| w.text == "pytest");
        }
        "uv" if words.get(1).is_some_and(|w| w.text == "run") => {
            return words.get(2).is_some_and(|w| w.text == "pytest");
        }
        p => p,
    };
    BUILDS.iter().any(|(name, subs)| {
        *name == program
            && (subs.is_empty()
                || words
                    .get(1)
                    .is_some_and(|sub| subs.contains(&sub.text.as_str())))
    })
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The path a word names from `cwd`, without `.` and `..`. A glob stops it at the folder the
/// glob is in; a `..` after a glob can't be followed, so it ends up at the root (outside).
fn resolve(word: &Word, cwd: &Path) -> PathBuf {
    let text = word.text.as_str();
    let joined = if word.tilde && (text == "~" || text.starts_with("~/")) {
        home().join(text.trim_start_matches('~').trim_start_matches('/'))
    } else {
        cwd.join(text)
    };
    let mut out = PathBuf::new();
    let mut globbed = false;
    for component in joined.components() {
        match component {
            Component::ParentDir if globbed => return PathBuf::from("/"),
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Normal(part) if globbed => {
                let _ = part;
            }
            Component::Normal(part) => {
                if part.to_string_lossy().contains(['*', '?', '[']) {
                    globbed = true;
                } else {
                    out.push(part);
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn inside(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

/// The command's simple commands, or `None` for anything this doesn't see through.
fn split(command: &str) -> Option<Vec<Segment>> {
    let mut segments = vec![Segment::default()];
    let mut word = String::new();
    let mut started = false;
    let mut tilde = false;
    // The next word is a redirect's target: `Some(true)` reads it, `Some(false)` writes it.
    let mut redirect: Option<bool> = None;
    let mut chars = command.chars().peekable();

    let end_word = |segments: &mut Vec<Segment>,
                        word: &mut String,
                        started: &mut bool,
                        tilde: &mut bool,
                        redirect: &mut Option<bool>|
     -> Option<()> {
        if !*started {
            return Some(());
        }
        let done = Word {
            text: std::mem::take(word),
            tilde: std::mem::take(tilde),
        };
        *started = false;
        let segment = segments.last_mut().expect("a segment");
        match redirect.take() {
            Some(true) => segment.inputs.push(done),
            // Only throwing output away.
            Some(false) if done.text == "/dev/null" => {}
            Some(false) => return None,
            None => segment.words.push(done),
        }
        Some(())
    };

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                started = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        c => word.push(c),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '$' | '`' => return None,
                        '\\' => match chars.next()? {
                            c @ ('"' | '\\' | '$' | '`') => word.push(c),
                            '\n' => {}
                            c => {
                                word.push('\\');
                                word.push(c);
                            }
                        },
                        c => word.push(c),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {}
                Some(c) => {
                    started = true;
                    word.push(c);
                }
                None => return None,
            },
            '$' | '`' | '(' | ')' => return None,
            '#' if !started => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                end_word(&mut segments, &mut word, &mut started, &mut tilde, &mut redirect)?;
                segments.push(Segment::default());
            }
            ' ' | '\t' => {
                end_word(&mut segments, &mut word, &mut started, &mut tilde, &mut redirect)?
            }
            ';' | '\n' | '|' | '&' => {
                if c == '&' && chars.peek() == Some(&'>') {
                    // `&>file`: both streams to a file.
                    chars.next();
                    if chars.peek() == Some(&'>') {
                        chars.next();
                    }
                    end_word(&mut segments, &mut word, &mut started, &mut tilde, &mut redirect)?;
                    redirect = Some(false);
                    continue;
                }
                end_word(&mut segments, &mut word, &mut started, &mut tilde, &mut redirect)?;
                if redirect.is_some() {
                    return None;
                }
                match (c, chars.peek()) {
                    ('&', Some('&')) | ('|', Some('|')) | ('|', Some('&')) => {
                        chars.next();
                    }
                    // Running in the background.
                    ('&', _) => return None,
                    _ => {}
                }
                segments.push(Segment::default());
            }
            '>' | '<' => {
                if c == '<' && chars.peek() == Some(&'<') {
                    return None;
                }
                // `2>` and `2>&1`: the digits are the stream, not a word.
                if started && !word.is_empty() && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    started = false;
                } else {
                    end_word(&mut segments, &mut word, &mut started, &mut tilde, &mut redirect)?;
                }
                if c == '>' && chars.peek() == Some(&'>') {
                    chars.next();
                }
                if c == '>' && chars.peek() == Some(&'&') {
                    // `>&2`: to another stream.
                    chars.next();
                    while chars.peek().is_some_and(|d| d.is_ascii_digit()) {
                        chars.next();
                    }
                    continue;
                }
                redirect = Some(c == '<');
            }
            '~' if !started => {
                started = true;
                tilde = true;
                word.push(c);
            }
            c => {
                started = true;
                word.push(c);
            }
        }
    }
    end_word(&mut segments, &mut word, &mut started, &mut tilde, &mut redirect)?;
    if redirect.is_some() {
        return None;
    }
    Some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(command: &str, rules: &[&str]) -> Verdict {
        let folder = PathBuf::from("/work/brain");
        let roots = vec![
            folder.clone(),
            home().join(".cargo/registry/src"),
            PathBuf::from("/tmp"),
        ];
        let rules: Vec<String> = rules.iter().map(|r| r.to_string()).collect();
        check_command(
            command,
            &CommandContext {
                folder: &folder,
                read_roots: &roots,
                rules: &rules,
            },
        )
    }

    fn ask(rules: &[&str]) -> Verdict {
        Verdict::Ask {
            rules: rules.iter().map(|r| r.to_string()).collect(),
        }
    }

    #[test]
    fn read_only_chains_inside_the_folders_run() {
        for command in [
            "cd /work/brain; cat src/main.rs; wc -l src/bin/spike.rs; ls -la models 2>&1",
            "cd ~/.cargo/registry/src/*/tokenizers-1.0.0-rc.3; ls; cat Cargo.toml | head -60; grep -n \"pub fn from_file\\|pub fn encode\" -r src | head",
            "cd ~/.cargo/registry/src/*/ort-2.0.0-rc.15; sed -n '/\\[features\\]/,$p' Cargo.toml | grep -v \"^api\" | sed -n 1,200p | head -60",
            "ls ~/.cargo/registry/src/*/ | grep -E \"^(rustfft|realfft)-\"",
            "find . -name '*.rs' | xargs-free-name 2>/dev/null; true",
            "git log --oneline -5 && git diff HEAD~1 -- src",
            "cat < Cargo.toml",
            "echo done; sed 's/\\x1b\\[[0-9;]*m//g' build.log",
        ] {
            let verdict = check(command, &[]);
            if command.contains("xargs-free-name") {
                assert_eq!(verdict, ask(&["Bash(xargs-free-name:*)"]), "{command}");
            } else {
                assert_eq!(verdict, Verdict::Allowed, "{command}");
            }
        }
    }

    #[test]
    fn builds_and_tests_run_in_the_folder() {
        for command in [
            "cargo build --release --bin probe 2>&1 | tail -5; ./target/release/probe",
            "RUST_LOG=info cargo test -p brain",
            "/usr/bin/time -v cargo run --release 2>&1 | grep -E \"Maximum resident\"",
            "timeout 60 just test",
            "python3 -m pytest tests",
            "flutter analyze",
        ] {
            assert_eq!(check(command, &[]), Verdict::Allowed, "{command}");
        }
        // Not in the folder: the project's code isn't what runs.
        assert_eq!(check("cd /elsewhere && cargo test", &[]), ask(&["Bash(cargo test:*)"]));
        assert_eq!(check("cargo publish", &[]), ask(&["Bash(cargo publish:*)"]));
    }

    #[test]
    fn outside_paths_writes_and_hidden_commands_ask() {
        // Reading outside the folders: ask, and `always` can't cover a read of anything.
        assert_eq!(check("cat ~/.ssh/id_ed25519", &[]), ask(&[]));
        assert_eq!(check("cd ~/.ssh; cat id_ed25519", &[]), ask(&[]));
        assert_eq!(
            check("cat ~/.cargo/registry/src/*/../../../credentials.toml", &[]),
            ask(&[])
        );
        assert_eq!(check("find / -name x", &[]), ask(&[]));
        assert_eq!(check("cat src/main.rs > out.txt", &[]), ask(&[]));
        assert_eq!(check("echo $(whoami)", &[]), ask(&[]));
        assert_eq!(check("cat \"$HOME/.ssh/id\"", &[]), ask(&[]));
        assert_eq!(check("sleep 10 &", &[]), ask(&[]));
        assert_eq!(check("python3 - <<EOF\nprint(1)\nEOF", &[]), ask(&[]));
        assert_eq!(check("find . -delete", &[]), ask(&[]));
        assert_eq!(check("sed -i 's/a/b/' src/x.rs", &[]), ask(&[]));
        assert_eq!(check("sed -n 'w /tmp/x' f", &[]), ask(&[]));
        assert_eq!(check("PATH=/tmp/evil cat x", &[]), ask(&[]));
        assert_eq!(check("git branch -D main", &[]), ask(&[]));
        assert_eq!(check("rg --pre ./run x", &[]), ask(&[]));
    }

    #[test]
    fn always_covers_each_part_that_asked() {
        let command = "cd /work/brain; python3 -c 'import onnx' 2>&1; hf download x config.json | head";
        assert_eq!(check(command, &[]), ask(&["Bash(python3:*)", "Bash(hf:*)"]));
        assert_eq!(
            check(command, &["Bash(python3:*)", "Bash(hf download:*)"]),
            Verdict::Allowed
        );
        // The allowlist's own forms.
        assert_eq!(check("npm install", &["Bash(npm install *)"]), Verdict::Allowed);
        assert_eq!(check("make deploy", &["Bash(make deploy)"]), Verdict::Allowed);
        assert_eq!(check("pip show onnx", &["Bash(pip show:*)"]), Verdict::Allowed);
    }
}
