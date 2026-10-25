//! The projects a client can pick or search: the folders in the projects directory, plus any
//! folder a session works in outside it, each with what is going on there.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use composer_protocol::http::ProjectSummary;
use composer_protocol::{ServerMessage, SessionInfo, TurnState, WorkerState};

use crate::server::Server;
use crate::tools::{expand_home, tilde};

/// The project a session belongs to: its own, or for a worker (whose project is its ticket's
/// worktree) its orchestrator's.
pub fn session_project(sessions: &[SessionInfo], session: &SessionInfo) -> Option<String> {
    match (&session.ticket, &session.parent) {
        (Some(_), Some(parent)) => sessions
            .iter()
            .find(|s| &s.id == parent)
            .and_then(|p| p.project.clone()),
        _ => session.project.clone(),
    }
}

/// Projects matching `query` (every word in the name, or in the path of a project outside the
/// projects directory; empty: all), the ones with something going on first, then the most
/// recently used or changed.
pub fn project_list(server: &Server, query: &str) -> Vec<ProjectSummary> {
    let worktrees = server.home.state().join("worktrees");
    let mut folders: BTreeMap<String, (String, SystemTime)> = BTreeMap::new();
    let dir = expand_home(&server.config().projects.dir);
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let folder = canonical(&entry.path());
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        folders.insert(folder, (name, modified));
    }

    let sessions = server.sessions.list();
    // Folders sessions work in outside the projects directory (not ticket worktrees).
    for s in &sessions {
        if let Some(folder) = &s.project
            && !folders.contains_key(folder)
            && !Path::new(folder).starts_with(&worktrees)
            && Path::new(folder).is_dir()
        {
            let name = Path::new(folder)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| folder.clone());
            let modified = std::fs::metadata(folder)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            folders.insert(folder.clone(), (name, modified));
        }
    }

    let waiting_sessions: Vec<String> = server
        .pending_approvals
        .all()
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::ApprovalRequest { session, .. } => Some(session),
            _ => None,
        })
        .collect();
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();

    let mut projects: Vec<(bool, String, ProjectSummary)> = folders
        .into_iter()
        .filter_map(|(folder, (name, modified))| {
            let path = tilde(Path::new(&folder));
            // By name; the path only for projects outside the projects directory (every path in
            // it would match its own name, `~/Projects`).
            let inside =
                Path::new(&folder).parent().map(canonical_path) == Some(canonical_path(&dir));
            let hay = if inside {
                name.to_lowercase()
            } else {
                format!("{} {}", name.to_lowercase(), path.to_lowercase())
            };
            if !words.iter().all(|w| hay.contains(w)) {
                return None;
            }
            let mine: Vec<&SessionInfo> = sessions
                .iter()
                .filter(|s| session_project(&sessions, s).as_deref() == Some(folder.as_str()))
                .collect();
            let busy = |s: &SessionInfo| {
                s.state != TurnState::Idle
                    || matches!(s.worker, Some(WorkerState::Starting | WorkerState::Running))
            };
            // Chats and orchestrators; workers count with their orchestrator.
            let top = mine.iter().filter(|s| s.ticket.is_none());
            let running = top
                .clone()
                .filter(|s| {
                    busy(s)
                        || mine
                            .iter()
                            .any(|w| w.parent.as_ref() == Some(&s.id) && busy(w))
                })
                .count() as u32;
            let waiting = mine
                .iter()
                .filter(|s| waiting_sessions.contains(&s.id))
                .count() as u32;
            let last_used = mine.iter().map(|s| s.updated.clone()).max();
            let modified = chrono::DateTime::<chrono::Local>::from(modified).to_rfc3339();
            // Ranked by the later of last use and the folder's own change.
            let recent = last_used
                .clone()
                .into_iter()
                .chain([modified.clone()])
                .max()
                .unwrap_or_default();
            Some((
                running > 0 || waiting > 0,
                recent,
                ProjectSummary {
                    name,
                    path,
                    modified,
                    sessions: top.count() as u32,
                    running,
                    waiting,
                    last_used,
                    folder,
                },
            ))
        })
        .collect();
    projects.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| cmp_time(&b.1, &a.1)));
    projects.into_iter().map(|(_, _, p)| p).collect()
}

/// RFC 3339 times in different offsets, compared as instants.
fn cmp_time(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |t: &str| chrono::DateTime::parse_from_rfc3339(t).ok();
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

fn canonical_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| PathBuf::from(path))
}

fn canonical(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}
