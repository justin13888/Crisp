//! Domain-separated derivation of every Crisp identifier (spec §7,
//! Appendix A).

use ciborium::value::Value;
use unicode_normalization::UnicodeNormalization;

use crate::cbor::encode_canonical;
use crate::crypto::bech32id::{Hrp, encode_id};
use crate::crypto::hash::sha256;
use crate::error::{CrispError, Result};
use crate::model::ids::{BlobId, BookId, RecipeId, RevisionId, UserId};
use crate::model::meta::PublicKeySet;
use crate::model::recipe::{Identity, Recipe};

/// Domain separator for recipe-id derivation.
const RECIPE_ID_DOMAIN: &str = "crisp/recipe-id/1";
/// Domain separator for book-id derivation.
const BOOK_ID_DOMAIN: &str = "crisp/book-id/1";
/// Domain separator for user-id derivation.
const USER_ID_DOMAIN: &str = "crisp/user-id/1";

/// Normalizes a title for identity derivation: Unicode NFC, then trimmed with
/// every internal run of whitespace collapsed to a single space. Case is
/// preserved (spec decision D11).
#[must_use]
pub fn normalize_title(title: &str) -> String {
    let composed: String = title.nfc().collect();
    composed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Encodes a CBOR value to canonical bytes for use as a hash preimage.
fn preimage(value: &Value) -> Result<Vec<u8>> {
    encode_canonical(value)
}

/// Serializes a model value to a CBOR [`Value`].
fn to_value<T: serde::Serialize>(value: &T) -> Result<Value> {
    Value::serialized(value)
        .map_err(|e| CrispError::input(format!("could not encode value for id derivation: {e}")))
}

/// Builds the shared id preimage for a recipe or book from an [`Identity`].
fn identity_preimage(domain: &str, identity: &Identity) -> Result<Vec<u8>> {
    preimage(&Value::Array(vec![
        Value::Text(domain.to_owned()),
        Value::Bytes(identity.author_fpr.clone()),
        Value::Text(normalize_title(&identity.title)),
        Value::Text(identity.language.0.clone()),
        Value::Integer(identity.created_at.0.into()),
        Value::Bytes(identity.birth_nonce.clone()),
    ]))
}

/// Derives the `rcp…` id of a recipe from its frozen [`Identity`].
///
/// # Errors
///
/// Returns an error if CBOR encoding or bech32m encoding fails.
pub fn derive_recipe_id(identity: &Identity) -> Result<RecipeId> {
    let digest = sha256(&identity_preimage(RECIPE_ID_DOMAIN, identity)?);
    Ok(RecipeId(encode_id(Hrp::Recipe, &digest)?))
}

/// Derives the `rbk…` id of a book from its frozen [`Identity`].
///
/// # Errors
///
/// Returns an error if CBOR encoding or bech32m encoding fails.
pub fn derive_book_id(identity: &Identity) -> Result<BookId> {
    let digest = sha256(&identity_preimage(BOOK_ID_DOMAIN, identity)?);
    Ok(BookId(encode_id(Hrp::Book, &digest)?))
}

/// Derives the `usr…` id that a [`PublicKeySet`] owns.
///
/// # Errors
///
/// Returns an error if CBOR encoding or bech32m encoding fails.
pub fn derive_user_id(keyset: &PublicKeySet) -> Result<UserId> {
    let bytes = preimage(&Value::Array(vec![
        Value::Text(USER_ID_DOMAIN.to_owned()),
        to_value(keyset)?,
    ]))?;
    Ok(UserId(encode_id(Hrp::User, &sha256(&bytes))?))
}

/// Computes the 32-byte fingerprint of a [`PublicKeySet`].
///
/// This is `SHA-256` of the keyset's canonical-CBOR encoding — the value that
/// appears as a recipe's `identity.author_fpr` and as a signature's `kid`.
///
/// # Errors
///
/// Returns an error if CBOR encoding fails.
pub fn key_fingerprint(keyset: &PublicKeySet) -> Result<[u8; 32]> {
    Ok(sha256(&preimage(&to_value(keyset)?)?))
}

/// Derives the `rev…` revision hash naming this exact version of a recipe.
///
/// Per Appendix A, the hash is `SHA-256` of the recipe's canonical CBOR with
/// the `revision` and `version` keys removed, so it is stable to recompute
/// and changes with every other edit.
///
/// # Errors
///
/// Returns an error if the recipe cannot be encoded, or if its CBOR form is
/// unexpectedly not a map.
pub fn derive_revision(recipe: &Recipe) -> Result<RevisionId> {
    let Value::Map(entries) = to_value(recipe)? else {
        return Err(CrispError::input("a recipe must encode as a CBOR map"));
    };
    let kept: Vec<(Value, Value)> = entries
        .into_iter()
        .filter(
            |(key, _)| !matches!(key, Value::Text(name) if name == "revision" || name == "version"),
        )
        .collect();
    let digest = sha256(&preimage(&Value::Map(kept))?);
    Ok(RevisionId(encode_id(Hrp::Revision, &digest)?))
}

/// Derives the `blb…` content-addressed id of a blob's bytes.
///
/// # Errors
///
/// Returns an error if bech32m encoding fails.
pub fn derive_blob_id(bytes: &[u8]) -> Result<BlobId> {
    Ok(BlobId(encode_id(Hrp::Blob, &sha256(bytes))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{sample_identity, sample_keyset, sample_recipe};
    use crate::model::primitives::Timestamp;

    #[test]
    fn normalize_title_trims_and_collapses_whitespace() {
        assert_eq!(normalize_title("  Hello   World  "), "Hello World");
        assert_eq!(normalize_title("Tab\tand\nnewline"), "Tab and newline");
    }

    #[test]
    fn normalize_title_preserves_case() {
        assert_eq!(normalize_title("CrIsP CoOkIeS"), "CrIsP CoOkIeS");
    }

    #[test]
    fn recipe_id_is_deterministic_and_well_formed() {
        let identity = sample_identity();
        let first = derive_recipe_id(&identity).expect("derive");
        let second = derive_recipe_id(&identity).expect("derive");
        assert_eq!(first, second);
        assert!(first.0.starts_with("rcp1"));
        assert_eq!(first.0.len(), 63);
    }

    #[test]
    fn a_different_birth_nonce_changes_the_recipe_id() {
        let mut identity = sample_identity();
        let original = derive_recipe_id(&identity).expect("derive");
        identity.birth_nonce = vec![9u8; 16];
        assert_ne!(derive_recipe_id(&identity).expect("derive"), original);
    }

    #[test]
    fn recipe_and_book_ids_differ_for_the_same_identity() {
        let identity = sample_identity();
        let recipe = derive_recipe_id(&identity).expect("derive");
        let book = derive_book_id(&identity).expect("derive");
        assert!(book.0.starts_with("rbk1"));
        assert_ne!(recipe.0, book.0, "domain separation must distinguish them");
    }

    #[test]
    fn user_id_is_well_formed_and_bound_to_the_keyset() {
        let id = derive_user_id(&sample_keyset()).expect("derive");
        assert!(id.0.starts_with("usr1"));
        let mut other = sample_keyset();
        other.ed25519 = vec![9u8; 32];
        assert_ne!(derive_user_id(&other).expect("derive"), id);
    }

    #[test]
    fn revision_ignores_the_revision_and_version_keys() {
        let mut recipe = sample_recipe();
        let baseline = derive_revision(&recipe).expect("derive");
        assert!(baseline.0.starts_with("rev1"));

        // Mutating `revision` or `version` must not change the hash.
        recipe.revision = RevisionId("rev-something-else".to_owned());
        recipe.version = Some(42);
        assert_eq!(derive_revision(&recipe).expect("derive"), baseline);

        // Mutating any other field must change it.
        recipe.modified_at = Timestamp(9_999);
        assert_ne!(derive_revision(&recipe).expect("derive"), baseline);
    }

    #[test]
    fn blob_id_is_content_addressed() {
        let a = derive_blob_id(b"image-one").expect("derive");
        let b = derive_blob_id(b"image-two").expect("derive");
        assert!(a.0.starts_with("blb1"));
        assert_ne!(a, b);
        assert_eq!(a, derive_blob_id(b"image-one").expect("derive"));
    }
}
