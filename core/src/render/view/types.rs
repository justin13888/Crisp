//! The recipe view's output types: what `recipe_view` returns, ready to
//! display without further format knowledge.

use serde::{Deserialize, Serialize};

use crate::model::primitives::Rational;

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
    /// The ingredient-index slug, verbatim (spec §13: an unknown code is
    /// still carried, never dropped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The formatted amount (empty when none).
    pub amount: String,
    /// Equivalent measures, formatted.
    pub alt: Vec<String>,
    /// A measure derived from the ingredient index's density, formatted with
    /// a leading `≈`, e.g. `"≈ 140 g"`. Present only when the author gave no
    /// `alt` (the author's measures always win), `code` names an entry with
    /// a density, and the amount is a mass or a volume. Whether it is shown
    /// is the reader's choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived: Option<String>,
    /// Preparation, e.g. "finely chopped".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation: Option<String>,
    /// A short note.
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    /// `active_timer_seconds`, formatted, e.g. `"1 hr 5 min"`.
    pub active_timer_text: String,
    /// Total passive (hands-off) timer time, in seconds.
    pub passive_timer_seconds: u64,
    /// `passive_timer_seconds`, formatted, e.g. `"45 min"`.
    pub passive_timer_text: String,
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
