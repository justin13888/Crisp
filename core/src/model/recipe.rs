//! The recipe data model: a recipe and all of its parts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::ids::{BlobId, RecipeId, RevisionId};
use crate::model::payload::ExtensionMap;
use crate::model::primitives::{LanguageTag, Quantity, Rational, Timestamp, UnitCode};
use crate::model::text::{LocalizedProse, LocalizedText};

/// The five identity fields frozen at a recipe's (or book's) birth
/// (`crisp-v1.cddl`: `identity`). The document id is derived from exactly
/// these fields, so they never change across edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// `SHA-256` of the author's canonical-CBOR public-key-set (32 bytes).
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
    /// An optional manually-tracked monotonic version counter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
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
    /// Named durations keyed by phase, e.g. `prep` or `bake`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub times: Option<BTreeMap<String, Quantity>>,
    /// Names of the ingredient groups, referenced by `ingredient.group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingredient_groups: Option<Vec<LocalizedText>>,
    /// The ingredients.
    pub ingredients: Vec<Ingredient>,
    /// The equipment used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment: Option<Vec<Equipment>>,
    /// The ordered steps; at least one.
    pub steps: Vec<Step>,
    /// Optional nutrition information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nutrition: Option<Nutrition>,
    /// Language-neutral keyword tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Language-neutral cuisine tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cuisine: Option<Vec<String>>,
    /// Course classifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course: Option<Vec<CourseCode>>,
    /// Diet classifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diet: Option<Vec<DietCode>>,
    /// Declared allergens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allergens: Option<Vec<AllergenCode>>,
    /// Associated media blobs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Vec<BlobId>>,
    /// Free-form notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<LocalizedProse>,
    /// Attribution text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<LocalizedProse>,
    /// Namespaced extension data, ignored by conformant readers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ExtensionMap>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingredient {
    /// A stable within-recipe handle, referenced by `step.ingredient_refs`.
    pub r#ref: u64,
    /// The index of the ingredient group this belongs to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// The ingredient name.
    pub name: LocalizedText,
    /// The quantity called for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
    /// A human-authored amount override, e.g. "about 2 cups".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<LocalizedText>,
    /// Preparation note, e.g. "finely chopped".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preparation: Option<LocalizedText>,
    /// Whether the ingredient is optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    /// Acceptable substitutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substitutes: Option<Vec<Substitute>>,
    /// An ingredient photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
}

/// A substitute for an ingredient (`crisp-v1.cddl`: `substitute`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Substitute {
    /// The substitute's name.
    pub name: LocalizedText,
    /// The substitute quantity, if it differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<Quantity>,
}

/// A single recipe step (`crisp-v1.cddl`: `step`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// A stable within-recipe handle.
    pub r#ref: u64,
    /// An optional step-group name, e.g. "Dough".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<LocalizedText>,
    /// The instruction text.
    pub text: LocalizedProse,
    /// Handles of the ingredients used in this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingredient_refs: Option<Vec<u64>>,
    /// Handles of the equipment used in this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment_refs: Option<Vec<u64>>,
    /// An optional duration that drives a client-side timer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<Quantity>,
    /// An optional target temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Temperature>,
    /// Names an intermediate result produced by this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub produces: Option<LocalizedText>,
    /// A step photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
}

/// A target temperature for a step (`crisp-v1.cddl`: `step.temperature`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Temperature {
    /// The temperature value.
    pub value: Rational,
    /// The temperature unit.
    pub unit: UnitCode,
}

/// A piece of equipment (`crisp-v1.cddl`: `equipment`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equipment {
    /// A stable within-recipe handle, referenced by `step.equipment_refs`.
    pub r#ref: u64,
    /// The equipment name.
    pub name: LocalizedText,
    /// An optional note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<LocalizedText>,
    /// An equipment photo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobId>,
}

/// Nutrition information (`crisp-v1.cddl`: `nutrition`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nutrition {
    /// The basis the values are measured against, e.g. per serving.
    pub basis: Quantity,
    /// The nutrient values, keyed by nutrient code.
    pub values: BTreeMap<NutrientCode, Quantity>,
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
