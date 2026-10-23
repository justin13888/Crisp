//! Quantities for people: display strings, metric/US conversion, and
//! parsing what a cook types ("1 1/2 cups", "2–3 tbsp", "1h 30m").

use serde::{Deserialize, Serialize};

use crate::error::{CrispError, Result};
use crate::model::primitives::{LanguageTag, Quantity, Rational, UnitCode};
use crate::model::recipe::{Temperature, TemperatureTarget, TemperatureUnit};
use crate::model::text::LocalizedText;
use crate::render::number::{
    NumberStyle, compare, divide, format_number, is_exactly_writable, multiply, reduce, round_to,
    to_f64,
};
use crate::render::units::{UnitInfo, UnitKind, UnitSystem, label, lookup, resolve_alias};

/// Which measurement system a reader wants amounts shown in.
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
fn snap_step(unit: Option<&UnitInfo>, value: Rational) -> Rational {
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

/// Picks the best text from a localized map.
fn pick_text<'a>(text: &'a LocalizedText, lang: &LanguageTag) -> &'a str {
    text.0
        .get(lang)
        .or_else(|| text.0.values().next())
        .map_or("", String::as_str)
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

// ---- parsing ----------------------------------------------------------------

/// The result of parsing a typed amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedAmount {
    /// The parsed quantity.
    pub quantity: Quantity,
    /// Whether the unit came from the catalogue (`false` for annotations
    /// like `{clove}` or when there is no unit).
    pub unit_known: bool,
}

/// Parses a single number: `3`, `1.5`, `1,5`, `1/2`, `½`, `1½`, `1 1/2`.
/// Returns the value and the unparsed remainder.
fn parse_number(input: &str) -> Option<(Rational, &str)> {
    let s = input.trim_start();
    let digits_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (whole, rest) = s.split_at(digits_end);
    let glyph = |rest: &str| -> Option<(Rational, usize)> {
        let c = rest.chars().next()?;
        let (n, d) = match c {
            '½' => (1, 2),
            '⅓' => (1, 3),
            '⅔' => (2, 3),
            '¼' => (1, 4),
            '¾' => (3, 4),
            '⅕' => (1, 5),
            '⅖' => (2, 5),
            '⅗' => (3, 5),
            '⅘' => (4, 5),
            '⅙' => (1, 6),
            '⅚' => (5, 6),
            '⅛' => (1, 8),
            '⅜' => (3, 8),
            '⅝' => (5, 8),
            '⅞' => (7, 8),
            _ => return None,
        };
        Some((Rational::new(n, d), c.len_utf8()))
    };

    if whole.is_empty() {
        let (value, len) = glyph(s)?;
        return Some((value, &s[len..]));
    }
    let w: i64 = whole.parse().ok()?;
    // 1½
    if let Some((frac, len)) = glyph(rest) {
        let value = reduce(
            i128::from(w) * i128::from(frac.denominator) + i128::from(frac.numerator),
            u128::from(frac.denominator),
        )
        .ok()?;
        return Some((value, &rest[len..]));
    }
    // 1.5 / 1,5
    if let Some(after) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(',')) {
        let frac_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if frac_end > 0 && frac_end <= 6 {
            let (frac, rest) = after.split_at(frac_end);
            let scale = 10i128.pow(u32::try_from(frac_end).ok()?);
            let f: i128 = frac.parse().ok()?;
            let value = reduce(i128::from(w) * scale + f, u128::try_from(scale).ok()?).ok()?;
            return Some((value, rest));
        }
    }
    // 1/2
    if let Some(after) = rest.strip_prefix('/') {
        let den_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        let d: u64 = after[..den_end].parse().ok()?;
        if d == 0 {
            return None;
        }
        return Some((
            reduce(i128::from(w), u128::from(d)).ok()?,
            &after[den_end..],
        ));
    }
    // 1 1/2 or 1 ½
    let trimmed = rest.trim_start();
    if trimmed.len() < rest.len() {
        if let Some((frac, len)) = glyph(trimmed) {
            let value = reduce(
                i128::from(w) * i128::from(frac.denominator) + i128::from(frac.numerator),
                u128::from(frac.denominator),
            )
            .ok()?;
            return Some((value, &trimmed[len..]));
        }
        let num_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        if num_end > 0
            && let Some(after) = trimmed[num_end..].strip_prefix('/')
        {
            let n: i64 = trimmed[..num_end].parse().ok()?;
            let den_end = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            if let Ok(d) = after[..den_end].parse::<u64>()
                && d > 0
                && n < i64::try_from(d).unwrap_or(i64::MAX)
            {
                let value =
                    reduce(i128::from(w) * i128::from(d) + i128::from(n), u128::from(d)).ok()?;
                return Some((value, &after[den_end..]));
            }
        }
    }
    Some((Rational::new(w, 1), rest))
}

/// Strips a leading "about" marker, reporting whether one was present.
fn strip_approx(input: &str) -> (&str, bool) {
    let s = input.trim_start();
    for marker in ["~", "≈", "about ", "approx. ", "approx ", "ca. ", "around "] {
        if let Some(rest) = s.strip_prefix(marker) {
            return (rest, true);
        }
    }
    (s, false)
}

/// Strips a range separator (`-`, `–`, `—`, `to`) from the start of `input`.
fn strip_range(input: &str) -> Option<&str> {
    let s = input.trim_start();
    for sep in ["-", "–", "—", "to "] {
        if let Some(rest) = s.strip_prefix(sep) {
            return Some(rest);
        }
    }
    None
}

/// Parses an amount a cook typed: a number or range, then an optional unit.
///
/// Anything that does not start with a number becomes a text amount in
/// `lang` ("to taste"). An unrecognized unit word becomes a UCUM annotation
/// (`3 cloves` → `{cloves}`).
///
/// # Errors
///
/// Returns [`CrispError::Input`] for empty input or an unparseable unit.
pub fn parse_amount(input: &str, lang: &LanguageTag) -> Result<ParsedAmount> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CrispError::input("the amount is empty"));
    }
    let (body, approx) = strip_approx(trimmed);
    let Some((min, rest)) = parse_number(body) else {
        let text = LocalizedText([(lang.clone(), trimmed.to_owned())].into_iter().collect());
        return Ok(ParsedAmount {
            quantity: Quantity {
                text: Some(text),
                ..Quantity::default()
            },
            unit_known: false,
        });
    };
    let (max, rest) = match strip_range(rest).and_then(parse_number) {
        Some((max, rest)) => (Some(max), rest),
        None => (None, rest),
    };
    let unit_word = rest.trim();
    let (unit, unit_known) = if unit_word.is_empty() {
        (None, false)
    } else if let Some(unit) = resolve_alias(unit_word) {
        (Some(unit.code.to_owned()), true)
    } else if unit_word
        .chars()
        .all(|c| c.is_alphabetic() || c == ' ' || c == '-')
        && unit_word.len() <= 62
    {
        (Some(format!("{{{}}}", unit_word.to_lowercase())), false)
    } else {
        return Err(CrispError::input(format!(
            "unrecognized unit {unit_word:?}"
        )));
    };
    Ok(ParsedAmount {
        quantity: Quantity {
            min: Some(min),
            max,
            unit: unit.map(UnitCode),
            approx: approx.then_some(true),
            text: None,
        },
        unit_known,
    })
}

/// Parses a duration: `90` (minutes), `90 min`, `1.5 h`, `1h 30m`,
/// `1 hr 30 min`, `1:30` (h:mm), or a range like `20-25 min`.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if the text is not a duration.
pub fn parse_duration(input: &str) -> Result<Quantity> {
    let bad = || CrispError::input(format!("{:?} is not a duration", input.trim()));
    let s = input.trim();
    if s.is_empty() {
        return Err(bad());
    }
    // h:mm or h:mm:ss
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let nums: Option<Vec<u64>> = parts.iter().map(|p| p.trim().parse().ok()).collect();
        let nums = nums.ok_or_else(bad)?;
        let seconds = match nums.as_slice() {
            [h, m] if *m < 60 => h * 3600 + m * 60,
            [h, m, sec] if *m < 60 && *sec < 60 => h * 3600 + m * 60 + sec,
            _ => return Err(bad()),
        };
        return Ok(seconds_quantity(seconds));
    }
    // Bare number => minutes.
    if let Some((value, rest)) = parse_number(s)
        && rest.trim().is_empty()
    {
        return Ok(Quantity {
            min: Some(value),
            unit: Some(UnitCode("min".to_owned())),
            ..Quantity::default()
        });
    }
    // A single amount (possibly a range) in one time unit.
    if let Ok(parsed) = parse_amount(s, &LanguageTag("und".to_owned()))
        && parsed
            .quantity
            .unit
            .as_ref()
            .and_then(|u| lookup(&u.0))
            .is_some_and(|u| u.kind == UnitKind::Time)
    {
        return Ok(parsed.quantity);
    }
    // A sum of number+unit pairs: "1h 30m".
    let mut rest = s;
    let mut total = Rational::new(0, 1);
    let mut pairs = 0;
    while !rest.trim().is_empty() {
        let (value, after) = parse_number(rest).ok_or_else(bad)?;
        let after = after.trim_start();
        let word_end = after
            .find(|c: char| c.is_ascii_digit() || c.is_whitespace())
            .unwrap_or(after.len());
        let unit = resolve_alias(&after[..word_end])
            .filter(|u| u.kind == UnitKind::Time)
            .ok_or_else(bad)?;
        total = crate::render::number::add(total, multiply(value, unit.factor)?)?;
        rest = &after[word_end..];
        pairs += 1;
    }
    if pairs == 0 {
        return Err(bad());
    }
    let seconds = u64::try_from(total.numerator)
        .map_err(|_| bad())?
        .div_ceil(total.denominator.max(1));
    Ok(seconds_quantity(seconds))
}

/// Expresses whole seconds in the largest unit that keeps them exact.
fn seconds_quantity(seconds: u64) -> Quantity {
    let (value, unit) = if seconds.is_multiple_of(60) {
        (seconds / 60, "min")
    } else {
        (seconds, "s")
    };
    Quantity {
        min: Some(Rational::new(i64::try_from(value).unwrap_or(i64::MAX), 1)),
        unit: Some(UnitCode(unit.to_owned())),
        ..Quantity::default()
    }
}

/// Writes a quantity back as editable ASCII-friendly text, the inverse of
/// [`parse_amount`] (`"1 1/2 cup"`, `"2-3 tbsp"`, `"to taste"`).
#[must_use]
pub fn quantity_to_input(quantity: &Quantity, lang: &LanguageTag) -> String {
    let Some(min) = quantity.min else {
        return quantity
            .text
            .as_ref()
            .map(|t| pick_text(t, lang).to_owned())
            .unwrap_or_default();
    };
    let number = |r: Rational| {
        let whole = r.numerator / i64::try_from(r.denominator).unwrap_or(1).max(1);
        let rem = r.numerator - whole * i64::try_from(r.denominator).unwrap_or(1);
        if rem == 0 {
            whole.to_string()
        } else if whole == 0 {
            format!("{rem}/{}", r.denominator)
        } else {
            format!("{whole} {}/{}", rem.abs(), r.denominator)
        }
    };
    let mut out = String::new();
    if quantity.approx == Some(true) {
        out.push('~');
    }
    out.push_str(&number(min));
    if let Some(max) = quantity.max {
        out.push('-');
        out.push_str(&number(max));
    }
    if let Some(unit) = &quantity.unit {
        out.push(' ');
        match lookup(&unit.0) {
            Some(info) => out.push_str(info.aliases.first().copied().unwrap_or(info.code)),
            None => out.push_str(&label(&unit.0, true)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parses_what_cooks_type() {
        let p = |s: &str| parse_amount(s, &en()).expect(s).quantity;
        assert_eq!(p("1 1/2 cups"), q(3, 2, "[cup_us]"));
        assert_eq!(p("1½ cups"), q(3, 2, "[cup_us]"));
        assert_eq!(p("½ tsp"), q(1, 2, "[tsp_us]"));
        assert_eq!(p("1.5 kg"), q(3, 2, "kg"));
        assert_eq!(p("1,5 L"), q(3, 2, "L"));
        assert_eq!(p("3 cloves"), q(3, 1, "{cloves}"));
        let range = p("2-3 Tbsp");
        assert_eq!(range.min, Some(Rational::new(2, 1)));
        assert_eq!(range.max, Some(Rational::new(3, 1)));
        assert_eq!(p("~200 g").approx, Some(true));
        assert!(p("to taste").text.is_some());
        assert_eq!(p("2").unit, None);
        assert!(parse_amount("", &en()).is_err());
        assert!(parse_amount("2 @#!", &en()).is_err());
    }

    #[test]
    fn parses_durations() {
        let mins = |n: i64| q(n, 1, "min");
        assert_eq!(parse_duration("90").unwrap(), mins(90));
        assert_eq!(parse_duration("1:30").unwrap(), mins(90));
        assert_eq!(parse_duration("1h 30m").unwrap(), mins(90));
        assert_eq!(parse_duration("1 hr 30 min").unwrap(), mins(90));
        assert_eq!(parse_duration("45 sec").unwrap(), q(45, 1, "s"));
        assert_eq!(parse_duration("1.5 h").unwrap(), q(3, 2, "h"));
        let range = parse_duration("20-25 min").unwrap();
        assert_eq!(range.max, Some(Rational::new(25, 1)));
        assert!(parse_duration("soon").is_err());
        assert_eq!(duration_seconds(&q(3, 2, "h")), Some((5400, None)));
    }

    #[test]
    fn input_text_round_trips_through_the_parser() {
        for text in [
            "1 1/2 cups",
            "2-3 tbsp",
            "200 g",
            "3 cloves",
            "1/2 tsp",
            "~2 cups",
        ] {
            let quantity = parse_amount(text, &en()).unwrap().quantity;
            let back = quantity_to_input(&quantity, &en());
            assert_eq!(
                parse_amount(&back, &en()).unwrap().quantity,
                quantity,
                "{text} -> {back}"
            );
        }
    }
}
