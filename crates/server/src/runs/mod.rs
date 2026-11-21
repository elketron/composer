//! Runs started from outside a conversation: by a webhook (`[hooks.<name>]`, `POST
//! /api/hooks/<name>`) or by another session (`spawn_run`). Either is a run of any kind, stored
//! as a session or not.

mod hook_config;
mod run_request;
mod stored_run;

pub use hook_config::HookConfig;
pub use run_request::RunRequest;
pub use stored_run::{StoredRun, tell_parent};
