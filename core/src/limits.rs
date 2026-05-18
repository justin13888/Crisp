//! The single-profile hard limits from spec §12.
//!
//! A conformant parser MUST reject any bundle that exceeds one of these. They
//! make the data model bounded by construction, so a verifier is single-pass,
//! total, and DoS-resistant. Limits expressed inline in `crisp-v1.cddl` (per-
//! field array bounds) are enforced as part of schema validation; the values
//! here are the explicit §12 table and are checked separately as `E.LIMIT`.

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

/// Smallest integer the format admits — integers are signed 64-bit.
pub const INT_MIN: i64 = i64::MIN;
/// Largest integer the format admits — integers are signed 64-bit.
pub const INT_MAX: i64 = i64::MAX;

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
