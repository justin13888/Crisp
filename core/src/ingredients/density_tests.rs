//! Contract tests for density conversion.

use super::*;
use crate::model::primitives::{Quantity, Rational, UnitCode};
use crate::render::Units;

fn amount(value: i64, unit: &str) -> Quantity {
    Quantity {
        min: Some(Rational::new(value, 1)),
        unit: Some(UnitCode(unit.to_owned())),
        ..Quantity::default()
    }
}

#[test]
fn density_relates_volume_and_mass_both_ways() {
    let water = Rational::new(1, 1);
    let grams = convert_with_density(&amount(250, "mL"), water, Units::Original).expect("mass");
    assert_eq!(grams.unit, Some(UnitCode("g".to_owned())));
    assert_eq!(grams.min, Some(Rational::new(250, 1)));
    assert_eq!(grams.approx, Some(true));
    let fish_sauce = Rational::new(6, 5);
    let ml = convert_with_density(&amount(120, "g"), fish_sauce, Units::Metric).expect("volume");
    assert_eq!(ml.unit, Some(UnitCode("mL".to_owned())));
    assert_eq!(ml.min, Some(Rational::new(100, 1)));
    let mut range = amount(1, "[cup_us]");
    range.max = Some(Rational::new(2, 1));
    let both = convert_with_density(&range, water, Units::Metric).expect("range");
    assert!(both.max.is_some());
    let cups = convert_with_density(&amount(240, "g"), water, Units::Us).expect("us");
    assert_eq!(cups.unit, Some(UnitCode("[cup_us]".to_owned())));
}

#[test]
fn density_snaps_to_measurable_amounts_but_keeps_tiny_ones() {
    let water = Rational::new(1, 1);
    let cup = convert_with_density(&amount(1, "[cup_us]"), water, Units::Metric).expect("mass");
    let grams = cup.min.expect("min");
    assert_eq!(grams.denominator, 1, "snapped to whole grams: {grams:?}");
    let drop = Quantity {
        min: Some(Rational::new(1, 10)),
        unit: Some(UnitCode("mL".to_owned())),
        ..Quantity::default()
    };
    let tiny = convert_with_density(&drop, water, Units::Metric).expect("tiny");
    assert_eq!(
        tiny.min,
        Some(Rational::new(1, 10)),
        "kept, not snapped to zero"
    );
}

#[test]
fn density_does_not_apply_to_counts_time_or_bad_densities() {
    let water = Rational::new(1, 1);
    assert!(convert_with_density(&amount(2, "{clove}"), water, Units::Metric).is_none());
    assert!(convert_with_density(&amount(2, "min"), water, Units::Metric).is_none());
    assert!(convert_with_density(&Quantity::default(), water, Units::Metric).is_none());
    assert!(convert_with_density(&amount(2, "g"), Rational::new(0, 1), Units::Metric).is_none());
    assert!(convert_with_density(&amount(2, "g"), Rational::new(-1, 1), Units::Metric).is_none());
}
