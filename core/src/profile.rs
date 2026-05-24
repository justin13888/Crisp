//! Signing and verification of self-asserted account profiles.
//!
//! An [`AccountProfile`] binds a username and display name to a composite
//! public key. A [`SignedProfile`] is that profile together with a Composite
//! ML-DSA signature *by the very key it describes* — so a server (or any
//! client) can verify the name is self-asserted rather than merely trusted.
//! It is carried out-of-band at account registration, not inside a `.crisp`
//! bundle.

use serde::{Deserialize, Serialize};

use crate::cbor::{Value, encode_canonical};
use crate::codec::{from_canonical_cbor, to_canonical_cbor};
use crate::error::{CrispError, Result};
use crate::model::primitives::Timestamp;
use crate::model::profile::AccountProfile;
use crate::sign::{SecretKeySet, verify_composite};

/// The domain separator for an account-profile signature.
///
/// It keeps a profile signature distinct from a bundle signature
/// ([`crate::sign::signing_message`]) or a login-challenge signature, so none
/// can be replayed in another's place.
const PROFILE_SIGNATURE_DOMAIN: &str = "crisp/profile-sig/1";

/// The maximum username length, in bytes (`crisp-v1.cddl`: `account-profile`).
const MAX_USERNAME_LEN: usize = 64;
/// The maximum display-name length, in bytes (`crisp-v1.cddl`: `account-profile`).
const MAX_DISPLAY_NAME_LEN: usize = 256;

/// A profile together with the signature that authenticates it
/// (`crisp-v1.cddl`: `signed-profile`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedProfile {
    /// The canonical-CBOR encoding of an [`AccountProfile`].
    #[serde(with = "serde_bytes")]
    pub profile: Vec<u8>,
    /// The Composite ML-DSA signature over the domain-separated profile.
    #[serde(with = "serde_bytes")]
    pub sig: Vec<u8>,
}

/// Builds the domain-separated message a profile signature covers.
fn profile_message(profile_bytes: &[u8]) -> Result<Vec<u8>> {
    encode_canonical(&Value::Array(vec![
        Value::Text(PROFILE_SIGNATURE_DOMAIN.to_owned()),
        Value::Bytes(profile_bytes.to_vec()),
    ]))
}

/// Rejects an empty or over-long username / display name.
fn check_fields(profile: &AccountProfile) -> Result<()> {
    if profile.username.is_empty() || profile.username.len() > MAX_USERNAME_LEN {
        return Err(CrispError::input(format!(
            "username must be 1..={MAX_USERNAME_LEN} bytes"
        )));
    }
    if profile.display_name.is_empty() || profile.display_name.len() > MAX_DISPLAY_NAME_LEN {
        return Err(CrispError::input(format!(
            "display name must be 1..={MAX_DISPLAY_NAME_LEN} bytes"
        )));
    }
    Ok(())
}

/// Builds a [`SignedProfile`] for `username` and `display_name`, signed by
/// `secret`, and returns its canonical-CBOR encoding.
///
/// # Errors
///
/// Returns an error if the username or display name is empty or too long, or
/// if CBOR encoding or signing fails.
pub fn sign_profile(
    secret: &SecretKeySet,
    username: &str,
    display_name: &str,
    signed_at: Timestamp,
) -> Result<Vec<u8>> {
    let profile = AccountProfile {
        keyset: secret.public_keyset(),
        username: username.to_owned(),
        display_name: display_name.to_owned(),
        signed_at,
    };
    check_fields(&profile)?;
    let profile_bytes = to_canonical_cbor(&profile)?;
    let sig = secret.sign(&profile_message(&profile_bytes)?)?;
    to_canonical_cbor(&SignedProfile {
        profile: profile_bytes,
        sig,
    })
}

/// Decodes and verifies a [`SignedProfile`], returning the [`AccountProfile`]
/// it carries.
///
/// The signature is checked against the keyset *inside* the profile, so a
/// caller can trust that the username and display name were asserted by the
/// holder of that key.
///
/// # Errors
///
/// Returns an error if the bytes are off-schema, a field is out of range, or
/// the signature does not verify.
pub fn verify_profile(bytes: &[u8]) -> Result<AccountProfile> {
    let signed: SignedProfile = from_canonical_cbor(bytes)?;
    let profile: AccountProfile = from_canonical_cbor(&signed.profile)?;
    check_fields(&profile)?;
    verify_composite(
        &profile.keyset,
        &profile_message(&signed.profile)?,
        &signed.sig,
    )?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign::derive_keyset;

    #[test]
    fn sign_then_verify_round_trips() {
        let secret = derive_keyset(&[1u8; 32]);
        let bytes =
            sign_profile(&secret, "alice", "Alice Cook", Timestamp(1_700_000_000)).expect("sign");
        let profile = verify_profile(&bytes).expect("verify");
        assert_eq!(profile.username, "alice");
        assert_eq!(profile.display_name, "Alice Cook");
        assert_eq!(profile.keyset, secret.public_keyset());
    }

    #[test]
    fn a_tampered_display_name_fails_verification() {
        let secret = derive_keyset(&[2u8; 32]);
        let bytes = sign_profile(&secret, "bob", "Bob", Timestamp(1)).expect("sign");
        let signed: SignedProfile = from_canonical_cbor(&bytes).expect("decode");
        let mut profile: AccountProfile =
            from_canonical_cbor(&signed.profile).expect("decode profile");
        profile.display_name = "Mallory".to_owned();
        let forged = to_canonical_cbor(&SignedProfile {
            profile: to_canonical_cbor(&profile).expect("re-encode"),
            sig: signed.sig,
        })
        .expect("encode");
        assert!(verify_profile(&forged).is_err());
    }

    #[test]
    fn a_signature_from_another_key_fails_verification() {
        let signer = derive_keyset(&[3u8; 32]);
        let other = derive_keyset(&[4u8; 32]);
        let bytes = sign_profile(&signer, "carol", "Carol", Timestamp(1)).expect("sign");
        let signed: SignedProfile = from_canonical_cbor(&bytes).expect("decode");
        // Re-point the profile's keyset at a key that did not sign it.
        let mut profile: AccountProfile =
            from_canonical_cbor(&signed.profile).expect("decode profile");
        profile.keyset = other.public_keyset();
        let forged = to_canonical_cbor(&SignedProfile {
            profile: to_canonical_cbor(&profile).expect("re-encode"),
            sig: signed.sig,
        })
        .expect("encode");
        assert!(verify_profile(&forged).is_err());
    }

    #[test]
    fn a_wrong_length_signature_is_rejected() {
        let secret = derive_keyset(&[5u8; 32]);
        let bytes = sign_profile(&secret, "dave", "Dave", Timestamp(1)).expect("sign");
        let signed: SignedProfile = from_canonical_cbor(&bytes).expect("decode");
        let truncated = to_canonical_cbor(&SignedProfile {
            profile: signed.profile,
            sig: vec![0u8; 10],
        })
        .expect("encode");
        assert!(verify_profile(&truncated).is_err());
    }

    #[test]
    fn an_empty_display_name_is_rejected_at_signing() {
        let secret = derive_keyset(&[6u8; 32]);
        assert!(sign_profile(&secret, "erin", "", Timestamp(1)).is_err());
    }
}
