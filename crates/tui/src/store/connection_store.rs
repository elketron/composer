use std::collections::HashMap;

use composer_protocol::ClientRequest;

use crate::effect::Effect;
use crate::store::types::{ConnState, Outgoing, Pending};

/// Server connection, request ids, model slots.
#[derive(Debug)]
pub struct ConnectionStore {
    pub state: ConnState,
    pub slots_used: u32,
    pub slots_total: u32,
    pub slots_waiting: u32,
    /// Compaction threshold, for `ctx 38K/160K`.
    pub context_limit: u32,
    /// Where apps are served (`http://host:port`), and the token that opens them.
    pub server_url: String,
    pub token: String,
    next_id: u64,
    pending: HashMap<u64, Pending>,
}

impl ConnectionStore {
    pub fn new(context_limit: u32) -> Self {
        Self {
            state: ConnState::Connecting,
            slots_used: 0,
            slots_total: 0,
            slots_waiting: 0,
            context_limit,
            server_url: String::new(),
            token: String::new(),
            next_id: 0,
            pending: HashMap::new(),
        }
    }

    /// Assigns an id and remembers what the reply will mean.
    pub fn request(&mut self, outgoing: Outgoing) -> Effect {
        self.next_id += 1;
        self.pending.insert(self.next_id, outgoing.pending);
        Effect::Send(ClientRequest {
            id: self.next_id,
            command: outgoing.command,
        })
    }

    pub fn take_pending(&mut self, id: u64) -> Pending {
        self.pending.remove(&id).unwrap_or(Pending::Ignore)
    }

    pub fn is_connected(&self) -> bool {
        self.state == ConnState::Connected
    }
}
