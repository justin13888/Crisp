//! The published shape of the Crisp ingredient index (`spec/ingredients/`).
//!
//! These types are the contract between the importer that writes the
//! dataset (`tools/ingredients`) and every reader of it. On disk the dataset
//! is split for review — `entries.json`, `locales.json`, one
//! `names/<shard>.json` per locale shard, and `VERSION` — and embedded into
//! `crisp-core` as one CBOR [`Dataset`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::primitives::{OpenCode, Rational};
use crate::model::vocab::{AllergenCode, DietCode};

/// The whole index: entries, the locale table, and localized names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    /// The dataset version: the SHA-256 (hex) of the generated files.
    pub version: String,
    /// The locale shards and how a language tag falls back across them.
    pub locales: LocaleTable,
    /// Every entry, sorted by slug.
    pub entries: Vec<Entry>,
    /// Localized names: shard tag → slug → names.
    pub names: BTreeMap<String, BTreeMap<String, Names>>,
}

/// One ingredient: a permanent slug in the hierarchy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The permanent identifier, e.g. `kimchi-aged`.
    pub slug: String,
    /// The more general entry this one specializes, e.g. `kimchi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// A coarse grouping for browsing and shopping lists, e.g. `fermented`.
    pub category: String,
    /// Grams per millilitre, when known; lets a reader relate volume and mass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Rational>,
    /// Allergens the ingredient usually carries (advisory hints only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allergens: Vec<OpenCode<AllergenCode>>,
    /// Diets the ingredient usually does not suit (advisory hints only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diet_conflicts: Vec<OpenCode<DietCode>>,
    /// The entry this one was merged into; slugs are never deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
    /// Where the entry came from, e.g. `off:en:kimchi` or `crisp:override`;
    /// re-imports map back to an existing slug through it.
    pub source: String,
}

/// An entry's names in one locale shard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Names {
    /// The preferred display name.
    pub name: String,
    /// Other spellings and synonyms a cook might type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

/// The locale shards and their fallback order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleTable {
    /// Every shard, as a region-qualified BCP 47 tag, e.g. `zh-Hant-HK`.
    pub shards: Vec<String>,
    /// Lowercase tag (or tag prefix) → the shards to try, in order, after
    /// an exact shard match. A tag is looked up, then each truncation of it
    /// (`zh-hant-mo`, `zh-hant`, `zh`).
    pub fallback: BTreeMap<String, Vec<String>>,
    /// Shards tried after everything else (English).
    pub last_resort: Vec<String>,
}
