//! Crossing the JS boundary: one serializer configuration and one error
//! shape for every export.

use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsValue;

/// Serializes a value for JavaScript: maps become plain objects, `None`
/// becomes `null`, byte strings become `Uint8Array`.
pub(crate) fn to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, JsValue> {
    let serializer = serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true);
    value
        .serialize(&serializer)
        .map_err(|e| failure(&format!("could not serialize a result: {e}"), None, None))
}

/// Deserializes a JavaScript value, reporting the path of a mismatch.
pub(crate) fn from_js<T: DeserializeOwned>(value: JsValue, what: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| failure(&format!("invalid {what}: {e}"), None, None))
}

/// Builds a JS `Error` carrying optional `code` and `status` properties.
pub(crate) fn failure(message: &str, code: Option<&str>, status: Option<u16>) -> JsValue {
    let error = js_sys::Error::new(message);
    error.set_name("CrispFailure");
    if let Some(code) = code {
        let _ = js_sys::Reflect::set(&error, &"code".into(), &code.into());
    }
    if let Some(status) = status {
        let _ = js_sys::Reflect::set(&error, &"status".into(), &status.into());
    }
    error.into()
}

/// Converts a core error into a JS failure, keeping its stable code.
pub(crate) fn core_error(error: &crisp_core::CrispError) -> JsValue {
    let message = match error {
        crisp_core::CrispError::Check { detail, .. } => detail.clone(),
        other => other.to_string(),
    };
    failure(&message, error.code().map(|c| c.as_str()), None)
}

/// `?`-friendly conversion of core results.
pub(crate) trait OrJs<T> {
    /// Maps the error into a JS failure.
    fn or_js(self) -> Result<T, JsValue>;
}

impl<T> OrJs<T> for crisp_core::Result<T> {
    fn or_js(self) -> Result<T, JsValue> {
        self.map_err(|e| core_error(&e))
    }
}

/// Decodes lowercase or uppercase hex, or `None` for odd-length or non-hex
/// text.
///
/// Each byte is read with `u8::from_str_radix`, which also accepts a leading
/// `+` within a two-character pair (`"+f"` decodes to `0x0f`). That leniency
/// is kept on purpose: it admits only a non-canonical spelling of the same
/// byte (`"+f"` and `"0f"` decode alike), never a different value, so the
/// nonces and platform keys read through it are unaffected.
pub(crate) fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Encodes bytes as lowercase hex.
pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads a JS epoch-seconds number as a timestamp, refusing non-integers.
pub(crate) fn timestamp(seconds: f64) -> Result<crisp_core::model::primitives::Timestamp, JsValue> {
    if seconds.is_finite() && seconds >= 0.0 && seconds.fract() == 0.0 && seconds < 9.0e15 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Ok(crisp_core::model::primitives::Timestamp(seconds as u64))
    } else {
        Err(failure(
            "a timestamp must be whole epoch seconds",
            None,
            None,
        ))
    }
}
