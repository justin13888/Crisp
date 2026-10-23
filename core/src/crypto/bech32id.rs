//! bech32m-encoded identifiers.
//!
//! Every Crisp identifier — recipe, book, user, revision, and blob — is the
//! bech32m encoding of a one-byte version prefix followed by a 32-byte
//! SHA-256 digest, under a three-letter human-readable part (spec §7). The
//! bech32m checksum makes copy-paste corruption and truncation detectable.

use bech32::primitives::decode::UncheckedHrpstring;
use bech32::{Bech32m, Hrp as Bech32Hrp};

use crate::error::{CrispError, ErrorCode, Result};

/// The version byte that prefixes every identifier's digest.
pub const ID_VERSION: u8 = 0x01;
/// The fixed character length of every encoded identifier.
pub const ID_STR_LEN: usize = 63;
/// The length of the digest carried by an identifier, in bytes.
pub const DIGEST_LEN: usize = 32;

/// The human-readable part (HRP) of an identifier — its type tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hrp {
    /// A recipe id (`rcp…`).
    Recipe,
    /// A book id (`rbk…`).
    Book,
    /// A user id (`usr…`).
    User,
    /// A revision hash (`rev…`).
    Revision,
    /// A blob id (`blb…`).
    Blob,
}

impl Hrp {
    /// Every HRP variant.
    pub const ALL: [Hrp; 5] = [Hrp::Recipe, Hrp::Book, Hrp::User, Hrp::Revision, Hrp::Blob];

    /// The three-letter HRP string, e.g. `"rcp"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Hrp::Recipe => "rcp",
            Hrp::Book => "rbk",
            Hrp::User => "usr",
            Hrp::Revision => "rev",
            Hrp::Blob => "blb",
        }
    }

    /// Resolves an HRP from its three-letter string, if recognised.
    #[must_use]
    fn from_prefix(prefix: &str) -> Option<Hrp> {
        Hrp::ALL.into_iter().find(|hrp| hrp.as_str() == prefix)
    }
}

/// Encodes a versioned 32-byte digest as a bech32m identifier under `hrp`.
///
/// # Errors
///
/// Returns [`CrispError::Crypto`] if bech32m encoding fails (not expected for
/// well-formed inputs).
pub fn encode_id(hrp: Hrp, digest: &[u8; DIGEST_LEN]) -> Result<String> {
    let mut payload = [0u8; 1 + DIGEST_LEN];
    payload[0] = ID_VERSION;
    payload[1..].copy_from_slice(digest);

    let bech_hrp = Bech32Hrp::parse_unchecked(hrp.as_str());
    bech32::encode::<Bech32m>(bech_hrp, &payload)
        .map_err(|e| CrispError::crypto(format!("bech32m encoding failed: {e}")))
}

/// Decodes a bech32m identifier, returning its HRP and 32-byte digest.
///
/// Verifies the fixed length, the bech32m checksum, a recognised HRP, and the
/// version prefix.
///
/// # Errors
///
/// Returns [`CrispError::Check`] tagged [`ErrorCode::IdChecksum`] for any
/// length, checksum, HRP, or version-prefix problem.
pub fn decode_id(text: &str) -> Result<(Hrp, [u8; DIGEST_LEN])> {
    let bad = |detail: &str| CrispError::check(ErrorCode::IdChecksum, detail.to_owned());

    if text.len() != ID_STR_LEN {
        return Err(bad("identifier is not 63 characters long"));
    }
    let unchecked = UncheckedHrpstring::new(text)
        .map_err(|e| CrispError::check(ErrorCode::IdChecksum, format!("invalid bech32: {e}")))?;
    // Validating against `Bech32m` rejects a plain bech32 (non-m) checksum too.
    unchecked
        .validate_checksum::<Bech32m>()
        .map_err(|e| CrispError::check(ErrorCode::IdChecksum, format!("invalid bech32m: {e}")))?;
    let parsed = unchecked.remove_checksum::<Bech32m>();

    let hrp = Hrp::from_prefix(&parsed.hrp().to_lowercase())
        .ok_or_else(|| bad("unrecognised identifier prefix"))?;

    let payload: Vec<u8> = parsed.byte_iter().collect();
    if payload.len() != 1 + DIGEST_LEN {
        return Err(bad("identifier payload is not 33 bytes"));
    }
    if payload[0] != ID_VERSION {
        return Err(bad("unsupported identifier version"));
    }
    let mut digest = [0u8; DIGEST_LEN];
    digest.copy_from_slice(&payload[1..]);
    Ok((hrp, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_for_every_hrp() {
        let digest = [0x42u8; DIGEST_LEN];
        for hrp in Hrp::ALL {
            let text = encode_id(hrp, &digest).expect("encode");
            assert_eq!(text.len(), ID_STR_LEN, "{hrp:?} id must be 63 chars");
            assert!(text.starts_with(hrp.as_str()));
            let (got_hrp, got_digest) = decode_id(&text).expect("decode");
            assert_eq!(got_hrp, hrp);
            assert_eq!(got_digest, digest);
        }
    }

    #[test]
    fn distinct_digests_produce_distinct_ids() {
        let a = encode_id(Hrp::Recipe, &[0u8; DIGEST_LEN]).expect("encode");
        let b = encode_id(Hrp::Recipe, &[1u8; DIGEST_LEN]).expect("encode");
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_a_single_character_typo() {
        let text = encode_id(Hrp::Recipe, &[7u8; DIGEST_LEN]).expect("encode");
        let mut chars: Vec<char> = text.chars().collect();
        // Flip the last data character to a different valid bech32 character.
        let last = chars.len() - 7;
        chars[last] = if chars[last] == 'q' { 'p' } else { 'q' };
        let mangled: String = chars.into_iter().collect();
        let err = decode_id(&mangled).expect_err("typo must be caught");
        assert_eq!(err.code(), Some(ErrorCode::IdChecksum));
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            decode_id("rcp1qqqq").unwrap_err().code(),
            Some(ErrorCode::IdChecksum)
        );
    }

    #[test]
    fn rejects_unknown_hrp() {
        // A structurally valid bech32m string under an HRP Crisp does not use.
        let text = bech32::encode::<Bech32m>(Bech32Hrp::parse_unchecked("xyz"), &{
            let mut p = [0u8; 1 + DIGEST_LEN];
            p[0] = ID_VERSION;
            p
        })
        .expect("encode");
        assert_eq!(
            decode_id(&text).unwrap_err().code(),
            Some(ErrorCode::IdChecksum)
        );
    }
}
