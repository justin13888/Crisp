//! Pure extraction of search metadata from a verified bundle payload.
//!
//! [`extract`] walks a [`BundlePayload`] and produces one [`IndexRow`] per
//! recipe — title, language, classifications, and a concatenated
//! `search_text` blob. It performs no I/O and makes no trust decisions, so it
//! is exhaustively unit-testable on its own.

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
            license: Set(self.license),
            created_at: Set(self.created_at),
            modified_at: Set(self.modified_at),
            search_text: Set(self.search_text),
        }
    }
}

/// Extracts one [`IndexRow`] per recipe in `payload`.
#[must_use]
pub fn extract(payload: &BundlePayload) -> Vec<IndexRow> {
    let default = &payload.default_language;
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
        .map(|recipe| row_for(payload, recipe, default, &authors))
        .collect()
}

/// Builds the index row for a single recipe.
fn row_for(
    payload: &BundlePayload,
    recipe: &Recipe,
    default: &LanguageTag,
    authors: &[AuthorRef],
) -> IndexRow {
    let title = text_or_empty(&recipe.title, default);
    let summary = recipe.summary.as_ref().map(|p| prose_text(p, default));

    let tags = recipe.tags.clone().unwrap_or_default();
    let cuisine = recipe.cuisine.clone().unwrap_or_default();
    let course = codes(recipe.course.as_deref());
    let diet = codes(recipe.diet.as_deref());
    let allergens = codes(recipe.allergens.as_deref());

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
        search_text: search_text(recipe, default, &title, summary.as_deref(), &tags, &cuisine),
    }
}

/// Builds the concatenated full-text blob for a recipe: its title, summary,
/// every ingredient name, and its keyword and cuisine tags.
fn search_text(
    recipe: &Recipe,
    default: &LanguageTag,
    title: &str,
    summary: Option<&str>,
    tags: &[String],
    cuisine: &[String],
) -> String {
    let mut parts: Vec<String> = vec![title.to_owned()];
    if let Some(summary) = summary {
        parts.push(summary.to_owned());
    }
    for ingredient in &recipe.ingredients {
        parts.push(text_or_empty(&ingredient.name, default));
    }
    parts.extend(tags.iter().cloned());
    parts.extend(cuisine.iter().cloned());
    parts.retain(|p| !p.is_empty());
    parts.join(" ")
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
mod tests {
    use super::*;

    #[test]
    fn codes_serialize_classification_enums_to_wire_strings() {
        use crisp_core::model::recipe::{AllergenCode, DietCode};
        assert_eq!(
            codes(Some(&[
                OpenCode::Known(DietCode::GlutenFree),
                OpenCode::Known(DietCode::Vegan),
            ])),
            vec!["gluten-free", "vegan"]
        );
        assert_eq!(
            codes(Some(&[OpenCode::Known(AllergenCode::TreeNuts)])),
            vec!["tree-nuts"]
        );
        assert!(codes::<DietCode>(None).is_empty());
    }

    #[test]
    fn codes_carry_unknown_codes_verbatim() {
        use crisp_core::model::recipe::DietCode;
        assert_eq!(
            codes(Some(&[
                OpenCode::<DietCode>::Other("low-fodmap".to_owned()),
                OpenCode::Known(DietCode::Vegan),
            ])),
            vec!["low-fodmap", "vegan"]
        );
    }

    fn paragraph(runs: &[&str]) -> crisp_core::model::text::Paragraph {
        crisp_core::model::text::Paragraph(
            runs.iter()
                .map(|t| crisp_core::model::text::Run {
                    t: (*t).to_owned(),
                    style: None,
                    link: None,
                    reference: None,
                })
                .collect(),
        )
    }

    #[test]
    fn prose_text_joins_paragraphs_with_a_single_space_and_trims() {
        let en = LanguageTag("en".to_owned());
        let prose = Prose(vec![
            paragraph(&["Rich ", "and dark."]),
            paragraph(&[]),
            paragraph(&["Serves four. "]),
        ]);
        let localized = LocalizedProse([(en.clone(), prose)].into_iter().collect());
        assert_eq!(prose_text(&localized, &en), "Rich and dark.  Serves four.");
    }

    #[test]
    fn prose_text_is_empty_when_no_language_resolves() {
        let en = LanguageTag("en".to_owned());
        let localized = LocalizedProse(std::collections::BTreeMap::new());
        assert_eq!(prose_text(&localized, &en), "");
    }
}
