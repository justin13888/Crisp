//! Strict, deterministic CBOR encoding (canonical form, RFC 8949 §4.2.1).
//!
//! Encoding canonicalizes the [`Value`] tree — sorting every map's entries by
//! the encoded bytes of their keys and rejecting floats, disallowed tags, and
//! out-of-range integers — and then serializes it with `ciborium`, which emits
//! definite-length, shortest-form items.

use ciborium::value::Value;

use crate::cbor::ALLOWED_TAGS;
use crate::error::{CrispError, ErrorCode, Result};

/// Encodes `value` as canonical CBOR.
///
/// # Errors
///
/// Returns a [`CrispError::Check`] if `value` contains a floating-point number
/// ([`ErrorCode::CborNonDeterministic`]), a tag outside [`ALLOWED_TAGS`]
/// ([`ErrorCode::CborTag`]), duplicate map keys ([`ErrorCode::CborDupKey`]), or
/// an integer outside the signed 64-bit range ([`ErrorCode::Limit`]).
pub fn encode_canonical(value: &Value) -> Result<Vec<u8>> {
    let canonical = canonicalize(value)?;
    encode_raw(&canonical)
}

/// Serializes an already-canonical [`Value`] to bytes.
fn encode_raw(value: &Value) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf)
        .map_err(|e| CrispError::input(format!("CBOR encoding failed: {e}")))?;
    Ok(buf)
}

/// Returns a deep clone of `value` in canonical form, or an error if it holds
/// content the Crisp format forbids.
fn canonicalize(value: &Value) -> Result<Value> {
    match value {
        Value::Integer(integer) => {
            let n: i128 = (*integer).into();
            if n < i128::from(i64::MIN) || n > i128::from(i64::MAX) {
                return Err(CrispError::check(
                    ErrorCode::Limit,
                    "integer is outside the signed 64-bit range",
                ));
            }
            Ok(value.clone())
        }
        Value::Float(_) => Err(CrispError::check(
            ErrorCode::CborNonDeterministic,
            "floating-point values are not permitted",
        )),
        Value::Bytes(_) | Value::Text(_) | Value::Bool(_) | Value::Null => Ok(value.clone()),
        Value::Tag(tag, inner) => {
            if !ALLOWED_TAGS.contains(tag) {
                return Err(CrispError::check(
                    ErrorCode::CborTag,
                    format!("CBOR tag {tag} is not permitted"),
                ));
            }
            Ok(Value::Tag(*tag, Box::new(canonicalize(inner)?)))
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(canonicalize(item)?);
            }
            Ok(Value::Array(out))
        }
        Value::Map(entries) => {
            // Canonicalize each entry, then sort by the encoded bytes of the key.
            let mut sortable: Vec<(Vec<u8>, Value, Value)> = Vec::with_capacity(entries.len());
            for (key, val) in entries {
                let canonical_key = canonicalize(key)?;
                let canonical_val = canonicalize(val)?;
                let key_bytes = encode_raw(&canonical_key)?;
                sortable.push((key_bytes, canonical_key, canonical_val));
            }
            sortable.sort_by(|a, b| a.0.cmp(&b.0));
            for pair in sortable.windows(2) {
                if pair[0].0 == pair[1].0 {
                    return Err(CrispError::check(
                        ErrorCode::CborDupKey,
                        "duplicate map key",
                    ));
                }
            }
            Ok(Value::Map(
                sortable.into_iter().map(|(_, k, v)| (k, v)).collect(),
            ))
        }
        // `ciborium::value::Value` is `#[non_exhaustive]`; reject anything new.
        _ => Err(CrispError::input("unsupported CBOR value variant")),
    }
}
