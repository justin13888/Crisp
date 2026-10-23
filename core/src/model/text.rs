//! Localized text and the non-executable "prose" rich-text structure.
//!
//! Localization is first-class: every human-readable field is a map from
//! [`LanguageTag`] to text. Rich text is pre-parsed structure — `prose` is a
//! list of paragraphs, each a list of styled runs — never a markup string,
//! so there is no parser attack surface (`crisp-v1.cddl` §"localized content").

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::ids::RecipeId;
use crate::model::primitives::LanguageTag;

/// A single-line string in one or more languages (`crisp-v1.cddl`:
/// `localized-text`). Must be non-empty.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalizedText(pub BTreeMap<LanguageTag, String>);

/// A rich-text block in one or more languages (`crisp-v1.cddl`:
/// `localized-prose`). Must be non-empty.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalizedProse(pub BTreeMap<LanguageTag, Prose>);

/// A rich-text body: an ordered list of paragraphs (`crisp-v1.cddl`: `prose`).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Prose(pub Vec<Paragraph>);

/// A paragraph: an ordered list of styled runs (`crisp-v1.cddl`: `paragraph`).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Paragraph(pub Vec<Run>);

/// A run of text with uniform styling (`crisp-v1.cddl`: `run`).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    /// The literal text of the run (NFC-normalized).
    pub t: String,
    /// Optional style flags applied to the whole run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<Vec<StyleFlag>>,
    /// An optional outbound hyperlink; always an `https://` URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// An optional typed internal reference.
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<RunRef>,
}

/// A character-style flag for a [`Run`] (`crisp-v1.cddl`: `style-flag`).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StyleFlag {
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Underlined text.
    Underline,
    /// Monospace / code text.
    Code,
}

/// A typed pointer from a run to another element (`crisp-v1.cddl`: `run-ref`).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRef {
    /// What kind of element the reference points at.
    pub kind: RunRefKind,
    /// The target — a within-recipe handle, or a recipe id.
    pub target: RunTarget,
}

/// The kind of element a [`RunRef`] addresses.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunRefKind {
    /// Points at an ingredient by its within-recipe `ref` handle.
    Ingredient,
    /// Points at a step by its within-recipe `ref` handle.
    Step,
    /// Points at a piece of equipment by its within-recipe `ref` handle.
    Equipment,
    /// Points at a recipe by its id.
    Recipe,
}

/// The target of a [`RunRef`]: either a numeric within-recipe handle or a
/// recipe identifier.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunTarget {
    /// A within-recipe `ref` handle (used with the `ingredient`, `step`, and
    /// `equipment` kinds).
    Handle(u64),
    /// A recipe id (used with the `recipe` kind).
    Recipe(RecipeId),
}
