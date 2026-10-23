//! Document-level metadata, authors, and public keys.

use serde::{Deserialize, Serialize};

use crate::model::ids::{BlobId, UserId};
use crate::model::payload::ExtensionMap;
use crate::model::primitives::Timestamp;
use crate::model::recipe::Identity;
use crate::model::text::{LocalizedProse, LocalizedText};

/// An author's composite public key (`crisp-v1.cddl`: `public-key-set`).
///
/// Its fingerprint ([`crate::identity::key_fingerprint`]) appears as a
/// recipe's `identity.author_fpr`, as a signature's `kid`, and inside the
/// author's `usr…` id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicKeySet {
    /// The Ed25519 public key (32 bytes).
    #[serde(with = "serde_bytes")]
    pub ed25519: Vec<u8>,
    /// The ML-DSA-65 public key (1952 bytes).
    #[serde(rename = "ml-dsa-65", with = "serde_bytes")]
    pub ml_dsa_65: Vec<u8>,
}

/// An author of a recipe or book (`crisp-v1.cddl`: `author`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    /// The author's user id (`usr…`), derived from `keyset`.
    pub uid: UserId,
    /// A human-readable display name.
    pub display_name: String,
    /// The author's composite public key set.
    pub keyset: PublicKeySet,
}

/// The metadata common to every bundle (`crisp-v1.cddl`: `document-meta`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentMeta {
    /// The document title.
    pub title: LocalizedText,
    /// The authors; at least one, at most 64.
    pub authors: Vec<Author>,
    /// An SPDX license identifier, or `"NONE"`.
    pub license: String,
    /// When the document was created (frozen at birth).
    pub created_at: Timestamp,
    /// When the document was last edited.
    pub modified_at: Timestamp,
    /// The frozen identity tuple — required if, and only if, the bundle is a
    /// book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    /// An optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedProse>,
    /// An optional cover image blob.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<BlobId>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}
