use base64::Engine;
use composer_protocol::Attachment;

use crate::attachments::{AttachmentKind, classify, resolve};
use crate::home::Home;

/// How a session's image attachment is referred to in model history until it is sent:
/// `attachment://<session>/<file>` (like site URLs, replaced by a data URL at send time).
pub const ATTACHMENT_SCHEME: &str = "attachment://";

/// What the model gets for an attachment: an image reference, or text to add to the message.
pub enum ForModel {
    Image(String),
    Text(String),
}

/// `attachment` of `session` for the model: images by reference; text and PDFs (their text,
/// via `pdftotext`) inline, cut at `max_inline` bytes and marked as file content.
pub fn for_model(
    home: &Home,
    session: &str,
    attachment: &Attachment,
    max_inline: usize,
) -> Result<ForModel, String> {
    let (attachment, path) = resolve(home, session, attachment)?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let kind = classify(&attachment.name, &bytes).map(|(k, _)| k);
    let text = match kind {
        Some(AttachmentKind::Image) => {
            return Ok(ForModel::Image(format!(
                "{ATTACHMENT_SCHEME}{session}/{}",
                attachment.file
            )));
        }
        Some(AttachmentKind::Pdf) => pdf_text(&path).unwrap_or_else(|| {
            "(its text could not be extracted; it may be scanned images)".into()
        }),
        _ => String::from_utf8_lossy(&bytes).into_owned(),
    };
    let cut = match text.char_indices().find(|(i, _)| *i >= max_inline) {
        Some((i, _)) => format!(
            "{}\n[… cut: the file is longer than {} KB]",
            &text[..i],
            max_inline / 1024
        ),
        None => text,
    };
    Ok(ForModel::Text(format!(
        "<attachment name=\"{}\" type=\"{}\">\n{}\n</attachment>",
        attachment.name, attachment.mime, cut
    )))
}

/// A data URL for an `attachment://<session>/<file>` reference, or `None` for anything else.
pub fn attachment_data_url(home: &Home, reference: &str) -> Option<String> {
    let rest = reference.strip_prefix(ATTACHMENT_SCHEME)?;
    let (session, file) = rest.split_once('/')?;
    let probe = Attachment {
        file: file.to_string(),
        name: String::new(),
        mime: String::new(),
        size: 0,
    };
    let (attachment, path) = resolve(home, session, &probe).ok()?;
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{};base64,{}",
        attachment.mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn pdf_text(path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("pdftotext")
        .args(["-layout", "-enc", "UTF-8"])
        .arg(path)
        .arg("-")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}
