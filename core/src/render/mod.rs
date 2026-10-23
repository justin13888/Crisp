//! Viewer helpers: locale resolution, scaling, unit conversion, quantity
//! formatting and parsing, and the render-ready [`view::RecipeView`].
//!
//! These are pure conveniences that make no trust decisions — a bundle must
//! already have been verified. They live in `core` so every client renders
//! amounts, times, and temperatures identically.

pub mod convert;
pub mod number;
pub mod parse;
pub mod quantity;
mod resolver;
pub mod scale;
pub mod units;
pub mod view;
#[cfg(test)]
mod view_tests;

use std::collections::BTreeMap;

use crate::model::primitives::LanguageTag;
use crate::model::text::{LocalizedProse, LocalizedText, Prose};

pub use number::multiply;
pub use parse::{parse_amount, parse_duration};
pub use quantity::{Units, format_quantity};
pub use scale::{scale_quantity, scale_recipe};
pub use view::{RecipeView, ViewOptions, recipe_view};

/// Resolves the best string from a [`LocalizedText`] for a reader.
///
/// Tries each `preferred` language in order, then the bundle's
/// `default_language`, then any available value (spec §5).
#[must_use]
pub fn resolve_text<'a>(
    text: &'a LocalizedText,
    preferred: &[LanguageTag],
    default_language: &LanguageTag,
) -> Option<&'a str> {
    pick(&text.0, preferred, default_language).map(String::as_str)
}

/// Resolves the best [`Prose`] from a [`LocalizedProse`] for a reader.
#[must_use]
pub fn resolve_prose<'a>(
    prose: &'a LocalizedProse,
    preferred: &[LanguageTag],
    default_language: &LanguageTag,
) -> Option<&'a Prose> {
    pick(&prose.0, preferred, default_language)
}

/// Shared best-match logic over a localized map.
fn pick<'a, V>(
    map: &'a BTreeMap<LanguageTag, V>,
    preferred: &[LanguageTag],
    default_language: &LanguageTag,
) -> Option<&'a V> {
    preferred
        .iter()
        .find_map(|tag| map.get(tag))
        .or_else(|| map.get(default_language))
        .or_else(|| map.values().next())
}

/// Flattens prose to plain text: runs joined, paragraphs separated by
/// newlines. Used for search indexing and previews.
#[must_use]
pub fn prose_to_plain(prose: &Prose) -> String {
    prose
        .0
        .iter()
        .map(|p| p.0.iter().map(|r| r.t.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(s: &str) -> LanguageTag {
        LanguageTag(s.to_owned())
    }

    #[test]
    fn resolve_prefers_a_preferred_language_then_the_default() {
        let mut text = crate::fixtures::english("Hello");
        text.0.insert(tag("fr"), "Bonjour".to_owned());
        let default = tag("en");
        assert_eq!(resolve_text(&text, &[tag("fr")], &default), Some("Bonjour"));
        assert_eq!(resolve_text(&text, &[tag("de")], &default), Some("Hello"));
    }

    #[test]
    fn resolve_falls_back_to_any_language_when_neither_matches() {
        let mut text = LocalizedText(BTreeMap::new());
        assert_eq!(resolve_text(&text, &[], &tag("en")), None);
        text.0.insert(tag("fr"), "Bonjour".to_owned());
        assert_eq!(resolve_text(&text, &[], &tag("en")), Some("Bonjour"));
    }

    #[test]
    fn prose_flattens_to_plain_text() {
        let prose = crate::fixtures::english_prose("Mix.");
        let body = resolve_prose(&prose, &[], &tag("en")).expect("prose");
        assert_eq!(prose_to_plain(body), "Mix.");
    }
}
