use composer_protocol::{ServerMessage, TurnState};

use crate::actions::PendingApprovals;
use crate::hub::EventHub;
use crate::session::Session;

/// An approval request that is open. Dropping it, whether the wait ended or the waiting future
/// was dropped (an HTTP client that went away), closes the request: clients stop showing it and
/// the session leaves `needs approval`.
pub struct ApprovalWait<'a> {
    pending: &'a PendingApprovals,
    hub: &'a EventHub,
    session: &'a Session,
    request: String,
    after: TurnState,
    approved: bool,
}

impl<'a> ApprovalWait<'a> {
    pub fn new(
        pending: &'a PendingApprovals,
        hub: &'a EventHub,
        session: &'a Session,
        request: String,
        after: TurnState,
    ) -> Self {
        Self {
            pending,
            hub,
            session,
            request,
            after,
            approved: false,
        }
    }

    /// What `ApprovalResolved` reports when this is dropped.
    pub fn set_approved(&mut self, approved: bool) {
        self.approved = approved;
    }
}

impl Drop for ApprovalWait<'_> {
    fn drop(&mut self) {
        self.pending.remove(&self.request);
        self.hub.send(ServerMessage::ApprovalResolved {
            request: self.request.clone(),
            session: self.session.id().to_string(),
            approved: self.approved,
        });
        let after = self.after;
        self.session
            .update_info(self.hub, |info| info.state = after);
    }
}
