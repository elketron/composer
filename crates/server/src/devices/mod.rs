//! Remote access: paired devices, each with its own token.

#[allow(clippy::module_inception)]
mod devices;
mod types;

pub use devices::Devices;
pub use types::{Device, PairingCode};
