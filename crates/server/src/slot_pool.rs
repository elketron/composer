use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use composer_protocol::ServerMessage;
use tokio::sync::Semaphore;

use crate::hub::EventHub;
use crate::slot_guard::SlotGuard;
use crate::worker::ParentLink;

/// Limits concurrent model requests to the provider's parallelism.
/// Every change is broadcast as a `slots` event for the status bar.
/// In a worker process the slots are borrowed from the parent server instead.
#[derive(Debug)]
pub struct SlotPool {
    semaphore: Arc<Semaphore>,
    total: u32,
    waiting: AtomicU32,
    hub: EventHub,
    remote: Option<Arc<ParentLink>>,
}

impl SlotPool {
    pub fn new(total: u32, hub: EventHub) -> Arc<Self> {
        Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(total.max(1) as usize)),
            total: total.max(1),
            waiting: AtomicU32::new(0),
            hub,
            remote: None,
        })
    }

    /// Slots granted by the parent server (worker processes).
    pub fn remote(link: Arc<ParentLink>, hub: EventHub) -> Arc<Self> {
        Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(1)),
            total: 1,
            waiting: AtomicU32::new(0),
            hub,
            remote: Some(link),
        })
    }

    /// Waits for a free slot. The slot is released when the guard drops.
    pub async fn acquire(self: &Arc<Self>) -> Result<SlotGuard, String> {
        if let Some(link) = &self.remote {
            let grant = link.acquire_slot().await?;
            return Ok(SlotGuard::remote(grant, link.clone()));
        }
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
        Ok(SlotGuard::new(permit, self.clone()))
    }

    /// Whether `acquire` would wait right now.
    pub fn is_full(&self) -> bool {
        self.remote.is_none() && self.semaphore.available_permits() == 0
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
