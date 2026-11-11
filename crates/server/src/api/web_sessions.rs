use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::api::Credential;

/// A session nobody used for this long is gone.
const IDLE: Duration = Duration::from_secs(12 * 3600);
/// Sessions kept at most; beyond it the one unused the longest goes.
const MAX_SESSIONS: usize = 4096;

/// One browser session: who opened it and when it was last used.
struct WebSession {
    minter: Credential,
    last_used: Instant,
}

/// Browser sessions: what the auth cookie holds instead of a token. Each is a random secret made
/// when a request presents `?once=<code>` or `?token=<token>`, kept here by its hash, in memory
/// only (a restart ends them all; the client opens its page again). A session is the kind of
/// credential that opened it ([`Credential`]): one a paired device opened is that device, and
/// ends with it. Unused for [`IDLE`], it ends too.
pub struct WebSessions {
    sessions: Mutex<HashMap<[u8; 32], WebSession>>,
    idle: Duration,
}

impl Default for WebSessions {
    fn default() -> Self {
        Self::with_idle(IDLE)
    }
}

impl WebSessions {
    /// Every session secret starts with this, so it is never taken for a token.
    pub const PREFIX: &'static str = "ses_";

    /// Sessions that end after `idle` without use.
    pub fn with_idle(idle: Duration) -> Self {
        Self {
            sessions: Mutex::default(),
            idle,
        }
    }

    /// A new session opened by `minter`: its secret.
    pub fn mint(&self, minter: Credential) -> String {
        let secret = format!(
            "{}{}",
            Self::PREFIX,
            (0..4)
                .map(|_| format!("{:016x}", rand::random::<u64>()))
                .collect::<String>()
        );
        let mut sessions = self.sessions.lock().expect("web sessions lock");
        sessions.retain(|_, session| session.last_used.elapsed() < self.idle);
        if sessions.len() >= MAX_SESSIONS
            && let Some(oldest) = sessions
                .iter()
                .min_by_key(|(_, session)| session.last_used)
                .map(|(key, _)| *key)
        {
            sessions.remove(&oldest);
        }
        sessions.insert(
            hash(&secret),
            WebSession {
                minter,
                last_used: Instant::now(),
            },
        );
        secret
    }

    /// Who opened the session `secret` names, when it is one of ours, was used within the idle
    /// time, and `alive` says its minter still counts (a revoked device does not: its sessions
    /// are dropped here). Using it keeps it.
    pub fn credential(
        &self,
        secret: &str,
        alive: impl Fn(&Credential) -> bool,
    ) -> Option<Credential> {
        if !secret.starts_with(Self::PREFIX) {
            return None;
        }
        // Looked up by hash: where a guess lands says nothing about a real secret.
        let key = hash(secret);
        let mut sessions = self.sessions.lock().expect("web sessions lock");
        let session = sessions.get_mut(&key)?;
        if session.last_used.elapsed() >= self.idle || !alive(&session.minter) {
            sessions.remove(&key);
            return None;
        }
        session.last_used = Instant::now();
        Some(session.minter.clone())
    }
}

fn hash(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> Credential {
        Credential::Device {
            name: name.into(),
            token_hash: format!("hash of {name}"),
        }
    }

    #[test]
    fn a_session_is_its_minter() {
        let sessions = WebSessions::default();
        let (local, phone) = (
            sessions.mint(Credential::Local),
            sessions.mint(device("phone")),
        );
        assert!(local.starts_with("ses_") && local.len() == 68 && local != phone);
        assert_eq!(
            sessions.credential(&local, |_| true),
            Some(Credential::Local)
        );
        assert_eq!(sessions.credential(&phone, |_| true), Some(device("phone")));
        assert_eq!(sessions.credential("ses_forged", |_| true), None);
        assert_eq!(sessions.credential(&local[4..], |_| true), None);
    }

    #[test]
    fn a_session_ends_with_its_device() {
        let sessions = WebSessions::default();
        let (local, phone) = (
            sessions.mint(Credential::Local),
            sessions.mint(device("phone")),
        );
        let revoked = |c: &Credential| *c == Credential::Local;
        assert_eq!(sessions.credential(&phone, revoked), None);
        // Gone for good, even if a device of that name and hash came back.
        assert_eq!(sessions.credential(&phone, |_| true), None);
        assert_eq!(
            sessions.credential(&local, revoked),
            Some(Credential::Local)
        );
    }

    #[test]
    fn an_idle_session_ends() {
        let sessions = WebSessions::with_idle(Duration::from_millis(40));
        let session = sessions.mint(Credential::Local);
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            sessions.credential(&session, |_| true).is_some(),
            "use keeps it"
        );
        std::thread::sleep(Duration::from_millis(25));
        assert!(sessions.credential(&session, |_| true).is_some());
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(sessions.credential(&session, |_| true), None);
    }
}
