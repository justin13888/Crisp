//! Bundle authoring: finalizing recipes, author-signing, and endorsing.
//!
//! Authoring a bundle assembles a [`BundlePayload`], derives every content-
//! bound identifier, hashes the payload into the protected header, and adds
//! signatures. [`author_sign`] produces an author-signed bundle; [`endorse`]
//! adds the platform's server endorsement, yielding a Published-class bundle.

use crate::cbor::Value;
use crate::codec::to_canonical_cbor;
use crate::crypto::hash::sha256;
use crate::error::{CrispError, Result};
use crate::identity::{derive_recipe_id, derive_revision, key_fingerprint};
use crate::model::envelope::{
    CONTAINER_VERSION, CoseSign, CrispEnvelope, HeaderMap, ProtectedHeader, SigAlg, SigProtected,
    SigRole, Signature,
};
use crate::model::payload::BundlePayload;
use crate::model::primitives::Timestamp;
use crate::model::recipe::Recipe;
use crate::sign::{SecretKeySet, signing_message};

/// An empty, advisory unprotected header.
fn empty_header() -> HeaderMap {
    Value::Map(Vec::new())
}

/// Finalizes a draft recipe: stamps the author fingerprint into its identity,
/// then derives and sets its `id` and `revision`.
///
/// # Errors
///
/// Returns an error if id or revision derivation fails.
pub fn finalize_recipe(mut recipe: Recipe, author_fpr: [u8; 32]) -> Result<Recipe> {
    recipe.identity.author_fpr = author_fpr.to_vec();
    recipe.id = derive_recipe_id(&recipe.identity)?;
    recipe.revision = derive_revision(&recipe)?;
    Ok(recipe)
}

/// Builds one signature over a bundle's protected header.
fn make_signature(
    protected_header: &[u8],
    key: &SecretKeySet,
    role: SigRole,
    signed_at: Timestamp,
    external_aad: &[u8],
) -> Result<Signature> {
    let sig_protected = SigProtected {
        alg: SigAlg::MlDsa65Ed25519,
        role,
        kid: key_fingerprint(&key.public_keyset())?.to_vec(),
        signed_at,
    };
    let sig_protected_bytes = to_canonical_cbor(&sig_protected)?;
    let message = signing_message(protected_header, &sig_protected_bytes, external_aad)?;
    let sig_value = key.sign(&message)?;
    Ok(Signature {
        protected: sig_protected_bytes,
        unprotected: empty_header(),
        sig_value,
    })
}

/// Assembles an author-signed [`CrispEnvelope`] from a finalized payload.
///
/// The payload's recipes must already be finalized (see [`finalize_recipe`])
/// and its `doc_id` set. The payload is canonically encoded, hashed into the
/// protected header, and signed with one author signature.
///
/// # Errors
///
/// Returns an error if encoding or signing fails.
pub fn author_sign(
    payload: &BundlePayload,
    author: &SecretKeySet,
    signed_at: Timestamp,
) -> Result<CrispEnvelope> {
    let payload_bytes = to_canonical_cbor(payload)?;
    let content_hash = sha256(&payload_bytes);
    let protected = ProtectedHeader {
        format_version: payload.v,
        doc_id: payload.doc_id.clone(),
        content_hash: content_hash.to_vec(),
    };
    let protected_bytes = to_canonical_cbor(&protected)?;
    let signature = make_signature(&protected_bytes, author, SigRole::Author, signed_at, &[])?;
    Ok(CrispEnvelope {
        container_version: CONTAINER_VERSION,
        signed_bundle: CoseSign {
            protected: protected_bytes,
            unprotected: empty_header(),
            payload: payload_bytes,
            signatures: vec![signature],
        },
    })
}

/// Adds the platform's server endorsement to an author-signed envelope.
///
/// The endorsement's `external_aad` is `SHA-256(canonical_cbor(author
/// signature))`, binding the endorsement to that exact author-signed bundle.
///
/// # Errors
///
/// Returns an error if the envelope has no author signature, or if encoding
/// or signing fails.
pub fn endorse(
    envelope: &CrispEnvelope,
    platform: &SecretKeySet,
    signed_at: Timestamp,
) -> Result<CrispEnvelope> {
    let author_signature = envelope
        .signed_bundle
        .signatures
        .first()
        .ok_or_else(|| CrispError::input("cannot endorse a bundle with no author signature"))?;
    let external_aad = sha256(&to_canonical_cbor(author_signature)?);
    let endorsement = make_signature(
        &envelope.signed_bundle.protected,
        platform,
        SigRole::ServerEndorsement,
        signed_at,
        &external_aad,
    )?;
    let mut endorsed = envelope.clone();
    endorsed.signed_bundle.signatures.push(endorsement);
    Ok(endorsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode_file, encode_file, from_canonical_cbor};
    use crate::fixtures::sample_payload;
    use crate::identity::key_fingerprint;
    use crate::model::envelope::SigProtected;
    use crate::sign::derive_keyset;
    use crate::sign::{signing_message, verify_composite};

    #[test]
    fn author_signed_bundle_round_trips_and_verifies() {
        let author = derive_keyset(&[11u8; 32]);
        let payload = sample_payload();
        let envelope = author_sign(&payload, &author, Timestamp(2_000)).expect("sign");
        let bytes = encode_file(&envelope).expect("encode");
        let decoded = decode_file(&bytes).expect("decode");
        assert_eq!(decoded, envelope);

        // The author signature verifies against the reconstructed message.
        let sig = &decoded.signed_bundle.signatures[0];
        let sig_protected: SigProtected =
            from_canonical_cbor(&sig.protected).expect("decode sig-protected");
        assert_eq!(sig_protected.role, SigRole::Author);
        let message =
            signing_message(&decoded.signed_bundle.protected, &sig.protected, &[]).expect("M");
        verify_composite(&author.public_keyset(), &message, &sig.sig_value).expect("verify");
    }

    #[test]
    fn endorsement_appends_a_second_signature_bound_to_the_author_signature() {
        let author = derive_keyset(&[12u8; 32]);
        let platform = derive_keyset(&[99u8; 32]);
        let signed = author_sign(&sample_payload(), &author, Timestamp(2_000)).expect("sign");
        let endorsed = endorse(&signed, &platform, Timestamp(3_000)).expect("endorse");

        assert_eq!(endorsed.signed_bundle.signatures.len(), 2);
        let author_sig = &endorsed.signed_bundle.signatures[0];
        let endorsement = &endorsed.signed_bundle.signatures[1];
        let external_aad = sha256(&to_canonical_cbor(author_sig).expect("encode"));
        let message = signing_message(
            &endorsed.signed_bundle.protected,
            &endorsement.protected,
            &external_aad,
        )
        .expect("M");
        verify_composite(&platform.public_keyset(), &message, &endorsement.sig_value)
            .expect("endorsement verifies");
    }

    #[test]
    fn finalize_recipe_sets_identity_id_and_revision() {
        let author = derive_keyset(&[13u8; 32]);
        let fpr = key_fingerprint(&author.public_keyset()).expect("fingerprint");
        let recipe = finalize_recipe(crate::fixtures::sample_recipe(), fpr).expect("finalize");
        assert_eq!(recipe.identity.author_fpr, fpr.to_vec());
        assert!(recipe.id.0.starts_with("rcp1"));
        assert!(recipe.revision.0.starts_with("rev1"));
    }
}
