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
    /// The platform's `usr…` id, derived once at construction so it can never
    /// fail (or silently degrade) at request time.
    usr_id: String,
}

impl std::fmt::Debug for PlatformKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the secret half.
        f.debug_struct("PlatformKey")
            .field("usr_id", &self.usr_id)
            .finish_non_exhaustive()
    }
}

impl PlatformKey {
    /// Derives the platform key from a 32-byte secret seed.
    ///
    /// # Errors
    ///
    /// Returns an error if the key's `usr…` id cannot be derived.
    pub fn from_seed(seed: &[u8; 32]) -> Result<Self> {
        let secret = derive_keyset(seed);
        let public = secret.public_keyset();
        let usr_id = secret.user_id()?.0;
        Ok(Self {
            secret,
            public,
            usr_id,
        })
    }

    /// The platform's public key set — the trust anchor for endorsements.
    #[must_use]
    pub fn public_keyset(&self) -> &PublicKeySet {
        &self.public
    }

    /// The trusted endorsement keys to hand to `crisp_core::verify::verify`.
    ///
    /// The single seam where the server's key material meets the verifier's
    /// `platform_keys` parameter, so a change to that parameter's type (e.g.
    /// keys carrying validity windows) is adapted here only.
    #[must_use]
    pub fn platform_keys(&self) -> Vec<PublicKeySet> {
        vec![self.public.clone()]
    }

    /// The platform's own `usr…` identifier.
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.usr_id
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
        let a = PlatformKey::from_seed(&[7u8; 32]).unwrap();
        let b = PlatformKey::from_seed(&[7u8; 32]).unwrap();
        assert_eq!(a.public_keyset(), b.public_keyset());
        assert_eq!(a.user_id(), b.user_id());
        assert!(a.user_id().starts_with("usr1"));
        let other = PlatformKey::from_seed(&[8u8; 32]).unwrap();
        assert_ne!(a.public_keyset(), other.public_keyset());
        assert_eq!(a.platform_keys(), vec![a.public_keyset().clone()]);
    }

    #[test]
    fn debug_output_omits_the_secret() {
        let key = PlatformKey::from_seed(&[7u8; 32]).unwrap();
        let shown = format!("{key:?}");
        assert!(shown.contains("usr1"));
        assert!(!shown.contains("secret"));
    }
}
