//! Composite secret keys and the identity seed they derive from.

use ed25519_dalek::SigningKey as Ed25519SigningKey;
use ml_dsa::{B32, Keypair, MlDsa65, SigningKey as MlDsaSigningKey};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::crypto::hash::sha256;
use crate::error::{CrispError, Result};
use crate::identity::{derive_user_id, key_fingerprint};
use crate::model::ids::UserId;
use crate::model::meta::PublicKeySet;

/// Domain separator for the Ed25519 sub-seed.
const ED25519_SUBSEED_DOMAIN: &[u8] = b"crisp/key/ed25519/1";
/// Domain separator for the ML-DSA-65 sub-seed.
const ML_DSA_SUBSEED_DOMAIN: &[u8] = b"crisp/key/ml-dsa-65/1";

/// The length of an identity seed, in bytes.
pub const SEED_LEN: usize = 32;

/// A 32-byte identity seed — the single secret an identity is made of.
///
/// It is drawn from the system CSPRNG ([`IdentitySeed::generate`]), stored
/// only inside a passphrase-wrapped key file ([`crate::keystore`]), and wiped
/// from memory on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct IdentitySeed([u8; SEED_LEN]);

impl IdentitySeed {
    /// Draws a fresh seed from the system CSPRNG.
    ///
    /// # Errors
    ///
    /// Returns [`CrispError::Crypto`] if the system RNG is unavailable.
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; SEED_LEN];
        getrandom::fill(&mut seed)
            .map_err(|e| CrispError::crypto(format!("system RNG unavailable: {e}")))?;
        Ok(Self(seed))
    }

    /// Wraps existing seed bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; SEED_LEN]) -> Self {
        Self(bytes)
    }

    /// The raw seed bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SEED_LEN] {
        &self.0
    }

    /// Expands this seed into its composite signing key.
    #[must_use]
    pub fn keyset(&self) -> SecretKeySet {
        SecretKeySet::from_seed(&self.0)
    }
}

impl core::fmt::Debug for IdentitySeed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentitySeed(<redacted>)")
    }
}

/// A composite secret key — the ML-DSA-65 and Ed25519 signing keys.
///
/// Both component keys wipe themselves on drop (`ml-dsa`'s `zeroize` feature
/// and `ed25519-dalek`'s default `zeroize`), so this type needs no `Drop` of
/// its own.
pub struct SecretKeySet {
    /// The ML-DSA-65 signing key.
    pub(crate) ml_dsa: MlDsaSigningKey<MlDsa65>,
    /// The Ed25519 signing key.
    pub(crate) ed25519: Ed25519SigningKey,
}

/// Computes a domain-separated 32-byte sub-seed from the identity seed.
fn sub_seed(domain: &[u8], seed: &[u8; SEED_LEN]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(domain.len() + seed.len());
    buf.extend_from_slice(domain);
    buf.extend_from_slice(seed);
    let digest = sha256(&buf);
    buf.zeroize();
    digest
}

impl SecretKeySet {
    /// Derives the composite key deterministically from a 32-byte identity
    /// seed via two domain-separated sub-seeds.
    #[must_use]
    pub fn from_seed(seed: &[u8; SEED_LEN]) -> Self {
        let mut ed_seed = sub_seed(ED25519_SUBSEED_DOMAIN, seed);
        let mut ml_dsa_seed = sub_seed(ML_DSA_SUBSEED_DOMAIN, seed);
        let keyset = Self::from_components(&ml_dsa_seed, &ed_seed);
        ed_seed.zeroize();
        ml_dsa_seed.zeroize();
        keyset
    }

    /// Builds the composite key from its two component secrets: the 32-byte
    /// ML-DSA-65 seed `ξ` (FIPS 204) and the 32-byte Ed25519 secret key — the
    /// layout of a draft composite private key.
    #[must_use]
    pub fn from_components(ml_dsa_seed: &[u8; 32], ed25519_secret: &[u8; 32]) -> Self {
        Self {
            ml_dsa: MlDsaSigningKey::<MlDsa65>::from_seed(&B32::from(*ml_dsa_seed)),
            ed25519: Ed25519SigningKey::from_bytes(ed25519_secret),
        }
    }

    /// Returns the public [`PublicKeySet`] for this secret key.
    #[must_use]
    pub fn public_keyset(&self) -> PublicKeySet {
        PublicKeySet {
            ed25519: self.ed25519.verifying_key().to_bytes().to_vec(),
            ml_dsa_65: self.ml_dsa.verifying_key().encode().as_slice().to_vec(),
        }
    }

    /// Returns the 32-byte fingerprint of this key's public half.
    ///
    /// # Errors
    ///
    /// Returns an error if CBOR encoding fails.
    pub fn fingerprint(&self) -> Result<[u8; 32]> {
        key_fingerprint(&self.public_keyset())
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
        let a = SecretKeySet::from_seed(&seed).public_keyset();
        let b = SecretKeySet::from_seed(&seed).public_keyset();
        assert_eq!(a, b);
        assert_eq!(a.ed25519.len(), 32);
        assert_eq!(a.ml_dsa_65.len(), 1952);
    }

    #[test]
    fn distinct_seeds_yield_distinct_keys() {
        let a = SecretKeySet::from_seed(&[1u8; 32]).public_keyset();
        let b = SecretKeySet::from_seed(&[2u8; 32]).public_keyset();
        assert_ne!(a, b);
    }

    #[test]
    fn generated_seeds_are_distinct_and_redacted() {
        let a = IdentitySeed::generate().expect("rng");
        let b = IdentitySeed::generate().expect("rng");
        assert_ne!(a.as_bytes(), b.as_bytes());
        assert_eq!(format!("{a:?}"), "IdentitySeed(<redacted>)");
    }

    #[test]
    fn user_id_embeds_the_fingerprint() {
        let keyset = SecretKeySet::from_seed(&[5u8; 32]);
        let uid = keyset.user_id().expect("derive");
        assert!(uid.0.starts_with("usr1"));
        let (_, digest) = crate::crypto::decode_id(&uid.0).expect("decode");
        assert_eq!(digest, keyset.fingerprint().expect("fpr"));
    }
}
