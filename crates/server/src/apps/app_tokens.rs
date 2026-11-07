use std::collections::HashMap;
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use crate::apps::{AppGrant, ProjectName};

/// The tokens apps were opened with (`POST /api/projects/<p>/apps/<site>/open`), by their hash:
/// one per opening, kept in memory (a restart revokes them all; a client opens the app again).
#[derive(Default)]
pub struct AppTokens {
    grants: Mutex<HashMap<[u8; 32], AppGrant>>,
}

/// Every app token starts with this, so logs and errors can tell it from a workspace token.
const PREFIX: &str = "app_";

impl AppTokens {
    /// A new token for `grant`.
    pub fn mint(&self, grant: AppGrant) -> String {
        let token = format!(
            "{PREFIX}{}",
            (0..4)
                .map(|_| format!("{:016x}", rand::random::<u64>()))
                .collect::<String>()
        );
        self.grants
            .lock()
            .expect("app tokens lock")
            .insert(hash(&token), grant);
        token
    }

    /// What `token` grants, if it is one of ours.
    pub fn grant(&self, token: &str) -> Option<AppGrant> {
        if !token.starts_with(PREFIX) {
            return None;
        }
        self.grants
            .lock()
            .expect("app tokens lock")
            .get(&hash(token))
            .cloned()
    }

    /// Revokes every token of `site`'s app on `project`; the number revoked.
    pub fn revoke(&self, project: &ProjectName, site: &str) -> usize {
        let mut grants = self.grants.lock().expect("app tokens lock");
        let before = grants.len();
        grants.retain(|_, g| !(g.project == *project && g.site == site));
        before - grants.len()
    }
}

fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(project: &str, site: &str) -> AppGrant {
        AppGrant {
            project: ProjectName::new(project).unwrap(),
            folder: format!("/p/{project}").into(),
            site: site.into(),
        }
    }

    #[test]
    fn minted_tokens_grant_until_revoked() {
        let tokens = AppTokens::default();
        let cad = tokens.mint(grant("game", "cad"));
        let video = tokens.mint(grant("game", "video"));
        assert_ne!(cad, video);
        assert_eq!(tokens.grant(&cad), Some(grant("game", "cad")));
        assert_eq!(tokens.grant("app_forged"), None);
        assert_eq!(tokens.grant(cad.trim_start_matches("app_")), None);
        assert_eq!(tokens.revoke(&ProjectName::new("game").unwrap(), "cad"), 1);
        assert_eq!(tokens.grant(&cad), None);
        assert!(tokens.grant(&video).is_some());
    }
}
