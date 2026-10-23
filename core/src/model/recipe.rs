//! The recipe data model: a recipe and all of its parts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::ids::{BlobId, RecipeId, RevisionId};
use crate::model::payload::ExtensionMap;
use crate::model::primitives::{LanguageTag, OpenCode, Quantity, Rational, Timestamp};
use crate::model::text::{LocalizedProse, LocalizedText};

/// The five identity fields frozen at a recipe's (or book's) birth
/// (`crisp-v1.cddl`: `identity`). The document id is derived from exactly
/// these fields, so they never change across edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// The author's key fingerprint (32 bytes); see
    /// [`crate::identity::key_fingerprint`].
    #[serde(with = "serde_bytes")]
    pub author_fpr: Vec<u8>,
    /// The original, normalized title.
    pub title: String,
    /// The language of `title`.
    pub language: LanguageTag,
    /// The creation instant (frozen).
    pub created_at: Timestamp,
    /// A 128-bit random nonce fixed at birth (16 bytes).
    #[serde(with = "serde_bytes")]
    pub birth_nonce: Vec<u8>,
}

/// A complete recipe (`crisp-v1.cddl`: `recipe`).
///
/// `Eq` is not derived because the optional `ext` map holds arbitrary CBOR,
/// which is only `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// The frozen birth identity.
    pub identity: Identity,
    /// The recipe id, derived from `identity`.
    pub id: RecipeId,
    /// The revision hash, derived from this recipe's content.
    pub revision: RevisionId,
    /// The revision this one supersedes, forming a hash-linked history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<RevisionId>,
    /// The parent recipe this one was forked from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<RecipeId>,
    /// When the recipe was last modified.
    pub modified_at: Timestamp,
    /// The displayed title.
    pub title: LocalizedText,
    /// An optional short summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<LocalizedProse>,
    /// What the recipe yields.
    pub r#yield: Yield,
    /// Whole-recipe durations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times: Option<Times>,
    /// Names of the ingredient groups, referenced by `ingredient.group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingredient_groups: Option<Vec<LocalizedText>>,
    /// The ingredients.
    pub ingredients: Vec<Ingredient>,
    /// The equipment used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment: Option<Vec<Equipment>>,
    /// Names of the step groups, referenced by `step.group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_groups: Option<Vec<LocalizedText>>,
    /// The ordered steps; at least one.
    pub steps: Vec<Step>,
    /// Optional nutrition information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nutrition: Option<Nutrition>,
    /// Language-neutral keyword tags (slugs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Language-neutral cuisine tags (slugs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cuisine: Option<Vec<String>>,
    /// Course classifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course: Option<Vec<OpenCode<CourseCode>>>,
    /// Diet classifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diet: Option<Vec<OpenCode<DietCode>>>,
    /// Declared allergens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allergens: Option<Vec<OpenCode<AllergenCode>>>,
    /// Associated media blobs; the first is the recipe's cover image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<BlobId>>,
    /// Free-form notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<LocalizedProse>,
    /// Where the recipe came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Attribution text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<LocalizedProse>,
    /// Namespaced extension data, ignored by conformant readers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// Whole-recipe durations (`crisp-v1.cddl`: `times`). Each uses a UCUM time
/// unit.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Times {
    /// Hands-on preparation time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prep: Option<Quantity>,
    /// Cooking time, active and passive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cook: Option<Quantity>,
    /// Resting, chilling, or proofing time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest: Option<Quantity>,
    /// Start-to-plate time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<Quantity>,
}

/// Where a recipe came from (`crisp-v1.cddl`: `source`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// A book or site name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The original author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// An `https://` URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// What a recipe produces (`crisp-v1.cddl`: `yield`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Yield {
    /// The amount produced.
    pub amount: Quantity,
    /// The unit of the amount, e.g. "servings" or "cookies".
    pub unit_text: LocalizedText,
}

/// A single ingredient (`crisp-v1.cddl`: `ingredient`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingredient {
    /// A stable within-recipe handle, referenced by steps.
    pub r#ref: u64,
    /// The index of the ingredient group this belongs to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// The ingredient name.
    pub name: LocalizedText,
    /// The quantity called for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
    /// Equivalent measures of the same amount, e.g. grams for a cup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<Vec<Quantity>>,
    /// When `true`, the amount never scales with the batch size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed: Option<bool>,
    /// Preparation note, e.g. "finely chopped".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation: Option<LocalizedText>,
    /// A short note, e.g. "room temperature".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<LocalizedText>,
    /// Whether the ingredient is optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    /// Acceptable substitutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substitutes: Option<Vec<Substitute>>,
    /// A component recipe this ingredient is made by (not resolved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<RecipeId>,
    /// An ingredient photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// A substitute for an ingredient (`crisp-v1.cddl`: `substitute`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Substitute {
    /// The substitute's name.
    pub name: LocalizedText,
    /// The substitute quantity, if it differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// A single recipe step (`crisp-v1.cddl`: `step`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// A stable within-recipe handle.
    pub r#ref: u64,
    /// The index of the step group this belongs to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// The instruction text.
    pub text: LocalizedProse,
    /// The ingredients used in this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingredients: Option<Vec<StepIngredient>>,
    /// Handles of the equipment used in this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment: Option<Vec<u64>>,
    /// Timers a cook can start during this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timers: Option<Vec<Timer>>,
    /// A stovetop heat level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heat: Option<HeatLevel>,
    /// A target temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,
    /// Names an intermediate result produced by this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produces: Option<LocalizedText>,
    /// Tips and warnings for this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<LocalizedProse>,
    /// A step photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// An ingredient used by a step (`crisp-v1.cddl`: `step-ingredient`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepIngredient {
    /// The ingredient's `ref` handle.
    pub r#ref: u64,
    /// The portion used in this step, when only part of it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
}

/// A timer attached to a step (`crisp-v1.cddl`: `timer`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timer {
    /// The duration, in a UCUM time unit; a range runs to `max`.
    pub duration: Quantity,
    /// What the timer is for, e.g. "simmer".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<LocalizedText>,
    /// Whether the wait is hands-off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passive: Option<bool>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// A stovetop heat level (`crisp-v1.cddl`: `heat-level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeatLevel {
    /// Low heat.
    Low,
    /// Medium-low heat.
    MediumLow,
    /// Medium heat.
    Medium,
    /// Medium-high heat.
    MediumHigh,
    /// High heat.
    High,
}

/// A target temperature for a step (`crisp-v1.cddl`: `temperature`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Temperature {
    /// The temperature, or the lower bound of a range.
    pub min: Rational,
    /// The upper bound of a range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Rational>,
    /// The temperature unit.
    pub unit: TemperatureUnit,
    /// What the temperature applies to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TemperatureTarget>,
}

/// A UCUM temperature unit (`crisp-v1.cddl`: `temperature.unit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemperatureUnit {
    /// Degrees Celsius (`Cel`).
    #[serde(rename = "Cel")]
    Celsius,
    /// Degrees Fahrenheit (`[degF]`).
    #[serde(rename = "[degF]")]
    Fahrenheit,
    /// Kelvin (`K`).
    #[serde(rename = "K")]
    Kelvin,
}

/// What a [`Temperature`] applies to (`crisp-v1.cddl`: `temperature.target`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureTarget {
    /// The oven.
    Oven,
    /// The food's internal (probe) temperature.
    Internal,
    /// Frying oil.
    Oil,
    /// A pan or griddle surface.
    Pan,
    /// A water bath.
    Water,
}

/// A piece of equipment (`crisp-v1.cddl`: `equipment`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equipment {
    /// A stable within-recipe handle, referenced by `step.equipment`.
    pub r#ref: u64,
    /// The equipment name.
    pub name: LocalizedText,
    /// How many are needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
    /// An optional note, e.g. a size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<LocalizedText>,
    /// An equipment photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
    /// Namespaced extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
}

/// Nutrition information (`crisp-v1.cddl`: `nutrition`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nutrition {
    /// The basis the values are measured against, e.g. per serving.
    pub basis: Quantity,
    /// The nutrient values, keyed by nutrient code.
    pub values: BTreeMap<OpenCode<NutrientCode>, Quantity>,
}

/// A course classification (`crisp-v1.cddl`: `course-code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CourseCode {
    /// An appetizer.
    Appetizer,
    /// A main course.
    Main,
    /// A side dish.
    Side,
    /// A dessert.
    Dessert,
    /// A beverage.
    Beverage,
    /// A breakfast dish.
    Breakfast,
    /// A sauce.
    Sauce,
    /// A baked good.
    Baked,
    /// A snack.
    Snack,
}

impl CourseCode {
    /// Every variant in schema order. The wire string is obtained by
    /// serializing — never hand-typed — so [`serde`]'s `rename_all` stays
    /// authoritative.
    pub const ALL: &'static [Self] = &[
        Self::Appetizer,
        Self::Main,
        Self::Side,
        Self::Dessert,
        Self::Beverage,
        Self::Breakfast,
        Self::Sauce,
        Self::Baked,
        Self::Snack,
    ];
}

/// A diet classification (`crisp-v1.cddl`: `diet-code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DietCode {
    /// Vegan.
    Vegan,
    /// Vegetarian.
    Vegetarian,
    /// Pescatarian.
    Pescatarian,
    /// Gluten-free.
    GlutenFree,
    /// Dairy-free.
    DairyFree,
    /// Nut-free.
    NutFree,
    /// Halal.
    Halal,
    /// Kosher.
    Kosher,
    /// Ketogenic.
    Keto,
    /// Low-FODMAP.
    LowFodmap,
}

impl DietCode {
    /// Every variant in schema order. The wire string is obtained by
    /// serializing — never hand-typed — so [`serde`]'s `rename_all` stays
    /// authoritative.
    pub const ALL: &'static [Self] = &[
        Self::Vegan,
        Self::Vegetarian,
        Self::Pescatarian,
        Self::GlutenFree,
        Self::DairyFree,
        Self::NutFree,
        Self::Halal,
        Self::Kosher,
        Self::Keto,
        Self::LowFodmap,
    ];
}

/// A declared allergen (`crisp-v1.cddl`: `allergen-code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AllergenCode {
    /// Gluten.
    Gluten,
    /// Crustaceans.
    Crustaceans,
    /// Eggs.
    Eggs,
    /// Fish.
    Fish,
    /// Peanuts.
    Peanuts,
    /// Soybeans.
    Soybeans,
    /// Milk.
    Milk,
    /// Tree nuts.
    TreeNuts,
    /// Celery.
    Celery,
    /// Mustard.
    Mustard,
    /// Sesame.
    Sesame,
    /// Sulphites.
    Sulphites,
    /// Lupin.
    Lupin,
    /// Molluscs.
    Molluscs,
}

impl AllergenCode {
    /// Every variant in schema order. The wire string is obtained by
    /// serializing — never hand-typed — so [`serde`]'s `rename_all` stays
    /// authoritative.
    pub const ALL: &'static [Self] = &[
        Self::Gluten,
        Self::Crustaceans,
        Self::Eggs,
        Self::Fish,
        Self::Peanuts,
        Self::Soybeans,
        Self::Milk,
        Self::TreeNuts,
        Self::Celery,
        Self::Mustard,
        Self::Sesame,
        Self::Sulphites,
        Self::Lupin,
        Self::Molluscs,
    ];
}

/// A nutrient code (`crisp-v1.cddl`: `nutrient-code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NutrientCode {
    /// Energy, in kilocalories.
    EnergyKcal,
    /// Protein, in grams.
    ProteinG,
    /// Total fat, in grams.
    FatG,
    /// Saturated fat, in grams.
    SaturatedFatG,
    /// Carbohydrate, in grams.
    CarbG,
    /// Sugar, in grams.
    SugarG,
    /// Fiber, in grams.
    FiberG,
    /// Sodium, in milligrams.
    SodiumMg,
}

impl NutrientCode {
    /// Every variant in schema order. The wire string is obtained by
    /// serializing — never hand-typed — so [`serde`]'s `rename_all` stays
    /// authoritative.
    pub const ALL: &'static [Self] = &[
        Self::EnergyKcal,
        Self::ProteinG,
        Self::FatG,
        Self::SaturatedFatG,
        Self::CarbG,
        Self::SugarG,
        Self::FiberG,
        Self::SodiumMg,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts the number of variants in [`CourseCode`] without relying on a
    /// derive macro. Compiles only because the `match` is exhaustive — a new
    /// variant breaks the build until [`CourseCode::ALL`] is updated.
    const fn course_variant_count() -> usize {
        let mut n = 0;
        let variants = [
            CourseCode::Appetizer,
            CourseCode::Main,
            CourseCode::Side,
            CourseCode::Dessert,
            CourseCode::Beverage,
            CourseCode::Breakfast,
            CourseCode::Sauce,
            CourseCode::Baked,
            CourseCode::Snack,
        ];
        let mut i = 0;
        while i < variants.len() {
            // Exhaustive match forces an update when the enum grows.
            match variants[i] {
                CourseCode::Appetizer
                | CourseCode::Main
                | CourseCode::Side
                | CourseCode::Dessert
                | CourseCode::Beverage
                | CourseCode::Breakfast
                | CourseCode::Sauce
                | CourseCode::Baked
                | CourseCode::Snack => {}
            }
            n += 1;
            i += 1;
        }
        n
    }

    #[test]
    fn course_all_covers_every_variant() {
        assert_eq!(CourseCode::ALL.len(), course_variant_count());
    }

    #[test]
    fn diet_all_covers_every_variant() {
        // Exhaustive: dropping a variant from this match breaks the build.
        for d in DietCode::ALL {
            match d {
                DietCode::Vegan
                | DietCode::Vegetarian
                | DietCode::Pescatarian
                | DietCode::GlutenFree
                | DietCode::DairyFree
                | DietCode::NutFree
                | DietCode::Halal
                | DietCode::Kosher
                | DietCode::Keto
                | DietCode::LowFodmap => {}
            }
        }
        assert_eq!(DietCode::ALL.len(), 10);
    }

    #[test]
    fn allergen_all_covers_every_variant() {
        for a in AllergenCode::ALL {
            match a {
                AllergenCode::Gluten
                | AllergenCode::Crustaceans
                | AllergenCode::Eggs
                | AllergenCode::Fish
                | AllergenCode::Peanuts
                | AllergenCode::Soybeans
                | AllergenCode::Milk
                | AllergenCode::TreeNuts
                | AllergenCode::Celery
                | AllergenCode::Mustard
                | AllergenCode::Sesame
                | AllergenCode::Sulphites
                | AllergenCode::Lupin
                | AllergenCode::Molluscs => {}
            }
        }
        assert_eq!(AllergenCode::ALL.len(), 14);
    }

    #[test]
    fn nutrient_all_covers_every_variant() {
        for n in NutrientCode::ALL {
            match n {
                NutrientCode::EnergyKcal
                | NutrientCode::ProteinG
                | NutrientCode::FatG
                | NutrientCode::SaturatedFatG
                | NutrientCode::CarbG
                | NutrientCode::SugarG
                | NutrientCode::FiberG
                | NutrientCode::SodiumMg => {}
            }
        }
        assert_eq!(NutrientCode::ALL.len(), 8);
    }

    #[test]
    fn each_variant_serializes_to_a_distinct_wire_string() {
        // Exercise the *serialization* path rather than hand-typing strings,
        // so a `rename_all` change is reflected automatically.
        use std::collections::BTreeSet;
        fn wire<T: Serialize>(value: &T) -> String {
            serde_json::to_string(value)
                .expect("serde_json")
                .trim_matches('"')
                .to_owned()
        }
        let mut all = BTreeSet::new();
        for c in CourseCode::ALL {
            assert!(all.insert(wire(c)));
        }
        for d in DietCode::ALL {
            assert!(all.insert(wire(d)));
        }
        for a in AllergenCode::ALL {
            assert!(all.insert(wire(a)));
        }
        for n in NutrientCode::ALL {
            assert!(all.insert(wire(n)));
        }
        assert_eq!(
            all.len(),
            CourseCode::ALL.len()
                + DietCode::ALL.len()
                + AllergenCode::ALL.len()
                + NutrientCode::ALL.len()
        );
        assert!(all.iter().all(|s| !s.is_empty()));
    }
}
