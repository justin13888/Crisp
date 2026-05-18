//! The platform endorsement key.
//!
//! Every published bundle carries exactly one server endorsement (spec D6).
//! The server holds one Composite ML-DSA key, derived deterministically from
//! a 32-byte secret seed, and uses it to sign that endorsement. The matching
//! public key set is published via the version endpoints so any client can
//! verify endorsements offline.

use crisp_core::Result;
use crisp_core::build::endorse;
use crisp_core::model::envelope::CrispEnvelope;
use crisp_core::model::meta::PublicKeySet;
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::{SecretKeySet, derive_keyset};

/// The server's endorsement signing key and its cached public projection.
pub struct PlatformKey {
    /// The live secret key set.
    secret: SecretKeySet,
    /// The public key set, derived once at construction.
    public: PublicKeySet,
}

impl PlatformKey {
    /// Derives the platform key from a 32-byte secret seed.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let secret = derive_keyset(seed);
        let public = secret.public_keyset();
        Self { secret, public }
    }

    /// The platform's public key set — the trust anchor for endorsements.
    #[must_use]
    pub fn public_keyset(&self) -> &PublicKeySet {
        &self.public
    }

    /// The platform's own `usr…` identifier.
    ///
    /// # Errors
    ///
    /// Returns an error if id derivation fails.
    pub fn user_id(&self) -> Result<String> {
        Ok(self.secret.user_id()?.0)
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
    use super::*;

    #[test]
    fn platform_key_is_deterministic_from_its_seed() {
        let a = PlatformKey::from_seed(&[7u8; 32]);
        let b = PlatformKey::from_seed(&[7u8; 32]);
        assert_eq!(a.public_keyset(), b.public_keyset());
        let other = PlatformKey::from_seed(&[8u8; 32]);
        assert_ne!(a.public_keyset(), other.public_keyset());
    }
}
