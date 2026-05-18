//! Passphrase-based key derivation.
//!
//! An author's cryptographic identity is derived deterministically from a
//! human-friendly **account username** and **passphrase**: no key files to
//! manage. [`derive_seed`] runs Argon2id over the passphrase, using a salt
//! bound to the username, and produces a 32-byte seed. The `identity` module
//! expands that seed into the Composite ML-DSA keypair.
//!
//! The Argon2 parameters below are part of the identity contract: changing
//! any of them changes every derived key, so they are pinned constants.

use argon2::{Algorithm, Argon2, Params, Version};

use crate::crypto::hash::sha256;
use crate::error::{CrispError, Result};

/// Argon2id memory cost, in KiB (19 MiB) — chosen to be tolerable in a
/// browser tab while remaining memory-hard.
pub const ARGON2_M_COST_KIB: u32 = 19_456;
/// Argon2id time cost (number of passes).
pub const ARGON2_T_COST: u32 = 2;
/// Argon2id parallelism (lanes).
pub const ARGON2_P_COST: u32 = 1;
/// The length of the derived seed, in bytes.
pub const SEED_LEN: usize = 32;

/// Domain separator mixed into the username-bound salt.
const SALT_DOMAIN: &str = "crisp/identity-salt/1";
/// The Argon2 salt length, in bytes.
const SALT_LEN: usize = 16;

/// Derives the 32-byte identity seed for `(username, passphrase)`.
///
/// The result is fully deterministic: the same pair always yields the same
/// seed, on any platform. The salt is `SHA-256("crisp/identity-salt/1" ||
/// username)`, truncated to 16 bytes, which cryptographically binds the
/// derived identity to the account username.
///
/// Callers should pass an already-normalised (NFC, trimmed) username so that
/// Unicode-equivalent spellings derive the same identity.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if either argument is empty, or
/// [`CrispError::Crypto`] if Argon2 derivation fails.
pub fn derive_seed(username: &str, passphrase: &str) -> Result<[u8; SEED_LEN]> {
    if username.is_empty() {
        return Err(CrispError::input("username must not be empty"));
    }
    if passphrase.is_empty() {
        return Err(CrispError::input("passphrase must not be empty"));
    }

    let mut salt_input = Vec::with_capacity(SALT_DOMAIN.len() + username.len());
    salt_input.extend_from_slice(SALT_DOMAIN.as_bytes());
    salt_input.extend_from_slice(username.as_bytes());
    let salt_digest = sha256(&salt_input);
    let salt = &salt_digest[..SALT_LEN];

    let params = Params::new(
        ARGON2_M_COST_KIB,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(SEED_LEN),
    )
    .map_err(|e| CrispError::crypto(format!("invalid Argon2 parameters: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut seed = [0u8; SEED_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut seed)
        .map_err(|e| CrispError::crypto(format!("Argon2 derivation failed: {e}")))?;
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic_for_the_same_credentials() {
        let a = derive_seed("alice", "correct horse battery staple").expect("derive");
        let b = derive_seed("alice", "correct horse battery staple").expect("derive");
        assert_eq!(a, b);
    }

    #[test]
    fn a_different_username_changes_the_seed() {
        let a = derive_seed("alice", "same passphrase").expect("derive");
        let b = derive_seed("bob", "same passphrase").expect("derive");
        assert_ne!(a, b, "the salt binds the seed to the username");
    }

    #[test]
    fn a_different_passphrase_changes_the_seed() {
        let a = derive_seed("alice", "passphrase one").expect("derive");
        let b = derive_seed("alice", "passphrase two").expect("derive");
        assert_ne!(a, b);
    }

    #[test]
    fn empty_inputs_are_rejected() {
        assert!(derive_seed("", "passphrase").is_err());
        assert!(derive_seed("alice", "").is_err());
    }
}
