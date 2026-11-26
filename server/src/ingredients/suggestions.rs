//! Ingredient suggestions (`docs/ingredient-index.md` §6): what a suggestion
//! is, how it is checked against the embedded index, and
//! `POST /ingredients/suggestions`, through which any signed-in account files
//! one.

use crisp_core::ingredients::{IngredientIndex, normalize};
use kynos::ParamValue;
use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::SessionToken;
use crate::db::ingredient_suggestion;
use crate::error::{BadRequest, Conflict, Internal};
use crate::limits::{Operation, Throttle};
use crate::operation_error;
use crate::state::Store;
use crate::storage::{FileSuggestion, NewSuggestion};
use crate::util::now_epoch;

/// The most characters a suggestion's proposed text may hold.
pub const MAX_TEXT_CHARS: usize = 200;
/// The most characters a suggestion's note may hold.
pub const MAX_NOTE_CHARS: usize = 2000;
/// The most open suggestions one account may hold at once.
pub const MAX_OPEN_SUGGESTIONS: u64 = 50;

/// What a suggestion proposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    /// An ingredient the index lacks; `text` is its name in `locale`, and
    /// `slug`, when given, the entry it specializes.
    NewEntry,
    /// A missing display name or alias of `slug` in `locale`.
    Name,
    /// A regional spelling of `slug` in a regional `locale` (e.g. `en-GB`).
    RegionalSpelling,
    /// `slug` is the same ingredient as the entry `text` names.
    Merge,
    /// Anything else; `text` describes it.
    Other,
}

impl SuggestionKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 5] = [
        Self::NewEntry,
        Self::Name,
        Self::RegionalSpelling,
        Self::Merge,
        Self::Other,
    ];

    /// The kind as stored and sent.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewEntry => "new_entry",
            Self::Name => "name",
            Self::RegionalSpelling => "regional_spelling",
            Self::Merge => "merge",
            Self::Other => "other",
        }
    }

    /// Reads a stored kind back.
    ///
    /// # Errors
    ///
    /// Returns [`Internal`] for a value no version of the server writes.
    pub fn stored(value: &str) -> Result<Self, Internal> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| Internal::logged("an unknown stored suggestion kind", &value))
    }

    /// Whether a suggestion of this kind must name the entry it is about.
    #[must_use]
    pub const fn needs_slug(self) -> bool {
        matches!(self, Self::Name | Self::RegionalSpelling | Self::Merge)
    }
}

/// Where a suggestion stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, ParamValue, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionStatus {
    /// No administrator has triaged it yet.
    Open,
    /// Accepted: it is exported as an override patch.
    Accepted,
    /// Declined.
    Declined,
    /// It repeats another suggestion or what the index already holds.
    Duplicate,
}

impl SuggestionStatus {
    /// Every status, in declaration order.
    pub const ALL: [Self; 4] = [Self::Open, Self::Accepted, Self::Declined, Self::Duplicate];

    /// The status as stored.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Duplicate => "duplicate",
        }
    }

    /// Reads a stored status back.
    ///
    /// # Errors
    ///
    /// Returns [`Internal`] for a value no version of the server writes.
    pub fn stored(value: &str) -> Result<Self, Internal> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| Internal::logged("an unknown stored suggestion status", &value))
    }
}

/// The `POST /ingredients/suggestions` request body.
#[derive(Debug, Clone, Schema, Serialize, Deserialize)]
pub struct SuggestionRequest {
    /// What the suggestion proposes.
    pub kind: SuggestionKind,
    /// The index entry it is about: required for `name`,
    /// `regional_spelling`, and `merge`; for `new_entry`, the entry the new
    /// one specializes. Must be a slug the index knows.
    pub slug: Option<String>,
    /// The locale shard it is for, e.g. `en-GB`; one the index declares.
    pub locale: String,
    /// The proposed name, alias, or spelling; for `merge`, the slug of the
    /// entry `slug` duplicates; for `other`, a description. At most 200
    /// characters.
    pub text: String,
    /// Why, at most 2000 characters.
    pub note: Option<String>,
}

/// A suggestion, as filed and as administrators see it.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct SuggestionView {
    /// The suggestion id.
    pub id: i64,
    /// The suggesting account, or `null` once it is deleted.
    pub suggester: Option<String>,
    /// What the suggestion proposes.
    pub kind: SuggestionKind,
    /// The index entry it is about, if it names one.
    pub slug: Option<String>,
    /// The locale shard it is for.
    pub locale: String,
    /// The proposed text.
    pub text: String,
    /// The suggester's explanation (possibly empty).
    pub note: String,
    /// Where it stands.
    pub status: SuggestionStatus,
    /// When it was filed (epoch seconds).
    pub created_at: i64,
    /// When it was triaged (epoch seconds), if it was.
    pub resolved_at: Option<i64>,
    /// The administrator who triaged it, if one did.
    pub resolved_by: Option<String>,
}

impl TryFrom<ingredient_suggestion::Model> for SuggestionView {
    type Error = Internal;

    fn try_from(row: ingredient_suggestion::Model) -> Result<Self, Internal> {
        Ok(Self {
            id: row.id,
            suggester: row.suggester,
            kind: SuggestionKind::stored(&row.kind)?,
            slug: row.slug,
            locale: row.locale,
            text: row.text,
            note: row.note,
            status: SuggestionStatus::stored(&row.status)?,
            created_at: row.created_at,
            resolved_at: row.resolved_at,
            resolved_by: row.resolved_by,
        })
    }
}

/// A [`SuggestionRequest`] that passed [`check`]: trimmed, within its
/// bounds, and consistent with the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// What it proposes.
    pub kind: SuggestionKind,
    /// The entry it is about, one the index knows.
    pub slug: Option<String>,
    /// A shard the index declares.
    pub locale: String,
    /// The proposed text, trimmed.
    pub text: String,
    /// The note, trimmed (possibly empty).
    pub note: String,
}

operation_error! {
    /// What `POST /ingredients/suggestions` can fail with.
    pub enum FileSuggestionError { BadRequest, Conflict, TooManyRequests } throttled
}

/// `value` trimmed, refused when longer than `max` characters, and, with
/// `required`, when empty.
fn bounded(field: &str, value: &str, max: usize, required: bool) -> Result<String, BadRequest> {
    let value = value.trim().to_owned();
    let chars = value.chars().count();
    if chars > max {
        return Err(BadRequest::new(format!(
            "the {field} is {chars} characters; at most {max} are allowed"
        )));
    }
    if required && value.is_empty() {
        return Err(BadRequest::new(format!("the {field} is empty")));
    }
    Ok(value)
}

/// Checks `request` against `index`: the locale is a declared shard, the
/// text and note are within their bounds, every slug is one the index
/// knows, the kinds that need a slug have one, a merge names two different
/// entries, and a name or spelling is not one the entry already has in that
/// shard.
///
/// # Errors
///
/// [`BadRequest`] for a malformed or inconsistent suggestion, and
/// [`Conflict`] for a name or spelling the index already holds.
pub fn check(
    request: SuggestionRequest,
    index: &IngredientIndex,
) -> Result<Checked, FileSuggestionError> {
    let kind = request.kind;
    let locale = request.locale.trim().to_owned();
    if !index.shards().contains(&locale) {
        return Err(BadRequest::new(format!(
            "{locale:?} is not a locale shard of the ingredient index"
        ))
        .into());
    }
    let text = bounded("text", &request.text, MAX_TEXT_CHARS, true)?;
    if text.chars().any(char::is_control) {
        return Err(BadRequest::new("the text holds a control character").into());
    }
    let note = bounded(
        "note",
        request.note.as_deref().unwrap_or_default(),
        MAX_NOTE_CHARS,
        false,
    )?;
    let slug = request.slug.map(|slug| slug.trim().to_owned());
    if let Some(slug) = &slug {
        if index.get(slug).is_none() {
            return Err(BadRequest::new(format!("{slug:?} is not in the ingredient index")).into());
        }
    } else if kind.needs_slug() {
        return Err(BadRequest::new(format!(
            "a {} suggestion names the entry it is about in slug",
            kind.as_str()
        ))
        .into());
    }
    match (kind, slug.as_deref()) {
        (SuggestionKind::Merge, Some(slug)) => {
            if index.get(&text).is_none() || text == slug {
                return Err(BadRequest::new(
                    "a merge names, in text, another entry the index knows",
                )
                .into());
            }
        }
        (SuggestionKind::Name | SuggestionKind::RegionalSpelling, Some(slug)) => {
            if let Some(names) = index.names(slug, &locale) {
                let wanted = normalize(&text);
                let known = std::iter::once(&names.name).chain(&names.aliases);
                if known.into_iter().any(|name| normalize(name) == wanted) {
                    return Err(Conflict::new(format!(
                        "{slug:?} is already called {text:?} in {locale}"
                    ))
                    .into());
                }
            }
        }
        _ => {}
    }
    Ok(Checked {
        kind,
        slug,
        locale,
        text,
        note,
    })
}

/// Files a suggestion for the ingredient index.
///
/// Spends the caller's suggestion budget (`CRISP_LIMIT_SUGGEST`) first, so a
/// refused suggestion still counts. Refused 400 when it does not fit the
/// index (see [`check`]), 409 when the entry already has that name or the
/// caller already holds 50 open suggestions, and 429 over budget.
#[kynos::post("/ingredients/suggestions")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id, kind = request.kind.as_str()))]
pub async fn file_suggestion(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Inject(throttle): Inject<Throttle>,
    Json(request): Json<SuggestionRequest>,
) -> Result<Json<SuggestionView>, FileSuggestionError> {
    let caller = auth.into_inner().resolve()?;
    let suggester = caller.account.usr_id;
    tracing::Span::current().record("usr_id", suggester.as_str());
    throttle.check(Operation::Suggest, &suggester).await?;
    let index = crisp_core::ingredients::index()
        .map_err(|e| Internal::logged("loading the ingredient index", &e))?;
    let checked = check(request, index).inspect_err(|error| {
        tracing::debug!(%error, "suggestion refused");
    })?;
    let suggestion = NewSuggestion {
        suggester,
        kind: checked.kind.as_str().to_owned(),
        slug: checked.slug,
        locale: checked.locale,
        text: checked.text,
        note: checked.note,
        at: now_epoch(),
    };
    match storage
        .file_suggestion(suggestion, MAX_OPEN_SUGGESTIONS)
        .await?
    {
        FileSuggestion::Filed(row) => {
            tracing::info!(
                suggestion_id = row.id,
                slug = row.slug.as_deref(),
                locale = %row.locale,
                "ingredient suggestion filed"
            );
            Ok(Json((*row).try_into()?))
        }
        FileSuggestion::TooMany => {
            tracing::info!("suggestion refused: too many open suggestions");
            Err(Conflict::new(format!(
                "you already have {MAX_OPEN_SUGGESTIONS} open suggestions; \
                 wait for one to be triaged"
            ))
            .into())
        }
    }
}

#[cfg(test)]
mod tests;
