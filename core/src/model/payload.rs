//! The signed bundle payload — the top of the recipe/book data model
//! (`crisp-v1.cddl`: `bundle-payload`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::blob::Blob;
use crate::model::book::BookStructure;
use crate::model::ids::{BlobId, DocId};
use crate::model::meta::DocumentMeta;
use crate::model::primitives::LanguageTag;
use crate::model::recipe::Recipe;

/// Namespaced, ignorable extension data (`crisp-v1.cddl`: `extension-map`).
///
/// Keys must be reverse-DNS namespaced. Conformant readers ignore unknown
/// entries; the values are opaque CBOR.
pub type ExtensionMap = BTreeMap<String, ciborium::value::Value>;

/// Whether a bundle holds a single recipe or a whole book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A single-recipe bundle.
    Recipe,
    /// A book bundle.
    Book,
}

/// The signed contents of a bundle (`crisp-v1.cddl`: `bundle-payload`).
///
/// `Eq` is not derived because the optional `ext` map holds arbitrary CBOR,
/// which is only `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundlePayload {
    /// The format version, e.g. `0x010000` for v1.0.0.
    pub v: u64,
    /// The minimum reader version required to open this bundle.
    pub min_v: u64,
    /// Whether this bundle is a recipe or a book.
    pub kind: Kind,
    /// The document id; a recipe id or a book id, consistent with `kind`.
    pub doc_id: DocId,
    /// The bundle's default language.
    pub default_language: LanguageTag,
    /// Every language used anywhere in the bundle.
    pub languages: Vec<LanguageTag>,
    /// The document metadata.
    pub meta: DocumentMeta,
    /// The recipes; at least one.
    pub recipes: Vec<Recipe>,
    /// The book structure — present if, and only if, `kind` is `book`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub book: Option<BookStructure>,
    /// The content-addressed blob store, keyed by blob id.
    pub blobs: BTreeMap<BlobId, Blob>,
    /// Namespaced extension data, ignored by conformant readers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}
