//! Typed-in values (amounts, durations, temperatures, closed vocabularies)
//! converted to format values and back, through `crisp-core`'s own parsers
//! so the draft reads exactly like the web app's editor.

use crisp_core::model::primitives::{LanguageTag, OpenCode, Quantity};
use crisp_core::model::recipe::{HeatLevel, Temperature, TemperatureTarget, TemperatureUnit};
use crisp_core::render::parse::quantity_to_input;
use crisp_core::render::quantity::parse_temperature;
use crisp_core::render::{parse_amount, parse_duration};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Parses an amount such as `1 1/2 cups` or `to taste`.
pub(super) fn quantity(input: &str, lang: &LanguageTag) -> Result<Quantity, String> {
    parse_amount(input, lang)
        .map(|parsed| parsed.quantity)
        .map_err(|e| format!("{input:?} is not an amount: {e}"))
}

/// Writes an amount back as typed text.
pub(super) fn quantity_text(quantity: &Quantity, lang: &LanguageTag) -> String {
    quantity_to_input(quantity, lang)
}

/// Parses a duration such as `20 min` or `1 h 30 min`.
pub(super) fn duration(input: &str) -> Result<Quantity, String> {
    parse_duration(input).map_err(|e| format!("{input:?} is not a duration: {e}"))
}

/// Parses a temperature, which must name its unit.
pub(super) fn temperature(
    value: &str,
    target: Option<&str>,
    lang: &LanguageTag,
) -> Result<Temperature, String> {
    let parsed = parse_temperature(value, lang).map_err(|e| e.to_string())?;
    let unit = parsed
        .unit
        .ok_or_else(|| format!("{value:?} needs a unit, e.g. \"{value} °C\""))?;
    let target = target
        .map(|t| closed::<TemperatureTarget>(t, "oven, internal, oil, pan, or water"))
        .transpose()?;
    Ok(Temperature {
        min: parsed.min,
        max: parsed.max,
        unit,
        target,
    })
}

/// Writes a temperature's value back as typed text.
pub(super) fn temperature_text(temperature: &Temperature, lang: &LanguageTag) -> String {
    let symbol = match temperature.unit {
        TemperatureUnit::Celsius => "°C",
        TemperatureUnit::Fahrenheit => "°F",
        TemperatureUnit::Kelvin => "K",
    };
    let number = quantity_to_input(
        &Quantity {
            min: Some(temperature.min),
            max: temperature.max,
            ..Quantity::default()
        },
        lang,
    );
    format!("{number} {symbol}")
}

/// Parses a burner setting.
pub(super) fn heat(input: &str) -> Result<HeatLevel, String> {
    closed(input, "low, medium-low, medium, medium-high, or high")
}

/// A closed-vocabulary value's wire string.
pub(super) fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Parses a closed-vocabulary string such as `medium-high` or `oven`.
fn closed<T: DeserializeOwned>(input: &str, expected: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(input.trim().to_owned()))
        .map_err(|_| format!("{input:?} is not one of {expected}"))
}

/// Parses an open-vocabulary code: a registry code, or any other slug
/// (whose shape the format's own checks judge).
pub(super) fn open_code<T: DeserializeOwned>(input: &str) -> OpenCode<T> {
    serde_json::from_value(serde_json::Value::String(input.trim().to_owned()))
        .unwrap_or_else(|_| OpenCode::Other(input.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crisp_core::model::primitives::{Rational, UnitCode};
    use crisp_core::model::recipe::CourseCode;

    fn en() -> LanguageTag {
        LanguageTag("en".into())
    }

    #[test]
    fn amounts_round_trip_through_typed_text() {
        for typed in [
            "1 1/2 cup",
            "2-3 tbsp",
            "200 g",
            "~1 tsp",
            "3 {cloves}",
            "to taste",
            "4",
        ] {
            let parsed = quantity(typed, &en()).expect(typed);
            let again = quantity(&quantity_text(&parsed, &en()), &en()).expect("reparses");
            assert_eq!(parsed, again, "{typed}");
        }
        assert_eq!(
            quantity("200 g", &en()).expect("g").unit,
            Some(UnitCode("g".into()))
        );
        assert!(
            quantity("", &en())
                .expect_err("empty")
                .contains("not an amount")
        );
    }

    #[test]
    fn durations_round_trip() {
        for typed in ["20 min", "1 h 30 min", "10-15 min", "2 h"] {
            let parsed = duration(typed).expect(typed);
            let again = duration(&quantity_text(&parsed, &en())).expect("reparses");
            assert_eq!(parsed, again, "{typed}");
        }
        assert!(
            duration("soon")
                .expect_err("bad")
                .contains("not a duration")
        );
    }

    #[test]
    fn temperatures_need_a_unit_and_round_trip() {
        let oven = temperature("220 °C", Some("oven"), &en()).expect("parses");
        assert_eq!(oven.min, Rational::new(220, 1));
        assert_eq!(oven.unit, TemperatureUnit::Celsius);
        assert_eq!(oven.target, Some(TemperatureTarget::Oven));
        for t in [
            &oven,
            &temperature("350-375 F", None, &en()).expect("range"),
            &temperature("300 K", None, &en()).expect("k"),
        ] {
            let again = temperature(&temperature_text(t, &en()), None, &en()).expect("reparses");
            assert_eq!((again.min, again.max, again.unit), (t.min, t.max, t.unit));
        }
        assert!(
            temperature("220", None, &en())
                .expect_err("unitless")
                .contains("needs a unit")
        );
        assert!(temperature("hot", None, &en()).is_err());
        assert!(
            temperature("220 °C", Some("grill"), &en())
                .expect_err("target")
                .contains("oven")
        );
    }

    #[test]
    fn closed_and_open_vocabularies() {
        assert_eq!(heat(" medium-high ").expect("heat"), HeatLevel::MediumHigh);
        assert!(heat("scorching").expect_err("bad").contains("medium-high"));
        assert_eq!(wire(&HeatLevel::MediumLow), "medium-low");
        assert_eq!(wire(&TemperatureTarget::Internal), "internal");
        assert_eq!(
            open_code::<CourseCode>("main"),
            OpenCode::Known(CourseCode::Main)
        );
        assert_eq!(
            open_code::<CourseCode>("brunch"),
            OpenCode::Other("brunch".into())
        );
    }
}
