//! A message the chat sends: what [`crate::chat_model::ChatModel`] answers a send with.

/// A message to send to `session`.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub session: String,
    pub text: String,
}
