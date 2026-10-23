//! Quantities for people: display strings for amounts, durations, and
//! temperatures. Conversion between measurement systems lives in
//! [`crate::render::convert`] and parsing typed amounts in
//! [`crate::render::parse`]; both are re-exported here.

use serde::{Deserialize, Serialize};

use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::{Temperature, TemperatureUnit};
use crate::model::text::LocalizedText;
use crate::render::number::{
    NumberStyle, compare, format_number, is_exactly_writable, multiply, round_to, to_f64,
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
    let approx = quantity.approx == Some(true) || min_rounded || max_rounded;
    let mut out = String::new();
    if approx {
        out.push('~');
    }
    out.push_str(&format_number(min, style));
    if let Some(max) = max {
        out.push('–');
        out.push_str(&format_number(max, style));
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
    match temperature.max {
        Some(max) => format!("{min}–{}{symbol}", format_number(max, NumberStyle::Decimal)),
        None => format!("{min}{symbol}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::primitives::UnitCode;

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
}
