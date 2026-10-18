use crate::store::types::PendingApproval;

/// Recipes waiting for approval, oldest first. The first one is shown.
#[derive(Debug, Default)]
pub struct ApprovalStore {
    pending: Vec<PendingApproval>,
}

impl ApprovalStore {
    pub fn add(&mut self, approval: PendingApproval) {
        if !self.pending.iter().any(|p| p.request == approval.request) {
            self.pending.push(approval);
        }
    }

    pub fn resolve(&mut self, request: &str) {
        self.pending.retain(|p| p.request != request);
    }

    pub fn current(&self) -> Option<&PendingApproval> {
        self.pending.first()
    }

    pub fn count(&self) -> usize {
        self.pending.len()
    }
}
