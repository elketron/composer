use std::path::{Path, PathBuf};

use composer_protocol::Attachment;

use crate::attachments::{AttachmentKind, classify, prepare_image};
use crate::config_sections::AttachmentsConfig;
use crate::hash::content_hash;
use crate::home::Home;

/// A session's attachments: `state/sessions/<id>/attachments`.
pub fn folder(home: &Home, session: &str) -> PathBuf {
    home.sessions().join(session).join("attachments")
}

/// A file name that is safe to store: no folders, only `A-Z a-z 0-9 . _ -`, no leading dot,
/// at most 80 characters. Empty becomes `file`.
pub fn sanitize_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut clean = String::new();
    for c in base.chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            c
        } else {
            '_'
        };
        if !(c == '_' && clean.ends_with('_')) {
            clean.push(c);
        }
    }
    let clean = clean.trim_start_matches(['.', '_']).to_string();
    let clean: String = if clean.chars().count() > 80 {
        // Keep the extension.
        let ext = Path::new(&clean)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| e.len() <= 10)
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        let stem: String = clean.chars().take(80 - ext.len()).collect();
        format!("{stem}{ext}")
    } else {
        clean
    };
    if clean.is_empty() || clean.chars().all(|c| c == '.') {
        "file".into()
    } else {
        clean
    }
}

/// Stores an upload for `session`: checked (size, type), images made safe (see
/// [`prepare_image`]), named `<hash>-<name>`. The same content uploaded again is stored once.
pub fn save(
    home: &Home,
    config: &AttachmentsConfig,
    session: &str,
    name: &str,
    bytes: &[u8],
) -> Result<Attachment, String> {
    let max = config.max_mb * 1024 * 1024;
    if bytes.len() as u64 > max {
        return Err(format!("{name} is larger than {} MB", config.max_mb));
    }
    let mut name = sanitize_name(name);
    let Some((kind, mime)) = classify(&name, bytes) else {
        return Err(format!(
            "{name}: only images, PDFs and text files (prose, data, code) can be attached"
        ));
    };
    let (bytes, mime) = match kind {
        AttachmentKind::Image => {
            let (bytes, mime, ext) = prepare_image(bytes, config.image_max_edge)?;
            let stem = Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("image")
                .to_string();
            name = format!("{stem}.{ext}");
            (bytes, mime)
        }
        _ => (bytes.to_vec(), mime),
    };
    let file = format!("{}-{name}", &content_hash(&bytes)[..12]);
    let dir = folder(home, session);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&file);
    if !path.exists() {
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(Attachment {
        file,
        name,
        mime: mime.into(),
        size: bytes.len() as u64,
    })
}

/// A client's reference to an attachment of `session`: the stored file it names (which must be
/// in that session's folder), described from the file itself. Returns it with its path.
pub fn resolve(
    home: &Home,
    session: &str,
    reference: &Attachment,
) -> Result<(Attachment, PathBuf), String> {
    let file = &reference.file;
    let refused = || format!("no attachment `{file}` in session {session}");
    if file.is_empty() || sanitize_name(file) != *file {
        return Err(refused());
    }
    let dir = folder(home, session)
        .canonicalize()
        .map_err(|_| refused())?;
    let path = dir.join(file).canonicalize().map_err(|_| refused())?;
    if !path.starts_with(&dir) || !path.is_file() {
        return Err(refused());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let name = file.split_once('-').map_or(file.as_str(), |(_, n)| n);
    let (_, mime) = classify(name, &bytes).ok_or_else(refused)?;
    Ok((
        Attachment {
            file: file.clone(),
            name: name.to_string(),
            mime: mime.into(),
            size: bytes.len() as u64,
        },
        path,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AttachmentsConfig {
        toml::from_str("").unwrap()
    }

    #[test]
    fn names() {
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(
            sanitize_name("C:\\Users\\me\\My Notes (1).txt"),
            "My_Notes_1_.txt"
        );
        assert_eq!(sanitize_name(".bashrc"), "bashrc");
        assert_eq!(sanitize_name(".."), "file");
        assert_eq!(sanitize_name(""), "file");
        assert_eq!(sanitize_name("résumé.pdf"), "r_sum_.pdf");
        let long = format!("{}.md", "a".repeat(200));
        assert_eq!(sanitize_name(&long).len(), 80);
        assert!(sanitize_name(&long).ends_with(".md"));
    }

    #[test]
    fn saves_once_and_resolves_only_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        let home = Home::new(tmp.path());
        let saved = save(&home, &config(), "s1", "../notes.md", b"# notes").unwrap();
        assert!(saved.file.ends_with("-notes.md"));
        assert_eq!((saved.mime.as_str(), saved.size), ("text/markdown", 7));
        let again = save(&home, &config(), "s1", "notes.md", b"# notes").unwrap();
        assert_eq!(again.file, saved.file);

        // The client's description is ignored; the file decides.
        let claimed = Attachment {
            mime: "image/png".into(),
            size: 1,
            ..saved.clone()
        };
        let (resolved, path) = resolve(&home, "s1", &claimed).unwrap();
        assert_eq!(resolved, saved);
        assert!(path.ends_with(&saved.file));

        // Not another session's, not outside the folder.
        assert!(resolve(&home, "s2", &saved).is_err());
        for file in ["../meta.json", "/etc/passwd", "", "x/../../y"] {
            let bad = Attachment {
                file: file.into(),
                ..saved.clone()
            };
            assert!(resolve(&home, "s1", &bad).is_err(), "{file}");
        }
    }

    #[test]
    fn refuses_what_it_does_not_take() {
        let tmp = tempfile::tempdir().unwrap();
        let home = Home::new(tmp.path());
        let small = AttachmentsConfig {
            max_mb: 1,
            ..config()
        };
        let big = vec![b'a'; 2 * 1024 * 1024];
        assert!(
            save(&home, &small, "s", "big.txt", &big)
                .unwrap_err()
                .contains("larger than 1 MB")
        );
        assert!(
            save(&home, &config(), "s", "tool.exe", b"MZ")
                .unwrap_err()
                .contains("only images")
        );
    }
}
