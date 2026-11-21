//! The draft contract: what converts, what is refused, what survives a
//! revision, and that a converted draft seals into a verifying file.

use std::collections::BTreeMap;

use crisp_core::author::{open_draft, seal, validate_draft};
use crisp_core::model::ids::BlobId;
use crisp_core::model::primitives::{LanguageTag, Quantity, Rational, Timestamp, UnitCode};
use crisp_core::model::recipe::Nutrition;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::verify;

use super::*;

/// A draft that uses every field, written in the forms extraction emits.
pub(crate) fn sample() -> RecipeDraft {
    serde_json::from_value(serde_json::json!({
        "language": "en",
        "title": "Spaghetti alla carbonara",
        "summary": "A Roman pasta of egg, cheese, and cured pork.",
        "yield": { "amount": "4", "unit_text": "servings" },
        "times": { "prep": "10 min", "cook": "15 min", "total": "25 min" },
        "ingredient_groups": ["Pasta", "Sauce"],
        "ingredients": [
            { "ref": 1, "group": 0, "name": "spaghetti", "quantity": "400 g", "alt": ["14 oz"] },
            { "ref": 2, "group": 1, "name": "guanciale", "quantity": "150 g",
              "preparation": "cut into strips",
              "substitutes": [{ "name": "pancetta", "quantity": "150 g" }] },
            { "ref": 3, "group": 1, "name": "egg yolks", "quantity": "4" },
            { "ref": 4, "group": 1, "name": "Pecorino Romano", "quantity": "50 g", "note": "finely grated" },
            { "ref": 5, "group": 1, "name": "black pepper", "quantity": "to taste", "fixed": true, "optional": true }
        ],
        "equipment": [{ "ref": 1, "name": "skillet", "quantity": "1", "note": "30 cm" }],
        "step_groups": ["Cook"],
        "steps": [
            { "ref": 1, "group": 0, "text": "Render the guanciale over medium heat.", "ingredients": [{ "ref": 2 }],
              "equipment": [1], "heat": "medium", "produces": "rendered fat",
              "timers": [{ "duration": "8 min", "label": "render", "passive": true }],
              "note": "Start in a cold pan so the fat renders before the meat browns." },
            { "ref": 2, "text": "Boil the pasta in salted water.", "ingredients": [{ "ref": 1, "quantity": "400 g" }],
              "temperature": { "value": "100 °C", "target": "water" } },
            { "ref": 3, "text": "Off the heat, toss with yolks and cheese.\n\nLoosen with pasta water.",
              "ingredients": [{ "ref": 3 }, { "ref": 4 }, { "ref": 5 }] }
        ],
        "tags": ["weeknight"],
        "cuisine": ["italian", "roman"],
        "course": ["main"],
        "diet": ["nut-free"],
        "allergens": ["eggs", "milk", "gluten"],
        "best_months": [1, 2, 12],
        "notes": "No cream is used; the sauce is an emulsion of egg, fat, and starch.",
        "source": { "title": "Example Source", "author": "A. Cook", "url": "https://example.com/carbonara" },
        "attribution": "Researched from [one source](https://example.com/a) and [another](https://example.org/b).",
        "license": "CC-BY-4.0"
    }))
    .expect("the sample draft deserializes")
}

fn secret(byte: u8) -> SecretKeySet {
    SecretKeySet::from_seed(&[byte; 32])
}

#[test]
fn a_full_draft_converts_seals_verifies_and_extracts_back_unchanged() {
    let draft = sample();
    let payload = to_payload(&draft, None).expect("converts");
    assert_eq!(validate_draft(&payload), vec![], "no format issues");
    let bytes = seal(
        &payload,
        &secret(1),
        "Tester",
        Timestamp(1_700_000_000),
        None,
    )
    .expect("seals");
    assert!(verify(&bytes, &[], Timestamp(1_700_000_001)).compliant);
    let reopened = open_draft(&bytes).expect("opens");
    let extracted = from_payload(&reopened).expect("extracts");
    assert_eq!(extracted.draft, draft);
    assert_eq!(extracted.notices, Vec::<String>::new());
}

#[test]
fn a_minimal_draft_defaults_its_license_and_omits_empty_lists() {
    let draft: RecipeDraft = serde_json::from_value(serde_json::json!({
        "language": "fr", "title": "Œuf", "yield": { "amount": "1", "unit_text": "œuf" },
        "steps": [{ "text": "Cuire." }]
    }))
    .expect("deserializes");
    let payload = to_payload(&draft, None).expect("converts");
    assert_eq!(payload.meta.license, "NONE");
    assert_eq!(payload.default_language, LanguageTag("fr".into()));
    let recipe = &payload.recipes[0];
    assert!(recipe.ingredients.is_empty());
    assert!(recipe.tags.is_none() && recipe.equipment.is_none() && recipe.course.is_none());
    assert_eq!(recipe.steps[0].r#ref, 1);
    assert_eq!(validate_draft(&payload), vec![]);
}

#[test]
fn every_conversion_problem_is_reported_with_its_draft_pointer() {
    let mut draft = sample();
    draft.language = " ".into();
    draft.title = String::new();
    draft.yields.amount = String::new();
    draft.times = Some(TimesDraft {
        rest: Some("a while".into()),
        ..TimesDraft::default()
    });
    draft.ingredients[0].quantity = Some(String::new());
    draft.ingredients[0].alt = vec![String::new()];
    draft.ingredients[1].substitutes[0].quantity = Some(String::new());
    draft.equipment[0].quantity = Some(String::new());
    draft.steps[0].heat = Some("volcanic".into());
    draft.steps[0].timers[0].duration = "eventually".into();
    draft.steps[0].note = Some("**bold**".into());
    draft.steps[1].temperature = Some(TemperatureDraft {
        value: "100".into(),
        target: None,
    });
    draft.steps[1].ingredients[0].quantity = Some(String::new());
    draft.steps[2].text = "[x](http://insecure)".into();
    draft.summary = Some(" ".into());
    draft.notes = Some("`code`".into());
    draft.attribution = Some(String::new());
    let paths: Vec<String> = to_payload(&draft, None)
        .expect_err("refused")
        .into_iter()
        .map(|i| i.path)
        .collect();
    for expected in [
        "/language",
        "/title",
        "/yield/amount",
        "/times/rest",
        "/ingredients/0/quantity",
        "/ingredients/0/alt/0",
        "/ingredients/1/substitutes/0/quantity",
        "/equipment/0/quantity",
        "/steps/0/heat",
        "/steps/0/timers/0/duration",
        "/steps/0/note",
        "/steps/1/temperature",
        "/steps/1/ingredients/0/quantity",
        "/steps/2/text",
        "/summary",
        "/notes",
        "/attribution",
    ] {
        assert!(
            paths.iter().any(|p| p == expected),
            "{expected} missing from {paths:?}"
        );
    }
    let mut stepless = sample();
    stepless.steps.clear();
    let issues = to_payload(&stepless, None).expect_err("refused");
    assert_eq!(issues[0].path, "/steps");
}

#[test]
fn a_revision_keeps_identity_and_everything_the_draft_cannot_express() {
    let author = secret(2);
    let mut first = to_payload(&sample(), None).expect("converts");
    let photo = BlobId("blb1photo".into());
    let recipe = &mut first.recipes[0];
    recipe.media = Some(vec![photo.clone()]);
    recipe.nutrition = Some(Nutrition {
        basis: Quantity {
            min: Some(Rational::new(1, 1)),
            ..Quantity::default()
        },
        values: BTreeMap::new(),
    });
    recipe.ingredients[1].blob = Some(photo.clone());
    recipe.steps[0].blob = Some(photo.clone());
    recipe.equipment.as_mut().expect("equipment")[0].blob = Some(photo.clone());
    // Seal without the fake blob references, then put them back on the base.
    let mut sealable = first.clone();
    sealable.recipes[0].media = None;
    sealable.recipes[0].ingredients[1].blob = None;
    sealable.recipes[0].steps[0].blob = None;
    sealable.recipes[0].equipment.as_mut().expect("equipment")[0].blob = None;
    let bytes = seal(&sealable, &author, "Tester", Timestamp(1_700_000_000), None).expect("seals");
    let mut base = open_draft(&bytes).expect("opens");
    base.recipes[0].media = first.recipes[0].media.clone();
    base.recipes[0].ingredients[1].blob = Some(photo.clone());
    base.recipes[0].steps[0].blob = Some(photo.clone());
    base.recipes[0].equipment.as_mut().expect("equipment")[0].blob = Some(photo.clone());

    let mut edited = sample();
    edited.title = "Carbonara, revised".into();
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    let recipe = &revised.recipes[0];
    assert_eq!(recipe.identity, base.recipes[0].identity);
    assert_eq!(recipe.id, base.recipes[0].id);
    assert_eq!(recipe.media, Some(vec![photo.clone()]));
    assert!(recipe.nutrition.is_some());
    assert_eq!(recipe.ingredients[1].blob, Some(photo.clone()));
    assert_eq!(recipe.ingredients[0].blob, None);
    assert_eq!(recipe.steps[0].blob, Some(photo.clone()));
    assert_eq!(
        recipe.equipment.as_ref().expect("equipment")[0].blob,
        Some(photo)
    );
    assert_eq!(
        revised.meta.title.0.values().next().map(String::as_str),
        Some("Carbonara, revised")
    );

    let notices = from_payload(&base).expect("extracts").notices;
    assert!(
        notices
            .iter()
            .any(|n| n.contains("photo(s)") && n.contains("nutrition")),
        "{notices:?}"
    );
    assert!(
        notices.iter().any(|n| n.contains("per-item photos")),
        "{notices:?}"
    );
}

#[test]
fn revising_without_a_license_keeps_the_base_license_and_filters_description() {
    let mut base = to_payload(&sample(), None).expect("converts");
    let fr = LanguageTag("fr".into());
    base.meta.description = Some(crisp_core::model::text::LocalizedProse(BTreeMap::from([(
        fr.clone(),
        crisp_core::model::text::Prose(vec![]),
    )])));
    let mut edited = sample();
    edited.license = None;
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    assert_eq!(revised.meta.license, "CC-BY-4.0");
    assert!(revised.meta.description.is_none());

    base.meta.description = Some(crisp_core::model::text::LocalizedProse(BTreeMap::from([
        (fr, crisp_core::model::text::Prose(vec![])),
        (
            LanguageTag("en".into()),
            crisp_core::model::text::Prose(vec![]),
        ),
    ])));
    let kept = to_payload(&edited, Some(&base)).expect("converts");
    assert_eq!(kept.meta.description.expect("kept").0.len(), 1);
}

#[test]
fn books_and_multi_recipe_bundles_are_refused() {
    let mut book = to_payload(&sample(), None).expect("converts");
    book.kind = crisp_core::model::payload::Kind::Book;
    assert!(to_payload(&sample(), Some(&book)).is_err());
    assert!(from_payload(&book).is_err());
    let mut two = to_payload(&sample(), None).expect("converts");
    two.recipes.push(two.recipes[0].clone());
    assert!(to_payload(&sample(), Some(&two)).is_err());
    assert!(from_payload(&two).is_err());
}

#[test]
fn extraction_notices_other_languages_formatting_and_lost_extensions() {
    let mut payload = to_payload(&sample(), None).expect("converts");
    payload.languages.push(LanguageTag("it".into()));
    let recipe = &mut payload.recipes[0];
    recipe
        .notes
        .as_mut()
        .expect("notes")
        .0
        .values_mut()
        .for_each(|p| {
            p.0[0].0[0].style = Some(vec![crisp_core::model::text::StyleFlag::Italic]);
        });
    recipe.steps[0].timers.as_mut().expect("timers")[0].ext = Some(BTreeMap::new());
    recipe.ext = Some(BTreeMap::new());
    payload.meta.cover = Some(BlobId("blb1cover".into()));
    let notices = from_payload(&payload).expect("extracts").notices;
    assert!(
        notices.iter().any(|n| n.contains("text in it")),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|n| n.contains("Formatting") && n.contains("notes")),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|n| n.contains("extension data") && n.contains("cover")),
        "{notices:?}"
    );
    assert!(
        notices.iter().any(|n| n.contains("substitutes or timers")),
        "{notices:?}"
    );
}

#[test]
fn extraction_falls_back_to_any_language_present() {
    let mut payload = to_payload(&sample(), None).expect("converts");
    payload.default_language = LanguageTag("de".into());
    let extracted = from_payload(&payload).expect("extracts");
    assert_eq!(extracted.draft.title, "Spaghetti alla carbonara");
    assert!(extracted.draft.notes.is_some());
    assert_eq!(
        payload.recipes[0].ingredients[0]
            .quantity
            .as_ref()
            .and_then(|q| q.unit.clone()),
        Some(UnitCode("g".into()))
    );
}

#[test]
fn the_draft_schema_names_its_fields_as_the_format_does() {
    let schema = serde_json::to_value(schemars::schema_for!(RecipeDraft)).expect("schema");
    let properties = schema["properties"].as_object().expect("properties");
    for field in [
        "language",
        "title",
        "yield",
        "ingredients",
        "steps",
        "attribution",
    ] {
        assert!(properties.contains_key(field), "{field}");
    }
    assert!(!properties.contains_key("yields"));
}

mod revisions;
