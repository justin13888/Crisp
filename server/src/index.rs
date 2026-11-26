//! Pure extraction of search metadata from a verified bundle payload.
//!
//! [`extract`] walks a [`BundlePayload`] and produces one [`IndexRow`] per
//! recipe — title, language, classifications, ingredient codes, and a
//! concatenated `search_text` blob. It performs no I/O and makes no trust
//! decisions, so it is exhaustively unit-testable on its own.
//!
//! Coded ingredients (`ingredient.code`, spec §13) contribute their slugs to
//! the ingredient facet as written, and their names from the embedded
//! ingredient index to `search_text`, in every language the bundle declares.

use std::collections::BTreeSet;

use crisp_core::ingredients::IngredientIndex;
use crisp_core::model::payload::BundlePayload;
use crisp_core::model::primitives::{LanguageTag, OpenCode};
use crisp_core::model::recipe::Recipe;
use crisp_core::model::text::{LocalizedProse, LocalizedText, Prose};
use crisp_core::render::{prose_to_plain, resolve_prose, resolve_text};
use kynos::Schema;
use sea_orm::ActiveValue::{NotSet, Set};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::recipe_index;

/// A reference to one author, as stored in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Schema)]
pub struct AuthorRef {
    /// The author's `usr…` id.
    pub uid: String,
    /// The author's display name.
    pub name: String,
}

/// The denormalized index data for a single recipe, ready to be persisted.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRow {
    /// The owning bundle's document id.
    pub doc_id: String,
    /// The recipe's `rcp…` id.
    pub recipe_id: String,
    /// The recipe's `rev…` revision id.
    pub recipe_revision: String,
    /// The title, resolved to the bundle default language.
    pub title: String,
    /// The recipe's identity language tag.
    pub language: String,
    /// An optional resolved plain-text summary.
    pub summary: Option<String>,
    /// The bundle authors.
    pub authors: Vec<AuthorRef>,
    /// Keyword tags.
    pub tags: Vec<String>,
    /// Cuisine tags.
    pub cuisine: Vec<String>,
    /// Course classifications.
    pub course: Vec<String>,
    /// Diet classifications.
    pub diet: Vec<String>,
    /// Declared allergens.
    pub allergens: Vec<String>,
    /// The distinct ingredient-index slugs the recipe's ingredients carry,
    /// sorted, exactly as written (a merged slug is not rewritten).
    pub ingredient_codes: Vec<String>,
    /// The bundle license identifier.
    pub license: String,
    /// The recipe creation time (epoch seconds).
    pub created_at: i64,
    /// The recipe modification time (epoch seconds).
    pub modified_at: i64,
    /// The concatenated full-text search blob.
    pub search_text: String,
}

impl IndexRow {
    /// Converts this row into a SeaORM insert model for `recipes_index`.
    #[must_use]
    pub fn into_active_model(self, bundle_hash: &str) -> recipe_index::ActiveModel {
        recipe_index::ActiveModel {
            id: NotSet,
            bundle_hash: Set(bundle_hash.to_owned()),
            doc_id: Set(self.doc_id),
            recipe_id: Set(self.recipe_id),
            recipe_revision: Set(self.recipe_revision),
            title: Set(self.title),
            language: Set(self.language),
            summary: Set(self.summary),
            authors: Set(json!(self.authors)),
            tags: Set(json!(self.tags)),
            cuisine: Set(json!(self.cuisine)),
            course: Set(json!(self.course)),
            diet: Set(json!(self.diet)),
            allergens: Set(json!(self.allergens)),
            ingredient_codes: Set(json!(self.ingredient_codes)),
            license: Set(self.license),
            created_at: Set(self.created_at),
            modified_at: Set(self.modified_at),
            search_text: Set(self.search_text),
        }
    }
}

/// Extracts one [`IndexRow`] per recipe in `payload`, using the embedded
/// ingredient index for coded ingredients' names.
#[must_use]
pub fn extract(payload: &BundlePayload) -> Vec<IndexRow> {
    extract_with(payload, crisp_core::ingredients::index().ok())
}

/// Extracts one [`IndexRow`] per recipe in `payload`, naming coded
/// ingredients from `ingredients`. Without an index, codes are still
/// indexed for the facet; only their index names are left out.
#[must_use]
pub fn extract_with(
    payload: &BundlePayload,
    ingredients: Option<&IngredientIndex>,
) -> Vec<IndexRow> {
    let authors: Vec<AuthorRef> = payload
        .meta
        .authors
        .iter()
        .map(|a| AuthorRef {
            uid: a.uid.0.clone(),
            name: a.display_name.clone(),
        })
        .collect();

    payload
        .recipes
        .iter()
        .map(|recipe| row_for(payload, recipe, &authors, ingredients))
        .collect()
}

/// Builds the index row for a single recipe.
fn row_for(
    payload: &BundlePayload,
    recipe: &Recipe,
    authors: &[AuthorRef],
    ingredients: Option<&IngredientIndex>,
) -> IndexRow {
    let default = &payload.default_language;
    let title = text_or_empty(&recipe.title, default);
    let summary = recipe.summary.as_ref().map(|p| prose_text(p, default));

    let tags = recipe.tags.clone().unwrap_or_default();
    let cuisine = recipe.cuisine.clone().unwrap_or_default();
    let course = codes(recipe.course.as_deref());
    let diet = codes(recipe.diet.as_deref());
    let allergens = codes(recipe.allergens.as_deref());
    let ingredient_codes = ingredient_codes(recipe);
    let index_names = ingredients.map_or_else(Vec::new, |index| {
        index_names(
            index,
            &ingredient_codes,
            &languages(default, &payload.languages),
        )
    });

    IndexRow {
        doc_id: payload.doc_id.0.clone(),
        recipe_id: recipe.id.0.clone(),
        recipe_revision: recipe.revision.0.clone(),
        title: title.clone(),
        language: recipe.identity.language.0.clone(),
        summary: summary.clone(),
        authors: authors.to_vec(),
        tags: tags.clone(),
        cuisine: cuisine.clone(),
        course,
        diet,
        allergens,
        license: payload.meta.license.clone(),
        created_at: i64_of(recipe.identity.created_at.0),
        modified_at: i64_of(recipe.modified_at.0),
        search_text: search_text(
            recipe,
            default,
            &title,
            summary.as_deref(),
            &[&tags, &cuisine, &index_names],
        ),
        ingredient_codes,
    }
}

/// Builds the concatenated full-text blob for a recipe: its title, summary,
/// every ingredient name, then each list in `extra` (keyword tags, cuisine
/// tags, ingredient-index names).
fn search_text(
    recipe: &Recipe,
    default: &LanguageTag,
    title: &str,
    summary: Option<&str>,
    extra: &[&[String]],
) -> String {
    let mut parts: Vec<String> = vec![title.to_owned()];
    if let Some(summary) = summary {
        parts.push(summary.to_owned());
    }
    for ingredient in &recipe.ingredients {
        parts.push(text_or_empty(&ingredient.name, default));
    }
    for list in extra {
        parts.extend(list.iter().cloned());
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" ")
}

/// The distinct, sorted, non-blank `code`s of a recipe's ingredients.
/// Substitutes' codes are left out: a recipe that only allows an ingredient
/// as a substitute is not a recipe made with it.
fn ingredient_codes(recipe: &Recipe) -> Vec<String> {
    recipe
        .ingredients
        .iter()
        .filter_map(|i| i.code.as_deref())
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The languages a bundle is written in: its default language first, then
/// every other language it declares, each once.
/// Tags differing only in case count once (BCP 47 tags are
/// case-insensitive).
fn languages<'a>(default: &'a LanguageTag, declared: &'a [LanguageTag]) -> Vec<&'a str> {
    let mut seen = BTreeSet::new();
    std::iter::once(default)
        .chain(declared)
        .map(|tag| tag.0.as_str())
        .filter(|tag| seen.insert(tag.to_ascii_lowercase()))
        .collect()
}

/// The index's names for `codes` in each of `languages`: the display name
/// the language's fallback chain finds, and that shard's aliases. A merged
/// slug is named as its successor; a slug the index does not know adds
/// nothing. Each name appears once.
fn index_names(index: &IngredientIndex, codes: &[String], languages: &[&str]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for code in codes {
        let Some(entry) = index.resolve(code) else {
            tracing::trace!(code = %code, "ingredient code not in the index");
            continue;
        };
        for language in languages {
            let Some(found) = index.display_name(&entry.slug, language) else {
                continue;
            };
            let aliases = index
                .names(&entry.slug, found.shard)
                .map(|n| n.aliases.as_slice())
                .unwrap_or_default();
            for name in std::iter::once(found.name).chain(aliases.iter().map(String::as_str)) {
                if seen.insert(name.to_owned()) {
                    out.push(name.to_owned());
                }
            }
        }
    }
    out
}

/// Resolves a localized text to the default language, or `""` if absent.
fn text_or_empty(text: &LocalizedText, default: &LanguageTag) -> String {
    resolve_text(text, &[], default)
        .unwrap_or_default()
        .to_owned()
}

/// Resolves a localized prose block to one line of plain text: each paragraph
/// flattened by [`prose_to_plain`], paragraphs joined by a single space (not
/// its newline), so a stored summary is a single line.
fn prose_text(prose: &LocalizedProse, default: &LanguageTag) -> String {
    let Some(prose) = resolve_prose(prose, &[], default) else {
        return String::new();
    };
    let paragraphs: Vec<String> = prose
        .0
        .iter()
        .map(|paragraph| prose_to_plain(&Prose(vec![paragraph.clone()])))
        .collect();
    paragraphs.join(" ").trim().to_owned()
}

/// Maps open-vocabulary classification codes to their wire strings.
fn codes<T: Serialize>(values: Option<&[OpenCode<T>]>) -> Vec<String> {
    values
        .unwrap_or_default()
        .iter()
        .map(OpenCode::as_wire)
        .collect()
}

/// Clamps a `u64` epoch timestamp into the `i64` the database stores.
fn i64_of(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
pub(crate) mod tests;
