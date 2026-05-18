//! Identity: deriving the stable, content-bound identifiers of spec §7.
//!
//! A recipe's id is a domain-separated SHA-256 over its five frozen identity
//! fields; the revision hash is a SHA-256 over the whole recipe (minus the
//! `revision` and `version` keys). Re-deriving these and checking them
//! against what a bundle claims is how the verifier proves a recipe cannot
//! impersonate an identity it does not own.

pub mod derive;

pub use derive::{
    derive_blob_id, derive_book_id, derive_recipe_id, derive_revision, derive_user_id,
    key_fingerprint, normalize_title,
};
