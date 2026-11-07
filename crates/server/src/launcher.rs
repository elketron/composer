//! Launcher: the `[[launcher]]` entries of `config.toml`, spawned on the desktop.
//!
//! An entry names a command, a scoped folder (a project in the projects directory, or a
//! folder inside one), and optional args. Declaring an entry in config.toml is its approval:
//! an app asks the server to launch it, the server spawns it on the desktop with the folder
//! as its last argument and working directory, and whatever it writes back lands in watched
//! folders like the output of an agent.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

use crate::config::Config;
use crate::config_sections::LauncherEntry;
use crate::tools::{expand_home, resolve_project, tilde};

/// An entry as the apps see it (`GET /api/launcher`): one launch button per entry.
#[derive(Debug, Serialize)]
pub struct LauncherInfo {
    pub name: String,
    pub command: String,
    pub folder: String,
    pub args: Vec<String>,
}

/// What a launch answers (`POST /api/launcher/<name>`).
#[derive(Debug, Serialize)]
pub struct LaunchResult {
    /// `launched`: the process started (it keeps running; nothing waits for it).
    pub status: String,
    pub pid: Option<u32>,
}

pub fn entries(config: &Config) -> Vec<LauncherInfo> {
    config
        .launcher
        .iter()
        .map(|e| LauncherInfo {
            name: e.name.clone(),
            command: e.command.clone(),
            folder: e.folder.clone(),
            args: e.args.clone(),
        })
        .collect()
}

pub fn entry<'a>(config: &'a Config, name: &str) -> Option<&'a LauncherEntry> {
    config.launcher.iter().find(|e| e.name == name)
}

/// The folder of an entry, checked against the allowed scope: under the projects directory
/// (which is where projects and their scoped slices live). The error names what to do.
pub fn scoped_folder(config: &Config, entry: &LauncherEntry) -> Result<PathBuf, String> {
    let resolved = resolve_project(&entry.folder, &config.projects.dir)?;
    let folder = PathBuf::from(resolved);
    let scope = expand_home(&config.projects.dir);
    // The folder comes back canonical from `resolve_project`; the scope must match it
    // even when the projects directory is itself a symlink.
    let scope = scope.canonicalize().unwrap_or(scope);
    if folder.starts_with(&scope) {
        Ok(folder)
    } else {
        Err(format!(
            "launcher entry {} opens {}, which is outside the allowed scope (projects \
             under {}); move it inside a project folder or point `folder` at one",
            entry.name,
            tilde(&folder),
            tilde(&scope),
        ))
    }
}

/// Spawns the entry: `command <args...> <folder>`, working in the folder. Detached: the
/// process is the desktop's from here on; what it writes lights the watchers like any other.
#[allow(
    clippy::disallowed_methods,
    reason = "launcher entries are the user's own apps, started by the user"
)]
pub async fn spawn(entry: &LauncherEntry, folder: &Path) -> Result<LaunchResult, String> {
    if entry.command.is_empty() {
        return Err(format!(
            "launcher entry {} has no command in config.toml",
            entry.name
        ));
    }
    let mut command = Command::new(&entry.command);
    command
        .args(&entry.args)
        .arg(folder)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|e| format!("could not start {}: {}", entry.command, e))?;
    Ok(LaunchResult {
        status: "launched".into(),
        pid: child.id(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(projects_dir: &str) -> Config {
        let mut c = Config::defaults();
        c.projects.dir = projects_dir.into();
        c
    }

    #[tokio::test]
    async fn a_folder_inside_the_projects_directory_launches() {
        let projects = tempfile::tempdir().unwrap();
        let slice = projects.path().join("app/blockouts");
        std::fs::create_dir_all(&slice).unwrap();
        let c = config_with(projects.path().to_str().unwrap());
        let e = LauncherEntry {
            name: "stub".into(),
            command: "/bin/sh".into(),
            folder: "app/blockouts".into(),
            args: vec!["-c".into()],
        };
        let folder = scoped_folder(&c, &e).unwrap();
        assert_eq!(folder, slice.canonicalize().unwrap());
        // The same command as the entry, run for the test: it writes a marker and exits.
        let e = LauncherEntry {
            args: vec![
                "-c".into(),
                format!("touch {}", slice.join("ran").display()),
                "x".into(),
            ],
            ..e
        };
        let r = spawn(&e, &folder).await.unwrap();
        assert_eq!(r.status, "launched");
        assert!(r.pid.is_some());
        for _ in 0..100 {
            if slice.join("ran").exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the launched process did not write its marker");
    }

    #[tokio::test]
    async fn a_folder_outside_the_scope_refuses_to_launch() {
        let projects = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let c = config_with(projects.path().to_str().unwrap());
        let e = LauncherEntry {
            name: "stub".into(),
            command: "touch".into(),
            folder: outside.path().to_str().unwrap().into(),
            args: vec!["marker".into()],
        };
        let err = scoped_folder(&c, &e).unwrap_err();
        assert!(err.contains("outside the allowed scope"), "{}", err);
        // And it did not run: no marker inside the folder.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(!outside.path().join("marker").exists());
    }
}
