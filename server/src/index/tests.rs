//! Unit tests for index extraction, and the small ingredient-index fixture
//! the search tests share.

use std::collections::BTreeMap;

use crisp_core::ingredients::{Dataset, Entry, LocaleTable, Names};

use super::*;

fn entry(slug: &str, parent: Option<&str>, replaced_by: Option<&str>) -> Entry {
    Entry {
        slug: slug.to_owned(),
        parent: parent.map(str::to_owned),
        category: "fermented".to_owned(),
        density: None,
        allergens: Vec::new(),
        diet_conflicts: Vec::new(),
        replaced_by: replaced_by.map(str::to_owned),
        source: format!("off:en:{slug}"),
    }
}

fn names(name: &str, aliases: &[&str]) -> Names {
    Names {
        name: name.to_owned(),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
    }
}

/// A small index: `kimchi` ⊃ `kimchi-aged`, and `kimchee` merged into
/// `kimchi`; names in `en-US` and `ko-KR`.
pub(crate) fn fixture() -> IngredientIndex {
    let shard = |pairs: [(&str, Names); 2]| {
        pairs
            .into_iter()
            .map(|(s, n)| (s.to_owned(), n))
            .collect::<BTreeMap<_, _>>()
    };
    let list = |tags: &[&str]| tags.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
    IngredientIndex::new(Dataset {
        version: "fixture".to_owned(),
        locales: LocaleTable {
            shards: list(&["en-US", "ko-KR"]),
            fallback: BTreeMap::from([
                ("en".to_owned(), list(&["en-US"])),
                ("ko".to_owned(), list(&["ko-KR"])),
            ]),
            last_resort: list(&["en-US"]),
        },
        entries: vec![
            entry("kimchee", None, Some("kimchi")),
            entry("kimchi", None, None),
            entry("kimchi-aged", Some("kimchi"), None),
        ],
        names: BTreeMap::from([
            (
                "en-US".to_owned(),
                shard([
                    ("kimchi", names("Kimchi", &["kimchee"])),
                    ("kimchi-aged", names("Aged kimchi", &["sour kimchi"])),
                ]),
            ),
            (
                "ko-KR".to_owned(),
                shard([
                    ("kimchi", names("김치", &[])),
                    ("kimchi-aged", names("묵은지", &[])),
                ]),
            ),
        ]),
    })
    .expect("the fixture index is valid")
}

fn tag(t: &str) -> LanguageTag {
    LanguageTag(t.to_owned())
}

#[test]
fn languages_put_the_default_first_and_count_each_tag_once() {
    let declared = [tag("ko"), tag("EN"), tag("ko"), tag("ja")];
    assert_eq!(languages(&tag("en"), &declared), vec!["en", "ko", "ja"]);
    assert_eq!(languages(&tag("en"), &[]), vec!["en"]);
}

#[test]
fn index_names_cover_every_language_with_aliases_once() {
    let index = fixture();
    let codes = vec!["kimchi-aged".to_owned()];
    assert_eq!(
        index_names(&index, &codes, &["ko", "en"]),
        vec!["묵은지", "Aged kimchi", "sour kimchi"]
    );
    // A language with no shard of its own falls back (here to English),
    // and a name already given is not repeated.
    assert_eq!(
        index_names(&index, &codes, &["en", "fr"]),
        vec!["Aged kimchi", "sour kimchi"]
    );
}

#[test]
fn index_names_follow_merges_and_skip_unknown_codes() {
    let index = fixture();
    let codes = vec!["kimchee".to_owned(), "not-in-the-index".to_owned()];
    assert_eq!(index_names(&index, &codes, &["ko"]), vec!["김치"]);
    assert!(index_names(&index, &[], &["en"]).is_empty());
}

#[test]
fn codes_serialize_classification_enums_to_wire_strings() {
    use crisp_core::model::recipe::{AllergenCode, DietCode};
    assert_eq!(
        codes(Some(&[
            OpenCode::Known(DietCode::GlutenFree),
            OpenCode::Known(DietCode::Vegan),
        ])),
        vec!["gluten-free", "vegan"]
    );
    assert_eq!(
        codes(Some(&[OpenCode::Known(AllergenCode::TreeNuts)])),
        vec!["tree-nuts"]
    );
    assert!(codes::<DietCode>(None).is_empty());
}

#[test]
fn codes_carry_unknown_codes_verbatim() {
    use crisp_core::model::recipe::DietCode;
    assert_eq!(
        codes(Some(&[
            OpenCode::<DietCode>::Other("low-fodmap".to_owned()),
            OpenCode::Known(DietCode::Vegan),
        ])),
        vec!["low-fodmap", "vegan"]
    );
}

fn paragraph(runs: &[&str]) -> crisp_core::model::text::Paragraph {
    crisp_core::model::text::Paragraph(
        runs.iter()
            .map(|t| crisp_core::model::text::Run {
                t: (*t).to_owned(),
                style: None,
                link: None,
                reference: None,
            })
            .collect(),
    )
}

#[test]
fn prose_text_joins_paragraphs_with_a_single_space_and_trims() {
    let en = LanguageTag("en".to_owned());
    let prose = Prose(vec![
        paragraph(&["Rich ", "and dark."]),
        paragraph(&[]),
        paragraph(&["Serves four. "]),
    ]);
    let localized = LocalizedProse([(en.clone(), prose)].into_iter().collect());
    assert_eq!(prose_text(&localized, &en), "Rich and dark.  Serves four.");
}

#[test]
fn prose_text_is_empty_when_no_language_resolves() {
    let en = LanguageTag("en".to_owned());
    let localized = LocalizedProse(BTreeMap::new());
    assert_eq!(prose_text(&localized, &en), "");
}
