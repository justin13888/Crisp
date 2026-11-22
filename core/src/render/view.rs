//! A render-ready view of one recipe.
//!
//! [`recipe_view`] resolves every localized value, applies scaling and unit
//! conversion, formats every amount, time, and temperature, and joins each
//! step to the ingredients and equipment it uses. A UI only lays the result
//! out; it never touches rationals, units, or locale fallback itself.

mod types;

pub use types::*;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::ingredients::density_measure;
use crate::model::payload::BundlePayload;
use crate::model::primitives::{LanguageTag, Rational};
use crate::model::recipe::{HeatLevel, Ingredient, Recipe, TemperatureTarget};
use crate::render::convert::convert_temperature;
use crate::render::quantity::{
    Units, duration_seconds, format_duration_quantity, format_quantity, format_seconds,
    format_temperature,
};
use crate::render::resolver::Resolver;
use crate::render::scale::scale_recipe;

/// How a reader wants a recipe shown.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewOptions {
    /// Preferred languages, best first.
    #[serde(default)]
    pub languages: Vec<LanguageTag>,
    /// The scale factor applied to every scalable amount.
    #[cfg_attr(feature = "ts", tsify(type = "Rational"))]
    #[serde(default = "one")]
    pub scale: Rational,
    /// The measurement system amounts are shown in.
    #[serde(default)]
    pub units: Units,
}

fn one() -> Rational {
    Rational::new(1, 1)
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            languages: Vec::new(),
            scale: one(),
            units: Units::Original,
        }
    }
}

fn heat_label(heat: HeatLevel) -> &'static str {
    match heat {
        HeatLevel::Low => "Low heat",
        HeatLevel::MediumLow => "Medium-low heat",
        HeatLevel::Medium => "Medium heat",
        HeatLevel::MediumHigh => "Medium-high heat",
        HeatLevel::High => "High heat",
    }
}

fn target_label(target: TemperatureTarget) -> &'static str {
    match target {
        TemperatureTarget::Oven => "oven",
        TemperatureTarget::Internal => "internal",
        TemperatureTarget::Oil => "oil",
        TemperatureTarget::Pan => "pan",
        TemperatureTarget::Water => "water",
    }
}

/// The density-derived measure of an (already scaled) ingredient, as
/// `"≈ 140 g"`. `None` whenever the author gave any `alt` of their own: an
/// author's measure is the authority and is never joined by a derived one.
fn derived_measure(ing: &Ingredient, units: Units, lang: &LanguageTag) -> Option<String> {
    if ing.alt.as_ref().is_some_and(|alt| !alt.is_empty()) {
        return None;
    }
    let code = ing.code.as_deref()?;
    let mut derived = density_measure(ing.quantity.as_ref()?, code, units)?;
    // `≈` already says the measure is approximate; a `~` beside it would
    // say it twice.
    derived.approx = None;
    let text = format_quantity(&derived, lang);
    Some(format!("≈ {}", text.trim_start_matches('~')))
}

/// Builds the render-ready view of `payload.recipes[index]`.
///
/// # Errors
///
/// Returns [`crate::CrispError::Input`] if `index` is out of range or
/// scaling overflows.
pub fn recipe_view(
    payload: &BundlePayload,
    index: usize,
    options: &ViewOptions,
) -> Result<RecipeView> {
    let original = payload
        .recipes
        .get(index)
        .ok_or_else(|| crate::CrispError::input(format!("no recipe at index {index}")))?;
    let recipe: Recipe = scale_recipe(original, options.scale)?;
    let r = Resolver::new(payload, &options.languages);
    let units = options.units;

    let ingredient_names: BTreeMap<u64, (String, String, Option<String>)> = recipe
        .ingredients
        .iter()
        .map(|i| {
            (
                i.r#ref,
                (
                    r.text(&i.name),
                    r.amount(i.quantity.as_ref(), units),
                    r.opt(i.preparation.as_ref()),
                ),
            )
        })
        .collect();
    let equipment_names: BTreeMap<u64, String> = recipe
        .equipment
        .iter()
        .flatten()
        .map(|e| (e.r#ref, r.text(&e.name)))
        .collect();

    // Ingredients, one section per consecutive run of the same group.
    let group_titles: Vec<String> = recipe
        .ingredient_groups
        .iter()
        .flatten()
        .map(|g| r.text(g))
        .collect();
    let mut sections: Vec<IngredientSection> = Vec::new();
    for ing in &recipe.ingredients {
        let title = ing
            .group
            .and_then(|g| usize::try_from(g).ok())
            .and_then(|g| group_titles.get(g).cloned());
        let item = IngredientView {
            r#ref: ing.r#ref,
            name: r.text(&ing.name),
            code: ing.code.clone(),
            amount: r.amount(ing.quantity.as_ref(), units),
            alt: ing
                .alt
                .iter()
                .flatten()
                .map(|q| r.amount(Some(q), Units::Original))
                .collect(),
            derived: derived_measure(ing, units, &r.lang),
            preparation: r.opt(ing.preparation.as_ref()),
            note: r.opt(ing.note.as_ref()),
            optional: ing.optional == Some(true),
            fixed: ing.fixed == Some(true),
            substitutes: ing
                .substitutes
                .iter()
                .flatten()
                .map(|s| {
                    let amount = r.amount(s.quantity.as_ref(), units);
                    if amount.is_empty() {
                        r.text(&s.name)
                    } else {
                        format!("{} ({amount})", r.text(&s.name))
                    }
                })
                .collect(),
            recipe: ing.recipe.as_ref().map(|id| id.0.clone()),
            image: ing.blob.as_ref().map(|b| b.0.clone()),
        };
        match sections.last_mut() {
            Some(section) if section.title == title => section.items.push(item),
            _ => sections.push(IngredientSection {
                title,
                items: vec![item],
            }),
        }
    }

    let step_titles: Vec<String> = recipe
        .step_groups
        .iter()
        .flatten()
        .map(|g| r.text(g))
        .collect();
    let mut active = 0u64;
    let mut passive = 0u64;
    let steps = recipe
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let number = i + 1;
            let timers = step
                .timers
                .iter()
                .flatten()
                .filter_map(|timer| {
                    let (seconds, max_seconds) = duration_seconds(&timer.duration)?;
                    let is_passive = timer.passive == Some(true);
                    if is_passive {
                        passive += seconds;
                    } else {
                        active += seconds;
                    }
                    Some(TimerView {
                        label: r
                            .opt(timer.label.as_ref())
                            .unwrap_or_else(|| format!("Step {number}")),
                        seconds,
                        max_seconds,
                        text: format_duration_quantity(&timer.duration).unwrap_or_default(),
                        passive: is_passive,
                    })
                })
                .collect();
            StepView {
                r#ref: step.r#ref,
                number,
                section: step
                    .group
                    .and_then(|g| usize::try_from(g).ok())
                    .and_then(|g| step_titles.get(g).cloned()),
                text: r.prose(Some(&step.text)),
                ingredients: step
                    .ingredients
                    .iter()
                    .flatten()
                    .filter_map(|used| {
                        let (name, whole, preparation) = ingredient_names.get(&used.r#ref)?;
                        let amount = match &used.quantity {
                            Some(q) => r.amount(Some(q), units),
                            None => whole.clone(),
                        };
                        Some(StepIngredientView {
                            r#ref: used.r#ref,
                            name: name.clone(),
                            amount,
                            preparation: preparation.clone(),
                        })
                    })
                    .collect(),
                equipment: step
                    .equipment
                    .iter()
                    .flatten()
                    .filter_map(|e| equipment_names.get(e).cloned())
                    .collect(),
                timers,
                heat: step.heat.map(|h| heat_label(h).to_owned()),
                temperature: step.temperature.as_ref().map(|t| {
                    let shown = format_temperature(&convert_temperature(t, units));
                    match t.target {
                        Some(target) => format!("{shown} {}", target_label(target)),
                        None => shown,
                    }
                }),
                produces: r.opt(step.produces.as_ref()),
                note: r.prose(step.note.as_ref()),
                image: step.blob.as_ref().map(|b| b.0.clone()),
            }
        })
        .collect();

    let times = recipe
        .times
        .as_ref()
        .map(|t| {
            [
                ("Prep", &t.prep),
                ("Cook", &t.cook),
                ("Rest", &t.rest),
                ("Total", &t.total),
            ]
            .into_iter()
            .filter_map(|(label, q)| {
                Some(LabeledValue {
                    label: label.to_owned(),
                    value: format_duration_quantity(q.as_ref()?)?,
                })
            })
            .collect()
        })
        .unwrap_or_default();

    let mut tags: Vec<String> = Vec::new();
    tags.extend(
        recipe
            .course
            .iter()
            .flatten()
            .map(crate::model::primitives::OpenCode::as_wire),
    );
    tags.extend(recipe.cuisine.iter().flatten().cloned());
    tags.extend(
        recipe
            .diet
            .iter()
            .flatten()
            .map(crate::model::primitives::OpenCode::as_wire),
    );
    tags.extend(recipe.tags.iter().flatten().cloned());

    let yield_unit = r.text(&recipe.r#yield.unit_text);
    let yield_amount_text = format_quantity(&recipe.r#yield.amount, &r.lang);
    Ok(RecipeView {
        id: recipe.id.0.clone(),
        revision: recipe.revision.0.clone(),
        language: r.lang.0.clone(),
        title: r.text(&recipe.title),
        summary: r.prose(recipe.summary.as_ref()),
        yield_text: format!("{yield_amount_text} {yield_unit}")
            .trim()
            .to_owned(),
        yield_amount: recipe.r#yield.amount.min,
        yield_unit,
        times,
        ingredients: sections,
        equipment: recipe
            .equipment
            .iter()
            .flatten()
            .map(|e| EquipmentView {
                r#ref: e.r#ref,
                name: r.text(&e.name),
                amount: r.amount(e.quantity.as_ref(), Units::Original),
                note: r.opt(e.note.as_ref()),
                image: e.blob.as_ref().map(|b| b.0.clone()),
            })
            .collect(),
        steps,
        active_timer_seconds: active,
        active_timer_text: format_seconds(active),
        passive_timer_seconds: passive,
        passive_timer_text: format_seconds(passive),
        notes: r.prose(recipe.notes.as_ref()),
        attribution: r.prose(recipe.attribution.as_ref()),
        source: recipe.source.as_ref().map(|s| SourceView {
            title: s.title.clone(),
            author: s.author.clone(),
            url: s.url.clone(),
        }),
        tags,
        allergens: recipe
            .allergens
            .iter()
            .flatten()
            .map(crate::model::primitives::OpenCode::as_wire)
            .collect(),
        nutrition: recipe
            .nutrition
            .as_ref()
            .map(|n| {
                n.values
                    .iter()
                    .map(|(code, q)| LabeledValue {
                        label: code.as_wire(),
                        value: format_quantity(q, &r.lang),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        nutrition_basis: recipe
            .nutrition
            .as_ref()
            .map(|n| format_quantity(&n.basis, &r.lang)),
        media: recipe.media.iter().flatten().map(|b| b.0.clone()).collect(),
        authors: payload
            .meta
            .authors
            .iter()
            .map(|a| a.display_name.clone())
            .collect(),
        license: payload.meta.license.clone(),
        derived_from: recipe.derived_from.as_ref().map(|d| d.0.clone()),
        modified_at: recipe.modified_at.0,
    })
}
