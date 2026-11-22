//! The Crisp ingredient index: permanent, hierarchical ingredient slugs with
//! localized names (spec §13, `docs/ingredient-index.md`).
//!
//! A recipe names an ingredient in free text and may also carry the slug of
//! the most specific index entry it is (`ingredient.code`). The index is an
//! **open vocabulary**: a bundle never needs it to verify or render, and a
//! code this index does not know is still valid. What the index adds is
//! knowledge — the hierarchy (`kimchi-aged` is a `kimchi`), names in each
//! locale shard, density, and allergen/diet hints — and the matcher that
//! links typed names to slugs.
//!
//! [`IngredientIndex::new`] validates a [`Dataset`] once and builds the
//! lookup tables every query uses.

mod dataset;
mod density;
mod locale;
mod matcher;

#[cfg(test)]
mod density_tests;
#[cfg(test)]
mod embedded_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use tracing::{debug, error};

pub use dataset::{Dataset, Entry, LocaleTable, Names};
pub use density::{convert_with_density, density_measure};
pub use matcher::{Candidate, Match, normalize};

use crate::verify::schema::is_token;

/// Why a dataset could not become an index.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    /// The embedded bytes are not a CBOR [`Dataset`].
    #[error("the ingredient dataset does not decode: {0}")]
    Decode(String),
    /// A slug is not a lowercase hyphenated slug.
    #[error("{0:?} is not a valid slug")]
    InvalidSlug(String),
    /// Two entries share a slug.
    #[error("slug {0:?} is listed twice")]
    DuplicateSlug(String),
    /// A `parent` or `replaced_by` names no entry.
    #[error("{slug:?} refers to unknown entry {target:?}")]
    UnknownTarget {
        /// The entry holding the reference.
        slug: String,
        /// The missing entry.
        target: String,
    },
    /// Following `parent` or `replaced_by` from an entry loops.
    #[error("following {0:?} up the hierarchy or its replacements loops")]
    Cycle(String),
    /// Names, a fallback list, or `last_resort` use a tag that is not a shard.
    #[error("{0:?} is not a declared locale shard")]
    UnknownShard(String),
    /// A fallback key is not a lowercase tag, so no lookup could reach it.
    #[error("fallback key {0:?} is not lowercase")]
    FallbackKey(String),
    /// A shard lists names for a slug that is not an entry.
    #[error("shard {shard:?} names unknown entry {slug:?}")]
    UnknownNamed {
        /// The shard.
        shard: String,
        /// The unknown slug.
        slug: String,
    },
    /// A display name is empty.
    #[error("shard {shard:?} gives {slug:?} an empty name")]
    EmptyName {
        /// The shard.
        shard: String,
        /// The entry.
        slug: String,
    },
}

/// The published index, as CBOR, embedded at build time from
/// `spec/ingredients/` (see `core/build.rs`).
static EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ingredients.cbor"));

/// The embedded index, decoded and validated on first use.
static INDEX: LazyLock<Result<IngredientIndex, IndexError>> = LazyLock::new(|| {
    let index = IngredientIndex::from_cbor(EMBEDDED);
    if let Err(e) = &index {
        error!(error = %e, "the embedded ingredient index is invalid");
    }
    index
});

/// The Crisp ingredient index this build carries.
///
/// # Errors
/// The [`IndexError`] the embedded dataset fails validation with — which a
/// release build never does, since its tests decode it.
pub fn index() -> Result<&'static IngredientIndex, IndexError> {
    INDEX.as_ref().map_err(Clone::clone)
}

/// A display name and the shard it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizedName<'a> {
    /// The name.
    pub name: &'a str,
    /// The shard it was found in, which may be a fallback.
    pub shard: &'a str,
}

/// A validated, queryable ingredient index.
#[derive(Debug, Clone)]
pub struct IngredientIndex {
    version: String,
    locales: LocaleTable,
    entries: BTreeMap<String, Entry>,
    names: BTreeMap<String, BTreeMap<String, Names>>,
    /// Shard → normalized spelling → (slug, spelling as written), sorted
    /// and unique; folded once here so matching never re-normalizes.
    spellings: BTreeMap<String, BTreeMap<String, Vec<(String, String)>>>,
}

impl IngredientIndex {
    /// Decodes a CBOR [`Dataset`] and validates it.
    ///
    /// # Errors
    /// [`IndexError::Decode`] when the bytes are not a dataset; otherwise as
    /// [`IngredientIndex::new`].
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, IndexError> {
        let dataset: Dataset =
            ciborium::from_reader(bytes).map_err(|e| IndexError::Decode(e.to_string()))?;
        Self::new(dataset)
    }

    /// Validates a dataset and builds its lookup tables.
    ///
    /// # Errors
    /// An [`IndexError`] naming the first inconsistency found: an invalid or
    /// duplicate slug, a dangling or cyclic `parent`/`replaced_by`, an
    /// undeclared shard, or names for an unknown entry.
    pub fn new(dataset: Dataset) -> Result<Self, IndexError> {
        let Dataset {
            version,
            locales,
            entries: list,
            names,
        } = dataset;
        let mut entries = BTreeMap::new();
        for entry in list {
            if !is_token(&entry.slug, '-') {
                return Err(IndexError::InvalidSlug(entry.slug));
            }
            if let Some(previous) = entries.insert(entry.slug.clone(), entry) {
                return Err(IndexError::DuplicateSlug(previous.slug));
            }
        }
        for entry in entries.values() {
            for target in [&entry.parent, &entry.replaced_by].into_iter().flatten() {
                if !entries.contains_key(target) {
                    return Err(IndexError::UnknownTarget {
                        slug: entry.slug.clone(),
                        target: target.clone(),
                    });
                }
            }
        }
        // An acyclic chain of links is shorter than the number of entries.
        let limit = entries.len();
        for slug in entries.keys() {
            let links: [fn(&Entry) -> Option<String>; 2] =
                [|e| e.parent.clone(), |e| e.replaced_by.clone()];
            for link in links {
                let mut at = slug.clone();
                for hop in 0.. {
                    if hop > limit {
                        return Err(IndexError::Cycle(slug.clone()));
                    }
                    match entries.get(&at).and_then(link) {
                        Some(next) => at = next,
                        None => break,
                    }
                }
            }
        }
        locale::validate(&locales)?;
        let mut spellings: BTreeMap<String, BTreeMap<String, Vec<(String, String)>>> =
            BTreeMap::new();
        for (shard, by_slug) in &names {
            if !locales.shards.contains(shard) {
                return Err(IndexError::UnknownShard(shard.clone()));
            }
            let table = spellings.entry(shard.clone()).or_default();
            for (slug, n) in by_slug {
                if !entries.contains_key(slug) {
                    return Err(IndexError::UnknownNamed {
                        shard: shard.clone(),
                        slug: slug.clone(),
                    });
                }
                if n.name.trim().is_empty() {
                    return Err(IndexError::EmptyName {
                        shard: shard.clone(),
                        slug: slug.clone(),
                    });
                }
                for spelling in std::iter::once(&n.name).chain(&n.aliases) {
                    let key = normalize(spelling);
                    if key.is_empty() {
                        continue;
                    }
                    let found = table.entry(key).or_default();
                    if !found.iter().any(|(s, _)| s == slug) {
                        found.push((slug.clone(), spelling.clone()));
                        found.sort();
                    }
                }
            }
        }
        debug!(
            version = %version,
            entries = entries.len(),
            shards = locales.shards.len(),
            named_shards = names.len(),
            "ingredient index built"
        );
        Ok(Self {
            version,
            locales,
            entries,
            names,
            spellings,
        })
    }

    /// The dataset version (SHA-256 hex of the generated files).
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// How many entries the index holds, replaced ones included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every entry, in slug order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// The locale shards, in declared order.
    #[must_use]
    pub fn shards(&self) -> &[String] {
        &self.locales.shards
    }

    /// The entry with exactly this slug, replaced or not.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<&Entry> {
        self.entries.get(slug)
    }

    /// The current entry for a slug: the entry itself, or the entry it was
    /// (transitively) merged into.
    #[must_use]
    pub fn resolve(&self, slug: &str) -> Option<&Entry> {
        let mut entry = self.entries.get(slug)?;
        for _ in 0..=self.entries.len() {
            match &entry.replaced_by {
                Some(next) => entry = self.entries.get(next)?,
                None => return Some(entry),
            }
        }
        None
    }

    /// The entry's ancestors, nearest first (the entry itself excluded),
    /// each resolved through `replaced_by` to its current entry.
    #[must_use]
    pub fn ancestors(&self, slug: &str) -> Vec<&Entry> {
        let mut out: Vec<&Entry> = Vec::new();
        let mut seen = std::collections::BTreeSet::from([slug]);
        let mut at = self.entries.get(slug);
        // No chain is longer than the index, which bounds the walk outright.
        for _ in 0..self.entries.len() {
            let Some(parent) = at.and_then(|e| e.parent.as_ref()) else {
                break;
            };
            at = self.resolve(parent);
            let Some(entry) = at else { break };
            // A parent merged back into a descendant would loop forever.
            if !seen.insert(entry.slug.as_str()) {
                break;
            }
            out.push(entry);
        }
        out
    }

    /// Whether `slug` is `ancestor` or one of its descendants, after both are
    /// resolved through `replaced_by`. Unknown slugs are only themselves.
    #[must_use]
    pub fn is_a(&self, slug: &str, ancestor: &str) -> bool {
        let slug = self.resolve(slug).map_or(slug, |e| e.slug.as_str());
        let ancestor = self.resolve(ancestor).map_or(ancestor, |e| e.slug.as_str());
        slug == ancestor || self.ancestors(slug).iter().any(|e| e.slug == ancestor)
    }

    /// Every other entry that [`is_a`](Self::is_a) `slug`, in slug order:
    /// the entries below it, the entries merged into it, and theirs — so
    /// expanding a search by it also finds bundles coded with a merged slug.
    /// A merged `slug` answers for its successor.
    #[must_use]
    pub fn descendants(&self, slug: &str) -> Vec<&Entry> {
        let current = self.resolve(slug).map_or(slug, |e| e.slug.as_str());
        self.entries
            .values()
            .filter(|e| e.slug != current && self.is_a(&e.slug, current))
            .collect()
    }

    /// The shards to consult for a BCP 47 tag, best first (spec: exact shard,
    /// the tag's fallback list and its truncations, the language's other
    /// shards, then English).
    #[must_use]
    pub fn fallback_chain(&self, tag: &str) -> Vec<&str> {
        locale::chain(&self.locales, tag)
    }

    /// The entry's names in exactly one shard.
    #[must_use]
    pub fn names(&self, slug: &str, shard: &str) -> Option<&Names> {
        self.names.get(shard)?.get(slug)
    }

    /// The best display name for a slug in a language, following the
    /// fallback chain.
    #[must_use]
    pub fn display_name(&self, slug: &str, tag: &str) -> Option<LocalizedName<'_>> {
        self.fallback_chain(tag).into_iter().find_map(|shard| {
            let (shard, by_slug) = self.names.get_key_value(shard)?;
            by_slug.get(slug).map(|n| LocalizedName {
                name: &n.name,
                shard,
            })
        })
    }
}
