use std::sync::Arc;

use tokio::sync::OwnedSemaphorePermit;

use crate::slot_pool::SlotPool;
use crate::worker::ParentLink;

/// A held model slot. Dropping it frees the slot: locally (and broadcasts the new
/// usage), or by telling the parent server in a worker process.
#[derive(Debug)]
pub struct SlotGuard {
    permit: Option<OwnedSemaphorePermit>,
    pool: Option<Arc<SlotPool>>,
    remote: Option<(u64, Arc<ParentLink>)>,
}

impl SlotGuard {
    pub(crate) fn new(permit: OwnedSemaphorePermit, pool: Arc<SlotPool>) -> Self {
        Self {
            permit: Some(permit),
            pool: Some(pool),
            remote: None,
        }
    }

    pub(crate) fn remote(grant: u64, link: Arc<ParentLink>) -> Self {
        Self {
            permit: None,
            pool: None,
            remote: Some((grant, link)),
        }
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        drop(self.permit.take());
        if let Some(pool) = &self.pool {
            pool.broadcast();
        }
        if let Some((grant, link)) = self.remote.take() {
            link.release_slot(grant);
        }
    }
}
