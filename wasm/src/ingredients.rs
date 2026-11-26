//! The ingredient index for JavaScript: matching what an author types to
//! index slugs, and reading an entry's localized name and attributes
//! (`docs/ingredient-index.md` §3.3).
//!
//! Matching never rewrites the author's `name`; it only proposes a slug. A
//! slug the index does not know (a code in a received bundle from a newer
//! index) reads as `undefined` here and still renders verbatim.

use crisp_core::ingredients::{Candidate, IngredientIndex, Match, index};
use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::boundary::{failure, to_js};

/// The most candidates one match returns, whatever `limit` asks for.
pub const MAX_CANDIDATES: usize = 20;

/// The outcome of matching a typed ingredient name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Tsify)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngredientMatch {
    /// One unambiguous entry: an editor attaches it at once.
    Exact {
        /// The current slug (after `replaced_by`).
        slug: String,
        /// Its display name in the asked-for language, when it has one.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Entries worth suggesting, best first; the author picks one or none.
    Candidates {
        /// The ranked candidates.
        candidates: Vec<IngredientCandidate>,
    },
    /// Nothing in the index resembles the name.
    None,
}

/// One suggested entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Tsify)]
pub struct IngredientCandidate {
    /// The current slug (after `replaced_by`).
    pub slug: String,
    /// Its display name in the asked-for language (the matched spelling
    /// when it has none).
    pub name: String,
    /// The index spelling that matched, as written in the dataset.
    pub spelling: String,
    /// The shard the spelling came from, e.g. `en-GB`.
    pub shard: String,
    /// Whether that shard is exactly the asked-for tag (not a fallback).
    pub exact_region: bool,
}

/// A display name and the locale shard it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Tsify)]
pub struct IngredientName {
    /// The name.
    pub name: String,
    /// The shard it was found in, which may be a fallback (e.g. `en-US`
    /// for a `fr` lookup of an entry with no French name).
    pub shard: String,
}

/// An index entry's attributes, for display and hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Tsify)]
pub struct IngredientEntry {
    /// The current slug (after `replaced_by`).
    pub slug: String,
    /// The more general entry this one specializes (resolved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// A coarse grouping, e.g. `produce`.
    pub category: String,
    /// Allergens the ingredient usually carries (advisory hints only).
    pub allergens: Vec<String>,
    /// Diets it usually does not suit (advisory hints only).
    pub diet_conflicts: Vec<String>,
    /// Whether the entry relates volume and mass.
    pub has_density: bool,
}

/// The embedded index, or a JS failure in the (never released) case that it
/// does not validate.
fn embedded() -> Result<&'static IngredientIndex, JsValue> {
    index().map_err(|e| failure(&format!("ingredient index unavailable: {e}"), None, None))
}

/// Matches `text` in language `tag`, returning at most `limit` (capped at
/// [`MAX_CANDIDATES`]) candidates when there is no exact link.
#[must_use]
pub fn match_in(index: &IngredientIndex, text: &str, tag: &str, limit: usize) -> IngredientMatch {
    let name = |slug: &str| index.display_name(slug, tag).map(|n| n.name.to_owned());
    match index.match_name(text, tag, limit.min(MAX_CANDIDATES)) {
        Match::Exact(slug) => IngredientMatch::Exact {
            name: name(&slug),
            slug,
        },
        // `match_name` answers `Match::None` rather than an empty list, so
        // every `Candidates` here holds at least one entry.
        Match::Candidates(found) => IngredientMatch::Candidates {
            candidates: found
                .into_iter()
                .map(|c: Candidate| IngredientCandidate {
                    name: name(&c.slug).unwrap_or_else(|| c.spelling.clone()),
                    slug: c.slug,
                    spelling: c.spelling,
                    shard: c.shard,
                    exact_region: c.exact_region,
                })
                .collect(),
        },
        Match::None => IngredientMatch::None,
    }
}

/// The best display name for `slug` (resolved through `replaced_by`) in
/// language `tag`, following the fallback chain.
#[must_use]
pub fn name_in(index: &IngredientIndex, slug: &str, tag: &str) -> Option<IngredientName> {
    let current = index.resolve(slug)?;
    index
        .display_name(&current.slug, tag)
        .map(|n| IngredientName {
            name: n.name.to_owned(),
            shard: n.shard.to_owned(),
        })
}

/// The entry for `slug`, resolved through `replaced_by`.
#[must_use]
pub fn entry_in(index: &IngredientIndex, slug: &str) -> Option<IngredientEntry> {
    let entry = index.resolve(slug)?;
    Some(IngredientEntry {
        slug: entry.slug.clone(),
        parent: entry
            .parent
            .as_deref()
            .map(|p| index.resolve(p).map_or(p, |e| e.slug.as_str()).to_owned()),
        category: entry.category.clone(),
        allergens: entry.allergens.iter().map(|a| a.as_wire()).collect(),
        diet_conflicts: entry.diet_conflicts.iter().map(|d| d.as_wire()).collect(),
        has_density: entry.density.is_some(),
    })
}

/// The primary language subtag of `tag`, lowercased.
fn language(tag: &str) -> String {
    tag.trim()
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The shard a name typed in language `tag` belongs to: the best shard of
/// the fallback chain in that same language, or `None` when the index has
/// no shard for the language (its English last resort is not one).
#[must_use]
pub fn shard_in(index: &IngredientIndex, tag: &str) -> Option<String> {
    let lang = language(tag);
    index
        .fallback_chain(tag)
        .into_iter()
        .find(|shard| language(shard) == lang)
        .map(str::to_owned)
}

/// The locale shard an ingredient suggestion for a recipe in language `tag`
/// (`"fr"`) is filed under (`"fr-FR"`), or `undefined` when the index has
/// no names in that language.
///
/// # Errors
///
/// Throws if the embedded index is unavailable.
#[wasm_bindgen(js_name = ingredientShard)]
pub fn ingredient_shard_js(tag: &str) -> Result<Option<String>, JsValue> {
    Ok(shard_in(embedded()?, tag))
}

/// Matches a typed ingredient name (`"aubergine"`) in language `tag`
/// (`"en-GB"`) against the ingredient index: an `exact` link, ranked
/// `candidates` (at most `limit`, and never more than 20), or `none`.
///
/// # Errors
///
/// Throws if the embedded index is unavailable.
#[wasm_bindgen(js_name = matchIngredient, unchecked_return_type = "IngredientMatch")]
pub fn match_ingredient_js(text: &str, tag: &str, limit: u32) -> Result<JsValue, JsValue> {
    let limit = usize::try_from(limit).unwrap_or(MAX_CANDIDATES);
    to_js(&match_in(embedded()?, text, tag, limit))
}

/// The index's name for `slug` in language `tag`, following the fallback
/// chain; `undefined` for a slug the index does not know.
///
/// # Errors
///
/// Throws if the embedded index is unavailable.
#[wasm_bindgen(
    js_name = ingredientName,
    unchecked_return_type = "IngredientName | undefined"
)]
pub fn ingredient_name_js(slug: &str, tag: &str) -> Result<JsValue, JsValue> {
    match name_in(embedded()?, slug, tag) {
        Some(name) => to_js(&name),
        None => Ok(JsValue::UNDEFINED),
    }
}

/// The index entry for `slug` (category, parent, hints); `undefined` for a
/// slug the index does not know.
///
/// # Errors
///
/// Throws if the embedded index is unavailable.
#[wasm_bindgen(
    js_name = ingredientEntry,
    unchecked_return_type = "IngredientEntry | undefined"
)]
pub fn ingredient_entry_js(slug: &str) -> Result<JsValue, JsValue> {
    match entry_in(embedded()?, slug) {
        Some(entry) => to_js(&entry),
        None => Ok(JsValue::UNDEFINED),
    }
}

// Native, not under Node: the pure functions cross no `JsValue`, so
// `cargo test` (and `cargo mutants`) reaches them here.
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crisp_core::ingredients::{Dataset, Entry, IngredientIndex, LocaleTable, Names, index};

    use super::*;

    fn embedded() -> &'static IngredientIndex {
        index().expect("the embedded index validates")
    }

    /// `old-butter` was merged into `butter`, whose parent `milk-fat` was
    /// merged into `dairy`.
    fn merged() -> IngredientIndex {
        let entry = |slug: &str, parent: Option<&str>, replaced_by: Option<&str>| Entry {
            slug: slug.to_owned(),
            parent: parent.map(str::to_owned),
            category: "dairy".to_owned(),
            density: None,
            allergens: Vec::new(),
            diet_conflicts: Vec::new(),
            replaced_by: replaced_by.map(str::to_owned),
            source: format!("off:en:{slug}"),
        };
        let name = |name: &str| Names {
            name: name.to_owned(),
            aliases: Vec::new(),
        };
        IngredientIndex::new(Dataset {
            version: "test".to_owned(),
            locales: LocaleTable {
                shards: vec!["en-US".to_owned()],
                fallback: BTreeMap::new(),
                last_resort: vec!["en-US".to_owned()],
            },
            entries: vec![
                entry("butter", Some("milk-fat"), None),
                entry("dairy", None, None),
                entry("milk-fat", None, Some("dairy")),
                entry("old-butter", None, Some("butter")),
            ],
            names: BTreeMap::from([(
                "en-US".to_owned(),
                BTreeMap::from([
                    ("butter".to_owned(), name("butter")),
                    ("dairy".to_owned(), name("dairy")),
                ]),
            )]),
        })
        .expect("fixture validates")
    }

    #[test]
    fn an_unambiguous_spelling_links_with_its_display_name() {
        // British `aubergine` links, and reads as the American `eggplant`
        // in `en-US`, where `aubergine` is an alias.
        assert_eq!(
            match_in(embedded(), "aubergine", "en-US", 5),
            IngredientMatch::Exact {
                slug: "aubergine".to_owned(),
                name: Some("eggplant".to_owned()),
            }
        );
        assert_eq!(
            match_in(embedded(), "Farine", "fr-FR", 5),
            IngredientMatch::Exact {
                slug: "flour".to_owned(),
                name: Some("farine".to_owned()),
            }
        );
    }

    #[test]
    fn a_partial_name_offers_ranked_candidates_within_the_cap() {
        let IngredientMatch::Candidates { candidates } = match_in(embedded(), "kim", "en", 3)
        else {
            panic!("expected candidates");
        };
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].slug, "kimchi");
        assert_eq!(candidates[0].name, "kimchi");
        assert!(candidates.iter().all(|c| !c.shard.is_empty()));

        let IngredientMatch::Candidates { candidates } = match_in(embedded(), "but", "en", 1_000)
        else {
            panic!("expected candidates for an ambiguous prefix");
        };
        assert!(candidates.len() <= MAX_CANDIDATES);
        assert!(candidates.len() > 1);
    }

    #[test]
    fn nothing_resembling_the_name_matches_none() {
        assert_eq!(
            match_in(embedded(), "zzzqqq", "en", 5),
            IngredientMatch::None
        );
        assert_eq!(match_in(embedded(), "", "en", 5), IngredientMatch::None);
        assert_eq!(match_in(embedded(), "kim", "en", 0), IngredientMatch::None);
    }

    #[test]
    fn names_follow_the_fallback_chain_and_report_their_shard() {
        assert_eq!(
            name_in(embedded(), "butter", "fr"),
            Some(IngredientName {
                name: "beurre".to_owned(),
                shard: "fr-FR".to_owned(),
            })
        );
        // No French name for kimchi: the chain falls back to English.
        let kimchi = name_in(embedded(), "kimchi", "fr").expect("kimchi");
        assert!(kimchi.shard.starts_with("en-"), "{kimchi:?}");
        assert_eq!(name_in(embedded(), "not-an-entry", "en"), None);
    }

    #[test]
    fn a_suggestion_shard_is_the_best_one_in_the_recipe_language() {
        let shard = |tag: &str| shard_in(embedded(), tag);
        assert_eq!(shard("fr").as_deref(), Some("fr-FR"));
        assert_eq!(shard("en-GB").as_deref(), Some("en-GB"));
        assert_eq!(shard(" EN-au ").as_deref(), Some("en-GB"));
        assert_eq!(shard("en").as_deref(), Some("en-US"));
        assert_eq!(shard("zh-Hant").as_deref(), Some("zh-Hant-TW"));
        // No Dutch shard: the English last resort is not a Dutch shard.
        assert_eq!(shard("nl"), None);
        assert_eq!(shard(""), None);
    }

    #[test]
    fn the_shard_export_answers_from_the_embedded_index() {
        // `ingredientShard` returns no `JsValue` on success, so it runs
        // natively: it must answer the embedded index's shard, not a stub.
        let shard = |tag: &str| ingredient_shard_js(tag).ok().flatten();
        assert_eq!(shard("fr").as_deref(), Some("fr-FR"));
        assert_eq!(shard("en-GB").as_deref(), Some("en-GB"));
        assert_eq!(shard("nl"), None);
    }

    #[test]
    fn entries_carry_category_parent_and_hints() {
        let butter = entry_in(embedded(), "butter").expect("butter");
        assert_eq!(butter.slug, "butter");
        assert_eq!(butter.parent.as_deref(), Some("dairy"));
        assert_eq!(butter.category, "dairy");
        assert_eq!(butter.allergens, ["milk"]);
        assert_eq!(butter.diet_conflicts, ["dairy-free", "vegan"]);
        assert!(!butter.has_density);
        assert!(entry_in(embedded(), "flour").expect("flour").has_density);
        assert_eq!(entry_in(embedded(), "not-an-entry"), None);
    }

    #[test]
    fn merged_slugs_resolve_to_their_successor() {
        let idx = merged();
        let old = entry_in(&idx, "old-butter").expect("resolves");
        assert_eq!(old.slug, "butter");
        assert_eq!(old.parent.as_deref(), Some("dairy"), "parent resolves too");
        assert_eq!(
            name_in(&idx, "old-butter", "en").map(|n| n.name),
            Some("butter".to_owned())
        );
    }
}
