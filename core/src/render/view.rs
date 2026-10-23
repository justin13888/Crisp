//! A render-ready view of one recipe.
//!
//! [`recipe_view`] resolves every localized value, applies scaling and unit
//! conversion, formats every amount, time, and temperature, and joins each
//! step to the ingredients and equipment it uses. A UI only lays the result
//! out; it never touches rationals, units, or locale fallback itself.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::payload::BundlePayload;
use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::{HeatLevel, Recipe, TemperatureTarget};
use crate::model::text::{LocalizedProse, LocalizedText, RunRefKind, RunTarget, StyleFlag};
use crate::render::quantity::{
    Units, convert_quantity, convert_temperature, duration_seconds, format_duration_quantity,
    format_quantity, format_temperature,
};
use crate::render::scale::scale_recipe;
use crate::render::{resolve_prose, resolve_text};

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

/// A run of styled text.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunView {
    /// The text.
    pub text: String,
    /// Bold.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    /// Italic.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Underlined.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    /// Monospace.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
    /// An `https` link (already verified).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// An internal reference: `"ingredient:3"`, `"step:2"`,
    /// `"equipment:1"`, or `"recipe:rcp1…"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A paragraph: a list of runs.
pub type ParagraphView = Vec<RunView>;

/// One ingredient line.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngredientView {
    /// The ingredient's `ref` handle.
    pub r#ref: u64,
    /// The name.
    pub name: String,
    /// The formatted amount (empty when none).
    pub amount: String,
    /// Equivalent measures, formatted.
    pub alt: Vec<String>,
    /// Preparation, e.g. "finely chopped".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation: Option<String>,
    /// A short note.
    pub note: Option<String>,
    /// Whether it is optional.
    pub optional: bool,
    /// Whether the amount ignores scaling.
    pub fixed: bool,
    /// Substitutes, formatted as `"name (amount)"`.
    pub substitutes: Vec<String>,
    /// A component recipe id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    /// An image blob id.
    pub image: Option<String>,
}

/// A titled run of ingredients.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngredientSection {
    /// The group title, if the ingredients are grouped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The ingredients, in recipe order.
    pub items: Vec<IngredientView>,
}

/// One piece of equipment.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentView {
    /// The equipment's `ref` handle.
    pub r#ref: u64,
    /// The name.
    pub name: String,
    /// How many, formatted (empty when unspecified).
    pub amount: String,
    /// A note, e.g. a size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// An image blob id.
    pub image: Option<String>,
}

/// An ingredient as used by a step.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepIngredientView {
    /// The ingredient's `ref` handle.
    pub r#ref: u64,
    /// The name.
    pub name: String,
    /// The amount used in this step (the portion, or the whole amount).
    pub amount: String,
    /// Preparation, e.g. "diced".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation: Option<String>,
}

/// A timer a cook can start.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerView {
    /// What the timer is for.
    pub label: String,
    /// The duration in seconds (the lower bound of a range).
    pub seconds: u64,
    /// The upper bound of a range, in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_seconds: Option<u64>,
    /// The duration, formatted.
    pub text: String,
    /// Whether the wait is hands-off.
    pub passive: bool,
}

/// One step.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepView {
    /// The step's `ref` handle.
    pub r#ref: u64,
    /// The 1-based step number.
    pub number: usize,
    /// The step-group title, if grouped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The instruction.
    #[cfg_attr(feature = "ts", tsify(type = "RunView[][]"))]
    pub text: Vec<ParagraphView>,
    /// The ingredients used, with amounts.
    pub ingredients: Vec<StepIngredientView>,
    /// The equipment used, by name.
    pub equipment: Vec<String>,
    /// Timers.
    pub timers: Vec<TimerView>,
    /// Heat level, e.g. `"Medium-high heat"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heat: Option<String>,
    /// Temperature, e.g. `"180°C oven"`.
    pub temperature: Option<String>,
    /// The intermediate result this step produces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produces: Option<String>,
    /// Tips and warnings.
    #[cfg_attr(feature = "ts", tsify(type = "RunView[][]"))]
    pub note: Vec<ParagraphView>,
    /// An image blob id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// A labelled formatted value (times, nutrition).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabeledValue {
    /// The label, e.g. `"Prep"`.
    pub label: String,
    /// The value, e.g. `"15 min"`.
    pub value: String,
}

/// Where the recipe came from.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceView {
    /// A book or site name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The original author.
    pub author: Option<String>,
    /// An `https` URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A whole recipe, ready to lay out.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeView {
    /// The recipe id.
    pub id: String,
    /// The revision id.
    pub revision: String,
    /// The language the view was resolved in.
    pub language: String,
    /// The title.
    pub title: String,
    /// The summary.
    #[cfg_attr(feature = "ts", tsify(type = "RunView[][]"))]
    pub summary: Vec<ParagraphView>,
    /// The yield, formatted, e.g. `"24 cookies"`.
    pub yield_text: String,
    /// The numeric yield after scaling, when there is one.
    #[cfg_attr(feature = "ts", tsify(type = "Rational"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yield_amount: Option<Rational>,
    /// The yield's unit text, e.g. `"cookies"`.
    pub yield_unit: String,
    /// Whole-recipe times.
    pub times: Vec<LabeledValue>,
    /// Ingredients, grouped.
    pub ingredients: Vec<IngredientSection>,
    /// Equipment.
    pub equipment: Vec<EquipmentView>,
    /// Steps.
    pub steps: Vec<StepView>,
    /// Total active timer time, in seconds.
    pub active_timer_seconds: u64,
    /// Total passive (hands-off) timer time, in seconds.
    pub passive_timer_seconds: u64,
    /// Notes.
    #[cfg_attr(feature = "ts", tsify(type = "RunView[][]"))]
    pub notes: Vec<ParagraphView>,
    /// Attribution.
    #[cfg_attr(feature = "ts", tsify(type = "RunView[][]"))]
    pub attribution: Vec<ParagraphView>,
    /// Source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceView>,
    /// Tags, cuisine, course, and diet codes.
    pub tags: Vec<String>,
    /// Declared allergens.
    pub allergens: Vec<String>,
    /// Nutrition per basis.
    pub nutrition: Vec<LabeledValue>,
    /// The nutrition basis, formatted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nutrition_basis: Option<String>,
    /// Image blob ids; the first is the cover.
    pub media: Vec<String>,
    /// The author display names.
    pub authors: Vec<String>,
    /// The license.
    pub license: String,
    /// The recipe id this was forked from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    /// When last modified (epoch seconds).
    pub modified_at: u64,
}

/// Resolves text in the view's language.
struct Resolver<'a> {
    preferred: Vec<LanguageTag>,
    default: &'a LanguageTag,
    lang: LanguageTag,
}

impl Resolver<'_> {
    fn text(&self, text: &LocalizedText) -> String {
        resolve_text(text, &self.preferred, self.default)
            .unwrap_or_default()
            .to_owned()
    }

    fn opt(&self, text: Option<&LocalizedText>) -> Option<String> {
        text.map(|t| self.text(t)).filter(|t| !t.is_empty())
    }

    fn prose(&self, prose: Option<&LocalizedProse>) -> Vec<ParagraphView> {
        let Some(body) = prose.and_then(|p| resolve_prose(p, &self.preferred, self.default)) else {
            return Vec::new();
        };
        body.0
            .iter()
            .map(|paragraph| {
                paragraph
                    .0
                    .iter()
                    .map(|run| {
                        let has = |flag| run.style.iter().flatten().any(|s| *s == flag);
                        RunView {
                            text: run.t.clone(),
                            bold: has(StyleFlag::Bold),
                            italic: has(StyleFlag::Italic),
                            underline: has(StyleFlag::Underline),
                            code: has(StyleFlag::Code),
                            link: run.link.clone(),
                            reference: run.reference.as_ref().map(|r| {
                                let kind = match r.kind {
                                    RunRefKind::Ingredient => "ingredient",
                                    RunRefKind::Step => "step",
                                    RunRefKind::Equipment => "equipment",
                                    RunRefKind::Recipe => "recipe",
                                };
                                match &r.target {
                                    RunTarget::Handle(h) => format!("{kind}:{h}"),
                                    RunTarget::Recipe(id) => format!("{kind}:{}", id.0),
                                }
                            }),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn amount(&self, quantity: Option<&Quantity>, units: Units) -> String {
        quantity
            .map(|q| format_quantity(&convert_quantity(q, units), &self.lang))
            .unwrap_or_default()
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

/// Picks the language a view resolves to: the first preferred language the
/// bundle declares, else its default.
fn view_language(payload: &BundlePayload, preferred: &[LanguageTag]) -> LanguageTag {
    preferred
        .iter()
        .find(|tag| payload.languages.contains(tag))
        .cloned()
        .unwrap_or_else(|| payload.default_language.clone())
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
    let lang = view_language(payload, &options.languages);
    let mut preferred = vec![lang.clone()];
    preferred.extend(options.languages.iter().cloned());
    let r = Resolver {
        preferred,
        default: &payload.default_language,
        lang,
    };
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

    // Ingredients, grouped in first-appearance order.
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
            amount: r.amount(ing.quantity.as_ref(), units),
            alt: ing
                .alt
                .iter()
                .flatten()
                .map(|q| r.amount(Some(q), Units::Original))
                .collect(),
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
        passive_timer_seconds: passive,
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
