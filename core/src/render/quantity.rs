//! Quantities for people: display strings for amounts, durations, and
//! temperatures, and reading a typed temperature. Conversion between measurement systems lives in
//! [`crate::render::convert`] and parsing typed amounts in
//! [`crate::render::parse`]; both are re-exported here.

use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::error::{CrispError, Result};
use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::{Temperature, TemperatureUnit};
use crate::model::text::LocalizedText;
use crate::render::number::{
    BELOW_HUNDREDTH, NumberStyle, compare, format_number, is_exactly_writable, multiply, round_to,
    to_f64,
};
use crate::render::resolve_text;
use crate::render::units::{UnitInfo, UnitKind, UnitSystem, label, lookup};

pub use crate::render::convert::{convert_quantity, convert_temperature};
pub use crate::render::parse::{ParsedAmount, parse_amount, parse_duration, quantity_to_input};

/// Which measurement system a reader wants amounts shown in.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    /// As the author wrote them.
    #[default]
    Original,
    /// Metric (g, mL, °C).
    Metric,
    /// US customary (oz, cups, °F).
    Us,
}

/// The number style a unit reads best in.
fn style_for(unit: Option<&str>) -> NumberStyle {
    match unit.and_then(lookup) {
        Some(u) if u.system == UnitSystem::Metric => NumberStyle::Decimal,
        _ => NumberStyle::Fraction,
    }
}

/// The step a value is snapped to when it cannot be written exactly.
pub(crate) fn snap_step(unit: Option<&UnitInfo>, value: Rational) -> Rational {
    match unit {
        Some(u) if u.system == UnitSystem::Metric => {
            let v = to_f64(value).abs();
            if v >= 100.0 {
                Rational::new(5, 1)
            } else if v >= 10.0 {
                Rational::new(1, 1)
            } else if v >= 1.0 {
                Rational::new(1, 10)
            } else {
                Rational::new(1, 100)
            }
        }
        Some(u) if matches!(u.code, "[oz_av]" | "[lb_av]") => Rational::new(1, 4),
        _ => Rational::new(1, 8),
    }
}

/// Snaps `value` to something writable, reporting whether it changed.
fn writable(value: Rational, unit: Option<&str>) -> (Rational, bool) {
    if is_exactly_writable(value, style_for(unit)) {
        return (value, false);
    }
    let step = snap_step(unit.and_then(lookup), value);
    match round_to(value, step) {
        Ok(snapped) if snapped.numerator != 0 => (snapped, true),
        _ => (value, true),
    }
}

/// The text of `text` in `lang`, else in any language, else empty.
pub(crate) fn pick_text<'a>(text: &'a LocalizedText, lang: &LanguageTag) -> &'a str {
    resolve_text(text, &[], lang).unwrap_or_default()
}

/// Formats a quantity for display, e.g. `"1½ cups"`, `"2–3 tbsp"`,
/// `"~200 g"`, or `"to taste"`. `lang` picks the text of non-numeric
/// amounts.
#[must_use]
pub fn format_quantity(quantity: &Quantity, lang: &LanguageTag) -> String {
    let Some(min) = quantity.min else {
        return quantity
            .text
            .as_ref()
            .map(|t| pick_text(t, lang).to_owned())
            .unwrap_or_default();
    };
    let unit = quantity.unit.as_ref().map(|u| u.0.as_str());
    if unit
        .and_then(lookup)
        .is_some_and(|u| u.kind == UnitKind::Time)
    {
        return format_duration_quantity(quantity).unwrap_or_default();
    }
    let style = style_for(unit);
    let (min, min_rounded) = writable(min, unit);
    let (max, max_rounded) = quantity.max.map_or((None, false), |m| {
        let (v, r) = writable(m, unit);
        (Some(v), r)
    });
    let min_text = format_number(min, style);
    let max_text = distinct_upper(&min_text, max.map(|m| format_number(m, style)));
    let below = min_text == BELOW_HUNDREDTH;
    // `<0.01` already says the low end is not exact, so its own rounding needs
    // no tilde, and neither does a lone `<0.01` (`~<0.01` would say it twice).
    // A range that is stated approximate, or whose high end was rounded,
    // keeps its tilde: `<` qualifies only the low end.
    let approx = quantity.approx == Some(true) || max_rounded || (min_rounded && !below);
    let mut out = String::new();
    if approx && !(below && max_text.is_none()) {
        out.push('~');
    }
    out.push_str(&min_text);
    if let Some(max_text) = max_text {
        out.push('–');
        out.push_str(&max_text);
    }
    if let Some(code) = unit {
        let largest = max.unwrap_or(min);
        let plural = compare(largest, Rational::new(1, 1)).is_gt();
        let temperature = lookup(code).is_some_and(|u| u.kind == UnitKind::Temperature);
        if !temperature {
            out.push(' ');
        }
        out.push_str(&label(code, plural));
    }
    if let Some(text) = &quantity.text {
        out.push_str(" (");
        out.push_str(pick_text(text, lang));
        out.push(')');
    }
    out
}

/// A duration in whole seconds, rounding fractions of a second up.
fn seconds_of(value: Rational, unit: &UnitInfo) -> Option<u64> {
    let secs = multiply(value, unit.factor).ok()?;
    let n = u64::try_from(secs.numerator).ok()?;
    Some(n.div_ceil(secs.denominator.max(1)))
}

/// Returns `(min, max)` seconds for a duration quantity in a time unit.
#[must_use]
pub fn duration_seconds(quantity: &Quantity) -> Option<(u64, Option<u64>)> {
    let unit = quantity
        .unit
        .as_ref()
        .and_then(|u| lookup(&u.0))
        .filter(|u| u.kind == UnitKind::Time)?;
    let min = seconds_of(quantity.min?, unit)?;
    let max = match quantity.max {
        Some(max) => Some(seconds_of(max, unit)?),
        None => None,
    };
    Some((min, max))
}

/// Formats a number of seconds compactly: `"45 sec"`, `"25 min"`,
/// `"1 hr 30 min"`, `"2 days 3 hr"`.
///
/// Precision coarsens with size: once there is a whole day, minutes and
/// seconds are dropped; once there is a whole hour, seconds are dropped.
/// Zero is `"0 sec"`.
#[must_use]
pub fn format_seconds(total: u64) -> String {
    let days = total / 86_400;
    let hours = total % 86_400 / 3600;
    let minutes = total % 3600 / 60;
    let seconds = total % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} {}", if days == 1 { "day" } else { "days" }));
    }
    if hours > 0 {
        parts.push(format!("{hours} hr"));
    }
    if minutes > 0 && days == 0 {
        parts.push(format!("{minutes} min"));
    }
    if seconds > 0 && days == 0 && hours == 0 {
        parts.push(format!("{seconds} sec"));
    }
    if parts.is_empty() {
        "0 sec".to_owned()
    } else {
        parts.join(" ")
    }
}

/// Formats a duration quantity, e.g. `"20–25 min"` or `"1 hr 30 min"`.
#[must_use]
pub fn format_duration_quantity(quantity: &Quantity) -> Option<String> {
    let (min, max) = duration_seconds(quantity)?;
    let prefix = if quantity.approx == Some(true) {
        "~"
    } else {
        ""
    };
    Some(match max {
        Some(max) if max != min => {
            if max < 3600 && min % 60 == 0 && max % 60 == 0 {
                format!("{prefix}{}–{} min", min / 60, max / 60)
            } else {
                format!("{prefix}{} – {}", format_seconds(min), format_seconds(max))
            }
        }
        _ => format!("{prefix}{}", format_seconds(min)),
    })
}

/// Formats a temperature, e.g. `"180°C"` or `"350–375°F"`.
#[must_use]
pub fn format_temperature(temperature: &Temperature) -> String {
    let symbol = match temperature.unit {
        TemperatureUnit::Celsius => "°C",
        TemperatureUnit::Fahrenheit => "°F",
        TemperatureUnit::Kelvin => " K",
    };
    let min = format_number(temperature.min, NumberStyle::Decimal);
    let max = temperature
        .max
        .map(|m| format_number(m, NumberStyle::Decimal));
    match distinct_upper(&min, max) {
        Some(max) => format!("{min}–{max}{symbol}"),
        None => format!("{min}{symbol}"),
    }
}

/// The upper bound's text of a range, or `None` when both ends read
/// [`BELOW_HUNDREDTH`]: `<0.01–<0.01` says nothing a lone `<0.01` does not.
fn distinct_upper(min_text: &str, max_text: Option<String>) -> Option<String> {
    max_text.filter(|m| !(min_text == BELOW_HUNDREDTH && m == BELOW_HUNDREDTH))
}

/// A temperature a cook typed: `"180"`, `"180 °C"`, `"350-375 F"`.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ParsedTemperature {
    /// The temperature, or the lower bound of a range.
    #[cfg_attr(feature = "ts", tsify(type = "Rational"))]
    pub min: Rational,
    /// The upper bound of a range.
    #[cfg_attr(feature = "ts", tsify(type = "Rational"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Rational>,
    /// The unit, when one was typed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<TemperatureUnit>,
}

/// Parses a typed temperature with [`parse_amount`]: a number or range, then
/// an optional temperature unit (`°C`, `F`, `kelvin`, ...).
///
/// # Errors
///
/// Returns [`CrispError::Input`] for empty input, text that is not a number,
/// an approximate (`~`) value, or a unit that is not a temperature unit.
pub fn parse_temperature(input: &str, lang: &LanguageTag) -> Result<ParsedTemperature> {
    let not_a_temperature =
        || CrispError::input(format!("{:?} is not a temperature", input.trim()));
    let quantity = parse_amount(input, lang)?.quantity;
    let (Some(min), None, None) = (quantity.min, &quantity.approx, &quantity.text) else {
        return Err(not_a_temperature());
    };
    let unit = match &quantity.unit {
        None => None,
        Some(code) => Some(
            TemperatureUnit::deserialize(code.0.as_str().into_deserializer())
                .map_err(|_: serde::de::value::Error| not_a_temperature())?,
        ),
    };
    Ok(ParsedTemperature {
        min,
        max: quantity.max,
        unit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::primitives::UnitCode;

    #[test]
    fn temperatures_parse_with_and_without_a_unit() {
        let parsed = |text| parse_temperature(text, &en());
        assert_eq!(
            parsed("180").unwrap(),
            ParsedTemperature {
                min: Rational::new(180, 1),
                max: None,
                unit: None,
            }
        );
        assert_eq!(
            parsed("180 °C").unwrap().unit,
            Some(TemperatureUnit::Celsius)
        );
        let range = parsed("350-375 F").unwrap();
        assert_eq!(range.min, Rational::new(350, 1));
        assert_eq!(range.max, Some(Rational::new(375, 1)));
        assert_eq!(range.unit, Some(TemperatureUnit::Fahrenheit));
        assert_eq!(
            parsed("300 kelvin").unwrap().unit,
            Some(TemperatureUnit::Kelvin)
        );
        assert_eq!(parsed("1.5").unwrap().min, Rational::new(3, 2));
    }

    #[test]
    fn non_temperatures_are_refused() {
        for text in ["", "hot", "180 g", "2 cups", "~180", "180 cloves"] {
            assert!(
                parse_temperature(text, &en()).is_err(),
                "{text:?} should not parse as a temperature"
            );
        }
    }

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
    fn formats_common_amounts() {
        assert_eq!(format_quantity(&q(3, 2, "[cup_us]"), &en()), "1½ cups");
        assert_eq!(format_quantity(&q(1, 2, "[cup_us]"), &en()), "½ cup");
        assert_eq!(format_quantity(&q(200, 1, "g"), &en()), "200 g");
        assert_eq!(format_quantity(&q(3, 1, "{clove}"), &en()), "3 clove");
        assert_eq!(format_quantity(&q(7, 12, "[cup_us]"), &en()), "~⅝ cup");
        assert_eq!(format_quantity(&q(90, 1, "min"), &en()), "1 hr 30 min");
        let mut range = q(2, 1, "[tbsp_us]");
        range.max = Some(Rational::new(3, 1));
        assert_eq!(format_quantity(&range, &en()), "2–3 tbsp");
        let text = Quantity {
            text: Some(LocalizedText(
                [(en(), "to taste".to_owned())].into_iter().collect(),
            )),
            ..Quantity::default()
        };
        assert_eq!(format_quantity(&text, &en()), "to taste");
    }

    #[test]
    fn tiny_amounts_read_below_a_hundredth_without_a_tilde() {
        assert_eq!(format_quantity(&q(1, 1000, "[tsp_us]"), &en()), "<0.01 tsp");
        assert_eq!(format_quantity(&q(1, 1000, "g"), &en()), "<0.01 g");
        assert_eq!(format_quantity(&q(-1, 1000, "g"), &en()), "<0.01 g");
        let mut stated = q(1, 1000, "g");
        stated.approx = Some(true);
        assert_eq!(format_quantity(&stated, &en()), "<0.01 g");
        let mut range = q(1, 1000, "[tsp_us]");
        range.max = Some(Rational::new(1, 1));
        assert_eq!(format_quantity(&range, &en()), "<0.01–1 tsp");
        // An approximate range keeps its tilde: `<` covers only the low end.
        let mut about_range = q(1, 1000, "[tsp_us]");
        about_range.max = Some(Rational::new(2, 7));
        assert_eq!(format_quantity(&about_range, &en()), "~<0.01–¼ tsp");
        about_range.max = Some(Rational::new(1, 4));
        about_range.approx = Some(true);
        assert_eq!(format_quantity(&about_range, &en()), "~<0.01–¼ tsp");
        // A range with both ends tiny collapses to a single `<0.01`.
        let mut tiny_range = q(1, 1000, "g");
        tiny_range.max = Some(Rational::new(2, 1000));
        assert_eq!(format_quantity(&tiny_range, &en()), "<0.01 g");
        tiny_range.approx = Some(true);
        assert_eq!(format_quantity(&tiny_range, &en()), "<0.01 g");
        // A stated approximation that is not tiny keeps its tilde.
        let mut about = q(2, 1, "g");
        about.approx = Some(true);
        assert_eq!(format_quantity(&about, &en()), "~2 g");
    }

    #[test]
    fn tiny_temperatures_read_below_a_hundredth() {
        let t = |min: Rational, max: Option<Rational>| Temperature {
            min,
            max,
            unit: TemperatureUnit::Celsius,
            target: None,
        };
        assert_eq!(
            format_temperature(&t(Rational::new(1, 1000), None)),
            "<0.01°C"
        );
        assert_eq!(
            format_temperature(&t(Rational::new(-1, 1000), None)),
            "<0.01°C"
        );
        assert_eq!(format_temperature(&t(Rational::new(0, 1), None)), "0°C");
        assert_eq!(
            format_temperature(&t(Rational::new(-1, 1000), Some(Rational::new(4, 1)))),
            "<0.01–4°C"
        );
        assert_eq!(
            format_temperature(&t(Rational::new(1, 1000), Some(Rational::new(2, 1000)))),
            "<0.01°C"
        );
    }
}
