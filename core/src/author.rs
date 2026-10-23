//! Authoring: turning an editor draft into a signed `.crisp` file.
//!
//! A *draft* is a [`BundlePayload`] whose derived fields may be blank: empty
//! `id`/`revision`/`doc_id` strings, an empty `author_fpr`, a zero
//! `created_at`, an empty `birth_nonce`. [`seal`] fills every derived field,
//! normalizes all text to NFC, refuses anything that would not verify, and
//! author-signs the result. It also decides what an edit *is*:
//!
//! - a **new recipe** (no `author_fpr` yet) is born under the signer;
//! - the owner's edit of an existing recipe is a **revision** — the identity
//!   is kept and `supersedes` links the previous revision;
//! - anyone else's edit is a **fork** — a fresh identity under the signer,
//!   with `derived_from` naming the original.

use ciborium::value::Value;
use tracing::{debug, info_span};
use unicode_normalization::UnicodeNormalization;

use crate::codec::{decode_file, encode_file, from_canonical_cbor, to_canonical_cbor};
use crate::crypto::hash::sha256;
use crate::error::{CrispError, Result};
use crate::identity::{derive_book_id, derive_recipe_id, derive_revision, normalize_title};
use crate::model::envelope::{
    CONTAINER_VERSION, CoseSign, CrispEnvelope, ProtectedHeader, SigAlg, SigProtected, SigRole,
    Signature,
};
use crate::model::ids::{DocId, RevisionId};
use crate::model::meta::Author;
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::{LanguageTag, Timestamp};
use crate::model::recipe::Identity;
use crate::model::text::LocalizedText;
use crate::sign::{SecretKeySet, signing_message};
use crate::verify::{self, Issue, author_binding, signatures};
use crate::{FORMAT_VERSION, MIN_READER_VERSION};

/// How signatures are produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Signing {
    /// Hedged (randomized) ML-DSA — always, outside reproducible artifacts.
    Hedged,
    /// Deterministic ML-DSA, for golden vectors and the compliance suite.
    #[cfg(test)]
    Deterministic,
}

/// Creates the identity of a brand-new recipe: a fresh 128-bit birth nonce,
/// the title and language it is born with, and its creation time. The
/// author fingerprint is stamped by [`seal`].
///
/// # Errors
///
/// Returns [`CrispError::Crypto`] if the system RNG is unavailable.
pub fn new_identity(title: &str, language: LanguageTag, created_at: Timestamp) -> Result<Identity> {
    let mut birth_nonce = vec![0u8; 16];
    getrandom::fill(&mut birth_nonce)
        .map_err(|e| CrispError::crypto(format!("system RNG unavailable: {e}")))?;
    Ok(Identity {
        author_fpr: Vec::new(),
        title: normalize_title(title),
        language,
        created_at,
        birth_nonce,
    })
}

/// Normalizes every text string in a payload to Unicode NFC.
fn normalize_text(payload: &BundlePayload) -> Result<BundlePayload> {
    fn nfc(value: Value) -> Value {
        match value {
            Value::Text(text) => Value::Text(text.nfc().collect()),
            Value::Array(items) => Value::Array(items.into_iter().map(nfc).collect()),
            Value::Map(entries) => {
                Value::Map(entries.into_iter().map(|(k, v)| (nfc(k), nfc(v))).collect())
            }
            Value::Tag(tag, inner) => Value::Tag(tag, Box::new(nfc(*inner))),
            other => other,
        }
    }
    let value = Value::serialized(payload)
        .map_err(|e| CrispError::input(format!("could not encode the draft: {e}")))?;
    nfc(value)
        .deserialized()
        .map_err(|e| CrispError::input(format!("could not decode the draft: {e}")))
}

/// The title an identity is born with, read from the recipe's display title.
fn birth_title(title: &LocalizedText, language: &LanguageTag) -> String {
    let text = title
        .0
        .get(language)
        .or_else(|| title.0.values().next())
        .map_or("", String::as_str);
    normalize_title(text)
}

/// Fills an identity's blank birth fields.
fn complete_identity(
    identity: &mut Identity,
    fpr: [u8; 32],
    title: &LocalizedText,
    default_language: &LanguageTag,
    now: Timestamp,
    fill_nonce: impl FnOnce() -> Result<Vec<u8>>,
) -> Result<()> {
    identity.author_fpr = fpr.to_vec();
    if identity.language.0.is_empty() {
        identity.language = default_language.clone();
    }
    if identity.title.trim().is_empty() {
        identity.title = birth_title(title, &identity.language);
    }
    identity.title = normalize_title(&identity.title);
    if identity.created_at.0 == 0 {
        identity.created_at = now;
    }
    if identity.birth_nonce.is_empty() {
        identity.birth_nonce = fill_nonce()?;
    }
    Ok(())
}

/// Fills every derived field of `draft` for `signer`, resolving revision vs
/// fork against `previous` (the payload of the file being edited, if any).
pub(crate) fn prepare(
    draft: &BundlePayload,
    signer: &SecretKeySet,
    display_name: &str,
    now: Timestamp,
    previous: Option<&BundlePayload>,
    nonce: &mut dyn FnMut() -> Result<Vec<u8>>,
) -> Result<BundlePayload> {
    let mut payload = normalize_text(draft)?;
    let fpr = signer.fingerprint()?;
    let default_language = payload.default_language.clone();
    // A fork is a new document: it is born now, not when its parent was.
    let mut forked = false;

    for recipe in &mut payload.recipes {
        let prior = previous.and_then(|p| {
            p.recipes
                .iter()
                .find(|r| r.id == recipe.id && !r.id.0.is_empty())
        });
        let owned_by_signer =
            recipe.identity.author_fpr.is_empty() || recipe.identity.author_fpr == fpr;
        match prior {
            Some(prior) if prior.identity.author_fpr == fpr => {
                recipe.identity = prior.identity.clone();
                recipe.supersedes = Some(prior.revision.clone());
                recipe.derived_from = prior.derived_from.clone();
            }
            Some(prior) => {
                forked = true;
                recipe.identity = Identity {
                    author_fpr: Vec::new(),
                    title: String::new(),
                    language: prior.identity.language.clone(),
                    created_at: now,
                    birth_nonce: Vec::new(),
                };
                recipe.supersedes = None;
                recipe.derived_from = Some(prior.id.clone());
            }
            None if !owned_by_signer => {
                forked = true;
                let original = recipe.id.clone();
                recipe.identity = Identity {
                    author_fpr: Vec::new(),
                    title: String::new(),
                    language: recipe.identity.language.clone(),
                    created_at: now,
                    birth_nonce: Vec::new(),
                };
                recipe.supersedes = None;
                recipe.derived_from = (!original.0.is_empty()).then_some(original);
            }
            None => {}
        }
        complete_identity(
            &mut recipe.identity,
            fpr,
            &recipe.title,
            &default_language,
            now,
            &mut *nonce,
        )?;
        recipe.modified_at = now;
        recipe.id = derive_recipe_id(&recipe.identity)?;
        recipe.revision = RevisionId(String::new());
        recipe.revision = derive_revision(recipe)?;
    }

    if let Some(identity) = &mut payload.meta.identity {
        if let Some(prior) = previous.and_then(|p| p.meta.identity.as_ref())
            && prior.author_fpr == fpr
            && identity.birth_nonce == prior.birth_nonce
        {
            *identity = prior.clone();
        } else if !identity.author_fpr.is_empty() && identity.author_fpr != fpr {
            forked = true;
            *identity = Identity {
                author_fpr: Vec::new(),
                title: String::new(),
                language: identity.language.clone(),
                created_at: now,
                birth_nonce: Vec::new(),
            };
        }
        let title = payload.meta.title.clone();
        complete_identity(identity, fpr, &title, &default_language, now, &mut *nonce)?;
    }

    let meta = &mut payload.meta;
    meta.created_at = if forked {
        now
    } else {
        previous
            .map(|p| p.meta.created_at)
            .filter(|t| t.0 != 0 && *t <= now)
            .unwrap_or(if meta.created_at.0 == 0 || meta.created_at > now {
                now
            } else {
                meta.created_at
            })
    };
    meta.modified_at = now;

    let signer_author = Author {
        uid: signer.user_id()?,
        display_name: display_name.trim().to_owned(),
        keyset: signer.public_keyset(),
    };
    let referenced: Vec<Vec<u8>> = payload
        .recipes
        .iter()
        .map(|r| r.identity.author_fpr.clone())
        .chain(meta.identity.iter().map(|i| i.author_fpr.clone()))
        .collect();
    let mut authors = vec![signer_author];
    for author in &meta.authors {
        let keep = crate::identity::key_fingerprint(&author.keyset)
            .is_ok_and(|f| f != fpr && referenced.contains(&f.to_vec()));
        if keep {
            authors.push(author.clone());
        }
    }
    meta.authors = authors;

    payload.v = FORMAT_VERSION;
    payload.min_v = MIN_READER_VERSION;
    payload.doc_id = match payload.kind {
        Kind::Recipe => DocId(
            payload
                .recipes
                .first()
                .map(|r| r.id.0.clone())
                .unwrap_or_default(),
        ),
        Kind::Book => DocId(
            payload
                .meta
                .identity
                .as_ref()
                .map(derive_book_id)
                .transpose()?
                .map(|id| id.0)
                .unwrap_or_default(),
        ),
    };
    Ok(payload)
}

/// Runs every Phase B–D check on a prepared payload.
fn check_prepared(payload: &BundlePayload) -> Vec<Issue> {
    let mut issues = Vec::new();
    for (_, found) in verify::shape_checks(payload) {
        issues.extend(found.0);
    }
    for (_, found) in verify::structural_checks(payload, None) {
        issues.extend(found.0);
    }
    issues
}

/// Validates an editor draft, returning every problem with a JSON Pointer to
/// the offending field. Derived fields (ids, revisions, fingerprints, times,
/// nonces) are filled with placeholders first, so only what the author wrote
/// is judged. An empty list means [`seal`] will succeed.
#[must_use]
pub fn validate_draft(draft: &BundlePayload) -> Vec<Issue> {
    let span = info_span!("validate_draft", recipes = draft.recipes.len());
    let _guard = span.enter();
    let placeholder = SecretKeySet::from_seed(&[0u8; 32]);
    let now = draft
        .recipes
        .iter()
        .map(|r| r.identity.created_at)
        .chain([draft.meta.created_at])
        .max()
        .unwrap_or(Timestamp(0))
        .max(Timestamp(1));
    match prepare(draft, &placeholder, "draft", now, None, &mut || {
        Ok(vec![0u8; 16])
    }) {
        Ok(prepared) => check_prepared(&prepared),
        Err(e) => vec![Issue::from_error("", &e)],
    }
}

/// Seals a draft into a signed `.crisp` file.
///
/// `previous` is the file being edited, if any: the owner's edit becomes a
/// revision of it and anyone else's a fork. `display_name` is the name the
/// signer is listed under.
///
/// # Errors
///
/// Returns a [`CrispError::Check`] naming the first problem if the draft
/// would not verify (run [`validate_draft`] for all of them), or an error if
/// `previous` cannot be decoded or signing fails.
pub fn seal(
    draft: &BundlePayload,
    signer: &SecretKeySet,
    display_name: &str,
    now: Timestamp,
    previous: Option<&[u8]>,
) -> Result<Vec<u8>> {
    seal_with(draft, signer, display_name, now, previous, Signing::Hedged)
}

/// [`seal`] with an explicit signing mode.
pub(crate) fn seal_with(
    draft: &BundlePayload,
    signer: &SecretKeySet,
    display_name: &str,
    now: Timestamp,
    previous: Option<&[u8]>,
    signing: Signing,
) -> Result<Vec<u8>> {
    let span = info_span!(
        "seal",
        recipes = draft.recipes.len(),
        revising = previous.is_some()
    );
    let _guard = span.enter();
    let previous = previous
        .map(|bytes| {
            let envelope = decode_file(bytes)?;
            from_canonical_cbor::<BundlePayload>(&envelope.signed_bundle.payload)
        })
        .transpose()?;
    let payload = prepare(
        draft,
        signer,
        display_name,
        now,
        previous.as_ref(),
        &mut || {
            let mut nonce = vec![0u8; 16];
            getrandom::fill(&mut nonce)
                .map_err(|e| CrispError::crypto(format!("system RNG unavailable: {e}")))?;
            Ok(nonce)
        },
    )?;
    if let Some(issue) = check_prepared(&payload).first() {
        return Err(issue.to_error());
    }
    let envelope = author_sign(&payload, signer, now, signing)?;
    let bytes = encode_file(&envelope)?;
    debug!(doc_id = %payload.doc_id.0, bytes = bytes.len(), "sealed");
    Ok(bytes)
}

/// Builds one signature over a bundle's protected header.
fn make_signature(
    protected_header: &[u8],
    key: &SecretKeySet,
    role: SigRole,
    signed_at: Timestamp,
    binds: Option<[u8; 32]>,
    signing: Signing,
) -> Result<Signature> {
    let sig_protected = SigProtected {
        alg: SigAlg::MlDsa65Ed25519,
        role,
        kid: key.fingerprint()?.to_vec(),
        signed_at,
        binds: binds.map(|b| b.to_vec()),
    };
    let sig_protected_bytes = to_canonical_cbor(&sig_protected)?;
    let aad = binds.map_or_else(Vec::new, |b| b.to_vec());
    let message = signing_message(protected_header, &sig_protected_bytes, &aad)?;
    let sig_value = match signing {
        Signing::Hedged => key.sign(&message)?,
        #[cfg(test)]
        Signing::Deterministic => key.sign_deterministic(&message)?,
    };
    Ok(Signature {
        protected: sig_protected_bytes,
        unprotected: Value::Map(Vec::new()),
        sig_value,
    })
}

/// Assembles an author-signed [`CrispEnvelope`] from a prepared payload.
pub(crate) fn author_sign(
    payload: &BundlePayload,
    author: &SecretKeySet,
    signed_at: Timestamp,
    signing: Signing,
) -> Result<CrispEnvelope> {
    let payload_bytes = to_canonical_cbor(payload)?;
    let protected = ProtectedHeader {
        format_version: payload.v,
        doc_id: payload.doc_id.clone(),
        content_hash: sha256(&payload_bytes).to_vec(),
    };
    let protected_bytes = to_canonical_cbor(&protected)?;
    let signature = make_signature(
        &protected_bytes,
        author,
        SigRole::Author,
        signed_at,
        None,
        signing,
    )?;
    Ok(CrispEnvelope {
        container_version: CONTAINER_VERSION,
        signed_bundle: CoseSign {
            protected: protected_bytes,
            unprotected: Value::Map(Vec::new()),
            payload: payload_bytes,
            signatures: vec![signature],
        },
    })
}

/// Adds the platform's server endorsement to an author-signed envelope,
/// bound to every author signature (spec §8.3).
///
/// # Errors
///
/// Returns [`CrispError::Input`] if the envelope has no author signature or
/// already carries an endorsement, or an error if signing fails.
pub fn endorse(
    envelope: &CrispEnvelope,
    platform: &SecretKeySet,
    signed_at: Timestamp,
) -> Result<CrispEnvelope> {
    endorse_with(envelope, platform, signed_at, Signing::Hedged)
}

/// [`endorse`] with an explicit signing mode.
pub(crate) fn endorse_with(
    envelope: &CrispEnvelope,
    platform: &SecretKeySet,
    signed_at: Timestamp,
    signing: Signing,
) -> Result<CrispEnvelope> {
    let parsed = signatures::parse(&envelope.signed_bundle);
    if parsed.error.is_some() || !parsed.endorsements.is_empty() {
        return Err(CrispError::input(
            "the bundle is already endorsed or has a malformed signature",
        ));
    }
    if parsed.authors.is_empty() {
        return Err(CrispError::input(
            "cannot endorse a bundle with no author signature",
        ));
    }
    let authors: Vec<&Signature> = parsed.authors.iter().map(|(_, s)| *s).collect();
    let binds = author_binding(&authors)?;
    let endorsement = make_signature(
        &envelope.signed_bundle.protected,
        platform,
        SigRole::ServerEndorsement,
        signed_at,
        Some(binds),
        signing,
    )?;
    let mut endorsed = envelope.clone();
    endorsed.signed_bundle.signatures.push(endorsement);
    Ok(endorsed)
}

/// Rebuilds a draft (with derived fields intact) from a `.crisp` file, for
/// editing. Signatures are dropped; [`seal`] re-derives everything.
///
/// # Errors
///
/// Returns an error if the file cannot be decoded.
pub fn open_draft(bytes: &[u8]) -> Result<BundlePayload> {
    let envelope = decode_file(bytes)?;
    from_canonical_cbor(&envelope.signed_bundle.payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::fixtures::draft_payload;
    use crate::verify::{PlatformKey, Trust, verify};

    fn platform_key(key: &SecretKeySet) -> PlatformKey {
        PlatformKey {
            keyset: key.public_keyset(),
            valid_from: Timestamp(0),
            valid_until: None,
        }
    }

    #[test]
    fn a_sealed_draft_verifies_as_author_signed() {
        let author = SecretKeySet::from_seed(&[11u8; 32]);
        let bytes = seal(&draft_payload(), &author, "Baker", Timestamp(2_000), None).expect("seal");
        let report = verify(&bytes, &[], Timestamp(3_000));
        assert!(report.compliant, "{report:#?}");
        assert_eq!(report.trust, Trust::AuthorSigned);
    }

    #[test]
    fn an_endorsed_bundle_is_published() {
        let author = SecretKeySet::from_seed(&[12u8; 32]);
        let platform = SecretKeySet::from_seed(&[99u8; 32]);
        let bytes = seal(&draft_payload(), &author, "Baker", Timestamp(2_000), None).expect("seal");
        let envelope = decode_file(&bytes).expect("decode");
        let endorsed = endorse(&envelope, &platform, Timestamp(2_100)).expect("endorse");
        assert!(
            endorse(&endorsed, &platform, Timestamp(2_200)).is_err(),
            "no double endorsement"
        );
        let bytes = encode_file(&endorsed).expect("encode");
        let report = verify(&bytes, &[platform_key(&platform)], Timestamp(3_000));
        assert_eq!(report.trust, Trust::Endorsed, "{report:#?}");
        assert_eq!(report.class, verify::ComplianceClass::Published);
    }

    #[test]
    fn the_owners_edit_is_a_revision_and_a_strangers_edit_is_a_fork() {
        let owner = SecretKeySet::from_seed(&[13u8; 32]);
        let stranger = SecretKeySet::from_seed(&[14u8; 32]);
        let first = seal(&draft_payload(), &owner, "Owner", Timestamp(2_000), None).expect("seal");
        let original = open_draft(&first).expect("open");

        let mut edit = original.clone();
        edit.recipes[0]
            .title
            .0
            .insert(LanguageTag("en".to_owned()), "Better cookies".to_owned());
        let revised = seal(&edit, &owner, "Owner", Timestamp(3_000), Some(&first)).expect("revise");
        let revised = open_draft(&revised).expect("open");
        assert_eq!(
            revised.recipes[0].id, original.recipes[0].id,
            "the id is stable"
        );
        assert_eq!(
            revised.recipes[0].supersedes,
            Some(original.recipes[0].revision.clone())
        );
        assert_ne!(revised.recipes[0].revision, original.recipes[0].revision);

        let forked =
            seal(&edit, &stranger, "Stranger", Timestamp(3_000), Some(&first)).expect("fork");
        let report = verify(&forked, &[], Timestamp(4_000));
        assert!(report.compliant, "{report:#?}");
        let forked = open_draft(&forked).expect("open");
        assert_ne!(forked.recipes[0].id, original.recipes[0].id);
        assert_eq!(
            forked.recipes[0].derived_from,
            Some(original.recipes[0].id.clone())
        );
        assert_eq!(
            forked.meta.authors.len(),
            1,
            "the fork lists only its own author"
        );
    }

    #[test]
    fn validate_draft_locates_problems() {
        let mut draft = draft_payload();
        draft.recipes[0].steps[0].ingredients = Some(vec![crate::model::recipe::StepIngredient {
            r#ref: 42,
            quantity: None,
        }]);
        let issues = validate_draft(&draft);
        assert!(
            issues
                .iter()
                .any(|i| i.code == ErrorCode::Ref
                    && i.path == "/recipes/0/steps/0/ingredients/0/ref"),
            "{issues:#?}"
        );
        assert!(validate_draft(&draft_payload()).is_empty());
    }

    #[test]
    fn seal_normalizes_text_to_nfc() {
        let author = SecretKeySet::from_seed(&[15u8; 32]);
        let mut draft = draft_payload();
        draft.recipes[0]
            .title
            .0
            .insert(LanguageTag("en".to_owned()), "Cre\u{301}pes".to_owned());
        let bytes = seal(&draft, &author, "Baker", Timestamp(2_000), None).expect("seal");
        let sealed = open_draft(&bytes).expect("open");
        assert_eq!(
            sealed.recipes[0].title.0[&LanguageTag("en".to_owned())],
            "Crépes"
        );
        assert!(verify(&bytes, &[], Timestamp(3_000)).compliant);
    }
}
