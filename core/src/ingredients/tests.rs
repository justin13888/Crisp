//! Contract tests for the ingredient index over a small fixture dataset.

use std::collections::BTreeMap;

use proptest::prelude::*;

use super::*;
use crate::model::primitives::{OpenCode, Rational};
use crate::model::vocab::{AllergenCode, DietCode};

fn entry(slug: &str, parent: Option<&str>, category: &str) -> Entry {
    Entry {
        slug: slug.to_owned(),
        parent: parent.map(str::to_owned),
        category: category.to_owned(),
        density: None,
        allergens: Vec::new(),
        diet_conflicts: Vec::new(),
        replaced_by: None,
        source: format!("off:en:{slug}"),
    }
}

fn names(name: &str, aliases: &[&str]) -> Names {
    Names {
        name: name.to_owned(),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
    }
}

const SHARDS: [&str; 6] = [
    "en-US",
    "en-GB",
    "ko-KR",
    "zh-Hans-CN",
    "zh-Hant-TW",
    "zh-Hant-HK",
];

fn locales() -> LocaleTable {
    let list = |tags: &[&str]| tags.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
    LocaleTable {
        shards: list(&SHARDS),
        fallback: BTreeMap::from([
            ("en".to_owned(), list(&["en-US", "en-GB"])),
            ("en-au".to_owned(), list(&["en-GB", "en-US"])),
            ("zh".to_owned(), list(&["zh-Hans-CN"])),
            ("zh-hant".to_owned(), list(&["zh-Hant-TW", "zh-Hant-HK"])),
            ("zh-hant-mo".to_owned(), list(&["zh-Hant-HK", "zh-Hant-TW"])),
            ("zh-tw".to_owned(), list(&["zh-Hant-TW"])),
        ]),
        last_resort: list(&["en-US", "en-GB"]),
    }
}

/// kimchi ⊃ kimchi-aged; vegetables ⊃ eggplant; dairy ⊃ butter ⊃
/// unsalted-butter; `sweet-butter` was merged into `unsalted-butter`.
fn dataset() -> Dataset {
    let mut fish_sauce = entry("fish-sauce", None, "condiment");
    fish_sauce.allergens = vec![OpenCode::Known(AllergenCode::Fish)];
    fish_sauce.diet_conflicts = vec![OpenCode::Known(DietCode::Vegan)];
    fish_sauce.density = Some(Rational::new(6, 5));
    let mut sweet_butter = entry("sweet-butter", Some("butter"), "dairy");
    sweet_butter.replaced_by = Some("unsalted-butter".to_owned());
    let mut kimchi_aged = entry("kimchi-aged", Some("kimchi"), "fermented");
    kimchi_aged.source = "crisp:override".to_owned();
    let shard = |pairs: Vec<(&str, Names)>| {
        pairs
            .into_iter()
            .map(|(s, n)| (s.to_owned(), n))
            .collect::<BTreeMap<_, _>>()
    };
    Dataset {
        version: "test".to_owned(),
        locales: locales(),
        entries: vec![
            entry("butter", Some("dairy"), "dairy"),
            entry("dairy", None, "dairy"),
            entry("eggplant", Some("vegetables"), "produce"),
            fish_sauce,
            entry("kimchi", None, "fermented"),
            kimchi_aged,
            entry("pickled-cabbage", None, "fermented"),
            entry("cultured-sweet-butter", Some("sweet-butter"), "dairy"),
            sweet_butter,
            entry("unsalted-butter", Some("butter"), "dairy"),
            entry("vegetables", None, "produce"),
        ],
        names: BTreeMap::from([
            (
                "en-US".to_owned(),
                shard(vec![
                    ("butter", names("Butter", &[])),
                    ("eggplant", names("Eggplant", &[])),
                    ("fish-sauce", names("Fish sauce", &["nuoc mam"])),
                    ("kimchi", names("Kimchi", &["kimchee"])),
                    (
                        "kimchi-aged",
                        names("Aged kimchi", &["sour kimchi", "mukeunji"]),
                    ),
                    ("sweet-butter", names("Sweet butter", &[])),
                    ("unsalted-butter", names("Unsalted butter", &[])),
                ]),
            ),
            (
                "en-GB".to_owned(),
                shard(vec![("eggplant", names("Aubergine", &[]))]),
            ),
            (
                "ko-KR".to_owned(),
                shard(vec![
                    ("kimchi", names("김치", &[])),
                    ("kimchi-aged", names("묵은지", &["신김치"])),
                ]),
            ),
            (
                "zh-Hans-CN".to_owned(),
                shard(vec![
                    ("kimchi", names("泡菜", &["韩国泡菜"])),
                    ("pickled-cabbage", names("泡菜", &[])),
                ]),
            ),
            (
                "zh-Hant-TW".to_owned(),
                shard(vec![("kimchi", names("韓式泡菜", &["泡菜"]))]),
            ),
            (
                "zh-Hant-HK".to_owned(),
                shard(vec![("kimchi", names("韓國泡菜", &[]))]),
            ),
        ]),
    }
}

fn index() -> IngredientIndex {
    IngredientIndex::new(dataset()).expect("fixture index is valid")
}

fn slugs(entries: &[&Entry]) -> Vec<String> {
    entries.iter().map(|e| e.slug.clone()).collect()
}

// ---- validation -------------------------------------------------------------

/// An edit that breaks the fixture dataset.
type Mutation = fn(&mut Dataset);

#[test]
fn a_valid_dataset_builds_and_round_trips_through_cbor() {
    let mut bytes = Vec::new();
    ciborium::into_writer(&dataset(), &mut bytes).expect("encode");
    let decoded = IngredientIndex::from_cbor(&bytes).expect("decode");
    assert_eq!(decoded.len(), 11);
    assert_eq!(decoded.version(), "test");
    assert_eq!(decoded.shards().len(), SHARDS.len());
    assert!(!decoded.is_empty());
}

#[test]
fn an_empty_dataset_is_an_empty_index() {
    let mut d = dataset();
    d.entries.clear();
    d.names.clear();
    let idx = IngredientIndex::new(d).expect("empty index");
    assert!(idx.is_empty());
    assert_eq!(idx.entries().count(), 0);
    let full = index();
    assert!(!full.is_empty());
    assert_eq!(full.entries().count(), full.len());
    assert_eq!(
        full.entries().next().map(|e| e.slug.as_str()),
        Some("butter")
    );
}

#[test]
fn a_long_acyclic_chain_is_not_mistaken_for_a_cycle() {
    let mut d = dataset();
    d.names.clear();
    d.entries = (0..5)
        .map(|i| {
            entry(
                &format!("e{i}"),
                (i > 0).then(|| format!("e{}", i - 1)).as_deref(),
                "x",
            )
        })
        .collect();
    let idx = IngredientIndex::new(d).expect("a chain through every entry");
    assert_eq!(idx.ancestors("e4").len(), 4);
}

#[test]
fn bytes_that_are_not_a_dataset_are_a_decode_error() {
    let err = IngredientIndex::from_cbor(&[0xa0]).expect_err("an empty map");
    assert!(matches!(err, IndexError::Decode(_)), "{err:?}");
}

#[test]
fn malformed_datasets_are_rejected_with_the_reason() {
    let cases: Vec<(Mutation, IndexError)> = vec![
        (
            |d| d.entries[0].slug = "Bad Slug".to_owned(),
            IndexError::InvalidSlug("Bad Slug".to_owned()),
        ),
        (
            |d| d.entries[1].slug = "butter".to_owned(),
            IndexError::DuplicateSlug("butter".to_owned()),
        ),
        (
            |d| d.entries[0].parent = Some("nowhere".to_owned()),
            IndexError::UnknownTarget {
                slug: "butter".to_owned(),
                target: "nowhere".to_owned(),
            },
        ),
        (
            |d| d.entries[1].parent = Some("unsalted-butter".to_owned()),
            IndexError::Cycle("butter".to_owned()),
        ),
        (
            |d| d.entries[8].replaced_by = Some("sweet-butter".to_owned()),
            IndexError::Cycle("sweet-butter".to_owned()),
        ),
        (
            |d| d.locales.last_resort.push("fr-FR".to_owned()),
            IndexError::UnknownShard("fr-FR".to_owned()),
        ),
        (
            |d| {
                d.locales.fallback.insert("en-CA".to_owned(), Vec::new());
            },
            IndexError::FallbackKey("en-CA".to_owned()),
        ),
        (
            |d| {
                d.names.insert("fr-FR".to_owned(), BTreeMap::new());
            },
            IndexError::UnknownShard("fr-FR".to_owned()),
        ),
        (
            |d| {
                let en = d.names.get_mut("en-US").expect("en-US");
                en.insert("ghost".to_owned(), names("Ghost", &[]));
            },
            IndexError::UnknownNamed {
                shard: "en-US".to_owned(),
                slug: "ghost".to_owned(),
            },
        ),
        (
            |d| {
                let en = d.names.get_mut("en-US").expect("en-US");
                en.insert("dairy".to_owned(), names("  ", &[]));
            },
            IndexError::EmptyName {
                shard: "en-US".to_owned(),
                slug: "dairy".to_owned(),
            },
        ),
    ];
    for (mutate, expected) in cases {
        let mut d = dataset();
        mutate(&mut d);
        assert_eq!(IngredientIndex::new(d).expect_err("invalid"), expected);
    }
}

// ---- hierarchy --------------------------------------------------------------

#[test]
fn the_hierarchy_answers_ancestors_descendants_and_is_a() {
    let idx = index();
    assert_eq!(
        slugs(&idx.ancestors("unsalted-butter")),
        ["butter", "dairy"]
    );
    assert!(idx.ancestors("dairy").is_empty());
    assert!(idx.ancestors("unknown").is_empty());
    assert_eq!(
        slugs(&idx.descendants("dairy")),
        [
            "butter",
            "cultured-sweet-butter",
            "sweet-butter",
            "unsalted-butter"
        ]
    );
    assert_eq!(
        slugs(&idx.descendants("unsalted-butter")),
        ["cultured-sweet-butter", "sweet-butter"],
        "a merged entry and its children count under the successor"
    );
    assert_eq!(
        slugs(&idx.descendants("sweet-butter")),
        ["cultured-sweet-butter", "sweet-butter"],
        "a merged slug answers for its successor"
    );
    assert_eq!(
        slugs(&idx.ancestors("cultured-sweet-butter")),
        ["unsalted-butter", "butter", "dairy"]
    );
    assert!(idx.is_a("cultured-sweet-butter", "unsalted-butter"));
    assert!(idx.is_a("kimchi-aged", "kimchi"));
    assert!(idx.is_a("kimchi", "kimchi"));
    assert!(!idx.is_a("kimchi", "kimchi-aged"));
    assert!(idx.is_a("sweet-butter", "unsalted-butter"));
    assert!(idx.is_a("not-indexed", "not-indexed"));
    assert!(!idx.is_a("not-indexed", "kimchi"));
}

#[test]
fn a_parent_merged_into_its_child_ends_the_ancestor_walk() {
    let mut d = dataset();
    d.names.clear();
    d.entries = vec![entry("a", Some("b"), "x"), entry("b", None, "x")];
    d.entries[1].replaced_by = Some("a".to_owned());
    let idx = IngredientIndex::new(d).expect("valid: each link alone is acyclic");
    assert!(idx.ancestors("a").is_empty());
    assert!(idx.is_a("a", "b"));
    // `b` is merged into `a`, so it counts under `a`; `a` never counts under itself.
    assert_eq!(slugs(&idx.descendants("a")), ["b"]);
    assert_eq!(slugs(&idx.descendants("b")), ["b"]);
}

#[test]
fn replaced_entries_resolve_to_their_successor() {
    let idx = index();
    assert_eq!(
        idx.get("sweet-butter").map(|e| e.slug.as_str()),
        Some("sweet-butter")
    );
    assert_eq!(
        idx.resolve("sweet-butter").map(|e| e.slug.as_str()),
        Some("unsalted-butter")
    );
    assert_eq!(
        idx.resolve("kimchi").map(|e| e.slug.as_str()),
        Some("kimchi")
    );
    assert!(idx.resolve("unknown").is_none());
    let hints = idx.get("fish-sauce").expect("fish sauce");
    assert_eq!(hints.allergens, [OpenCode::Known(AllergenCode::Fish)]);
    assert_eq!(hints.diet_conflicts, [OpenCode::Known(DietCode::Vegan)]);
}

// ---- locales ----------------------------------------------------------------

#[test]
fn fallback_chains_follow_exact_table_language_then_english() {
    let idx = index();
    let cases: [(&str, &[&str]); 9] = [
        ("en-GB", &["en-GB", "en-US"]),
        ("en-US", &["en-US", "en-GB"]),
        ("en", &["en-US", "en-GB"]),
        ("en-AU", &["en-GB", "en-US"]),
        (
            "zh-Hant-MO",
            &["zh-Hant-HK", "zh-Hant-TW", "zh-Hans-CN", "en-US", "en-GB"],
        ),
        (
            "zh-hant-hk",
            &["zh-Hant-HK", "zh-Hant-TW", "zh-Hans-CN", "en-US", "en-GB"],
        ),
        (
            "zh-TW",
            &["zh-Hant-TW", "zh-Hans-CN", "zh-Hant-HK", "en-US", "en-GB"],
        ),
        ("ko", &["ko-KR", "en-US", "en-GB"]),
        ("fr-FR", &["en-US", "en-GB"]),
    ];
    for (tag, chain) in cases {
        assert_eq!(idx.fallback_chain(tag), chain, "{tag}");
    }
}

#[test]
fn display_names_come_from_the_best_shard_that_has_one() {
    let idx = index();
    let name = |slug, tag| idx.display_name(slug, tag).map(|n| (n.name, n.shard));
    assert_eq!(name("eggplant", "en-GB"), Some(("Aubergine", "en-GB")));
    assert_eq!(name("eggplant", "en-US"), Some(("Eggplant", "en-US")));
    assert_eq!(name("eggplant", "en-AU"), Some(("Aubergine", "en-GB")));
    assert_eq!(
        name("kimchi", "zh-Hant-MO"),
        Some(("韓國泡菜", "zh-Hant-HK"))
    );
    assert_eq!(name("kimchi-aged", "ko"), Some(("묵은지", "ko-KR")));
    assert_eq!(name("butter", "ko-KR"), Some(("Butter", "en-US")));
    assert_eq!(name("dairy", "en"), None);
    assert_eq!(
        idx.names("kimchi", "ko-KR").map(|n| n.name.as_str()),
        Some("김치")
    );
}

// ---- matcher ----------------------------------------------------------------

fn exact(idx: &IngredientIndex, text: &str, tag: &str) -> Option<String> {
    match idx.match_name(text, tag, 5) {
        Match::Exact(slug) => Some(slug),
        _ => None,
    }
}

fn suggested(idx: &IngredientIndex, text: &str, tag: &str) -> Vec<String> {
    match idx.match_name(text, tag, 5) {
        Match::Candidates(list) => list.into_iter().map(|c| c.slug).collect(),
        other => panic!("expected candidates for {text:?}, got {other:?}"),
    }
}

#[test]
fn the_kimchi_example_links_the_most_specific_entry() {
    let idx = index();
    assert_eq!(
        exact(&idx, "묵은지", "ko-KR").as_deref(),
        Some("kimchi-aged")
    );
    assert_eq!(exact(&idx, "신김치", "ko").as_deref(), Some("kimchi-aged"));
    assert_eq!(exact(&idx, "Kimchi", "en").as_deref(), Some("kimchi"));
    assert_eq!(
        exact(&idx, "  sour   KIMCHI ", "en-US").as_deref(),
        Some("kimchi-aged")
    );
    assert_eq!(
        exact(&idx, "mukeunji", "ko-KR").as_deref(),
        Some("kimchi-aged")
    );
}

#[test]
fn regional_spellings_link_through_their_own_shard() {
    let idx = index();
    assert_eq!(
        exact(&idx, "aubergine", "en-GB").as_deref(),
        Some("eggplant")
    );
    assert_eq!(
        exact(&idx, "eggplant", "en-US").as_deref(),
        Some("eggplant")
    );
    assert_eq!(exact(&idx, "泡菜", "zh-Hant-TW").as_deref(), Some("kimchi"));
}

#[test]
fn an_ambiguous_spelling_suggests_instead_of_linking() {
    let idx = index();
    let list = suggested(&idx, "泡菜", "zh-Hans-CN");
    assert_eq!(&list[..2], ["kimchi", "pickled-cabbage"]);
}

#[test]
fn accents_must_match_for_an_automatic_link() {
    let idx = index();
    assert_eq!(exact(&idx, "Kimchí", "en"), None);
    assert_eq!(suggested(&idx, "Kimchí", "en")[0], "kimchi");
    assert_eq!(exact(&idx, "ＫＩＭＣＨＩ", "en").as_deref(), Some("kimchi"));
}

#[test]
fn a_merged_entry_links_to_its_successor() {
    let idx = index();
    assert_eq!(
        exact(&idx, "sweet butter", "en").as_deref(),
        Some("unsalted-butter")
    );
}

#[test]
fn partial_names_are_ranked_suggestions() {
    let idx = index();
    let list = suggested(&idx, "kim", "en-US");
    assert_eq!(list, ["kimchi", "kimchi-aged"]);
    let butter = suggested(&idx, "butt", "en-US");
    assert_eq!(butter[0], "butter");
    assert!(butter.contains(&"unsalted-butter".to_owned()));
    let Match::Candidates(detail) = idx.match_name("aub", "en-GB", 5) else {
        panic!("expected candidates");
    };
    assert_eq!(detail[0].slug, "eggplant");
    assert_eq!(detail[0].shard, "en-GB");
    assert!(detail[0].exact_region);
}

#[test]
fn an_exact_region_hit_outranks_a_fallback_hit() {
    let idx = index();
    let top = |tag| match idx.match_name("aub", tag, 5) {
        Match::Candidates(list) => list[0].clone(),
        other => panic!("expected candidates, got {other:?}"),
    };
    let in_region = top("en-GB");
    let by_fallback = top("en-AU");
    assert_eq!(
        (in_region.slug.as_str(), in_region.shard.as_str()),
        ("eggplant", "en-GB")
    );
    assert_eq!(
        (by_fallback.slug.as_str(), by_fallback.shard.as_str()),
        ("eggplant", "en-GB")
    );
    assert!(in_region.exact_region && !by_fallback.exact_region);
    assert!(in_region.score > by_fallback.score);
    let further = match idx.match_name("aub", "en-US", 5) {
        Match::Candidates(list) => list[0].clone(),
        other => panic!("expected candidates, got {other:?}"),
    };
    assert!(
        by_fallback.score > further.score,
        "a later chain shard scores lower"
    );
}

#[test]
fn nothing_matches_empty_or_unknown_text_and_limits_apply() {
    let idx = index();
    assert_eq!(idx.match_name("", "en", 5), Match::None);
    assert_eq!(idx.match_name("  -- ", "en", 5), Match::None);
    assert_eq!(idx.match_name("quinoa", "en", 5), Match::None);
    assert_eq!(idx.match_name("kim", "en", 0), Match::None);
    let Match::Candidates(one) = idx.match_name("kim", "en", 1) else {
        panic!("expected candidates");
    };
    assert_eq!(one.len(), 1);
}

#[test]
fn contains_matches_need_two_characters() {
    let idx = index();
    let one = suggested(&idx, "u", "en-US");
    assert!(one.contains(&"unsalted-butter".to_owned()), "{one:?}");
    assert!(
        !one.contains(&"butter".to_owned()),
        "a lone letter inside a word: {one:?}"
    );
    let two = suggested(&idx, "tt", "en-US");
    assert!(two.contains(&"butter".to_owned()), "{two:?}");
}

#[test]
fn every_separator_class_splits_words() {
    for separator in [
        '\u{2010}', '\u{3001}', '\u{FE30}', '\u{00B7}', '\u{30FB}', '!',
    ] {
        assert_eq!(normalize(&format!("a{separator}b")), "a b", "{separator:?}");
    }
    assert_eq!(normalize("a\u{00E9}b"), "aeb");
}

#[test]
fn normalization_folds_case_accents_width_and_punctuation() {
    assert_eq!(normalize("  Crème-Fraîche!! "), "creme fraiche");
    assert_eq!(normalize("ＭＩＳＯ"), "miso");
    assert_eq!(normalize("Phở"), "pho");
    assert_eq!(normalize("ネギ・しょうが"), "ネギ しょうが");
    // Thai keeps its vowel and tone marks; SARA AM folds to its compatibility
    // spelling on both sides of a match, so either way of typing it links.
    assert_eq!(normalize("น้ำปลา"), normalize("น\u{e49}\u{e4d}\u{e32}ปลา"));
    assert!(normalize("น้ำปลา").contains('\u{e49}'));
    assert_eq!(normalize("묵은지"), "묵은지");
    assert_eq!(normalize("İstanbul"), "istanbul");
}

proptest! {
    #[test]
    fn normalization_is_idempotent(text in "\\PC{0,24}") {
        let once = normalize(&text);
        prop_assert_eq!(normalize(&once), once);
    }
}
