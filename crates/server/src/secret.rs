//! Comparing secrets (tokens, codes and their hashes) without telling, through how long it takes,
//! how much of a guess was right.

use subtle::ConstantTimeEq;

/// Whether `a` and `b` are equal, in time that depends only on their lengths.
pub(crate) fn secret_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::secret_eq;

    #[test]
    fn equal_only_when_every_byte_is() {
        assert!(secret_eq("", ""));
        assert!(secret_eq("abc123", "abc123"));
        assert!(!secret_eq("abc123", "abc124"));
        assert!(!secret_eq("abc123", "xbc123"));
        assert!(!secret_eq("abc", "abc1"));
        assert!(!secret_eq("abc1", "abc"));
        assert!(!secret_eq("", "a"));
    }
}
