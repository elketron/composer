//! A message the chat sends: what [`crate::chat_model::ChatModel`] answers a send or a Pass
//! with, and what comes back to it when the send fails.

/// A message to send to `session`. `review` is set when this send put the review's verdict in
/// the outbox: a failure takes that entry out again (in `session`, whichever is shown by then).
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub session: String,
    pub text: String,
    pub review: Option<String>,
}
