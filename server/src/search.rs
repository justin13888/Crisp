//! Recipe browsing and full-text + faceted search (plan Stage 14).
//!
//! Both endpoints read only the denormalized `recipes_index` table, so a
//! search never opens a stored bundle. `GET /api/v1/recipes` is a newest-first
//! browse; `GET /api/v1/search` adds a PostgreSQL `tsvector` text match and
//! cuisine / course / diet / allergen-free / language facets.
//!
//! Multi-valued facets are comma-separated (`?diet=vegan,gluten-free`): a
//! query parameter is one name with one value in the OpenAPI contract.

use kynos::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::db::recipe_index;
use crate::error::Internal;
use crate::index::AuthorRef;
use crate::operation_error;
use crate::state::Store;
use crate::storage::SearchQuery;

/// The default page size for browse and search.
pub const DEFAULT_LIMIT: u64 = 50;
/// The largest page a caller may request.
pub const MAX_LIMIT: u64 = 200;

operation_error! {
    /// What the browse and search operations can fail with.
    pub enum SearchError {}
}

/// A single recipe search/browse result row.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RecipeSummary {
    /// The owning bundle's document id.
    pub doc_id: String,
    /// The recipe's `rcp…` id.
    pub recipe_id: String,
    /// The recipe's `rev…` revision id.
    pub recipe_revision: String,
    /// The recipe title, in the bundle default language.
    pub title: String,
    /// The recipe's identity language.
    pub language: String,
    /// An optional plain-text summary.
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
}

/// Decodes one JSONB column into its typed form.
fn column<T: DeserializeOwned>(
    name: &'static str,
    value: serde_json::Value,
) -> Result<T, Internal> {
    serde_json::from_value(value).map_err(|e| Internal::logged(name, &e))
}

impl TryFrom<recipe_index::Model> for RecipeSummary {
    type Error = Internal;

    fn try_from(model: recipe_index::Model) -> Result<Self, Internal> {
        Ok(Self {
            doc_id: model.doc_id,
            recipe_id: model.recipe_id,
            recipe_revision: model.recipe_revision,
            title: model.title,
            language: model.language,
            summary: model.summary,
            authors: column("recipes_index.authors", model.authors)?,
            tags: column("recipes_index.tags", model.tags)?,
            cuisine: column("recipes_index.cuisine", model.cuisine)?,
            course: column("recipes_index.course", model.course)?,
            diet: column("recipes_index.diet", model.diet)?,
            allergens: column("recipes_index.allergens", model.allergens)?,
            license: model.license,
            created_at: model.created_at,
            modified_at: model.modified_at,
        })
    }
}

/// A page of recipe results.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RecipePage {
    /// The result rows.
    pub recipes: Vec<RecipeSummary>,
    /// The page size that was applied.
    pub limit: u64,
    /// The offset that was applied.
    pub offset: u64,
}

impl RecipePage {
    /// Converts storage rows into a page.
    fn from_rows(
        rows: Vec<recipe_index::Model>,
        limit: u64,
        offset: u64,
    ) -> Result<Self, Internal> {
        let recipes = rows
            .into_iter()
            .map(RecipeSummary::try_from)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            recipes,
            limit,
            offset,
        })
    }
}

/// Pagination for `GET /recipes`.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct BrowseParams {
    /// How many recipes to return (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u64>,
    /// How many recipes to skip (default 0).
    pub offset: Option<u64>,
}

/// Text, facets, and pagination for `GET /search`.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct SearchParams {
    /// Free text matched against title, summary, ingredients, and tags.
    pub q: Option<String>,
    /// Comma-separated cuisine tags; every one must match.
    pub cuisine: Option<String>,
    /// Comma-separated course codes; every one must match.
    pub course: Option<String>,
    /// Comma-separated diet codes; every one must match.
    pub diet: Option<String>,
    /// Comma-separated allergen codes; none may be declared.
    pub allergen_free: Option<String>,
    /// An exact identity-language tag, e.g. `en`.
    pub language: Option<String>,
    /// How many recipes to return (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u64>,
    /// How many recipes to skip (default 0).
    pub offset: Option<u64>,
}

/// Splits a comma-separated facet list, dropping empty entries.
fn facet_list(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Trims an optional free-text parameter, treating blank as absent.
fn non_blank(raw: Option<String>) -> Option<String> {
    raw.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

impl SearchParams {
    /// Normalizes the raw parameters into a storage [`SearchQuery`].
    #[must_use]
    pub fn into_query(self) -> SearchQuery {
        SearchQuery {
            cuisine: facet_list(self.cuisine.as_deref()),
            course: facet_list(self.course.as_deref()),
            diet: facet_list(self.diet.as_deref()),
            allergen_free: facet_list(self.allergen_free.as_deref()),
            text: non_blank(self.q),
            language: non_blank(self.language),
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: self.offset.unwrap_or(0),
        }
    }
}

/// Lists indexed recipes, newest first.
#[kynos::get("/recipes")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn browse(
    Inject(storage): Inject<Store>,
    Query(params): Query<BrowseParams>,
) -> Result<Json<RecipePage>, SearchError> {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);
    let rows = storage.browse(limit, offset).await?;
    tracing::debug!(rows = rows.len(), "browse served");
    Ok(Json(RecipePage::from_rows(rows, limit, offset)?))
}

/// Full-text and faceted recipe search.
#[kynos::get("/search")]
#[tracing::instrument(level = "debug", skip(storage))]
pub async fn search(
    Inject(storage): Inject<Store>,
    Query(params): Query<SearchParams>,
) -> Result<Json<RecipePage>, SearchError> {
    let query = params.into_query();
    let rows = storage.search(&query).await?;
    tracing::debug!(rows = rows.len(), "search served");
    Ok(Json(RecipePage::from_rows(
        rows,
        query.limit,
        query.offset,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facets_are_comma_separated_lists() {
        let params = SearchParams {
            q: Some("  chocolate cake ".into()),
            diet: Some("vegan, gluten-free,,".into()),
            allergen_free: Some("peanuts".into()),
            language: Some("  ".into()),
            limit: Some(10),
            ..SearchParams::default()
        };
        let query = params.into_query();
        assert_eq!(query.text.as_deref(), Some("chocolate cake"));
        assert_eq!(query.diet, vec!["vegan", "gluten-free"]);
        assert_eq!(query.allergen_free, vec!["peanuts"]);
        assert!(query.cuisine.is_empty());
        assert_eq!(query.language, None);
        assert_eq!(query.limit, 10);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn limit_defaults_and_is_clamped() {
        assert_eq!(SearchParams::default().into_query().limit, DEFAULT_LIMIT);
        let huge = SearchParams {
            limit: Some(9_999),
            ..SearchParams::default()
        };
        assert_eq!(huge.into_query().limit, MAX_LIMIT);
    }

    #[test]
    fn malformed_jsonb_columns_become_internal_errors() {
        let bad: Result<Vec<String>, Internal> = column("x", serde_json::json!({"not": "a list"}));
        assert!(bad.is_err());
    }
}
