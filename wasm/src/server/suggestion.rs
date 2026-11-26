//! The typed values `CrispServer.suggestIngredient` takes and resolves to,
//! and their conversion to and from the generated client's types
//! (`POST /ingredients/suggestions`, `docs/ingredient-index.md` §6).

use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::types;

/// What an ingredient suggestion proposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    /// An ingredient the index lacks; `text` is its name in `locale`.
    NewEntry,
    /// A missing display name or alias of `slug` in `locale`.
    Name,
    /// A regional spelling of `slug` in a regional `locale`.
    RegionalSpelling,
    /// `slug` duplicates the entry named in `text`.
    Merge,
    /// Anything else, described in `text`.
    Other,
}

/// Where a filed suggestion stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    /// Waiting for an administrator.
    Open,
    /// Accepted into the server's queue for the index.
    Accepted,
    /// Declined.
    Declined,
    /// Already suggested.
    Duplicate,
}

/// A suggestion to file with the signed-in account.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
pub struct IngredientSuggestion {
    /// What it proposes.
    pub kind: SuggestionKind,
    /// The locale shard it is for, e.g. `en-GB` (see `ingredientShard`).
    pub locale: String,
    /// The proposed name, alias, or spelling (at most 200 characters).
    pub text: String,
    /// The index entry it is about: required for `name`,
    /// `regional_spelling`, and `merge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Why, at most 2000 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A suggestion the server filed.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
pub struct FiledSuggestion {
    /// The server's suggestion id.
    pub id: i64,
    /// What it proposes.
    pub kind: SuggestionKind,
    /// The locale shard it is for.
    pub locale: String,
    /// The proposed text, as the server stored it.
    pub text: String,
    /// The index entry it is about, if it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Where it stands.
    pub status: SuggestionStatus,
}

impl From<SuggestionKind> for types::SuggestionKind {
    fn from(kind: SuggestionKind) -> Self {
        match kind {
            SuggestionKind::NewEntry => Self::NewEntry,
            SuggestionKind::Name => Self::Name,
            SuggestionKind::RegionalSpelling => Self::RegionalSpelling,
            SuggestionKind::Merge => Self::Merge,
            SuggestionKind::Other => Self::Other,
        }
    }
}

impl From<types::SuggestionKind> for SuggestionKind {
    fn from(kind: types::SuggestionKind) -> Self {
        match kind {
            types::SuggestionKind::NewEntry => Self::NewEntry,
            types::SuggestionKind::Name => Self::Name,
            types::SuggestionKind::RegionalSpelling => Self::RegionalSpelling,
            types::SuggestionKind::Merge => Self::Merge,
            types::SuggestionKind::Other => Self::Other,
        }
    }
}

impl From<types::SuggestionStatus> for SuggestionStatus {
    fn from(status: types::SuggestionStatus) -> Self {
        match status {
            types::SuggestionStatus::Open => Self::Open,
            types::SuggestionStatus::Accepted => Self::Accepted,
            types::SuggestionStatus::Declined => Self::Declined,
            types::SuggestionStatus::Duplicate => Self::Duplicate,
        }
    }
}

impl From<IngredientSuggestion> for types::SuggestionRequest {
    fn from(suggestion: IngredientSuggestion) -> Self {
        Self {
            kind: suggestion.kind.into(),
            locale: suggestion.locale,
            text: suggestion.text,
            slug: suggestion.slug,
            note: suggestion.note,
        }
    }
}

impl From<types::SuggestionView> for FiledSuggestion {
    fn from(view: types::SuggestionView) -> Self {
        Self {
            id: view.id,
            kind: view.kind.into(),
            locale: view.locale,
            text: view.text,
            slug: view.slug,
            status: view.status.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{FiledSuggestion, IngredientSuggestion, SuggestionKind, SuggestionStatus};
    use crate::api::types;

    #[test]
    fn a_suggestion_from_javascript_becomes_the_documented_request() {
        let suggestion: IngredientSuggestion = serde_json::from_value(json!({
            "kind": "new_entry", "locale": "fr-FR", "text": "piment d'Espelette",
        }))
        .expect("a suggestion");
        let request = types::SuggestionRequest::from(suggestion);
        assert_eq!(
            serde_json::to_value(&request).expect("serializes"),
            json!({ "kind": "new_entry", "locale": "fr-FR", "text": "piment d'Espelette" })
        );

        for (kind, wire) in [
            (SuggestionKind::Name, "name"),
            (SuggestionKind::RegionalSpelling, "regional_spelling"),
            (SuggestionKind::Merge, "merge"),
            (SuggestionKind::Other, "other"),
        ] {
            let request = types::SuggestionRequest::from(IngredientSuggestion {
                kind,
                locale: "en-GB".to_owned(),
                text: "t".to_owned(),
                slug: Some("butter".to_owned()),
                note: Some("why".to_owned()),
            });
            let value = serde_json::to_value(&request).expect("serializes");
            assert_eq!(value["kind"], wire);
            assert_eq!(value["slug"], "butter");
            assert_eq!(value["note"], "why");
        }
    }

    #[test]
    fn a_filed_suggestion_keeps_what_the_client_shows() {
        let filed = |kind: &str, status: &str| -> FiledSuggestion {
            let view: types::SuggestionView = serde_json::from_value(json!({
                "id": 7, "kind": kind, "locale": "en-US", "text": "mugunji",
                "slug": "kimchi-aged", "note": "", "status": status, "created_at": 1,
                "suggester": "usrx",
            }))
            .expect("a suggestion view");
            view.into()
        };
        assert_eq!(
            filed("name", "open"),
            FiledSuggestion {
                id: 7,
                kind: SuggestionKind::Name,
                locale: "en-US".to_owned(),
                text: "mugunji".to_owned(),
                slug: Some("kimchi-aged".to_owned()),
                status: SuggestionStatus::Open,
            }
        );
        let kinds = ["new_entry", "regional_spelling", "merge", "other"];
        let parsed: Vec<SuggestionKind> = kinds.iter().map(|k| filed(k, "open").kind).collect();
        assert_eq!(
            parsed,
            [
                SuggestionKind::NewEntry,
                SuggestionKind::RegionalSpelling,
                SuggestionKind::Merge,
                SuggestionKind::Other,
            ]
        );
        let statuses = ["accepted", "declined", "duplicate"];
        let parsed: Vec<SuggestionStatus> =
            statuses.iter().map(|s| filed("other", s).status).collect();
        assert_eq!(
            parsed,
            [
                SuggestionStatus::Accepted,
                SuggestionStatus::Declined,
                SuggestionStatus::Duplicate,
            ]
        );
    }
}
