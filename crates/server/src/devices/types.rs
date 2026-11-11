use serde::{Deserialize, Serialize};

/// A paired device (`state/devices.json`). Only the token's hash is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    pub token_hash: String,
    pub paired_at: String,
}

/// An open pairing code (`state/pairing.json`), stored hashed, valid until `expires` or until too
/// many wrong codes were tried while it was open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairingCode {
    pub code_hash: String,
    pub name: String,
    /// RFC 3339.
    pub expires: String,
    /// Wrong codes tried while this one was open.
    #[serde(default)]
    pub failures: u32,
}
