//! The book structure: a fixed-depth tree of parts, chapters, and entries
//! (`crisp-v1.cddl`: `book-structure`). There is no recursion.

use serde::{Deserialize, Serialize};

use crate::model::ids::BlobId;
use crate::model::payload::ExtensionMap;
use crate::model::text::{LocalizedProse, LocalizedText};

/// The top-level structure of a book bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookStructure {
    /// Optional front-matter pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub front_matter: Option<Vec<Page>>,
    /// The book's parts; at least one.
    pub parts: Vec<Part>,
    /// Optional back-matter pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_matter: Option<Vec<Page>>,
}

/// A part of a book (`crisp-v1.cddl`: `part`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// The part title.
    pub title: LocalizedText,
    /// The chapters in this part; at least one.
    pub chapters: Vec<Chapter>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// A chapter of a book (`crisp-v1.cddl`: `chapter`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chapter {
    /// The chapter title.
    pub title: LocalizedText,
    /// An optional chapter introduction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<LocalizedProse>,
    /// The chapter's entries; at least one.
    pub entries: Vec<Entry>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// A chapter entry — either a recipe reference or an inline page
/// (`crisp-v1.cddl`: `entry`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Entry {
    /// A reference to a recipe by its index into `bundle-payload.recipes`.
    Recipe {
        /// The index into the bundle's `recipes` array.
        recipe: u64,
    },
    /// An inline content page.
    Page {
        /// The page content.
        page: Page,
    },
}

/// A non-recipe content page (`crisp-v1.cddl`: `page`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    /// The page title.
    pub title: LocalizedText,
    /// The page body.
    pub body: LocalizedProse,
    /// An optional page image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}
