use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use composer_protocol::ServerMessage;
use tokio::sync::Semaphore;

use crate::hub::EventHub;
use crate::slot_guard::SlotGuard;

/// Limits concurrent model requests to the provider's parallelism.
/// Every change is broadcast as a `slots` event for the status bar.
#[derive(Debug)]
pub struct SlotPool {
    semaphore: Arc<Semaphore>,
    total: u32,
    waiting: AtomicU32,
    hub: EventHub,
}

impl SlotPool {
    pub fn new(total: u32, hub: EventHub) -> Arc<Self> {
        Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(total.max(1) as usize)),
            total: total.max(1),
            waiting: AtomicU32::new(0),
            hub,
        })
    }

    /// Waits for a free slot. The slot is released when the guard drops.
    pub async fn acquire(self: &Arc<Self>) -> SlotGuard {
        self.waiting.fetch_add(1, Ordering::SeqCst);
        self.broadcast();
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore never closes");
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        self.broadcast();
        SlotGuard::new(permit, self.clone())
    }

    /// Whether `acquire` would wait right now.
    pub fn is_full(&self) -> bool {
        self.semaphore.available_permits() == 0
    }

    pub fn broadcast(&self) {
        self.hub.send(self.status());
    }

    pub fn status(&self) -> ServerMessage {
        ServerMessage::Slots {
            used: self.total - self.semaphore.available_permits() as u32,
            total: self.total,
            waiting: self.waiting.load(Ordering::SeqCst),
        }
    }
}
