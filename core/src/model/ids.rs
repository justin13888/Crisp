//! Strongly-typed identifier newtypes.
//!
//! Every identifier is a 63-character bech32m string (see
//! [`crate::crypto::bech32id`]). Distinct newtypes per kind keep a book id
//! from being used where a recipe id is expected. The strings are stored
//! verbatim as they appear on the wire; deriving and checking them against
//! their content is the job of [`crate::identity`] and [`crate::verify`].

use serde::{Deserialize, Serialize};

/// A recipe identifier (`rcp…`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeId(pub String);

/// A book identifier (`rbk…`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BookId(pub String);

/// A user identifier (`usr…`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub String);

/// A revision hash (`rev…`) naming one exact version of a recipe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RevisionId(pub String);

/// A content-addressed blob identifier (`blb…`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobId(pub String);

/// A document identifier — a bundle's `doc_id`, which is a recipe id for a
/// recipe bundle and a book id for a book bundle.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocId(pub String);
