//! The open recipe vocabularies (spec §13): course, diet, allergen, and
//! nutrient codes. A reader accepts any well-formed code; these enums name
//! the ones the schema lists, and [`crate::model::primitives::OpenCode`]
//! carries the rest.

use serde::{Deserialize, Serialize};

/// A course classification (`crisp-v1.cddl`: `course-code`).
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
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
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
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
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
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
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
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
