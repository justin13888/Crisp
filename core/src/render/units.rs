//! The UCUM unit catalogue Crisp understands: labels for display, aliases
//! for parsing, and exact factors for conversion.
//!
//! Any UCUM code is valid on the wire; this catalogue is what a viewer can
//! *explain*. Codes outside it render verbatim, and UCUM annotations such as
//! `{clove}` render as their inner word.

use serde::Serialize;

use crate::model::primitives::Rational;

/// The physical dimension a unit measures.
#[cfg_attr(feature = "ts", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitKind {
    /// Mass; base unit gram.
    Mass,
    /// Volume; base unit millilitre.
    Volume,
    /// Time; base unit second.
    Time,
    /// Temperature (converted affinely, not by factor).
    Temperature,
    /// Length; base unit millimetre.
    Length,
}

/// Which measurement system a unit belongs to.
#[cfg_attr(feature = "ts", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitSystem {
    /// SI / metric.
    Metric,
    /// US customary.
    Us,
    /// Used by both (time).
    Neutral,
}

/// One catalogued unit.
#[cfg_attr(feature = "ts", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnitInfo {
    /// The UCUM code, e.g. `[tbsp_us]`.
    pub code: &'static str,
    /// The dimension.
    pub kind: UnitKind,
    /// The measurement system.
    pub system: UnitSystem,
    /// The label for an amount of exactly one, e.g. `"tbsp"` or `"cup"`.
    pub singular: &'static str,
    /// The label for any other amount, e.g. `"cups"`.
    pub plural: &'static str,
    /// How many base units one of this unit is (exact). Zero for
    /// temperatures, which convert affinely.
    #[cfg_attr(feature = "ts", tsify(type = "Rational"))]
    #[serde(skip)]
    pub factor: Rational,
    /// Lowercase spellings accepted by the amount parser.
    #[serde(skip)]
    pub aliases: &'static [&'static str],
}

const fn r(numerator: i64, denominator: u64) -> Rational {
    Rational::new(numerator, denominator)
}

use UnitKind::{Length, Mass, Temperature, Time, Volume};
use UnitSystem::{Metric, Neutral, Us};

/// Every catalogued unit. Volume factors are exact in millilitres (US
/// teaspoon = 4.92892159375 mL).
pub const UNITS: &[UnitInfo] = &[
    UnitInfo {
        code: "g",
        kind: Mass,
        system: Metric,
        singular: "g",
        plural: "g",
        factor: r(1, 1),
        aliases: &["g", "gram", "grams", "gramme", "grammes", "gr"],
    },
    UnitInfo {
        code: "kg",
        kind: Mass,
        system: Metric,
        singular: "kg",
        plural: "kg",
        factor: r(1000, 1),
        aliases: &["kg", "kilo", "kilos", "kilogram", "kilograms"],
    },
    UnitInfo {
        code: "mg",
        kind: Mass,
        system: Metric,
        singular: "mg",
        plural: "mg",
        factor: r(1, 1000),
        aliases: &["mg", "milligram", "milligrams"],
    },
    UnitInfo {
        code: "[oz_av]",
        kind: Mass,
        system: Us,
        singular: "oz",
        plural: "oz",
        factor: r(28_349_523_125, 1_000_000_000),
        aliases: &["oz", "ounce", "ounces"],
    },
    UnitInfo {
        code: "[lb_av]",
        kind: Mass,
        system: Us,
        singular: "lb",
        plural: "lb",
        factor: r(45_359_237, 100_000),
        aliases: &["lb", "lbs", "pound", "pounds"],
    },
    UnitInfo {
        code: "mL",
        kind: Volume,
        system: Metric,
        singular: "mL",
        plural: "mL",
        factor: r(1, 1),
        aliases: &[
            "ml",
            "millilitre",
            "millilitres",
            "milliliter",
            "milliliters",
            "cc",
        ],
    },
    UnitInfo {
        code: "cL",
        kind: Volume,
        system: Metric,
        singular: "cL",
        plural: "cL",
        factor: r(10, 1),
        aliases: &[
            "cl",
            "centilitre",
            "centilitres",
            "centiliter",
            "centiliters",
        ],
    },
    UnitInfo {
        code: "dL",
        kind: Volume,
        system: Metric,
        singular: "dL",
        plural: "dL",
        factor: r(100, 1),
        aliases: &["dl", "decilitre", "decilitres", "deciliter", "deciliters"],
    },
    UnitInfo {
        code: "L",
        kind: Volume,
        system: Metric,
        singular: "L",
        plural: "L",
        factor: r(1000, 1),
        aliases: &["l", "litre", "litres", "liter", "liters"],
    },
    UnitInfo {
        code: "[tsp_us]",
        kind: Volume,
        system: Us,
        singular: "tsp",
        plural: "tsp",
        factor: r(492_892_159_375, 100_000_000_000),
        aliases: &["tsp", "tsps", "teaspoon", "teaspoons"],
    },
    UnitInfo {
        code: "[tbsp_us]",
        kind: Volume,
        system: Us,
        singular: "tbsp",
        plural: "tbsp",
        factor: r(1_478_676_478_125, 100_000_000_000),
        aliases: &["tbsp", "tbsps", "tablespoon", "tablespoons", "tbs", "tbl"],
    },
    UnitInfo {
        code: "[foz_us]",
        kind: Volume,
        system: Us,
        singular: "fl oz",
        plural: "fl oz",
        factor: r(295_735_295_625, 10_000_000_000),
        aliases: &["fl oz", "floz", "fluid ounce", "fluid ounces"],
    },
    UnitInfo {
        code: "[cup_us]",
        kind: Volume,
        system: Us,
        singular: "cup",
        plural: "cups",
        factor: r(2_365_882_365, 10_000_000),
        aliases: &["cup", "cups", "c"],
    },
    UnitInfo {
        code: "[pt_us]",
        kind: Volume,
        system: Us,
        singular: "pint",
        plural: "pints",
        factor: r(473_176_473, 1_000_000),
        aliases: &["pt", "pint", "pints"],
    },
    UnitInfo {
        code: "[qt_us]",
        kind: Volume,
        system: Us,
        singular: "quart",
        plural: "quarts",
        factor: r(946_352_946, 1_000_000),
        aliases: &["qt", "quart", "quarts"],
    },
    UnitInfo {
        code: "[gal_us]",
        kind: Volume,
        system: Us,
        singular: "gallon",
        plural: "gallons",
        factor: r(3_785_411_784, 1_000_000),
        aliases: &["gal", "gallon", "gallons"],
    },
    UnitInfo {
        code: "s",
        kind: Time,
        system: Neutral,
        singular: "sec",
        plural: "sec",
        factor: r(1, 1),
        aliases: &["s", "sec", "secs", "second", "seconds"],
    },
    UnitInfo {
        code: "min",
        kind: Time,
        system: Neutral,
        singular: "min",
        plural: "min",
        factor: r(60, 1),
        aliases: &["min", "mins", "minute", "minutes", "m"],
    },
    UnitInfo {
        code: "h",
        kind: Time,
        system: Neutral,
        singular: "hr",
        plural: "hr",
        factor: r(3600, 1),
        aliases: &["h", "hr", "hrs", "hour", "hours"],
    },
    UnitInfo {
        code: "d",
        kind: Time,
        system: Neutral,
        singular: "day",
        plural: "days",
        factor: r(86_400, 1),
        aliases: &["d", "day", "days"],
    },
    UnitInfo {
        code: "Cel",
        kind: Temperature,
        system: Metric,
        singular: "°C",
        plural: "°C",
        factor: r(0, 1),
        aliases: &["°c", "cel", "celsius", "degc"],
    },
    UnitInfo {
        code: "[degF]",
        kind: Temperature,
        system: Us,
        singular: "°F",
        plural: "°F",
        factor: r(0, 1),
        aliases: &["°f", "f", "fahrenheit", "degf"],
    },
    UnitInfo {
        code: "K",
        kind: Temperature,
        system: Metric,
        singular: "K",
        plural: "K",
        factor: r(0, 1),
        aliases: &["k", "kelvin"],
    },
    UnitInfo {
        code: "mm",
        kind: Length,
        system: Metric,
        singular: "mm",
        plural: "mm",
        factor: r(1, 1),
        aliases: &[
            "mm",
            "millimetre",
            "millimetres",
            "millimeter",
            "millimeters",
        ],
    },
    UnitInfo {
        code: "cm",
        kind: Length,
        system: Metric,
        singular: "cm",
        plural: "cm",
        factor: r(10, 1),
        aliases: &[
            "cm",
            "centimetre",
            "centimetres",
            "centimeter",
            "centimeters",
        ],
    },
    UnitInfo {
        code: "m",
        kind: Length,
        system: Metric,
        singular: "m",
        plural: "m",
        factor: r(1000, 1),
        aliases: &["metre", "metres", "meter", "meters"],
    },
    UnitInfo {
        code: "[in_i]",
        kind: Length,
        system: Us,
        singular: "in",
        plural: "in",
        factor: r(254, 10),
        aliases: &["in", "inch", "inches", "\""],
    },
];

/// Looks up a catalogued unit by its exact UCUM code.
#[must_use]
pub fn lookup(code: &str) -> Option<&'static UnitInfo> {
    UNITS.iter().find(|u| u.code == code)
}

/// Resolves a typed unit word (any case, optional trailing `.`) to a
/// catalogued unit. Time words win over the bare `m` for metres.
#[must_use]
pub fn resolve_alias(word: &str) -> Option<&'static UnitInfo> {
    let word = word.trim().trim_end_matches('.').to_lowercase();
    if word.is_empty() {
        return None;
    }
    UNITS
        .iter()
        .find(|u| u.aliases.contains(&word.as_str()))
        .or_else(|| UNITS.iter().find(|u| u.code.eq_ignore_ascii_case(&word)))
}

/// The display label for a unit code at a given amount: catalogue labels,
/// the inner word of a UCUM annotation (`{clove}` → `clove`), or the code.
#[must_use]
pub fn label(code: &str, plural: bool) -> String {
    if let Some(unit) = lookup(code) {
        return if plural { unit.plural } else { unit.singular }.to_owned();
    }
    if let Some(inner) = code.strip_prefix('{').and_then(|c| c.strip_suffix('}')) {
        return inner.to_owned();
    }
    code.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_aliases_are_lowercase() {
        let mut codes = std::collections::BTreeSet::new();
        for unit in UNITS {
            assert!(codes.insert(unit.code), "duplicate {}", unit.code);
            for alias in unit.aliases {
                assert_eq!(*alias, alias.to_lowercase());
            }
        }
    }

    #[test]
    fn aliases_resolve_case_insensitively() {
        assert_eq!(resolve_alias("Tbsp.").map(|u| u.code), Some("[tbsp_us]"));
        assert_eq!(resolve_alias("cups").map(|u| u.code), Some("[cup_us]"));
        assert_eq!(resolve_alias("m").map(|u| u.code), Some("min"));
        assert_eq!(resolve_alias("mL").map(|u| u.code), Some("mL"));
        assert!(resolve_alias("handful").is_none());
    }

    #[test]
    fn labels_cover_catalogue_annotations_and_unknowns() {
        assert_eq!(label("[cup_us]", true), "cups");
        assert_eq!(label("[cup_us]", false), "cup");
        assert_eq!(label("{clove}", true), "clove");
        assert_eq!(label("[smoot]", true), "[smoot]");
    }
}
