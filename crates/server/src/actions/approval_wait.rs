use composer_protocol::ServerMessage;

use crate::server::Server;
use crate::session::Session;

/// An approval request that is open, from the moment the session shows `needs approval`, through
/// `[system1]` or the orchestrator deciding it and any question to the clients. Dropping it,
/// whether the wait ended or the waiting future was dropped (an HTTP client that went away),
/// closes the request: nobody can answer it any more, clients stop showing it, and the session
/// leaves `needs approval` once it has no other request open.
pub struct ApprovalWait<'a> {
    server: &'a Server,
    session: &'a Session,
    request: String,
    /// Clients were shown the request.
    announced: bool,
    approved: bool,
}

impl<'a> ApprovalWait<'a> {
    pub fn new(server: &'a Server, session: &'a Session, request: String) -> Self {
        session.approval_opened(&server.hub);
        Self {
            server,
            session,
            request,
            announced: false,
            approved: false,
        }
    }

    /// Clients are shown the request from here on: dropping this tells them it closed.
    pub fn set_announced(&mut self) {
        self.announced = true;
    }

    /// What `ApprovalResolved` reports when this is dropped.
    pub fn set_approved(&mut self, approved: bool) {
        self.approved = approved;
    }
}

impl Drop for ApprovalWait<'_> {
    fn drop(&mut self) {
        self.server.worker_approvals.forget(&self.request);
        self.server.pending_approvals.remove(&self.request);
        if self.announced {
            self.server.hub.send(ServerMessage::ApprovalResolved {
                request: self.request.clone(),
                session: self.session.id().to_string(),
                approved: self.approved,
            });
        }
        self.session.approval_closed(&self.server.hub);
    }
}
