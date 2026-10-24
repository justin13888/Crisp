//! Tests for [`recipe_view`]: grouping, scaling, conversion, step joins,
//! timers, and language fallback.

use super::quantity::Units;
use super::view::{RecipeView, ViewOptions, recipe_view};
use crate::fixtures::{rich_draft, sample_payload};
use crate::model::payload::BundlePayload;
use crate::model::primitives::{LanguageTag, Rational};

fn view(payload: &BundlePayload, options: &ViewOptions) -> RecipeView {
    recipe_view(payload, 0, options).expect("view")
}

fn tag(code: &str) -> LanguageTag {
    LanguageTag(code.to_owned())
}

/// Each section's title and the names of its items.
fn sections(view: &RecipeView) -> Vec<(Option<&str>, Vec<&str>)> {
    view.ingredients
        .iter()
        .map(|s| {
            let names = s.items.iter().map(|i| i.name.as_str()).collect();
            (s.title.as_deref(), names)
        })
        .collect()
}

#[test]
fn an_out_of_range_index_is_an_input_error() {
    let err =
        recipe_view(&sample_payload(), 1, &ViewOptions::default()).expect_err("only one recipe");
    assert!(
        matches!(&err, crate::CrispError::Input(m) if m == "no recipe at index 1"),
        "{err:?}"
    );
}

#[test]
fn ingredients_are_grouped_into_runs_in_recipe_order() {
    let v = view(&rich_draft(), &ViewOptions::default());
    assert_eq!(
        sections(&v),
        [
            (
                Some("Dough"),
                vec![
                    "all-purpose flour",
                    "unsalted butter",
                    "light brown sugar",
                    "granulated sugar",
                    "flaky salt",
                    "eggs"
                ]
            ),
            (Some("Mix-ins"), vec!["dark chocolate", "toasted walnuts"]),
        ]
    );
}

#[test]
fn a_group_that_recurs_starts_a_new_section_and_unknown_groups_are_untitled() {
    let mut payload = rich_draft();
    let ingredients = &mut payload.recipes[0].ingredients;
    ingredients.truncate(4);
    ingredients[0].group = Some(1);
    ingredients[1].group = Some(0);
    ingredients[2].group = Some(1);
    ingredients[3].group = Some(9);
    let v = view(&payload, &ViewOptions::default());
    assert_eq!(
        sections(&v),
        [
            (Some("Mix-ins"), vec!["all-purpose flour"]),
            (Some("Dough"), vec!["unsalted butter"]),
            (Some("Mix-ins"), vec!["light brown sugar"]),
            (None, vec!["granulated sugar"]),
        ]
    );
}

#[test]
fn the_unscaled_view_formats_amounts_as_written() {
    let v = view(&rich_draft(), &ViewOptions::default());
    let flour = &v.ingredients[0].items[0];
    assert_eq!(flour.amount, "2¼ cups");
    assert_eq!(flour.alt, ["280 g"]);
    assert_eq!(v.yield_text, "24 cookies");
    assert_eq!(v.yield_amount, Some(Rational::new(24, 1)));
}

#[test]
fn scaling_reaches_amounts_and_alternatives_but_conversion_spares_alternatives() {
    let options = ViewOptions {
        scale: Rational::new(2, 1),
        units: Units::Metric,
        ..ViewOptions::default()
    };
    let v = view(&rich_draft(), &options);
    let dough = &v.ingredients[0].items;
    // Scaled and converted.
    assert_eq!(dough[0].amount, "~1.1 L");
    // Scaled, but kept in the unit the author wrote.
    assert_eq!(dough[0].alt, ["560 g"]);
    // Fixed: converted, never scaled.
    assert!(dough[4].fixed);
    assert_eq!(dough[4].amount, "~2.5 mL");
    assert_eq!(dough[5].amount, "4");
    // Equipment counts never scale.
    assert_eq!(v.equipment[0].amount, "2");
    assert_eq!(v.yield_text, "48 cookies");
    assert_eq!(v.yield_amount, Some(Rational::new(48, 1)));
}

#[test]
fn a_step_shows_its_portion_or_else_the_whole_amount() {
    let v = view(&rich_draft(), &ViewOptions::default());
    let portion = &v.steps[0].ingredients;
    assert_eq!(portion.len(), 1);
    assert_eq!(
        (portion[0].name.as_str(), portion[0].amount.as_str()),
        ("unsalted butter", "½ cup")
    );
    assert_eq!(portion[0].preparation.as_deref(), Some("softened"));
    let whole: Vec<(&str, &str)> = v.steps[1]
        .ingredients
        .iter()
        .map(|i| (i.name.as_str(), i.amount.as_str()))
        .collect();
    assert_eq!(
        whole,
        [
            ("unsalted butter", "1 cup"),
            ("light brown sugar", "¾ cup"),
            ("granulated sugar", "½ cup"),
        ]
    );
    assert_eq!(v.steps[0].equipment, ["light-colored saucepan"]);
    assert_eq!(v.steps[4].equipment, ["baking sheet"]);
}

#[test]
fn a_step_portion_scales_with_the_recipe() {
    let options = ViewOptions {
        scale: Rational::new(2, 1),
        ..ViewOptions::default()
    };
    let v = view(&rich_draft(), &options);
    assert_eq!(v.steps[0].ingredients[0].amount, "1 cup");
}

#[test]
fn unknown_step_references_are_dropped() {
    let mut payload = rich_draft();
    let step = &mut payload.recipes[0].steps[0];
    step.ingredients.as_mut().expect("uses")[0].r#ref = 99;
    step.equipment = Some(vec![99]);
    let v = view(&payload, &ViewOptions::default());
    assert!(v.steps[0].ingredients.is_empty());
    assert!(v.steps[0].equipment.is_empty());
}

#[test]
fn timers_are_totalled_by_kind_and_labelled_by_step_when_unnamed() {
    let mut payload = rich_draft();
    payload.recipes[0].steps[1].timers.as_mut().expect("timers")[0].label = None;
    let v = view(&payload, &ViewOptions::default());
    let timers: Vec<(&str, u64, Option<u64>, &str, bool)> = v
        .steps
        .iter()
        .flat_map(|s| &s.timers)
        .map(|t| {
            (
                t.label.as_str(),
                t.seconds,
                t.max_seconds,
                t.text.as_str(),
                t.passive,
            )
        })
        .collect();
    assert_eq!(
        timers,
        [
            ("brown butter", 300, Some(420), "5–7 min", false),
            ("Step 2", 180, None, "3 min", false),
            ("chill", 1800, None, "30 min", true),
            ("bake", 600, Some(720), "10–12 min", true),
        ]
    );
    // Ranges count their lower bound.
    assert_eq!(v.active_timer_seconds, 300 + 180);
    assert_eq!(v.passive_timer_seconds, 1800 + 600);
    assert_eq!(v.active_timer_text, "8 min");
    assert_eq!(v.passive_timer_text, "40 min");
}

#[test]
fn steps_are_numbered_sectioned_and_annotated() {
    let v = view(&rich_draft(), &ViewOptions::default());
    let numbered: Vec<(u64, usize, Option<&str>)> = v
        .steps
        .iter()
        .map(|s| (s.r#ref, s.number, s.section.as_deref()))
        .collect();
    assert_eq!(
        numbered,
        [
            (0, 1, Some("Dough")),
            (1, 2, Some("Dough")),
            (2, 3, Some("Dough")),
            (3, 4, Some("Rest")),
            (4, 5, Some("Bake")),
        ]
    );
    assert_eq!(v.steps[0].heat.as_deref(), Some("Medium heat"));
    assert_eq!(v.steps[4].temperature.as_deref(), Some("180°C oven"));
    assert_eq!(v.steps[3].produces.as_deref(), Some("cookie dough"));
    let times: Vec<(&str, &str)> = v
        .times
        .iter()
        .map(|t| (t.label.as_str(), t.value.as_str()))
        .collect();
    assert_eq!(
        times,
        [
            ("Prep", "20 min"),
            ("Cook", "12 min"),
            ("Rest", "30 min"),
            ("Total", "1 hr 5 min"),
        ]
    );
    assert_eq!(
        v.tags,
        ["dessert", "baked", "american", "cookies", "make-ahead"]
    );
}

#[test]
fn a_preferred_declared_language_wins_and_missing_text_falls_back_to_the_default() {
    let mut payload = sample_payload();
    payload.languages.push(tag("fr"));
    let recipe = &mut payload.recipes[0];
    recipe.title.0.insert(tag("fr"), "Biscuits".to_owned());
    let options = ViewOptions {
        languages: vec![tag("fr")],
        ..ViewOptions::default()
    };
    let v = view(&payload, &options);
    assert_eq!(v.language, "fr");
    assert_eq!(v.title, "Biscuits");
    // The ingredient has no French name: the default language fills in.
    assert_eq!(v.ingredients[0].items[0].name, "Flour");
    assert_eq!(v.yield_unit, "cookies");
}

#[test]
fn an_undeclared_preferred_language_resolves_to_the_default() {
    let mut payload = sample_payload();
    payload.recipes[0]
        .title
        .0
        .insert(tag("de"), "Kekse".to_owned());
    let options = ViewOptions {
        languages: vec![tag("de")],
        ..ViewOptions::default()
    };
    let v = view(&payload, &options);
    assert_eq!(v.language, "en");
    // The undeclared language still ranks after the view language.
    assert_eq!(v.title, "Cookies");
}

/// Every `null` in a JSON value, by JSON Pointer.
fn nulls(value: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Null => out.push(path.to_owned()),
        serde_json::Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                nulls(item, &format!("{path}/{i}"), out);
            }
        }
        serde_json::Value::Object(fields) => {
            for (key, item) in fields {
                nulls(item, &format!("{path}/{key}"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn absent_optional_fields_are_omitted_not_null() {
    for payload in [sample_payload(), rich_draft()] {
        let json = serde_json::to_value(view(&payload, &ViewOptions::default())).expect("json");
        let mut found = Vec::new();
        nulls(&json, "", &mut found);
        assert_eq!(found, Vec::<String>::new());
    }
}
