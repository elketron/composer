//! The gate that keeps an unapproved `folder:` from counting.
//!
//! A site's `index.md` is a page an agent can write, and its `folder:` decides where terminals,
//! background processes, recipes and outside agents start, what the changes view diffs and where
//! a mount may point. So it is asked about the way a site's `mcp.toml` is: the same request, the
//! same store, keyed by a hash of the folder ([`crate::agent::folder_hash`]). Only a yes is
//! remembered; a refused or unanswered folder is asked about again the next time the site is set,
//! and a `folder:` naming another folder is a new hash, so it asks again too (approving it forgets
//! the old one). The question goes to the user's clients only; nothing answers it for them.

use std::path::Path;
use std::sync::Arc;

use crate::actions::Ask;
use tokio_util::sync::CancellationToken;

use crate::agent::{FOLDER_APPROVAL, FolderNote, Site, folder_hash};
use crate::server::Server;
use crate::session::Session;

/// Whether `site`'s `folder:` counts, asking the user first (in `session`) when that folder was
/// never approved for the site. Without a session (a run that is not stored) nobody can be asked:
/// only an earlier approval counts. A yes makes the folder a place mounts may point into.
pub async fn ensure_folder_approved(
    server: &Arc<Server>,
    session: Option<&Session>,
    site: &Site,
    cancel: &CancellationToken,
) -> FolderNote {
    // None named, or not there: nothing to approve, and nothing works in it either way.
    let Ok(folder) = site.declared_folder() else {
        return FolderNote::default();
    };
    let hash = folder_hash(&folder);
    let note = |approved: bool| FolderNote {
        folder: Some(folder.clone()),
        approved,
    };
    if server
        .approvals
        .is_approved(&site.name, FOLDER_APPROVAL, &hash)
    {
        return note(true);
    }
    let Some(session) = session else {
        return note(false);
    };
    let ask = Ask::trust(
        &site.name,
        FOLDER_APPROVAL,
        vec![folder.display().to_string()],
        asking(&site.name, &folder),
        &hash,
    );
    let yes = server
        .ask(session, ask, cancel)
        .await
        .is_some_and(|answer| answer.approved());
    if yes {
        if let Err(e) = server.approvals.approve(&site.name, FOLDER_APPROVAL, &hash) {
            tracing::warn!("remembering that {}'s folder is approved: {e}", site.name);
        }
        // Mounts may point into it from now on, and their pages are searchable.
        server.reindex();
    }
    note(yes)
}

/// What the user reads: the site, the absolute folder, and what approving lets happen there.
fn asking(site: &str, folder: &Path) -> String {
    format!(
        "site://{site} names {} as its folder (`folder:` in its index.md).\n\n\
         Once you approve, sessions in the site work there: terminals and background processes \
         start in it, recipes get it as COMPOSER_FOLDER, outside agents start in it, the changes \
         view diffs it, `open` may read its images, and a symlink inside a site may point into \
         it. Until then the site works as if it named no folder. The approval holds until \
         `folder:` names another folder.",
        folder.display()
    )
}
