//! Composite secret keys, derived deterministically from a seed.

use ed25519_dalek::SigningKey as Ed25519SigningKey;
use ml_dsa::{B32, Keypair, MlDsa65, SigningKey as MlDsaSigningKey};
use zeroize::Zeroize;

use crate::crypto::hash::sha256;
use crate::crypto::kdf::derive_seed;
use crate::error::Result;
use crate::identity::derive_user_id;
use crate::model::ids::UserId;
use crate::model::meta::PublicKeySet;

/// Domain separator for the Ed25519 sub-seed.
const ED25519_SUBSEED_DOMAIN: &[u8] = b"crisp/key/ed25519/1";
/// Domain separator for the ML-DSA-65 sub-seed.
const ML_DSA_SUBSEED_DOMAIN: &[u8] = b"crisp/key/ml-dsa-65/1";

/// An author's composite secret key — the ML-DSA-65 and Ed25519 signing keys.
///
/// Holds live secret material. The two component signing keys are derived
/// from a single 32-byte identity seed via domain-separated sub-seeds; the
/// intermediate sub-seeds are zeroized immediately after use.
pub struct SecretKeySet {
    /// The ML-DSA-65 signing key.
    pub(crate) ml_dsa: MlDsaSigningKey<MlDsa65>,
    /// The Ed25519 signing key.
    pub(crate) ed25519: Ed25519SigningKey,
}

/// Computes a domain-separated 32-byte sub-seed from the identity seed.
fn sub_seed(domain: &[u8], seed: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(domain.len() + seed.len());
    buf.extend_from_slice(domain);
    buf.extend_from_slice(seed);
    let digest = sha256(&buf);
    buf.zeroize();
    digest
}

/// Derives a [`SecretKeySet`] deterministically from a 32-byte identity seed.
///
/// The same seed always yields the same composite keypair.
#[must_use]
pub fn derive_keyset(seed: &[u8; 32]) -> SecretKeySet {
    let mut ed_seed = sub_seed(ED25519_SUBSEED_DOMAIN, seed);
    let mut ml_dsa_seed = sub_seed(ML_DSA_SUBSEED_DOMAIN, seed);

    let ed25519 = Ed25519SigningKey::from_bytes(&ed_seed);
    let ml_dsa = MlDsaSigningKey::<MlDsa65>::from_seed(&B32::from(ml_dsa_seed));

    ed_seed.zeroize();
    ml_dsa_seed.zeroize();
    SecretKeySet { ml_dsa, ed25519 }
}

/// Derives a [`SecretKeySet`] from an account username and passphrase.
///
/// This composes the Argon2id key-derivation function ([`derive_seed`]) with
/// [`derive_keyset`], so an author's cryptographic identity is reconstructed
/// from credentials alone — and is cryptographically bound to the username.
///
/// # Errors
///
/// Returns an error if either credential is empty or key derivation fails.
pub fn keyset_from_passphrase(username: &str, passphrase: &str) -> Result<SecretKeySet> {
    let mut seed = derive_seed(username, passphrase)?;
    let keyset = derive_keyset(&seed);
    seed.zeroize();
    Ok(keyset)
}

impl SecretKeySet {
    /// Returns the public [`PublicKeySet`] for this secret key.
    #[must_use]
    pub fn public_keyset(&self) -> PublicKeySet {
        PublicKeySet {
            ed25519: self.ed25519.verifying_key().to_bytes().to_vec(),
            ml_dsa_65: self.ml_dsa.verifying_key().encode().as_slice().to_vec(),
        }
    }

    /// Returns the `usr…` identity this key owns.
    ///
    /// # Errors
    ///
    /// Returns an error if id derivation fails.
    pub fn user_id(&self) -> Result<UserId> {
        derive_user_id(&self.public_keyset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivation_is_deterministic() {
        let seed = [3u8; 32];
        let a = derive_keyset(&seed).public_keyset();
        let b = derive_keyset(&seed).public_keyset();
        assert_eq!(a, b);
        assert_eq!(a.ed25519.len(), 32);
        assert_eq!(a.ml_dsa_65.len(), 1952);
    }

    #[test]
    fn distinct_seeds_yield_distinct_keys() {
        let a = derive_keyset(&[1u8; 32]).public_keyset();
        let b = derive_keyset(&[2u8; 32]).public_keyset();
        assert_ne!(a, b);
    }

    #[test]
    fn passphrase_derivation_is_deterministic_and_bound_to_credentials() {
        let a = keyset_from_passphrase("alice", "passphrase").expect("derive");
        let b = keyset_from_passphrase("alice", "passphrase").expect("derive");
        assert_eq!(a.public_keyset(), b.public_keyset());
        let other = keyset_from_passphrase("bob", "passphrase").expect("derive");
        assert_ne!(a.public_keyset(), other.public_keyset());
    }

    #[test]
    fn user_id_is_derivable() {
        let keyset = derive_keyset(&[5u8; 32]);
        assert!(keyset.user_id().expect("derive").0.starts_with("usr1"));
    }
}
