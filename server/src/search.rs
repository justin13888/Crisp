//! Recipe browsing and full-text + faceted search (plan Stage 14).
//!
//! Both endpoints read only the denormalized `recipes_index` table, so a
//! search never opens a stored bundle. `GET /api/v1/recipes` is a newest-first
//! browse; `GET /api/v1/search` adds a PostgreSQL `tsvector` text match and
//! cuisine / course / diet / allergen-free / language facets.

use axum::Json;
use axum::extract::{Query, State};
use serde::Serialize;

use crate::db::recipe_index;
use crate::error::ApiResult;
use crate::state::AppState;
use crate::storage::SearchQuery;

/// The default and maximum page sizes for browse and search.
const DEFAULT_LIMIT: u64 = 50;
/// The largest page a caller may request.
const MAX_LIMIT: u64 = 200;

/// A single recipe search/browse result row.
#[derive(Debug, Serialize)]
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
    /// The authors, as a JSON array of `{ uid, name }`.
    pub authors: serde_json::Value,
    /// Keyword tags.
    pub tags: serde_json::Value,
    /// Cuisine tags.
    pub cuisine: serde_json::Value,
    /// Course classifications.
    pub course: serde_json::Value,
    /// Diet classifications.
    pub diet: serde_json::Value,
    /// Declared allergens.
    pub allergens: serde_json::Value,
    /// The bundle license identifier.
    pub license: String,
    /// The recipe creation time (epoch seconds).
    pub created_at: i64,
    /// The recipe modification time (epoch seconds).
    pub modified_at: i64,
}

impl From<recipe_index::Model> for RecipeSummary {
    fn from(model: recipe_index::Model) -> Self {
        Self {
            doc_id: model.doc_id,
            recipe_id: model.recipe_id,
            recipe_revision: model.recipe_revision,
            title: model.title,
            language: model.language,
            summary: model.summary,
            authors: model.authors,
            tags: model.tags,
            cuisine: model.cuisine,
            course: model.course,
            diet: model.diet,
            allergens: model.allergens,
            license: model.license,
            created_at: model.created_at,
            modified_at: model.modified_at,
        }
    }
}

/// A page of recipe results.
#[derive(Debug, Serialize)]
pub struct RecipePage {
    /// The result rows.
    pub recipes: Vec<RecipeSummary>,
    /// The page size that was applied.
    pub limit: u64,
    /// The offset that was applied.
    pub offset: u64,
}

/// Parses `limit`, clamped to `[1, MAX_LIMIT]`.
fn parse_limit(raw: Option<&str>) -> u64 {
    raw.and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT)
}

/// Parses `offset`, defaulting to `0`.
fn parse_offset(raw: Option<&str>) -> u64 {
    raw.and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// `GET /api/v1/recipes` — a newest-first browse of indexed recipes.
pub async fn browse(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult<Json<RecipePage>> {
    let find = |key: &str| {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let limit = parse_limit(find("limit"));
    let offset = parse_offset(find("offset"));

    let rows = state.storage.browse(limit, offset).await?;
    Ok(Json(RecipePage {
        recipes: rows.into_iter().map(RecipeSummary::from).collect(),
        limit,
        offset,
    }))
}

/// Builds a [`SearchQuery`] from the raw query-string pairs, collecting
/// repeated facet keys into lists.
fn build_query(params: &[(String, String)]) -> SearchQuery {
    let mut query = SearchQuery {
        limit: DEFAULT_LIMIT,
        ..SearchQuery::default()
    };
    for (key, value) in params {
        match key.as_str() {
            "q" if !value.is_empty() => query.text = Some(value.clone()),
            "cuisine" => query.cuisine.push(value.clone()),
            "course" => query.course.push(value.clone()),
            "diet" => query.diet.push(value.clone()),
            "allergen_free" => query.allergen_free.push(value.clone()),
            "language" if !value.is_empty() => query.language = Some(value.clone()),
            "limit" => query.limit = parse_limit(Some(value)),
            "offset" => query.offset = parse_offset(Some(value)),
            _ => {}
        }
    }
    query
}

/// `GET /api/v1/search` — full-text and faceted recipe search.
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> ApiResult<Json<RecipePage>> {
    let query = build_query(&params);
    let rows = state.storage.search(&query).await?;
    Ok(Json(RecipePage {
        recipes: rows.into_iter().map(RecipeSummary::from).collect(),
        limit: query.limit,
        offset: query.offset,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_query_collects_repeated_facets() {
        let params = vec![
            ("q".to_owned(), "chocolate cake".to_owned()),
            ("diet".to_owned(), "vegan".to_owned()),
            ("diet".to_owned(), "gluten-free".to_owned()),
            ("allergen_free".to_owned(), "peanuts".to_owned()),
            ("limit".to_owned(), "10".to_owned()),
        ];
        let query = build_query(&params);
        assert_eq!(query.text.as_deref(), Some("chocolate cake"));
        assert_eq!(query.diet, vec!["vegan", "gluten-free"]);
        assert_eq!(query.allergen_free, vec!["peanuts"]);
        assert_eq!(query.limit, 10);
    }

    #[test]
    fn limit_is_clamped_to_the_maximum() {
        assert_eq!(parse_limit(Some("9999")), MAX_LIMIT);
        assert_eq!(parse_limit(Some("0")), 1);
        assert_eq!(parse_limit(None), DEFAULT_LIMIT);
    }
}
