//! The words a server is described in. What the user reads before approving is the only thing
//! standing between an agent-written `mcp.toml` and a program started as the user, so it says
//! what the file says: the arguments one by one, the environment too, and control characters
//! spelled out so that no value can add a line of its own.

use std::collections::BTreeMap;

/// A command and its arguments as one thing to read, the way it would be typed. An argument is
/// quoted when it would otherwise run into its neighbours, so a two-argument `"a b", "c"` does
/// not read as a three-argument `a, b, c`.
pub(crate) fn command_line(command: &str, args: &[String]) -> String {
    let said: Vec<String> = std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(word)
        .collect();
    format!("`{}`", said.join(" "))
}

/// The environment a program would be started with, said back in full. `None` when there is
/// none, so a plain command is not followed by an empty clause.
pub(crate) fn environment(env: &BTreeMap<String, String>) -> Option<String> {
    if env.is_empty() {
        return None;
    }
    let said: Vec<String> = env
        .iter()
        .map(|(name, value)| format!("{}={}", word(name), word(value)))
        .collect();
    Some(format!("in the environment {}", said.join(", ")))
}

/// One value as it would be typed: bare when it reads as a single word, quoted (and escaped, a
/// newline as `\n`) when it would not.
pub(crate) fn word(value: &str) -> String {
    let one_word = !value.is_empty()
        && !value.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '\\' | '$' | '`')
        });
    if one_word {
        value.to_string()
    } else {
        format!("\"{}\"", value.escape_debug())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    /// The whole point of quoting one by one: two arguments are not mistaken for three.
    #[test]
    fn arguments_that_need_quotes_are_quoted_one_by_one() {
        assert_eq!(command_line("blender", &args(&[])), "`blender`");
        assert_eq!(
            command_line("blender", &args(&["--factory-startup", "scene.blend"])),
            "`blender --factory-startup scene.blend`"
        );
        assert_eq!(command_line("x", &args(&["a b", "c"])), "`x \"a b\" c`");
        assert_eq!(command_line("x", &args(&[""])), "`x \"\"`");
    }

    #[test]
    fn an_environment_is_said_in_full_and_left_out_when_empty() {
        assert_eq!(environment(&BTreeMap::new()), None);
        let env = [
            ("LD_PRELOAD".to_string(), "/tmp/hook.so".to_string()),
            ("PATH".to_string(), "/tmp/a b:/tmp/c".to_string()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            environment(&env).unwrap(),
            "in the environment LD_PRELOAD=/tmp/hook.so, PATH=\"/tmp/a b:/tmp/c\""
        );
    }

    /// A value with a newline in it cannot write a line the file does not have.
    #[test]
    fn control_characters_are_spelled_out() {
        let env = [("K".to_string(), "\tvalue\nfake: runs `true`".to_string())]
            .into_iter()
            .collect();
        let said = environment(&env).unwrap();
        assert_eq!(said.lines().count(), 1, "{said:?}");
        assert!(said.contains("\\t") && said.contains("\\n"), "{said}");
        assert_eq!(word("a\u{1b}[2J"), "\"a\\u{1b}[2J\"");
    }
}
