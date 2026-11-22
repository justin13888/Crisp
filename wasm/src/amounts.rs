//! Amounts for JavaScript: parsing what a cook types and formatting exact
//! numbers and quantities, so an editor never reimplements either.

use crisp_core::ingredients::density_measure;
use crisp_core::model::primitives::{LanguageTag, Quantity, Rational};
use crisp_core::render::number::{NumberStyle, format_number};
use crisp_core::render::quantity::{format_quantity, parse_temperature, quantity_to_input};
use crisp_core::render::{Units, parse_amount, parse_duration};
use wasm_bindgen::prelude::*;

use crate::boundary::{OrJs, from_js, to_js};

/// Parses a typed amount (`"1 1/2 cups"`, `"to taste"`) in `lang`.
///
/// # Errors
///
/// Throws if the text is empty or its unit is unparseable.
#[wasm_bindgen(js_name = parseAmount, unchecked_return_type = "ParsedAmount")]
pub fn parse_amount_js(text: &str, lang: &str) -> Result<JsValue, JsValue> {
    to_js(&parse_amount(text, &LanguageTag(lang.to_owned())).or_js()?)
}

/// Parses a typed duration (`"90"`, `"1h 30m"`, `"20-25 min"`).
///
/// # Errors
///
/// Throws if the text is not a duration.
#[wasm_bindgen(js_name = parseDuration, unchecked_return_type = "Quantity")]
pub fn parse_duration_js(text: &str) -> Result<JsValue, JsValue> {
    to_js(&parse_duration(text).or_js()?)
}

/// Parses a typed temperature (`"180"`, `"180 °C"`, `"350-375 F"`) in
/// `lang`.
///
/// # Errors
///
/// Throws if the text is not a number or range with an optional
/// temperature unit.
#[wasm_bindgen(js_name = parseTemperature, unchecked_return_type = "ParsedTemperature")]
pub fn parse_temperature_js(text: &str, lang: &str) -> Result<JsValue, JsValue> {
    to_js(&parse_temperature(text, &LanguageTag(lang.to_owned())).or_js()?)
}

/// Formats an exact number for a cook (`"1½"` as a fraction, `"1.5"` as a
/// decimal).
///
/// # Errors
///
/// Throws if `value` is not a rational or `style` is not a number style.
#[wasm_bindgen(js_name = formatNumber)]
pub fn format_number_js(
    #[wasm_bindgen(unchecked_param_type = "Rational")] value: JsValue,
    #[wasm_bindgen(unchecked_param_type = "NumberStyle")] style: JsValue,
) -> Result<String, JsValue> {
    let value: Rational = from_js(value, "number")?;
    let style: NumberStyle = from_js(style, "number style")?;
    Ok(format_number(value, style))
}

/// Formats a quantity for display (`"1½ cups"`).
///
/// # Errors
///
/// Throws if the quantity is malformed.
#[wasm_bindgen(js_name = formatQuantity)]
pub fn format_quantity_js(
    #[wasm_bindgen(unchecked_param_type = "Quantity")] quantity: JsValue,
    lang: &str,
) -> Result<String, JsValue> {
    let quantity: Quantity = from_js(quantity, "quantity")?;
    Ok(format_quantity(&quantity, &LanguageTag(lang.to_owned())))
}

/// The other measure an ingredient-index density derives from a typed
/// amount, as editable text in metric (`"250 mL"` of `flour` → `"~150 g"`),
/// for an editor to offer as an `alt` the author then owns. `undefined` when
/// the amount does not parse, is neither a mass nor a volume, or `code` names
/// no entry with a density.
#[wasm_bindgen(js_name = densityAlternative)]
#[must_use]
pub fn density_alternative_js(amount: &str, code: &str, lang: &str) -> Option<String> {
    let lang = LanguageTag(lang.to_owned());
    let parsed = parse_amount(amount, &lang).ok()?;
    let derived = density_measure(&parsed.quantity, code, Units::Metric)?;
    Some(quantity_to_input(&derived, &lang))
}

/// Writes a quantity back as editable text (`"1 1/2 cups"`).
///
/// # Errors
///
/// Throws if the quantity is malformed.
#[wasm_bindgen(js_name = quantityToInput)]
pub fn quantity_to_input_js(
    #[wasm_bindgen(unchecked_param_type = "Quantity")] quantity: JsValue,
    lang: &str,
) -> Result<String, JsValue> {
    let quantity: Quantity = from_js(quantity, "quantity")?;
    Ok(quantity_to_input(&quantity, &LanguageTag(lang.to_owned())))
}

// Native, not under Node: `density_alternative_js` crosses no `JsValue`, so
// `cargo test` (and `cargo mutants`) reaches it here.
#[cfg(test)]
mod tests {
    use super::density_alternative_js;

    #[test]
    fn a_density_alternative_is_offered_only_for_a_known_density() {
        // `flour` carries 3/5 g/mL in the published index.
        let grams = density_alternative_js("250 mL", "flour", "en").expect("flour");
        assert_eq!(grams, "~150 g");
        // A mass derives a volume.
        let volume = density_alternative_js("250 g", "flour", "en").expect("volume");
        assert_eq!(volume, "~415 ml");
        assert_eq!(density_alternative_js("2 cloves", "flour", "en"), None);
        assert_eq!(density_alternative_js("250 mL", "abalone", "en"), None);
        assert_eq!(density_alternative_js("", "flour", "en"), None);
    }
}
