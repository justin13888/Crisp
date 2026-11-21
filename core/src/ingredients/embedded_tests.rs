//! Tests over the embedded, published index (`spec/ingredients/`).

use std::collections::{BTreeMap, BTreeSet};

use super::matcher::exact_key;
use super::*;
use crate::verify::schema::is_token;

/// The embedded index may not grow past this without revisiting how the
/// client loads it (`docs/ingredient-index.md`, "Distribution").
const SIZE_BUDGET: usize = 2 * 1024 * 1024;

fn embedded() -> &'static IngredientIndex {
    index().expect("the embedded index decodes and validates")
}

#[test]
fn the_embedded_index_is_valid_versioned_and_within_budget() {
    let idx = embedded();
    assert!(idx.len() > 1000, "{} entries", idx.len());
    assert_eq!(idx.version().len(), 64, "a SHA-256 hex version");
    assert!(EMBEDDED.len() <= SIZE_BUDGET, "{} bytes", EMBEDDED.len());
    println!("embedded index: {} bytes of {SIZE_BUDGET}", EMBEDDED.len());
    assert_eq!(idx.shards().len(), 16);
    for entry in idx.entries() {
        assert!(is_token(&entry.slug, '-'), "{}", entry.slug);
        assert!(!entry.category.is_empty(), "{}", entry.slug);
        for target in [&entry.parent, &entry.replaced_by].into_iter().flatten() {
            assert!(idx.get(target).is_some(), "{} → {target}", entry.slug);
        }
        if let Some(d) = entry.density {
            assert!(d.numerator > 0 && d.denominator > 0, "{}", entry.slug);
        }
    }
}

#[test]
fn every_shard_tag_is_region_qualified() {
    for shard in embedded().shards() {
        let parts: Vec<&str> = shard.split('-').collect();
        let region = parts.last().copied().unwrap_or_default();
        assert!(parts.len() >= 2, "{shard} lacks a region");
        assert!(
            region.len() == 2 && region.bytes().all(|b| b.is_ascii_uppercase()),
            "{shard}"
        );
    }
}

#[test]
fn the_kimchi_example_holds_on_the_published_data() {
    let idx = embedded();
    assert!(idx.is_a("kimchi-aged", "kimchi"));
    let exact = |text, tag| match idx.match_name(text, tag, 5) {
        Match::Exact(slug) => Some(slug),
        _ => None,
    };
    assert_eq!(exact("묵은지", "ko-KR").as_deref(), Some("kimchi-aged"));
    assert_eq!(exact("aged kimchi", "en").as_deref(), Some("kimchi-aged"));
    assert_eq!(exact("kimchi", "en").as_deref(), Some("kimchi"));
    assert_eq!(exact("김치", "ko").as_deref(), Some("kimchi"));
    assert_eq!(exact("キムチ", "ja-JP").as_deref(), Some("kimchi"));
}

#[test]
fn regional_english_and_chinese_resolve_on_the_published_data() {
    let idx = embedded();
    let exact = |text, tag| match idx.match_name(text, tag, 5) {
        Match::Exact(slug) => Some(slug),
        _ => None,
    };
    let eggplant = exact("eggplant", "en-US").expect("eggplant links");
    assert_eq!(
        exact("aubergine", "en-GB").as_deref(),
        Some(eggplant.as_str())
    );
    assert_eq!(
        idx.display_name(&eggplant, "en-GB").map(|n| n.name),
        Some("aubergine")
    );
    assert_eq!(
        idx.display_name(&eggplant, "en-US").map(|n| n.name),
        Some("eggplant")
    );
    assert_eq!(
        idx.display_name("kimchi", "zh-Hant-MO")
            .map(|n| (n.name, n.shard)),
        Some(("韓國泡菜", "zh-Hant-HK"))
    );
    for (text, tag) in [
        ("泡菜", "zh-Hant-TW"),
        ("fish sauce", "en-AU"),
        ("nuoc mam", "vi"),
    ] {
        println!("{tag} {text:?} → {:?}", idx.match_name(text, tag, 3));
    }
}

#[test]
fn a_spelling_that_differs_only_in_accents_is_suggested_not_linked() {
    let idx = embedded();
    for (text, tag) in [("bò", "vi"), ("pâté", "fr"), ("bo", "vi-VN")] {
        let outcome = idx.match_name(text, tag, 5);
        assert!(
            matches!(outcome, Match::Candidates(_) | Match::None),
            "{tag} {text:?} → {outcome:?}"
        );
    }
    let butter = idx.match_name("bơ", "vi-VN", 5);
    assert!(matches!(butter, Match::Exact(_)), "{butter:?}");
}

#[test]
fn shared_spellings_never_link_automatically() {
    let idx = embedded();
    let mut checked = 0;
    for (shard, table) in &idx.spellings {
        for found in table.values() {
            let mut by_exact: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
            for (slug, spelling) in found {
                let current = idx.resolve(slug).map_or(slug.as_str(), |e| e.slug.as_str());
                by_exact
                    .entry(exact_key(spelling))
                    .or_default()
                    .insert(current);
            }
            for (spelling, slugs) in by_exact.iter().filter(|(_, s)| s.len() > 1) {
                checked += 1;
                assert!(
                    !matches!(idx.match_name(spelling, shard, 5), Match::Exact(_)),
                    "{shard} {spelling:?} is shared by {slugs:?} yet links exactly"
                );
            }
        }
    }
    println!("{checked} shared spellings checked");
}
