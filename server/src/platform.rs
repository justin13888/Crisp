//! The platform endorsement keys.
//!
//! Every published bundle carries exactly one server endorsement (spec D6).
//! The server holds one current Composite ML-DSA key, derived
//! deterministically from a 32-byte secret seed, and signs that endorsement
//! with it. It may also hold retired keys, each with a `valid_until`, which
//! it publishes after the current key and which keep their endorsements
//! verifying. Every held key signs the published key set once at startup, so
//! a client that pinned any of them as current accepts the set (spec §8.5).

use crisp_core::Result;
use crisp_core::author::endorse;
use crisp_core::model::envelope::CrispEnvelope;
use crisp_core::model::meta::PublicKeySet;
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::{PlatformKey as TrustedKey, sign_key_set};

use crate::config::RetiredKey;

/// A published platform key: its `usr…` id and its trusted window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedKey {
    /// The key's `usr…` id.
    pub usr_id: String,
    /// The key and its validity window.
    pub key: TrustedKey,
}

/// A signature over the published key set by one held key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySetSignature {
    /// The `usr…` id of the signing key.
    pub usr_id: String,
    /// The Composite ML-DSA signature.
    pub sig: Vec<u8>,
}

/// The server's endorsement keys, their published set, and its signatures.
pub struct PlatformKey {
    /// The live secret key set of the current key.
    secret: SecretKeySet,
    /// The current key as published, kept apart so reading it cannot fail.
    current: PublishedKey,
    /// The published keys, current first, derived once at construction so
    /// nothing about them can fail (or silently degrade) at request time.
    published: Vec<PublishedKey>,
    /// The key-set signatures, one per held key, in published order.
    signatures: Vec<KeySetSignature>,
}

impl std::fmt::Debug for PlatformKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the secret half.
        f.debug_struct("PlatformKey")
            .field("usr_id", &self.user_id())
            .field("published", &self.published.len())
            .finish_non_exhaustive()
    }
}

/// The published form of the key held as `secret`.
fn published(secret: &SecretKeySet, valid_until: Option<Timestamp>) -> Result<PublishedKey> {
    Ok(PublishedKey {
        usr_id: secret.user_id()?.0,
        key: TrustedKey {
            keyset: secret.public_keyset(),
            valid_from: Timestamp(0),
            valid_until,
        },
    })
}

impl PlatformKey {
    /// Derives the platform key from a 32-byte secret seed, with no retired
    /// key.
    ///
    /// # Errors
    ///
    /// Returns an error if the key's `usr…` id cannot be derived or the key
    /// set cannot be signed.
    pub fn from_seed(seed: &[u8; 32]) -> Result<Self> {
        Self::new(seed, &[])
    }

    /// Derives the current key from `seed` and each retired key from its
    /// seed, then signs the published set with every one of them.
    ///
    /// # Errors
    ///
    /// Returns an error if a `usr…` id cannot be derived or signing fails.
    pub fn new(seed: &[u8; 32], retired: &[RetiredKey]) -> Result<Self> {
        let secret = SecretKeySet::from_seed(seed);
        let retired: Vec<(SecretKeySet, Timestamp)> = retired
            .iter()
            .map(|r| (SecretKeySet::from_seed(&r.seed), Timestamp(r.valid_until)))
            .collect();
        let current = published(&secret, None)?;
        let mut keys = vec![current.clone()];
        for (held, until) in &retired {
            keys.push(published(held, Some(*until))?);
        }
        let set: Vec<TrustedKey> = keys.iter().map(|k| k.key.clone()).collect();
        let holders = std::iter::once(&secret).chain(retired.iter().map(|(s, _)| s));
        let signatures = holders
            .zip(&keys)
            .map(|(held, key)| {
                Ok(KeySetSignature {
                    usr_id: key.usr_id.clone(),
                    sig: sign_key_set(held, &set)?,
                })
            })
            .collect::<Result<_>>()?;
        tracing::debug!(keys = keys.len(), "signed the platform key set");
        Ok(Self {
            secret,
            current,
            published: keys,
            signatures,
        })
    }

    /// The current key's public key set — the trust anchor for new
    /// endorsements.
    #[must_use]
    pub fn public_keyset(&self) -> &PublicKeySet {
        &self.current.key.keyset
    }

    /// The published keys, current first.
    #[must_use]
    pub fn published(&self) -> &[PublishedKey] {
        &self.published
    }

    /// The signatures over the published key set, one per held key.
    #[must_use]
    pub fn key_set_signatures(&self) -> &[KeySetSignature] {
        &self.signatures
    }

    /// The trusted endorsement keys to hand to `crisp_core::verify::verify`:
    /// the current key with an open window, then each retired key.
    #[must_use]
    pub fn platform_keys(&self) -> Vec<TrustedKey> {
        self.published.iter().map(|k| k.key.clone()).collect()
    }

    /// The current key's `usr…` identifier — the login audience.
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.current.usr_id
    }

    /// Adds the platform's server endorsement to an author-signed envelope.
    ///
    /// # Errors
    ///
    /// Returns an error if the envelope carries no author signature or if
    /// signing fails.
    pub fn endorse(&self, envelope: &CrispEnvelope, signed_at: i64) -> Result<CrispEnvelope> {
        let at = Timestamp(u64::try_from(signed_at).unwrap_or(0));
        endorse(envelope, &self.secret, at)
    }
}

#[cfg(test)]
mod tests {
    use crisp_core::verify::{PinUpdate, update_pin};

    use super::*;

    fn retired(byte: u8, valid_until: u64) -> RetiredKey {
        RetiredKey {
            seed: [byte; 32],
            valid_until,
        }
    }

    #[test]
    fn platform_key_is_deterministic_from_its_seed() {
        let a = PlatformKey::from_seed(&[7u8; 32]).unwrap();
        let b = PlatformKey::from_seed(&[7u8; 32]).unwrap();
        assert_eq!(a.public_keyset(), b.public_keyset());
        assert_eq!(a.user_id(), b.user_id());
        assert!(a.user_id().starts_with("usr1"));
        let other = PlatformKey::from_seed(&[8u8; 32]).unwrap();
        assert_ne!(a.public_keyset(), other.public_keyset());
        assert_eq!(&a.platform_keys()[0].keyset, a.public_keyset());
        assert_eq!(a.platform_keys().len(), 1);
    }

    #[test]
    fn retired_keys_follow_the_current_key_with_their_windows() {
        let key = PlatformKey::new(&[7u8; 32], &[retired(6, 100), retired(5, 50)]).unwrap();
        let old = PlatformKey::from_seed(&[6u8; 32]).unwrap();
        let keys = key.platform_keys();
        assert_eq!(keys.len(), 3);
        assert_eq!(&keys[0].keyset, key.public_keyset());
        assert_eq!(keys[0].valid_until, None);
        assert_eq!(&keys[1].keyset, old.public_keyset());
        assert_eq!(keys[1].valid_until, Some(Timestamp(100)));
        assert_eq!(keys[2].valid_until, Some(Timestamp(50)));
        assert_eq!(key.published()[1].usr_id, old.user_id());
        // The current key, not a retired one, is the login audience.
        assert_eq!(key.user_id(), key.published()[0].usr_id);
        let signers: Vec<&str> = key
            .key_set_signatures()
            .iter()
            .map(|s| s.usr_id.as_str())
            .collect();
        let ids: Vec<&str> = key.published().iter().map(|k| k.usr_id.as_str()).collect();
        assert_eq!(signers, ids);
    }

    #[test]
    fn a_client_pinned_to_the_old_key_accepts_the_rotated_set() {
        let before = PlatformKey::from_seed(&[6u8; 32]).unwrap();
        let after = PlatformKey::new(&[7u8; 32], &[retired(6, 100)]).unwrap();
        let sigs: Vec<Vec<u8>> = after
            .key_set_signatures()
            .iter()
            .map(|s| s.sig.clone())
            .collect();
        let outcome = update_pin(&before.platform_keys(), &after.platform_keys(), &sigs);
        assert_eq!(
            outcome,
            PinUpdate::Updated {
                keys: after.platform_keys()
            }
        );
    }

    #[test]
    fn debug_output_omits_the_secret() {
        let key = PlatformKey::from_seed(&[7u8; 32]).unwrap();
        let shown = format!("{key:?}");
        assert!(shown.contains("usr1"));
        assert!(!shown.contains("secret"));
    }
}
