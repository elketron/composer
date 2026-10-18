use std::io::Read;
use std::process::Stdio;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::actions::justfile::just;
use crate::actions::{Justfile, RunOutput};

/// Output kept for the model: the start and the end, which is where errors are.
const MAX_OUTPUT: usize = 16_000;

/// Context passed to recipes as environment variables.
#[derive(Debug, Clone, Default)]
pub struct RecipeEnv {
    pub site: String,
    pub session: String,
    pub project: Option<String>,
}

/// Runs a recipe in its site folder. Killed on timeout or cancel.
pub async fn run_recipe(
    justfile: &Justfile,
    recipe: &str,
    args: &[String],
    env: &RecipeEnv,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<RunOutput, String> {
    // One pipe for stdout and stderr, so the output keeps its real order (as in a terminal).
    let (mut reader, writer) = std::io::pipe().map_err(|e| e.to_string())?;
    let mut command = just(justfile.dir());
    command
        .arg(recipe)
        .args(args)
        .env("COMPOSER_SITE", &env.site)
        .env("COMPOSER_SESSION", &env.session)
        .env("COMPOSER_PROJECT", env.project.as_deref().unwrap_or(""))
        .stdin(Stdio::null())
        .stdout(writer.try_clone().map_err(|e| e.to_string())?)
        .stderr(writer)
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not run `just`: {e}"))?;
    // Drop our copies of the write end, or reading never sees end-of-file.
    drop(command);
    let read = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        let _ = reader.read_to_end(&mut out);
        out
    });

    tokio::select! {
        status = child.wait() => {
            // A background process the recipe started can keep the pipe open: don't wait for it forever.
            let out = match tokio::time::timeout(Duration::from_secs(5), read).await {
                Ok(Ok(out)) => out,
                _ => b"(output unavailable: a background process kept it open)".to_vec(),
            };
            let text = String::from_utf8_lossy(&out);
            Ok(RunOutput { exit_code: status.ok().and_then(|s| s.code()), output: trim_output(&text), timed_out: false })
        }
        _ = tokio::time::sleep(timeout) => {
            Ok(RunOutput { exit_code: None, output: format!("killed after {}s", timeout.as_secs()), timed_out: true })
        }
        _ = cancel.cancelled() => Err("cancelled by the user".into()),
    }
}

fn trim_output(text: &str) -> String {
    if text.len() <= MAX_OUTPUT {
        return text.to_string();
    }
    let head_end = text.floor_char_boundary(MAX_OUTPUT / 2);
    let tail_start = text.ceil_char_boundary(text.len() - MAX_OUTPUT / 2);
    format!(
        "{}\n… ({} bytes omitted) …\n{}",
        &text[..head_end],
        tail_start - head_end,
        &text[tail_start..]
    )
}
