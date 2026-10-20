//! Files attached to messages: uploaded per session to
//! `state/sessions/<id>/attachments/<hash>-<name>`, referred to by messages (and tickets), and
//! shown to the model as images or (text, PDF) as marked, untrusted text.

mod classify;
mod for_model;
mod image_prep;
mod store;
mod ticket_files;

pub use classify::{AttachmentKind, classify};
pub use for_model::{ATTACHMENT_SCHEME, ForModel, attachment_data_url, for_model};
pub use image_prep::prepare_image;
pub use store::{folder, resolve, sanitize_name, save};
pub use ticket_files::{TicketFile, ticket_files};
