//! Assigning permanent slugs.
//!
//! A slug is derived once from the entry's English canonical id and never
//! changes: a re-import looks the source key up in the previously published
//! entries first. New slugs fold the id to the slug alphabet (accents
//! dropped, anything else a hyphen); a collision takes the lowest free
//! `-2`, `-3`, … suffix, assigned in source-key order so the result is
//! deterministic.

use std::collections::{BTreeMap, BTreeSet};

use unicode_normalization::UnicodeNormalization;

/// Folds a source key (`off:en:crème-fraîche`) to a slug (`creme-fraiche`), or
/// `None` when nothing slug-worthy remains.
#[must_use]
pub fn derive(off_id: &str) -> Option<String> {
    let bare = off_id.rsplit_once(':').map_or(off_id, |(_, rest)| rest);
    let ascii: String = bare
        .nfkd()
        .filter(char::is_ascii)
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = ascii
        .split('-')
        .filter(|run| !run.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug: String = slug.chars().take(60).collect();
    let slug = slug.trim_end_matches('-').to_owned();
    (!slug.is_empty()).then_some(slug)
}

/// Assigns a slug to every source key (sorted), keeping previously published
/// ones. Keys with nothing slug-worthy are returned separately.
#[must_use]
pub fn assign(
    keys: &[String],
    previous: &BTreeMap<String, String>,
    reserved: &BTreeSet<String>,
) -> (BTreeMap<String, String>, Vec<String>) {
    let mut taken: BTreeSet<String> = previous.values().cloned().collect();
    taken.extend(reserved.iter().cloned());
    let mut out = BTreeMap::new();
    let mut unusable = Vec::new();
    let mut fresh: Vec<&String> = Vec::new();
    for key in keys {
        match previous.get(key) {
            Some(slug) => {
                out.insert(key.clone(), slug.clone());
            }
            None => fresh.push(key),
        }
    }
    fresh.sort();
    for key in fresh {
        let Some(base) = derive(key) else {
            unusable.push(key.clone());
            continue;
        };
        // Among `taken.len() + 1` candidates at least one is free.
        let slug = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}-{n}")))
            .take(taken.len() + 1)
            .find(|s| !taken.contains(s))
            .unwrap_or(base);
        taken.insert(slug.clone());
        out.insert(key.clone(), slug);
    }
    (out, unusable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_fold_to_the_slug_alphabet() {
        assert_eq!(derive("en:fish-sauce").as_deref(), Some("fish-sauce"));
        assert_eq!(derive("en:crème-fraîche").as_deref(), Some("creme-fraiche"));
        assert_eq!(derive("en:lion's-mane").as_deref(), Some("lion-s-mane"));
        assert_eq!(
            derive("en:sour-cream-&-chive").as_deref(),
            Some("sour-cream-chive")
        );
        assert_eq!(derive("en:김치"), None);
        assert!(derive(&format!("en:{}", "a".repeat(80))).is_some_and(|s| s.len() == 60));
    }

    #[test]
    fn published_slugs_never_change_and_collisions_get_suffixes() {
        let keys: Vec<String> = [
            "off:en:creme-fraiche",
            "off:en:crème-fraîche",
            "off:en:kimchi",
            "off:en:김치",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let previous = BTreeMap::from([("off:en:kimchi".to_owned(), "kimchi-old".to_owned())]);
        let reserved = BTreeSet::from(["kimchi-aged".to_owned()]);
        let (slugs, unusable) = assign(&keys, &previous, &reserved);
        assert_eq!(slugs["off:en:kimchi"], "kimchi-old");
        assert_eq!(slugs["off:en:creme-fraiche"], "creme-fraiche");
        assert_eq!(slugs["off:en:crème-fraîche"], "creme-fraiche-2");
        assert_eq!(unusable, ["off:en:김치"]);
        let triple: Vec<String> = ["off:en:a", "off:en:á", "off:en:à"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let (slugs, _) = assign(&triple, &BTreeMap::new(), &BTreeSet::new());
        let mut got: Vec<&str> = slugs.values().map(String::as_str).collect();
        got.sort_unstable();
        assert_eq!(got, ["a", "a-2", "a-3"]);
    }
}
