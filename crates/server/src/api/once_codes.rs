use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::api::Credential;

/// How long a code waits to be used.
const LIFETIME: Duration = Duration::from_secs(60);

/// One-time codes that open a client app (ticket 0168): its URL carries `?once=<code>`, which the
/// workspace exchanges on first use for a browser session of the credential that asked for the
/// code ([`crate::api::WebSessions`]), so no token sits in an app's URL (or history) or in a
/// cookie. Used once, or gone after a minute. Kept by their hashes.
#[derive(Default)]
pub struct OnceCodes {
    codes: Mutex<HashMap<[u8; 32], (Instant, Credential)>>,
}

impl OnceCodes {
    /// A new code, asked for by `minter`: what redeeming it gives is `minter`'s kind of access.
    pub fn mint(&self, minter: Credential) -> String {
        let code = format!(
            "{:016x}{:016x}",
            rand::random::<u64>(),
            rand::random::<u64>()
        );
        let mut codes = self.codes.lock().expect("once codes lock");
        codes.retain(|_, (made, _)| made.elapsed() < LIFETIME);
        codes.insert(hash(&code), (Instant::now(), minter));
        code
    }

    /// Uses `code`: who asked for it, once, if it is ours and fresh.
    pub fn take(&self, code: &str) -> Option<Credential> {
        let (made, minter) = self
            .codes
            .lock()
            .expect("once codes lock")
            .remove(&hash(code))?;
        (made.elapsed() < LIFETIME).then_some(minter)
    }
}

fn hash(code: &str) -> [u8; 32] {
    Sha256::digest(code.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_works_once_and_remembers_who_asked() {
        let codes = OnceCodes::default();
        let code = codes.mint(Credential::Local);
        assert_eq!(codes.take(&code), Some(Credential::Local));
        assert_eq!(codes.take(&code), None);
        assert_eq!(codes.take("nope"), None);
        let phone = Credential::Device {
            name: "phone".into(),
            token_hash: "h".into(),
        };
        let code = codes.mint(phone.clone());
        assert_eq!(codes.take(&code), Some(phone));
    }
}
