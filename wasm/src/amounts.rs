//! Amounts for JavaScript: parsing what a cook types and formatting exact
//! numbers and quantities, so an editor never reimplements either.

use crisp_core::model::primitives::{LanguageTag, Quantity, Rational};
use crisp_core::render::number::{NumberStyle, format_number};
use crisp_core::render::quantity::{format_quantity, parse_temperature, quantity_to_input};
use crisp_core::render::{parse_amount, parse_duration};
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
