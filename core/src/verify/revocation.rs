//! Endorsement revocation (spec §8.8).
//!
//! A platform withdraws its endorsement of a bundle by listing the bundle's
//! `content_hash` in its revocation list and signing that list with its
//! current key. A verifier holding the list fails E2 for every endorsement
//! the platform made of a revoked bundle (`E.SIG.REVOKED`). A revocation is
//! permanent: a client keeps every revocation it has accepted, so replaying
//! an older list never restores trust.

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::cbor::{Value, encode_canonical};
use crate::codec::to_canonical_cbor;
use crate::error::{CrispError, Result};
use crate::model::primitives::Timestamp;
use crate::sign::{SecretKeySet, verify_composite};
use crate::verify::PlatformKey;

/// The domain separator for a revocation-list signature.
const REVOCATIONS_DOMAIN: &str = "crisp/platform-revocations/1";

/// The length of a content hash (SHA-256).
const CONTENT_HASH_LEN: usize = 32;

/// One withdrawn endorsement (`crisp-v1.cddl`: `revocation`): the platform
/// no longer vouches for the bundle whose payload hashes to `content_hash`.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revocation {
    /// `SHA-256(payload)` of the revoked bundle, as in its protected header.
    #[cfg_attr(feature = "ts", tsify(type = "Uint8Array"))]
    #[serde(with = "serde_bytes")]
    pub content_hash: Vec<u8>,
    /// When the platform revoked it. Informational: a revocation applies to
    /// the endorsement whatever the verification time.
    pub revoked_at: Timestamp,
}

/// Whether `revocations` withdraws the endorsement of the bundle whose
/// payload hashes to `content_hash`.
#[must_use]
pub fn revoked<'a>(revocations: &'a [Revocation], content_hash: &[u8]) -> Option<&'a Revocation> {
    revocations
        .iter()
        .find(|r| r.content_hash.as_slice() == content_hash)
}

/// Builds the message a revocation-list signature covers:
/// `canonical_cbor(["crisp/platform-revocations/1", .cbor platform-revocations])`,
/// where the list is the revocations in the order the server publishes them.
///
/// # Errors
///
/// Returns an error if canonical CBOR encoding fails.
pub fn revocations_message(revocations: &[Revocation]) -> Result<Vec<u8>> {
    encode_canonical(&Value::Array(vec![
        Value::Text(REVOCATIONS_DOMAIN.to_owned()),
        Value::Bytes(to_canonical_cbor(&revocations)?),
    ]))
}

/// Signs the revocation list `revocations` with `secret` (the platform's
/// current key).
///
/// # Errors
///
/// Returns an error if encoding or signing fails.
pub fn sign_revocations(secret: &SecretKeySet, revocations: &[Revocation]) -> Result<Vec<u8>> {
    secret.sign(&revocations_message(revocations)?)
}

/// Accepts a server's `offered` revocation list, given the key set `pinned`
/// for that server and the revocations already `held` for it.
///
/// The list is accepted only when one of `signatures` over it verifies under
/// a key pinned as current (no `valid_until`): a retired key may have leaked
/// and must not be able to revoke. The result is `held` plus every offered
/// revocation it lacks, sorted by content hash; where both hold the same
/// bundle, the earlier `revoked_at` is kept.
///
/// # Errors
///
/// Returns an error when an offered content hash is not 32 bytes, or when no
/// signature verifies under a current pinned key.
pub fn accept_revocations(
    pinned: &[PlatformKey],
    held: &[Revocation],
    offered: &[Revocation],
    signatures: &[Vec<u8>],
) -> Result<Vec<Revocation>> {
    if offered
        .iter()
        .any(|r| r.content_hash.len() != CONTENT_HASH_LEN)
    {
        return Err(CrispError::input(
            "a revocation's content hash is not 32 bytes",
        ));
    }
    let message = revocations_message(offered)?;
    let vouched = pinned.iter().filter(|k| k.valid_until.is_none()).any(|k| {
        signatures
            .iter()
            .any(|sig| verify_composite(&k.keyset, &message, sig).is_ok())
    });
    if !vouched {
        info!(
            offered = offered.len(),
            "revocation list rejected: no current pinned key signed it"
        );
        return Err(CrispError::input(
            "no current pinned key signed the server's revocation list",
        ));
    }
    let mut merged: Vec<Revocation> = held.to_vec();
    for revocation in offered {
        match merged
            .iter_mut()
            .find(|r| r.content_hash == revocation.content_hash)
        {
            Some(existing) => existing.revoked_at = existing.revoked_at.min(revocation.revoked_at),
            None => merged.push(revocation.clone()),
        }
    }
    merged.sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    debug!(
        held = held.len(),
        offered = offered.len(),
        merged = merged.len(),
        "accepted a revocation list"
    );
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(byte: u8) -> SecretKeySet {
        SecretKeySet::from_seed(&[byte; 32])
    }

    fn key(byte: u8, valid_until: Option<u64>) -> PlatformKey {
        PlatformKey {
            keyset: secret(byte).public_keyset(),
            valid_from: Timestamp(0),
            valid_until: valid_until.map(Timestamp),
        }
    }

    fn revocation(hash: u8, at: u64) -> Revocation {
        Revocation {
            content_hash: vec![hash; 32],
            revoked_at: Timestamp(at),
        }
    }

    fn signed(by: u8, list: &[Revocation]) -> Vec<Vec<u8>> {
        vec![sign_revocations(&secret(by), list).unwrap()]
    }

    #[test]
    fn a_list_signed_by_a_current_pinned_key_is_merged_sorted() {
        let offered = vec![revocation(3, 30), revocation(1, 10)];
        let held = vec![revocation(2, 20)];
        let merged =
            accept_revocations(&[key(1, None)], &held, &offered, &signed(1, &offered)).unwrap();
        assert_eq!(
            merged,
            vec![revocation(1, 10), revocation(2, 20), revocation(3, 30)]
        );
    }

    #[test]
    fn a_held_revocation_survives_a_list_that_omits_it() {
        let held = vec![revocation(2, 20)];
        let merged = accept_revocations(&[key(1, None)], &held, &[], &signed(1, &[])).unwrap();
        assert_eq!(merged, held);
    }

    #[test]
    fn the_earlier_revocation_time_is_kept() {
        let offered = vec![revocation(1, 5), revocation(2, 50)];
        let held = vec![revocation(1, 10), revocation(2, 20)];
        let merged =
            accept_revocations(&[key(1, None)], &held, &offered, &signed(1, &offered)).unwrap();
        assert_eq!(merged, vec![revocation(1, 5), revocation(2, 20)]);
    }

    #[test]
    fn a_list_without_a_vouching_signature_is_rejected() {
        let offered = vec![revocation(1, 10)];
        let pinned = [key(2, None), key(1, Some(100))];
        for signatures in [vec![], signed(1, &offered), signed(9, &offered)] {
            assert!(accept_revocations(&pinned, &[], &offered, &signatures).is_err());
        }
        let other = signed(2, &[revocation(1, 11)]);
        assert!(accept_revocations(&pinned, &[], &offered, &other).is_err());
        assert!(accept_revocations(&pinned, &[], &offered, &signed(2, &offered)).is_ok());
    }

    #[test]
    fn a_malformed_content_hash_is_rejected() {
        let offered = vec![Revocation {
            content_hash: vec![1; 31],
            revoked_at: Timestamp(1),
        }];
        let signatures = signed(1, &offered);
        assert!(accept_revocations(&[key(1, None)], &[], &offered, &signatures).is_err());
    }

    #[test]
    fn revoked_finds_only_a_listed_hash() {
        let list = vec![revocation(1, 10), revocation(2, 20)];
        assert_eq!(revoked(&list, &[2; 32]), Some(&list[1]));
        assert_eq!(revoked(&list, &[3; 32]), None);
        assert_eq!(revoked(&[], &[1; 32]), None);
    }

    #[test]
    fn the_revocations_message_is_domain_separated() {
        let list = vec![revocation(1, 10)];
        let message = revocations_message(&list).unwrap();
        assert_eq!(message, revocations_message(&list).unwrap());
        assert_ne!(message, revocations_message(&[revocation(1, 11)]).unwrap());
        assert_ne!(
            message,
            crate::verify::key_set_message(&[key(1, None)]).unwrap()
        );
    }
}
