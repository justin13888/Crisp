//! The recipe shape an LLM writes: plain JSON with typed-in amounts and
//! lightweight prose, converted to and from a Crisp [`BundlePayload`].
//!
//! Field names follow the format (`spec/crisp-v1.cddl`) wherever the shape
//! allows, so a validation issue's JSON Pointer reads the same against the
//! draft as against the payload.
//!
//! [`BundlePayload`]: crisp_core::model::payload::BundlePayload

mod blank;
mod carry;
mod convert;
mod extract;
mod pointer;
mod prose;
mod values;

#[cfg(test)]
pub(crate) mod tests;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use convert::{DraftIssue, to_payload};
pub use extract::{Extracted, from_payload};
pub use pointer::reroot;

/// One recipe, as an LLM drafts it. Every human-readable string is in
/// `language`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeDraft {
    /// BCP-47 language tag of every text field, e.g. `en` or `it`.
    pub language: String,
    /// The recipe title.
    pub title: String,
    /// A short description. Prose: blank lines separate paragraphs and
    /// `[text](https://…)` makes a link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// What the recipe makes.
    #[serde(rename = "yield")]
    pub yields: YieldDraft,
    /// Whole-recipe durations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub times: Option<TimesDraft>,
    /// Ingredient section headings; an ingredient's `group` indexes this.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ingredient_groups: Vec<String>,
    /// The ingredients, in order.
    #[serde(default)]
    pub ingredients: Vec<IngredientDraft>,
    /// Tools and vessels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment: Vec<EquipmentDraft>,
    /// Step section headings; a step's `group` indexes this.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub step_groups: Vec<String>,
    /// The method, in order. At least one.
    pub steps: Vec<StepDraft>,
    /// Free tags: lowercase slugs such as `weeknight`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Cuisine slugs such as `italian` or `sichuan`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cuisine: Vec<String>,
    /// Course codes: appetizer, main, side, dessert, beverage, breakfast,
    /// sauce, baked, snack (or another slug).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub course: Vec<String>,
    /// Diet codes: vegan, vegetarian, pescatarian, gluten-free, dairy-free,
    /// nut-free, halal, kosher, keto, low-fodmap (or another slug).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diet: Vec<String>,
    /// Allergen codes: gluten, crustaceans, eggs, fish, peanuts, soybeans,
    /// milk, tree-nuts, celery, mustard, sesame, sulphites, lupin, molluscs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allergens: Vec<String>,
    /// Months (1–12) the dish is best made in, ascending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub best_months: Vec<u64>,
    /// Background, goals, and verifiable facts about the dish. Prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// The primary reference the recipe follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceDraft>,
    /// Credits and citations, with links. Prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// SPDX license identifier (e.g. `CC-BY-4.0`), or `NONE` (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// What a recipe makes: an amount and what it counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YieldDraft {
    /// A number or range, e.g. `4` or `4-6`.
    pub amount: String,
    /// What is counted, e.g. `servings` or `cookies`.
    pub unit_text: String,
}

/// Whole-recipe durations, each typed like `20 min`, `1 h 30 min`, or
/// `10-15 min`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimesDraft {
    /// Hands-on preparation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prep: Option<String>,
    /// Active and passive cooking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cook: Option<String>,
    /// Resting, chilling, or proofing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<String>,
    /// Start to plate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}

/// One ingredient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IngredientDraft {
    /// A handle unique among the ingredients, used by steps (e.g. 1, 2, 3).
    /// Keep it when editing; a new ingredient takes an unused number.
    #[serde(rename = "ref")]
    pub handle: u64,
    /// Index into `ingredient_groups`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// The ingredient, e.g. `guanciale`.
    pub name: String,
    /// The amount as a cook would type it: `200 g`, `1 1/2 cups`,
    /// `2-3 tbsp`, `3 cloves`, `~1 tsp`, or text such as `to taste`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// Equivalent measures of the same amount, e.g. `["1 cup"]` for `120 g`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alt: Vec<String>,
    /// True if the amount must not scale with the yield.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    /// How it is prepared, e.g. `finely grated`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation: Option<String>,
    /// A short note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// True if the dish works without it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// Acceptable substitutes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substitutes: Vec<SubstituteDraft>,
}

/// A substitute for an ingredient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubstituteDraft {
    /// The substitute, e.g. `pancetta`.
    pub name: String,
    /// Its amount, typed like an ingredient's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
}

/// A tool or vessel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EquipmentDraft {
    /// A handle unique among the equipment, used by steps. Keep it when
    /// editing; a new item takes an unused number.
    #[serde(rename = "ref")]
    pub handle: u64,
    /// The item, e.g. `large skillet`.
    pub name: String,
    /// How many, e.g. `2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// A short note, e.g. `30 cm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One step of the method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepDraft {
    /// The step's handle, as `get_recipe` returns it. Keep it when editing an
    /// existing step so its photo stays attached; omit it for a new step.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<u64>,
    /// Index into `step_groups`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// What to do. Prose.
    pub text: String,
    /// The ingredients this step uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ingredients: Vec<StepIngredientDraft>,
    /// The `ref`s of the equipment this step uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment: Vec<u64>,
    /// Timers the cook can start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timers: Vec<TimerDraft>,
    /// Burner setting: low, medium-low, medium, medium-high, high.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat: Option<String>,
    /// A target temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<TemperatureDraft>,
    /// The intermediate this step produces, e.g. `the sauce`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produces: Option<String>,
    /// Tips and warnings, ideally with the reason they work. Prose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An ingredient a step uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepIngredientDraft {
    /// The ingredient's `ref`.
    #[serde(rename = "ref")]
    pub handle: u64,
    /// The portion used here, when only part of the ingredient is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
}

/// A step timer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimerDraft {
    /// How long, e.g. `8 min` or `8-10 min`.
    pub duration: String,
    /// What it times, e.g. `simmer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// True for hands-off waiting.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub passive: bool,
}

/// A temperature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemperatureDraft {
    /// The value with its unit, e.g. `220 °C`, `425 F`, or `70-75 °C`.
    pub value: String,
    /// What is measured: oven, internal, oil, pan, or water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// The recipe's primary reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceDraft {
    /// A book or site name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Its author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// An `https://` link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}
