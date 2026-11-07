use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a code waits to be used.
const LIFETIME: Duration = Duration::from_secs(60);

/// One-time codes that open a client app (ticket 0168): its URL carries `?once=<code>`, which the
/// workspace exchanges for its cookie on first use, so the workspace token itself never sits in
/// an app's URL (or history). Used once, or gone after a minute.
#[derive(Default)]
pub struct OnceCodes {
    codes: Mutex<HashMap<String, Instant>>,
}

impl OnceCodes {
    pub fn mint(&self) -> String {
        let code = format!(
            "{:016x}{:016x}",
            rand::random::<u64>(),
            rand::random::<u64>()
        );
        let mut codes = self.codes.lock().expect("once codes lock");
        codes.retain(|_, made| made.elapsed() < LIFETIME);
        codes.insert(code.clone(), Instant::now());
        code
    }

    /// Uses `code`: `true` once, if it is ours and fresh.
    pub fn take(&self, code: &str) -> bool {
        let made = self.codes.lock().expect("once codes lock").remove(code);
        made.is_some_and(|made| made.elapsed() < LIFETIME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_works_once() {
        let codes = OnceCodes::default();
        let code = codes.mint();
        assert!(codes.take(&code));
        assert!(!codes.take(&code));
        assert!(!codes.take("nope"));
    }
}
