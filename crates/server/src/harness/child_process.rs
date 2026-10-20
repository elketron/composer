use std::time::Duration;

use async_trait::async_trait;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};

use crate::harness::HarnessRun;

/// How long an interrupted process gets to exit before it is killed.
const GRACE: Duration = Duration::from_secs(5);

/// A harness run that is a child process in its own process group, so interrupting it also
/// stops whatever it started (shells, test runs, dev servers).
pub struct ChildProcess {
    pid: Option<u32>,
    child: Mutex<Child>,
    interrupt: Notify,
}

impl ChildProcess {
    pub fn spawn(mut command: Command) -> std::io::Result<Self> {
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn()?;
        Ok(Self {
            pid: child.id(),
            child: Mutex::new(child),
            interrupt: Notify::new(),
        })
    }

    #[cfg(unix)]
    fn signal_group(&self, signal: libc::c_int) {
        if let Some(pid) = self.pid {
            // SAFETY: killpg only sends a signal; the group id is our child's own pid.
            unsafe {
                libc::killpg(pid as libc::pid_t, signal);
            }
        }
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
