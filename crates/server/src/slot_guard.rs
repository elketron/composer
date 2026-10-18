use std::sync::Arc;

use tokio::sync::OwnedSemaphorePermit;

use crate::slot_pool::SlotPool;

/// A held model slot. Dropping it frees the slot and broadcasts the new usage.
#[derive(Debug)]
pub struct SlotGuard {
    permit: Option<OwnedSemaphorePermit>,
    pool: Arc<SlotPool>,
}

impl SlotGuard {
    pub(crate) fn new(permit: OwnedSemaphorePermit, pool: Arc<SlotPool>) -> Self {
        Self {
            permit: Some(permit),
            pool,
        }
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        drop(self.permit.take());
        self.pool.broadcast();
    }
}
