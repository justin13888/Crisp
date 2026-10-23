//! The single-profile hard limits from spec §12 plus the per-field array and
//! string-length bounds mirrored from `crisp-v1.cddl`.
//!
//! A conformant parser MUST reject any bundle that exceeds one of these. They
//! make the data model bounded by construction, so a verifier is single-pass,
//! total, and DoS-resistant. The §12 table and the CDDL inline bounds are both
//! enforced as `E.LIMIT` by the B3 check in [`crate::verify`]. This module is
//! the single Rust-side source of truth for those numbers — touch a constant
//! here only when the corresponding bound in `spec/crisp-v1.cddl` changes.

use crate::error::{CrispError, ErrorCode, Result};

/// Maximum size of a whole `.crisp` file, in bytes (64 MiB).
pub const MAX_BUNDLE_SIZE: usize = 64 * 1024 * 1024;
/// Maximum number of recipes in one bundle.
pub const MAX_RECIPES: usize = 4096;
/// Maximum number of blobs in the content-addressed blob store.
pub const MAX_BLOBS: usize = 8192;
/// Maximum size of a single blob, in bytes (16 MiB).
pub const MAX_BLOB_SIZE: usize = 16 * 1024 * 1024;
/// Maximum combined size of all blob bytes in a bundle (60 MiB).
pub const MAX_TOTAL_BLOB_BYTES: usize = 60 * 1024 * 1024;
/// Maximum number of steps in one recipe.
pub const MAX_STEPS: usize = 512;
/// Maximum number of ingredients in one recipe.
pub const MAX_INGREDIENTS: usize = 512;
/// Maximum number of equipment items in one recipe.
pub const MAX_EQUIPMENT: usize = 256;
/// Maximum number of ingredient groups in one recipe.
pub const MAX_INGREDIENT_GROUPS: usize = 32;
/// Maximum length of any array not otherwise bounded.
pub const MAX_ARRAY_LEN: usize = 4096;
/// Maximum length of any single text string, in bytes (64 KiB).
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Maximum length of a title value, in bytes (`identity.title` and any
/// localized title value).
pub const MAX_TITLE_BYTES: usize = 512;
/// Maximum number of runs in one paragraph.
pub const MAX_RUNS_PER_PARAGRAPH: usize = 1024;
/// Maximum number of paragraphs in one prose block.
pub const MAX_PARAGRAPHS: usize = 4096;
/// Maximum number of declared languages in one bundle.
pub const MAX_LANGUAGES: usize = 32;
/// Maximum number of authors in one bundle.
pub const MAX_AUTHORS: usize = 64;
/// Maximum number of parts in a book.
pub const MAX_BOOK_PARTS: usize = 256;
/// Maximum number of chapters in one book part.
pub const MAX_CHAPTERS_PER_PART: usize = 256;
/// Maximum number of entries in one book chapter.
pub const MAX_ENTRIES_PER_CHAPTER: usize = 4096;
/// Maximum CBOR nesting depth.
pub const MAX_CBOR_DEPTH: usize = 32;

// ---- per-field CDDL array bounds (mirror `spec/crisp-v1.cddl`) -------------

/// Maximum number of language-neutral tags on one recipe (`tags *256`).
pub const MAX_TAGS: usize = 256;
/// Maximum number of cuisine tags on one recipe (`cuisine *32`).
pub const MAX_CUISINE: usize = 32;
/// Maximum number of course-code entries on one recipe (`course *16`).
pub const MAX_COURSE: usize = 16;
/// Maximum number of diet-code entries on one recipe (`diet *32`).
pub const MAX_DIET: usize = 32;
/// Maximum number of allergen-code entries on one recipe (`allergens *32`).
pub const MAX_ALLERGENS: usize = 32;
/// Maximum number of media blob references on one recipe (`media *64`).
pub const MAX_MEDIA: usize = 64;
/// Maximum number of substitutes on one ingredient (`substitutes *16`).
pub const MAX_SUBSTITUTES: usize = 16;
/// Maximum number of ingredients a step can use (`step.ingredients *512`).
pub const MAX_STEP_INGREDIENTS: usize = 512;
/// Maximum number of equipment handles a step can reference
/// (`step.equipment *256`).
pub const MAX_STEP_EQUIPMENT: usize = 256;
/// Maximum number of step groups in one recipe (`step_groups *32`).
pub const MAX_STEP_GROUPS: usize = 32;
/// Maximum number of timers on one step (`step.timers *8`).
pub const MAX_STEP_TIMERS: usize = 8;
/// Maximum number of alternative measures on one ingredient (`alt *4`).
pub const MAX_ALT_QUANTITIES: usize = 4;
/// Maximum number of style flags on one run (`style *4`).
pub const MAX_RUN_STYLES: usize = 4;
/// Maximum number of front- or back-matter pages in a book (`*64 page`).
pub const MAX_MATTER_PAGES: usize = 64;

// ---- primitive string-length bounds (mirror `spec/crisp-v1.cddl`) ----------

/// Minimum byte length of a BCP-47 language tag (`language-tag .size (2..35)`).
pub const LANGUAGE_TAG_MIN_BYTES: usize = 2;
/// Maximum byte length of a BCP-47 language tag (`language-tag .size (2..35)`).
pub const LANGUAGE_TAG_MAX_BYTES: usize = 35;
/// Minimum byte length of a UCUM unit code (`unit-code .size (1..64)`).
pub const UNIT_CODE_MIN_BYTES: usize = 1;
/// Maximum byte length of a UCUM unit code (`unit-code .size (1..64)`).
pub const UNIT_CODE_MAX_BYTES: usize = 64;

/// Returns `Err` with [`ErrorCode::Limit`] if `actual` exceeds `max`.
///
/// `what` names the thing being measured and appears in the error detail.
///
/// # Errors
///
/// Returns [`CrispError::Check`] tagged [`ErrorCode::Limit`] when the limit is
/// exceeded.
pub fn enforce(actual: usize, max: usize, what: &str) -> Result<()> {
    if actual > max {
        Err(CrispError::check(
            ErrorCode::Limit,
            format!("{what}: {actual} exceeds the maximum of {max}"),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_limits_match_the_spec_table() {
        assert_eq!(MAX_BUNDLE_SIZE, 67_108_864);
        assert_eq!(MAX_BLOB_SIZE, 16_777_216);
        assert_eq!(MAX_TOTAL_BLOB_BYTES, 62_914_560);
        assert_eq!(MAX_TEXT_BYTES, 65_536);
    }

    /// The normative CDDL schema these constants mirror.
    const CDDL: &str = include_str!("../../spec/crisp-v1.cddl");

    /// The CDDL rules that start with `key` (after an optional `? `), with
    /// their trailing `;` comments removed.
    fn cddl_rules(key: &str) -> Vec<&'static str> {
        CDDL.lines()
            .map(|line| line.split(';').next().unwrap_or_default().trim())
            .map(|line| line.strip_prefix("? ").unwrap_or(line))
            .filter(|line| line.starts_with(key))
            .collect()
    }

    /// The `N` of every `key ... [ M*N elem ]` rule in the CDDL. Panics
    /// unless at least one rule matches and every match agrees.
    fn array_bound(key: &str, elem: &str) -> usize {
        let bounds: Vec<usize> = cddl_rules(key)
            .into_iter()
            .filter_map(|rule| {
                let inner = rule.split_once('[')?.1.split_once(']')?.0;
                let mut tokens = inner.split_whitespace();
                let (bound, element) = (tokens.next()?, tokens.next()?);
                (element == elem && tokens.next().is_none()).then_some(bound)
            })
            .map(|bound| {
                let (_, max) = bound.split_once('*').expect("a `*N` occurrence bound");
                max.parse().expect("a numeric upper bound")
            })
            .collect();
        assert!(!bounds.is_empty(), "no CDDL rule `{key} [ *N {elem} ]`");
        assert!(
            bounds.windows(2).all(|w| w[0] == w[1]),
            "CDDL rules `{key} [ *N {elem} ]` disagree: {bounds:?}"
        );
        bounds[0]
    }

    /// The `(min, max)` of every `key ... .size (min..max)` rule in the CDDL.
    /// Panics unless at least one rule matches and every match agrees.
    fn size_bound(key: &str) -> (usize, usize) {
        let bounds: Vec<(usize, usize)> = cddl_rules(key)
            .into_iter()
            .filter_map(|rule| {
                let range = rule.split_once(".size (")?.1.split_once(')')?.0;
                let (min, max) = range.split_once("..")?;
                Some((
                    min.parse().expect("a numeric lower bound"),
                    max.parse().expect("a numeric upper bound"),
                ))
            })
            .collect();
        assert!(!bounds.is_empty(), "no CDDL rule `{key} .size (..)`");
        assert!(
            bounds.windows(2).all(|w| w[0] == w[1]),
            "CDDL rules `{key} .size (..)` disagree: {bounds:?}"
        );
        bounds[0]
    }

    #[test]
    fn per_field_bounds_match_the_cddl_numbers() {
        let cases = [
            (MAX_LANGUAGES, "\"languages\":", "language-tag"),
            (MAX_RECIPES, "\"recipes\":", "recipe"),
            (MAX_AUTHORS, "\"authors\":", "author"),
            (
                MAX_INGREDIENT_GROUPS,
                "\"ingredient_groups\":",
                "localized-text",
            ),
            (MAX_INGREDIENTS, "\"ingredients\":", "ingredient"),
            (MAX_EQUIPMENT, "\"equipment\":", "equipment"),
            (MAX_STEP_GROUPS, "\"step_groups\":", "localized-text"),
            (MAX_STEPS, "\"steps\":", "step"),
            (MAX_TAGS, "\"tags\":", "slug"),
            (MAX_CUISINE, "\"cuisine\":", "slug"),
            (MAX_COURSE, "\"course\":", "course-code"),
            (MAX_DIET, "\"diet\":", "diet-code"),
            (MAX_ALLERGENS, "\"allergens\":", "allergen-code"),
            (MAX_MEDIA, "\"media\":", "blob-id"),
            (MAX_ALT_QUANTITIES, "\"alt\":", "quantity"),
            (MAX_SUBSTITUTES, "\"substitutes\":", "substitute"),
            (MAX_STEP_INGREDIENTS, "\"ingredients\":", "step-ingredient"),
            (MAX_STEP_EQUIPMENT, "\"equipment\":", "uint"),
            (MAX_STEP_TIMERS, "\"timers\":", "timer"),
            (MAX_PARAGRAPHS, "prose ", "paragraph"),
            (MAX_RUNS_PER_PARAGRAPH, "paragraph ", "run"),
            (MAX_RUN_STYLES, "\"style\":", "style-flag"),
            (MAX_MATTER_PAGES, "\"front_matter\":", "page"),
            (MAX_MATTER_PAGES, "\"back_matter\":", "page"),
            (MAX_BOOK_PARTS, "\"parts\":", "part"),
            (MAX_CHAPTERS_PER_PART, "\"chapters\":", "chapter"),
            (MAX_ENTRIES_PER_CHAPTER, "\"entries\":", "entry"),
        ];
        for (constant, key, elem) in cases {
            assert_eq!(constant, array_bound(key, elem), "{key} [ *N {elem} ]");
        }
    }

    #[test]
    fn string_bounds_match_the_cddl_sizes() {
        assert_eq!(
            (LANGUAGE_TAG_MIN_BYTES, LANGUAGE_TAG_MAX_BYTES),
            size_bound("language-tag ")
        );
        assert_eq!(
            (UNIT_CODE_MIN_BYTES, UNIT_CODE_MAX_BYTES),
            size_bound("unit-code ")
        );
        assert_eq!(MAX_TITLE_BYTES, size_bound("\"title\":").1);
        assert_eq!(MAX_TEXT_BYTES, size_bound("localized-text ").1);
        assert_eq!(MAX_TEXT_BYTES, size_bound("\"t\":").1);
    }

    #[test]
    fn enforce_accepts_values_up_to_and_including_the_limit() {
        assert!(enforce(0, MAX_RECIPES, "recipes").is_ok());
        assert!(enforce(MAX_RECIPES, MAX_RECIPES, "recipes").is_ok());
    }

    #[test]
    fn enforce_rejects_values_over_the_limit() {
        let err = enforce(MAX_RECIPES + 1, MAX_RECIPES, "recipes").unwrap_err();
        assert_eq!(err.code(), Some(ErrorCode::Limit));
        assert!(err.to_string().contains("recipes"));
    }
}
