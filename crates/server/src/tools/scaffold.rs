//! Copying a template folder (an app, a project) with a name filled in.

use std::path::Path;

/// Copies `from` into `to`, replacing `placeholder` with `value` in text files. Skips build
/// output (`node_modules`, `dist`).
pub fn copy_template(
    from: &Path,
    to: &Path,
    placeholder: &str,
    value: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "node_modules" || name == "dist" || name == "__pycache__" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_template(&entry.path(), &target, placeholder, value)?;
            continue;
        }
        let bytes = std::fs::read(entry.path())?;
        match String::from_utf8(bytes) {
            Ok(text) => std::fs::write(&target, text.replace(placeholder, value))?,
            Err(binary) => std::fs::write(&target, binary.into_bytes())?,
        }
    }
    Ok(())
}
