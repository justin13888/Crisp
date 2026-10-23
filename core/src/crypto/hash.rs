//! SHA-256 hashing.
//!
//! SHA-256 is the single hash primitive used throughout the Crisp format
//! (spec decision D4): content hashes, key thumbprints, blob ids, and the
//! domain-separated identifier digests are all 256-bit.

use sha2::{Digest, Sha256};

/// The length of a SHA-256 digest, in bytes.
pub const SHA256_LEN: usize = 32;

/// Computes the SHA-256 digest of `data`.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; SHA256_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders a digest as lowercase hex for comparison against known vectors.
    fn hex(digest: &[u8; SHA256_LEN]) -> String {
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn matches_known_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        );
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(sha256(b"crisp"), sha256(b"crisp"));
        assert_ne!(sha256(b"crisp"), sha256(b"criss"));
    }
}
