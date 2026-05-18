//! Strict, deterministic CBOR decoding.
//!
//! [`scan_canonical`] is a recursive-descent validator over the raw bytes: it
//! confirms the input is a single, well-formed item in canonical form and is
//! the authority for the precise `E.CBOR.*` codes. [`decode_canonical`] runs
//! the scan and then materializes a [`Value`] with `ciborium`.

use ciborium::value::Value;

use crate::cbor::ALLOWED_TAGS;
use crate::error::{CrispError, ErrorCode, Result};
use crate::limits::MAX_CBOR_DEPTH;

/// `i64::MAX` as a `u64`, for range-checking decoded CBOR integers.
const I64_MAX_AS_U64: u64 = i64::MAX as u64;

fn malformed(detail: impl Into<String>) -> CrispError {
    CrispError::check(ErrorCode::CborMalformed, detail)
}

fn non_deterministic(detail: impl Into<String>) -> CrispError {
    CrispError::check(ErrorCode::CborNonDeterministic, detail)
}

/// Decodes a single canonical CBOR item from `input`.
///
/// The input MUST be exactly one item in CBOR Core Deterministic Encoding
/// (RFC 8949 §4.2.1) — see [`scan_canonical`] for the enforced rules.
///
/// # Errors
///
/// Returns a [`CrispError::Check`] carrying the appropriate `E.CBOR.*` code if
/// the input is malformed or not in canonical form.
pub fn decode_canonical(input: &[u8]) -> Result<Value> {
    scan_canonical(input)?;
    // The scan has proven `input` is one well-formed, canonical item, so the
    // permissive `ciborium` decoder cannot disagree; map any surprise to
    // `E.CBOR.MALFORMED` rather than panic.
    ciborium::de::from_reader(input).map_err(|e| malformed(format!("CBOR decode failed: {e}")))
}

/// Validates that `input` is exactly one canonical CBOR item.
///
/// Rejects, with the precise code: indefinite-length items
/// ([`ErrorCode::CborIndefinite`]); non-shortest arguments, unsorted map keys,
/// and floating-point values ([`ErrorCode::CborNonDeterministic`]); duplicate
/// map keys ([`ErrorCode::CborDupKey`]); tags outside [`ALLOWED_TAGS`]
/// ([`ErrorCode::CborTag`]); nesting beyond [`MAX_CBOR_DEPTH`]
/// ([`ErrorCode::Limit`]); integers outside the signed 64-bit range
/// ([`ErrorCode::Limit`]); and anything not well-formed or with trailing bytes
/// ([`ErrorCode::CborMalformed`]).
///
/// # Errors
///
/// Returns a [`CrispError::Check`] with the code described above.
pub fn scan_canonical(input: &[u8]) -> Result<()> {
    let mut scanner = Scanner { buf: input, pos: 0 };
    scanner.item(0)?;
    if scanner.pos != input.len() {
        return Err(malformed(format!(
            "{} trailing byte(s) after the top-level item",
            input.len() - scanner.pos
        )));
    }
    Ok(())
}

/// A cursor over the bytes being validated.
struct Scanner<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Scanner<'_> {
    /// Consumes and returns the next `n` bytes.
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| malformed("length overflow"))?;
        if end > self.buf.len() {
            return Err(malformed("unexpected end of input"));
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Consumes and returns the next single byte.
    fn next_byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    /// Reads the argument encoded by additional-information value `ai`,
    /// enforcing shortest-form. The caller must handle `ai == 31` first.
    fn argument(&mut self, ai: u8) -> Result<u64> {
        match ai {
            0..=23 => Ok(u64::from(ai)),
            24 => {
                let value = u64::from(self.next_byte()?);
                if value < 24 {
                    return Err(non_deterministic("1-byte argument could be inlined"));
                }
                Ok(value)
            }
            25 => {
                let mut arr = [0u8; 2];
                arr.copy_from_slice(self.take(2)?);
                let value = u64::from(u16::from_be_bytes(arr));
                if value <= u64::from(u8::MAX) {
                    return Err(non_deterministic("2-byte argument is not shortest-form"));
                }
                Ok(value)
            }
            26 => {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(self.take(4)?);
                let value = u64::from(u32::from_be_bytes(arr));
                if value <= u64::from(u16::MAX) {
                    return Err(non_deterministic("4-byte argument is not shortest-form"));
                }
                Ok(value)
            }
            27 => {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(self.take(8)?);
                let value = u64::from_be_bytes(arr);
                if value <= u64::from(u32::MAX) {
                    return Err(non_deterministic("8-byte argument is not shortest-form"));
                }
                Ok(value)
            }
            28..=30 => Err(malformed("reserved additional-information value")),
            31 => Err(malformed("unexpected indefinite-length marker")),
            _ => Err(malformed("invalid additional-information value")),
        }
    }

    /// Validates one CBOR data item at the given nesting `depth`.
    fn item(&mut self, depth: usize) -> Result<()> {
        if depth > MAX_CBOR_DEPTH {
            return Err(CrispError::check(
                ErrorCode::Limit,
                format!("CBOR nesting exceeds the maximum depth of {MAX_CBOR_DEPTH}"),
            ));
        }
        let initial = self.next_byte()?;
        let major = initial >> 5;
        let ai = initial & 0x1F;
        match major {
            // Major 0/1 — unsigned and negative integers.
            0 | 1 => {
                let argument = self.argument(ai)?;
                if argument > I64_MAX_AS_U64 {
                    return Err(CrispError::check(
                        ErrorCode::Limit,
                        "integer is outside the signed 64-bit range",
                    ));
                }
            }
            // Major 2/3 — byte and text strings.
            2 | 3 => {
                if ai == 31 {
                    return Err(CrispError::check(
                        ErrorCode::CborIndefinite,
                        "indefinite-length string",
                    ));
                }
                let len = usize::try_from(self.argument(ai)?)
                    .map_err(|_| malformed("string length overflow"))?;
                let bytes = self.take(len)?;
                if major == 3 && core::str::from_utf8(bytes).is_err() {
                    return Err(malformed("text string is not valid UTF-8"));
                }
            }
            // Major 4 — arrays.
            4 => {
                if ai == 31 {
                    return Err(CrispError::check(
                        ErrorCode::CborIndefinite,
                        "indefinite-length array",
                    ));
                }
                let count = self.argument(ai)?;
                for _ in 0..count {
                    self.item(depth + 1)?;
                }
            }
            // Major 5 — maps, with sorted, unique keys.
            5 => {
                if ai == 31 {
                    return Err(CrispError::check(
                        ErrorCode::CborIndefinite,
                        "indefinite-length map",
                    ));
                }
                let count = self.argument(ai)?;
                let mut previous_key: Option<core::ops::Range<usize>> = None;
                for _ in 0..count {
                    let key_start = self.pos;
                    self.item(depth + 1)?;
                    let key = key_start..self.pos;
                    self.item(depth + 1)?;
                    if let Some(prev) = previous_key {
                        match self.buf[prev].cmp(&self.buf[key.clone()]) {
                            core::cmp::Ordering::Greater => {
                                return Err(non_deterministic("map keys are not sorted"));
                            }
                            core::cmp::Ordering::Equal => {
                                return Err(CrispError::check(
                                    ErrorCode::CborDupKey,
                                    "duplicate map key",
                                ));
                            }
                            core::cmp::Ordering::Less => {}
                        }
                    }
                    previous_key = Some(key);
                }
            }
            // Major 6 — tags.
            6 => {
                if ai == 31 {
                    return Err(malformed("indefinite-length tag"));
                }
                let tag = self.argument(ai)?;
                if !ALLOWED_TAGS.contains(&tag) {
                    return Err(CrispError::check(
                        ErrorCode::CborTag,
                        format!("CBOR tag {tag} is not permitted"),
                    ));
                }
                self.item(depth + 1)?;
            }
            // Major 7 — simple values and floats.
            7 => match ai {
                // false, true, null, undefined.
                20..=23 => {}
                // One-byte simple value; values below 32 must use the inline form.
                24 => {
                    let simple = self.next_byte()?;
                    if simple < 32 {
                        return Err(malformed("invalid one-byte simple value"));
                    }
                }
                // Half-, single-, and double-precision floats.
                25..=27 => {
                    return Err(non_deterministic("floating-point values are not permitted"));
                }
                28..=30 => return Err(malformed("reserved additional-information value")),
                31 => return Err(malformed("unexpected break code")),
                // Inline simple values 0..=19.
                _ => {}
            },
            _ => return Err(malformed("invalid CBOR major type")),
        }
        Ok(())
    }
}
