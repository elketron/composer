use std::ffi::OsStr;
use std::process::Stdio;
use std::sync::OnceLock;

use tokio::process::Command;

/// The env `systemd-run --user` needs to reach the user's systemd (a harness clears the rest).
const SYSTEMD_ENV: [&str; 2] = ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"];

/// A worker run's own cgroup: a transient systemd scope (`systemd-run --user --scope`). It caps
/// the run's memory, so a runaway test suite is killed inside it instead of the server and the
/// terminal it runs in, and stopping it ends everything the run started, daemons that left its
/// process group included.
pub struct WorkerScope {
    unit: String,
}

impl WorkerScope {
    /// `program` for the run of `session`: inside a scope capped at `memory_max` (systemd's
    /// syntax: `50%`, `12G`; empty: no cap) where systemd-run works, else as it is. Arguments
    /// added to the command go to `program`.
    pub fn command(program: &OsStr, session: &str, memory_max: &str) -> (Command, Option<Self>) {
        if !available() {
            return (Command::new(program), None);
        }
        let stamp = chrono::Utc::now().timestamp_millis();
        let unit = format!("composer-worker-{session}-{stamp}.scope");
        let mut command = Command::new("systemd-run");
        command.args(["--user", "--scope", "--quiet", "--collect", "--unit", &unit]);
        if !memory_max.trim().is_empty() {
            // At the cap the kernel reclaims, then kills inside the scope: no swapping the
            // machine to a halt first.
            command
                .arg("-p")
                .arg(format!("MemoryMax={}", memory_max.trim()))
                .args(["-p", "MemorySwapMax=0"]);
        }
        command.arg("--").arg(program);
        (command, Some(Self { unit }))
    }

    /// Kept by a command whose env was cleared: systemd-run needs them.
    pub fn keep_env(command: &mut Command) {
        for key in SYSTEMD_ENV {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
    }

    /// Ends whatever is still running in it.
    pub fn stop(&self) {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", "--no-block", &self.unit])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Whether runs can go in scopes here (systemd with a user manager; not WSL without it, not
/// macOS).
fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect", "true"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}
