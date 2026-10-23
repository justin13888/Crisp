//! Converting amounts and temperatures between measurement systems, snapped
//! to values a cook can measure or set on a dial.

use crate::error::{CrispError, Result};
use crate::model::primitives::{Quantity, Rational, UnitCode};
use crate::model::recipe::{Temperature, TemperatureTarget, TemperatureUnit};
use crate::render::number::{compare, divide, multiply, reduce, round_to, to_f64};
use crate::render::quantity::{Units, snap_step};
use crate::render::units::{UnitInfo, UnitKind, UnitSystem, lookup};

/// Picks the unit a converted amount reads best in.
fn target_unit(kind: UnitKind, system: Units, base: Rational) -> Option<&'static UnitInfo> {
    let v = to_f64(base).abs();
    let code = match (kind, system) {
        (UnitKind::Mass, Units::Metric) => {
            if v >= 1000.0 {
                "kg"
            } else {
                "g"
            }
        }
        (UnitKind::Mass, Units::Us) => {
            if v >= 453.0 {
                "[lb_av]"
            } else {
                "[oz_av]"
            }
        }
        (UnitKind::Volume, Units::Metric) => {
            if v >= 1000.0 {
                "L"
            } else {
                "mL"
            }
        }
        (UnitKind::Volume, Units::Us) => {
            if v < 14.0 {
                "[tsp_us]"
            } else if v < 59.0 {
                "[tbsp_us]"
            } else {
                "[cup_us]"
            }
        }
        (UnitKind::Length, Units::Metric) => "cm",
        (UnitKind::Length, Units::Us) => "[in_i]",
        _ => return None,
    };
    lookup(code)
}

/// Converts `value` in `from` to the best unit of `system`, snapping it to a
/// value a cook can measure.
fn convert_value(
    value: Rational,
    from: &UnitInfo,
    system: Units,
) -> Result<(Rational, &'static UnitInfo)> {
    let base = multiply(value, from.factor)?;
    let to = target_unit(from.kind, system, base)
        .ok_or_else(|| CrispError::input("unit cannot be converted"))?;
    let raw = divide(base, to.factor)?;
    let snapped = round_to(raw, snap_step(Some(to), raw))?;
    Ok((if snapped.numerator == 0 { raw } else { snapped }, to))
}

/// Converts a quantity to a measurement system. Amounts already in that
/// system, or in units that do not convert (counts, time, annotations), come
/// back unchanged; converted amounts are marked approximate when rounding
/// moved them.
#[must_use]
pub fn convert_quantity(quantity: &Quantity, system: Units) -> Quantity {
    let Some(from) = quantity.unit.as_ref().and_then(|u| lookup(&u.0)) else {
        return quantity.clone();
    };
    let wanted = match system {
        Units::Original => return quantity.clone(),
        Units::Metric => UnitSystem::Metric,
        Units::Us => UnitSystem::Us,
    };
    if from.system == wanted
        || !matches!(
            from.kind,
            UnitKind::Mass | UnitKind::Volume | UnitKind::Length
        )
    {
        return quantity.clone();
    }
    let Some(min) = quantity.min else {
        return quantity.clone();
    };
    let Ok((new_min, to)) = convert_value(min, from, system) else {
        return quantity.clone();
    };
    let new_max = quantity.max.and_then(|max| {
        let base = multiply(max, from.factor).ok()?;
        let raw = divide(base, to.factor).ok()?;
        round_to(raw, snap_step(Some(to), raw)).ok()
    });
    let exact_min = multiply(min, from.factor)
        .and_then(|b| divide(b, to.factor))
        .ok();
    let moved = exact_min.is_none_or(|exact| compare(exact, new_min).is_ne());
    Quantity {
        min: Some(new_min),
        max: new_max,
        unit: Some(UnitCode(to.code.to_owned())),
        approx: if moved || quantity.approx == Some(true) {
            Some(true)
        } else {
            None
        },
        text: quantity.text.clone(),
    }
}

/// Converts one temperature value between units, rounding to a dial-friendly
/// step (5° for ovens and anything above 100°, else 1°).
fn convert_degrees(
    value: Rational,
    from: TemperatureUnit,
    to: TemperatureUnit,
    target: Option<TemperatureTarget>,
) -> Result<Rational> {
    if from == to {
        return Ok(value);
    }
    let celsius = match from {
        TemperatureUnit::Celsius => value,
        TemperatureUnit::Fahrenheit => multiply(
            reduce(
                i128::from(value.numerator) - 32 * i128::from(value.denominator),
                u128::from(value.denominator),
            )?,
            Rational::new(5, 9),
        )?,
        TemperatureUnit::Kelvin => reduce(
            i128::from(value.numerator) * 20 - 5463 * i128::from(value.denominator),
            u128::from(value.denominator) * 20,
        )?,
    };
    let converted = match to {
        TemperatureUnit::Celsius => celsius,
        TemperatureUnit::Fahrenheit => {
            let scaled = multiply(celsius, Rational::new(9, 5))?;
            reduce(
                i128::from(scaled.numerator) + 32 * i128::from(scaled.denominator),
                u128::from(scaled.denominator),
            )?
        }
        TemperatureUnit::Kelvin => reduce(
            i128::from(celsius.numerator) * 20 + 5463 * i128::from(celsius.denominator),
            u128::from(celsius.denominator) * 20,
        )?,
    };
    let step = if target == Some(TemperatureTarget::Oven) || to_f64(converted) > 100.0 {
        Rational::new(5, 1)
    } else {
        Rational::new(1, 1)
    };
    round_to(converted, step)
}

/// Converts a step temperature to a measurement system.
#[must_use]
pub fn convert_temperature(temperature: &Temperature, system: Units) -> Temperature {
    let to = match system {
        Units::Original => return temperature.clone(),
        Units::Metric => TemperatureUnit::Celsius,
        Units::Us => TemperatureUnit::Fahrenheit,
    };
    if temperature.unit == to {
        return temperature.clone();
    }
    let convert = |v| convert_degrees(v, temperature.unit, to, temperature.target);
    match (
        convert(temperature.min),
        temperature.max.map(convert).transpose(),
    ) {
        (Ok(min), Ok(max)) => Temperature {
            min,
            max,
            unit: to,
            target: temperature.target,
        },
        _ => temperature.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::primitives::LanguageTag;
    use crate::render::quantity::{format_quantity, format_temperature};

    fn en() -> LanguageTag {
        LanguageTag("en".to_owned())
    }

    fn q(n: i64, d: u64, unit: &str) -> Quantity {
        Quantity {
            min: Some(Rational::new(n, d)),
            unit: Some(UnitCode(unit.to_owned())),
            ..Quantity::default()
        }
    }

    #[test]
    fn converts_between_systems() {
        let cup = convert_quantity(&q(1, 1, "[cup_us]"), Units::Metric);
        assert_eq!(cup.unit.as_ref().map(|u| u.0.as_str()), Some("mL"));
        assert_eq!(format_quantity(&cup, &en()), "~235 mL");
        let grams = convert_quantity(&q(454, 1, "g"), Units::Us);
        assert_eq!(format_quantity(&grams, &en()), "~1 lb");
        let tsp = convert_quantity(&q(5, 1, "mL"), Units::Us);
        assert_eq!(format_quantity(&tsp, &en()), "~1 tsp");
        // Counts never convert.
        assert_eq!(
            convert_quantity(&q(2, 1, "{egg}"), Units::Us),
            q(2, 1, "{egg}")
        );
    }

    #[test]
    fn converts_temperatures_to_dial_steps() {
        let oven = Temperature {
            min: Rational::new(180, 1),
            max: None,
            unit: TemperatureUnit::Celsius,
            target: Some(TemperatureTarget::Oven),
        };
        let f = convert_temperature(&oven, Units::Us);
        assert_eq!(format_temperature(&f), "355°F");
        let probe = Temperature {
            min: Rational::new(145, 1),
            max: None,
            unit: TemperatureUnit::Fahrenheit,
            target: Some(TemperatureTarget::Internal),
        };
        assert_eq!(
            format_temperature(&convert_temperature(&probe, Units::Metric)),
            "63°C"
        );
    }
}
