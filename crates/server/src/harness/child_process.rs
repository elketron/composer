use std::time::Duration;

use async_trait::async_trait;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{Mutex, Notify};

use crate::harness::{HarnessRun, WorkerScope};

/// How long an interrupted process gets to exit before it is killed.
const GRACE: Duration = Duration::from_secs(5);

/// A harness run that is a child process in its own process group, so interrupting it also
/// stops whatever it started (shells, test runs, dev servers). So does its ending by itself:
/// what it left running in the background (Claude Code's `run_in_background`) goes with it,
/// instead of piling up for as long as the server runs.
pub struct ChildProcess {
    pid: Option<u32>,
    child: Mutex<Child>,
    interrupt: Notify,
    /// Its systemd scope, when it runs in one: stopped when it ends, which ends what left the
    /// process group too.
    scope: Option<WorkerScope>,
}

impl ChildProcess {
    pub fn spawn(command: Command) -> std::io::Result<Self> {
        Self::spawn_in(command, None)
    }

    /// `command` from [`WorkerScope::command`], with its scope.
    pub fn spawn_in(mut command: Command, scope: Option<WorkerScope>) -> std::io::Result<Self> {
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn()?;
        Ok(Self {
            pid: child.id(),
            child: Mutex::new(child),
            interrupt: Notify::new(),
            scope,
        })
    }

    /// Its stdout, when piped; once.
    pub fn take_stdout(&self) -> Option<ChildStdout> {
        self.child.try_lock().ok()?.stdout.take()
    }

    #[cfg(unix)]
    fn signal_group(&self, signal: libc::c_int) {
        if let Some(pid) = self.pid {
            signal_group(pid, signal);
        }
    }

    /// What is left of its group after it exited: asked to stop, then killed.
    #[cfg(unix)]
    fn clear_group(&self) {
        let Some(pid) = self.pid else {
            return;
        };
        signal_group(pid, libc::SIGTERM);
        tokio::spawn(async move {
            tokio::time::sleep(GRACE).await;
            signal_group(pid, libc::SIGKILL);
        });
    }
}

/// Signals process group `pid` (a no-op once it is empty: a group's id isn't reused while any
/// member lives).
#[cfg(unix)]
pub fn signal_group(pid: u32, signal: libc::c_int) {
    // SAFETY: killpg only sends a signal.
    unsafe {
        libc::killpg(pid as libc::pid_t, signal);
    }
}

#[async_trait]
impl HarnessRun for ChildProcess {
    async fn wait(&self) -> String {
        let mut child = self.child.lock().await;
        let status = tokio::select! {
            status = child.wait() => status,
            _ = self.interrupt.notified() => {
                #[cfg(unix)]
                self.signal_group(libc::SIGTERM);
                #[cfg(not(unix))]
                let _ = child.start_kill();
                match tokio::time::timeout(GRACE, child.wait()).await {
                    Ok(status) => status,
                    Err(_) => {
                        #[cfg(unix)]
                        self.signal_group(libc::SIGKILL);
                        let _ = child.start_kill();
                        child.wait().await
                    }
                }
            }
        };
        #[cfg(unix)]
        self.clear_group();
        if let Some(scope) = &self.scope {
            scope.stop();
        }
        match status {
            Ok(status) => status
                .code()
                .map_or("killed".into(), |c| format!("exit {c}")),
            Err(e) => format!("lost track of it: {e}"),
        }
    }

    fn interrupt(&self) {
        // Stored if `wait` isn't waiting yet.
        self.interrupt.notify_one();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[tokio::test]
    async fn what_a_run_leaves_behind_goes_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pid");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("sleep 60 & echo $! > {}", file.display()));
        let run = ChildProcess::spawn(command).unwrap();
        assert_eq!(run.wait().await, "exit 0");
        let pid: i32 = std::fs::read_to_string(&file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        for _ in 0..50 {
            if !alive(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("the background sleep ({pid}) outlived its run");
    }

    /// Needs a systemd user manager; passes without one (nothing to test there).
    #[tokio::test]
    async fn a_scope_ends_what_left_the_group_and_caps_memory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pid");
        let (mut command, scope) = WorkerScope::command(std::ffi::OsStr::new("sh"), "test", "64M");
        if scope.is_none() {
            return;
        }
        // A daemon in a session of its own: the process group doesn't reach it.
        command
            .arg("-c")
            .arg(format!("setsid sleep 60 & echo $! > {}", file.display()));
        let run = ChildProcess::spawn_in(command, scope).unwrap();
        assert_eq!(run.wait().await, "exit 0");
        let pid: i32 = std::fs::read_to_string(&file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut gone = false;
        for _ in 0..50 {
            if !alive(pid) {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the daemon ({pid}) outlived its run's scope");

        // 300 MB under a 64 MB cap: killed in its scope (and only there).
        let (mut command, scope) =
            WorkerScope::command(std::ffi::OsStr::new("python3"), "test", "64M");
        command.args(["-c", "b = b\"x\" * (300 * 1024 * 1024); print(len(b))"]);
        let run = ChildProcess::spawn_in(command, scope).unwrap();
        assert_eq!(run.wait().await, "killed");
    }
}
