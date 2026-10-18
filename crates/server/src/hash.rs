use sha2::{Digest, Sha256};

/// Hex SHA-256 of `content`. Used for page ETags and to tell our own writes from external ones.
pub fn content_hash(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
