//! End-to-end tests of the pipeline over a tiny taxonomy and rules file.

use std::collections::BTreeMap;

use crisp_core::ingredients::{Entry, Names};

use super::*;
use crate::english::Term;
use crate::names::NameOverride;
use crate::off::parse;

const TAXONOMY: &str = "\
en: vegetable

< en: vegetable
en: aubergine
ko: 가지
usda_fdc_code:en: 11

< en: vegetable
en: eggplant
fr: aubergine (variété)
vegan:en: no
density_g_per_ml:en: 0.5
usda_fdc_code:en: 12

< en: vegetable
en: cabbage

< en: cabbage
en: kimchi
ko: 김치
allergens:en: en:fish

< en: vegetable
en: vegetable flavouring
";

const RULES: &str = r#"
[sources.off]
url = "https://example.com/off.txt"
file = "off.txt"
sha256 = "0"
[sources.fdc]
url = "https://example.com/fdc.zip"
file = "fdc.zip"
sha256 = "0"
[sources.opencc]

[locales]
shards = ["en-US", "en-GB", "en-CA", "ko-KR", "fr-FR", "fr-CA"]
last_resort = ["en-US"]
english_lexicon = "en-regional.toml"
english = { us = "en-US", gb = "en-GB", ca = "en-CA" }
[locales.off_language]
en = "en-US"
ko = "ko-KR"
fr = "fr-FR"
[locales.fallback]
"en" = ["en-US", "en-GB"]

[[locales.derive]]
shard = "fr-CA"
from = "fr-FR"
stages = []

[select]
include = [{ id = "en:vegetable", category = "produce" }]
[[select.exclude]]
name = "flavourings"
pattern = "(^|-)flavou?ring(-|$)"

[hints.allergens]
"en:fish" = "fish"
[hints.diet_when_no]
vegan = "vegan"
[hints.diet_from_allergen]
fish = ["vegan"]

[density]
accept = ["off", "linked", "exact"]
denominator = 100
[density.volume_units]
cup = 240.0
"#;

fn entry(slug: &str, source: &str) -> Entry {
    Entry {
        slug: slug.to_owned(),
        parent: None,
        category: "produce".to_owned(),
        density: None,
        allergens: Vec::new(),
        diet_conflicts: Vec::new(),
        replaced_by: None,
        source: source.to_owned(),
    }
}

struct Fixture {
    rules: String,
    taxonomy: Taxonomy,
    foods: Vec<Food>,
    lexicon: Lexicon,
    converters: BTreeMap<String, Converter>,
    added: Vec<Entry>,
    name_overrides: BTreeMap<String, BTreeMap<String, NameOverride>>,
    previous: Vec<Entry>,
    previous_names: ShardNames,
    merges: BTreeMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let mut aged = entry("kimchi-aged", "crisp:override");
        aged.parent = Some("kimchi".to_owned());
        Self {
            rules: RULES.to_owned(),
            taxonomy: parse(TAXONOMY),
            foods: vec![
                Food {
                    fdc_id: "11".to_owned(),
                    description: "Eggplant, raw".to_owned(),
                    density: 0.345,
                },
                Food {
                    fdc_id: "12".to_owned(),
                    description: "Eggplant, linked".to_owned(),
                    density: 0.9,
                },
                Food {
                    fdc_id: "13".to_owned(),
                    description: "Aubergine".to_owned(),
                    density: 0.8,
                },
                Food {
                    fdc_id: "14".to_owned(),
                    description: "Cabbage".to_owned(),
                    density: 0.4,
                },
                Food {
                    fdc_id: "15".to_owned(),
                    description: "Cabbages, raw".to_owned(),
                    density: 0.6,
                },
            ],
            lexicon: Lexicon::new(vec![Term {
                us: "eggplant".to_owned(),
                gb: "aubergine".to_owned(),
                ca: "eggplant".to_owned(),
            }])
            .expect("lexicon"),
            converters: BTreeMap::from([("fr-CA".to_owned(), Converter::default())]),
            added: vec![aged],
            name_overrides: BTreeMap::from([
                (
                    "ko-KR".to_owned(),
                    BTreeMap::from([(
                        "kimchi-aged".to_owned(),
                        NameOverride {
                            name: Some("묵은지".to_owned()),
                            aliases: Vec::new(),
                        },
                    )]),
                ),
                (
                    "fr-CA".to_owned(),
                    BTreeMap::from([(
                        "eggplant".to_owned(),
                        NameOverride {
                            name: Some("aubergine du Québec".to_owned()),
                            aliases: Vec::new(),
                        },
                    )]),
                ),
            ]),
            previous: Vec::new(),
            previous_names: ShardNames::new(),
            merges: BTreeMap::new(),
        }
    }

    fn run(&self) -> Result<Assembled> {
        let rules: Rules = toml::from_str(&self.rules).expect("rules");
        assemble(
            &rules,
            &Inputs {
                taxonomy: &self.taxonomy,
                foods: &self.foods,
                lexicon: &self.lexicon,
                converters: &self.converters,
                added: &self.added,
                name_overrides: &self.name_overrides,
                previous: &self.previous,
                previous_names: &self.previous_names,
                merges: &self.merges,
            },
        )
    }
}

fn find<'a>(a: &'a Assembled, slug: &str) -> &'a Entry {
    a.dataset
        .entries
        .iter()
        .find(|e| e.slug == slug)
        .unwrap_or_else(|| panic!("{slug} missing"))
}

#[test]
fn the_pipeline_selects_names_hints_and_densities() {
    let a = Fixture::new().run().expect("assemble");
    let slugs: Vec<&str> = a.dataset.entries.iter().map(|e| e.slug.as_str()).collect();
    assert_eq!(
        slugs,
        [
            "aubergine",
            "cabbage",
            "eggplant",
            "kimchi",
            "kimchi-aged",
            "vegetable"
        ]
    );
    assert_eq!(find(&a, "kimchi").parent.as_deref(), Some("cabbage"));
    assert_eq!(find(&a, "kimchi").allergens.len(), 1);
    assert_eq!(find(&a, "kimchi").diet_conflicts.len(), 1);
    assert_eq!(find(&a, "aubergine").density, Some(Rational::new(7, 20)));
    assert_eq!(a.decisions.excluded["pattern: flavourings"], 1);
    let names = &a.dataset.names;
    assert_eq!(names["ko-KR"]["kimchi-aged"].name, "묵은지");
    // `eggplant` owns "eggplant" in the source, so `aubergine` is not renamed to it.
    assert_eq!(names["en-US"]["aubergine"].name, "aubergine");
    assert_eq!(names["en-US"]["eggplant"].name, "eggplant");
    assert!(!names.contains_key("en-GB") || !names["en-GB"].contains_key("aubergine"));
}

#[test]
fn hints_densities_and_derived_shard_overrides_follow_their_rules() {
    let a = Fixture::new().run().expect("assemble");
    let eggplant = find(&a, "eggplant");
    assert_eq!(eggplant.diet_conflicts.len(), 1, "vegan: no");
    assert!(find(&a, "aubergine").diet_conflicts.is_empty());
    // The taxonomy's own density outranks its linked USDA food (0.9).
    assert_eq!(eggplant.density, Some(Rational::new(1, 2)));
    // `aubergine` has a linked food (0.345) and an exact one (0.8): linked wins.
    assert_eq!(find(&a, "aubergine").density, Some(Rational::new(7, 20)));
    let names = &a.dataset.names;
    // An override on a derived shard is applied after derivation.
    assert_eq!(names["fr-CA"]["eggplant"].name, "aubergine du Québec");
    assert_eq!(names["fr-FR"]["eggplant"].name, "aubergine (variété)");
}

#[test]
fn equally_ranked_densities_pool_and_unaccepted_kinds_are_ignored() {
    let a = Fixture::new().run().expect("assemble");
    // Two exact matches (0.4, 0.6) pool to their median.
    assert_eq!(find(&a, "cabbage").density, Some(Rational::new(1, 2)));
    let mut f = Fixture::new();
    f.rules = RULES.replace(
        r#"accept = ["off", "linked", "exact"]"#,
        r#"accept = ["linked", "exact"]"#,
    );
    let a = f.run().expect("assemble");
    // Without `off`, the taxonomy's own 0.5 yields to the linked food (0.9).
    assert_eq!(find(&a, "eggplant").density, Some(Rational::new(9, 10)));
}

#[test]
fn published_slugs_survive_and_retained_entries_keep_their_names() {
    let mut f = Fixture::new();
    let mut renamed = entry("brinjal", "off:en:aubergine");
    renamed.parent = Some("vegetable".to_owned());
    let gone = entry("purple-thing", "off:en:purple-thing");
    let old_override = entry("old-override", "crisp:override");
    f.previous = vec![renamed, gone, old_override];
    f.previous_names = ShardNames::from([(
        "en-US".to_owned(),
        BTreeMap::from([(
            "purple-thing".to_owned(),
            Names {
                name: "purple thing".to_owned(),
                aliases: Vec::new(),
            },
        )]),
    )]);
    let a = f.run().expect("assemble");
    assert_eq!(find(&a, "brinjal").source, "off:en:aubergine");
    assert!(a.dataset.entries.iter().all(|e| e.slug != "aubergine"));
    assert_eq!(a.retained, ["purple-thing", "old-override"]);
    assert_eq!(
        a.dataset.names["en-US"]["purple-thing"].name,
        "purple thing"
    );
}

#[test]
fn recorded_merges_apply_and_must_name_entries() {
    let mut f = Fixture::new();
    f.merges = BTreeMap::from([("aubergine".to_owned(), "eggplant".to_owned())]);
    let a = f.run().expect("assemble");
    assert_eq!(
        find(&a, "aubergine").replaced_by.as_deref(),
        Some("eggplant")
    );
    f.merges = BTreeMap::from([("aubergine".to_owned(), "nowhere".to_owned())]);
    assert!(f.run().is_err());
    f.merges = BTreeMap::from([("nowhere".to_owned(), "eggplant".to_owned())]);
    assert!(f.run().is_err());
}

#[test]
fn an_override_may_not_take_a_published_imported_slug() {
    let mut f = Fixture::new();
    f.added.push(entry("cabbage", "crisp:override"));
    let fresh = f
        .run()
        .expect("an unpublished slug is reserved for the override");
    assert_eq!(find(&fresh, "cabbage").source, "crisp:override");
    assert_eq!(find(&fresh, "cabbage-2").source, "off:en:cabbage");
    f.previous = vec![entry("cabbage", "off:en:cabbage")];
    assert!(f.run().is_err());
}

#[test]
fn assembling_twice_gives_the_same_dataset() {
    let f = Fixture::new();
    let first = f.run().expect("assemble");
    let second = f.run().expect("assemble");
    assert_eq!(first.dataset, second.dataset);
}

#[test]
fn densities_round_to_reduced_rationals() {
    assert_eq!(rational(1.03, 100), Some(Rational::new(103, 100)));
    assert_eq!(rational(0.5, 100), Some(Rational::new(1, 2)));
    assert_eq!(rational(0.0, 100), None);
}
