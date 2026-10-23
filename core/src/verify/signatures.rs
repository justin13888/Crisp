//! Phase E — signatures (spec §11.1, E1–E3).

use serde::{Deserialize, Serialize};

use crate::codec::from_canonical_cbor;
use crate::crypto::hash::sha256;
use crate::error::{CrispError, ErrorCode, Result};
use crate::identity::key_fingerprint;
use crate::model::envelope::{CoseSign, SigProtected, SigRole, Signature};
use crate::model::meta::PublicKeySet;
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::Timestamp;
use crate::sign::{signing_message, verify_composite};

/// Clock-skew tolerance, in seconds, allowed when comparing a `modified_at`
/// against a server endorsement's `signed_at` (check E3).
pub const ENDORSEMENT_SKEW_SECS: u64 = 300;

/// A trusted platform endorsement key and the window it may sign in
/// (spec §8.5).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformKey {
    /// The platform's composite public key.
    pub keyset: PublicKeySet,
    /// The first instant the key may sign.
    pub valid_from: Timestamp,
    /// The last instant the key may sign, if it has been retired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
}

impl PlatformKey {
    /// Whether `at` falls inside this key's validity window.
    #[must_use]
    pub fn valid_at(&self, at: Timestamp) -> bool {
        at >= self.valid_from && self.valid_until.is_none_or(|until| at <= until)
    }
}

/// `SHA-256` of the canonical CBOR array of every author signature — the
/// value an endorsement's `binds` must carry.
///
/// # Errors
///
/// Returns an error if encoding fails.
pub fn author_binding(author_signatures: &[&Signature]) -> Result<[u8; 32]> {
    let values: Vec<ciborium::value::Value> = author_signatures
        .iter()
        .map(ciborium::value::Value::serialized)
        .collect::<core::result::Result<_, _>>()
        .map_err(|e| CrispError::input(format!("could not encode a signature: {e}")))?;
    Ok(sha256(&crate::cbor::encode_canonical(
        &ciborium::value::Value::Array(values),
    )?))
}

/// The fingerprint of the key that owns the document: the identity of the
/// only recipe in a recipe bundle, or the book's identity.
fn owner_fingerprint(payload: &BundlePayload) -> Option<&[u8]> {
    match payload.kind {
        Kind::Recipe => payload
            .recipes
            .first()
            .map(|r| r.identity.author_fpr.as_slice()),
        Kind::Book => payload
            .meta
            .identity
            .as_ref()
            .map(|i| i.author_fpr.as_slice()),
    }
}

/// Signatures split by role, each with its decoded protected header.
pub(crate) struct Parsed<'a> {
    pub(crate) authors: Vec<(SigProtected, &'a Signature)>,
    pub(crate) endorsements: Vec<(usize, SigProtected, &'a Signature)>,
    pub(crate) error: Option<CrispError>,
}

/// Decodes every signature's protected header.
pub(crate) fn parse(cose: &CoseSign) -> Parsed<'_> {
    let mut parsed = Parsed {
        authors: Vec::new(),
        endorsements: Vec::new(),
        error: None,
    };
    for (i, sig) in cose.signatures.iter().enumerate() {
        match from_canonical_cbor::<SigProtected>(&sig.protected) {
            Ok(sp) if sp.role == SigRole::Author => parsed.authors.push((sp, sig)),
            Ok(sp) => parsed.endorsements.push((i, sp, sig)),
            Err(e) => {
                parsed.error.get_or_insert(CrispError::check(
                    ErrorCode::SigAuthor,
                    format!("signature {i} has a malformed protected header: {e}"),
                ));
            }
        }
    }
    parsed
}

/// E1 — at least one author signature, the document owner among the
/// signers, and every author signature valid and from a listed author.
pub(crate) fn authors(cose: &CoseSign, payload: &BundlePayload, parsed: &Parsed<'_>) -> Result<()> {
    if let Some(e) = &parsed.error {
        return Err(e.clone());
    }
    if parsed.authors.is_empty() {
        return Err(CrispError::check(
            ErrorCode::SigMissing,
            "no author signature is present (an unsigned draft)",
        ));
    }
    let mut owner_signed = false;
    for (i, (sp, sig)) in parsed.authors.iter().enumerate() {
        if sp.binds.is_some() {
            return Err(CrispError::check(
                ErrorCode::SigAuthor,
                format!("author signature {i} carries an endorsement binding"),
            ));
        }
        let keyset = payload
            .meta
            .authors
            .iter()
            .map(|a| &a.keyset)
            .find(|k| key_fingerprint(k).is_ok_and(|f| f.as_slice() == sp.kid.as_slice()))
            .ok_or_else(|| {
                CrispError::check(
                    ErrorCode::SigAuthor,
                    format!("author signature {i} is by a key no listed author holds"),
                )
            })?;
        let message = signing_message(&cose.protected, &sig.protected, &[])?;
        verify_composite(keyset, &message, &sig.sig_value).map_err(|_| {
            CrispError::check(
                ErrorCode::SigAuthor,
                format!("author signature {i} does not verify"),
            )
        })?;
        owner_signed |= owner_fingerprint(payload) == Some(sp.kid.as_slice());
    }
    if !owner_signed {
        return Err(CrispError::check(
            ErrorCode::SigMissing,
            "the document's own author has not signed it",
        ));
    }
    Ok(())
}

/// E2 — exactly one endorsement, last, by a trusted key inside its window,
/// bound to every author signature, and valid.
pub(crate) fn endorsement(
    cose: &CoseSign,
    parsed: &Parsed<'_>,
    platform_keys: &[PlatformKey],
) -> Result<()> {
    let [(index, sp, sig)] = parsed.endorsements.as_slice() else {
        return Err(CrispError::check(
            ErrorCode::SigMissing,
            format!(
                "expected exactly one server endorsement, found {}",
                parsed.endorsements.len()
            ),
        ));
    };
    if *index + 1 != cose.signatures.len() {
        return Err(CrispError::check(
            ErrorCode::SigEndorsement,
            "the server endorsement must be the last signature",
        ));
    }
    let key = platform_keys
        .iter()
        .find(|k| key_fingerprint(&k.keyset).is_ok_and(|f| f.as_slice() == sp.kid.as_slice()))
        .ok_or_else(|| {
            CrispError::check(ErrorCode::KeyUnknown, "the endorsement key is not trusted")
        })?;
    if !key.valid_at(sp.signed_at) {
        return Err(CrispError::check(
            ErrorCode::KeyWindow,
            "the endorsement was made outside its key's validity window",
        ));
    }
    let authors: Vec<&Signature> = parsed.authors.iter().map(|(_, s)| *s).collect();
    let expected = author_binding(&authors)?;
    if sp.binds.as_deref() != Some(expected.as_slice()) {
        return Err(CrispError::check(
            ErrorCode::SigBinding,
            "the endorsement is not bound to this bundle's author signatures",
        ));
    }
    let message = signing_message(&cose.protected, &sig.protected, &expected)?;
    verify_composite(&key.keyset, &message, &sig.sig_value).map_err(|_| {
        CrispError::check(
            ErrorCode::SigEndorsement,
            "the server endorsement does not verify",
        )
    })
}

/// E3 — nothing was modified after the (verified) endorsement time.
pub(crate) fn timing(payload: &BundlePayload, parsed: &Parsed<'_>) -> Result<()> {
    let Some((_, sp, _)) = parsed.endorsements.first() else {
        return Ok(());
    };
    let limit = sp.signed_at.0.saturating_add(ENDORSEMENT_SKEW_SECS);
    if payload.meta.modified_at.0 > limit || payload.recipes.iter().any(|r| r.modified_at.0 > limit)
    {
        return Err(CrispError::check(
            ErrorCode::TimeOrder,
            "the bundle was modified after it was endorsed",
        ));
    }
    Ok(())
}
