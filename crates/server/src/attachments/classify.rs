use std::path::Path;

/// What an attachment is, which decides how the model sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Pdf,
    Text,
}

/// Text files by extension (or whole name): prose, data and common code.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "rst", "csv", "tsv", "json", "jsonl", "toml", "yaml", "yml", "xml",
    "html", "htm", "css", "scss", "js", "mjs", "cjs", "ts", "tsx", "jsx", "svelte", "vue", "rs",
    "py", "rb", "go", "java", "kt", "kts", "swift", "c", "h", "cc", "cpp", "hpp", "cs", "dart",
    "lua", "php", "r", "scala", "sh", "bash", "zsh", "fish", "sql", "log", "ini", "cfg", "conf",
    "diff", "patch", "tex", "org", "gradle", "nix", "proto", "graphql",
];
const TEXT_NAMES: &[&str] = &["Makefile", "Dockerfile", "justfile", "LICENSE", "README"];

/// The kind and MIME type of an upload, from its content (images, PDF) or its name and being
/// valid UTF-8 (text). `None`: not a type composer takes.
pub fn classify(name: &str, bytes: &[u8]) -> Option<(AttachmentKind, &'static str)> {
    let image = |mime| Some((AttachmentKind::Image, mime));
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return image("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return image("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return image("image/gif");
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return image("image/webp");
    }
    if bytes.starts_with(b"%PDF-") {
        return Some((AttachmentKind::Pdf, "application/pdf"));
    }
    let path = Path::new(name);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let named = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| TEXT_NAMES.contains(&n));
    let texty = named
        || extension
            .as_deref()
            .is_some_and(|e| TEXT_EXTENSIONS.contains(&e));
    if !texty || bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return None;
    }
    let mime = match extension.as_deref() {
        Some("md" | "markdown") => "text/markdown",
        Some("csv") => "text/csv",
        Some("json" | "jsonl") => "application/json",
        Some("html" | "htm") => "text/html",
        _ => "text/plain",
    };
    Some((AttachmentKind::Text, mime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_content_then_name() {
        assert_eq!(
            classify("x.bin", b"\x89PNG\r\n\x1a\nrest"),
            Some((AttachmentKind::Image, "image/png"))
        );
        assert_eq!(
            classify("photo", &[0xff, 0xd8, 0xff, 0xe0]),
            Some((AttachmentKind::Image, "image/jpeg"))
        );
        assert_eq!(
            classify("doc.pdf", b"%PDF-1.7"),
            Some((AttachmentKind::Pdf, "application/pdf"))
        );
        assert_eq!(
            classify("main.rs", b"fn main() {}"),
            Some((AttachmentKind::Text, "text/plain"))
        );
        assert_eq!(
            classify("notes.md", b"# hi"),
            Some((AttachmentKind::Text, "text/markdown"))
        );
        assert_eq!(
            classify("Makefile", b"all:"),
            Some((AttachmentKind::Text, "text/plain"))
        );
        // Not allowed: unknown types, binaries posing as text, a PDF name without PDF bytes.
        assert_eq!(classify("app.exe", b"MZ\x90\x00"), None);
        assert_eq!(classify("x.txt", b"a\0b"), None);
        assert_eq!(classify("x.pdf", b"not a pdf"), None);
    }
}
