/// Lowercase, every run of characters outside `[a-z0-9]` one `-`, trimmed of `-`
/// (`"Blender MCP"` → `blender-mcp`).
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slugs() {
        assert_eq!(slug("Blender"), "blender");
        assert_eq!(slug("  My  Cool_Server!! v2 "), "my-cool-server-v2");
        assert_eq!(slug("--a--"), "a");
        assert_eq!(slug("日本"), "");
        assert_eq!(slug(""), "");
    }
}
