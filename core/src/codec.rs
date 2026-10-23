//! Conversion between the data model and canonical CBOR bytes.
//!
//! This is the contract boundary: model values become canonical CBOR through
//! [`to_canonical_cbor`], and untrusted bytes become model values through
//! [`from_canonical_cbor`], which first enforces canonical form (via
//! [`crate::cbor`]) and then the closed schema. A schema mismatch — a missing
//! field, a wrong type, or an unknown core field — is reported as
//! [`crate::ErrorCode::Schema`].

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::cbor::{Value, decode_canonical, decode_canonical_file, encode_canonical};
use crate::error::{CrispError, ErrorCode, Result};
use crate::limits::{MAX_BUNDLE_SIZE, enforce};
use crate::model::envelope::CrispEnvelope;

/// The RFC 8949 §3.4.6 self-describe tag wrapping every `.crisp` file.
const SELF_DESCRIBE_TAG: u64 = 55_799;
/// The Crisp application tag (a placeholder pending IANA registration).
const CRISP_APP_TAG: u64 = 1_129_468_752;

/// Builds an [`ErrorCode::Schema`] error.
fn schema(detail: impl Into<String>) -> CrispError {
    CrispError::check(ErrorCode::Schema, detail)
}

/// Serializes a model value to canonical CBOR bytes.
///
/// # Errors
///
/// Returns [`ErrorCode::Schema`] if serialization fails, or a `E.CBOR.*` /
/// `E.LIMIT` error if the value cannot be canonically encoded.
pub fn to_canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = Value::serialized(value)
        .map_err(|e| schema(format!("could not serialize to CBOR: {e}")))?;
    encode_canonical(&value)
}

/// Rejects `null` anywhere in `value`: Crisp structures have no nullable
/// fields, and refusing `null` keeps decoding lossless (an explicit `null`
/// would otherwise decode as an absent field and re-encode differently).
fn reject_null(value: &Value) -> Result<()> {
    match value {
        Value::Null => Err(schema("null is not permitted")),
        Value::Array(items) => items.iter().try_for_each(reject_null),
        Value::Map(entries) => entries
            .iter()
            .try_for_each(|(k, v)| reject_null(k).and_then(|()| reject_null(v))),
        Value::Tag(_, inner) => reject_null(inner),
        _ => Ok(()),
    }
}

/// Decodes canonical CBOR bytes into a model value, enforcing the schema.
///
/// # Errors
///
/// Returns a `E.CBOR.*` error if the bytes are not canonical CBOR, or
/// [`ErrorCode::Schema`] if they contain `null` or do not match `T`'s schema.
pub fn from_canonical_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let value = decode_canonical(bytes)?;
    reject_null(&value)?;
    value
        .deserialized()
        .map_err(|e| schema(format!("does not match the schema: {e}")))
}

/// Encodes a [`CrispEnvelope`] as the bytes of a `.crisp` file.
///
/// The envelope is wrapped in the Crisp application tag and the CBOR
/// self-describe tag, as required by `crisp-v1.cddl`'s `crisp-file`.
///
/// # Errors
///
/// Returns a `E.SCHEMA` / `E.CBOR.*` error if encoding fails.
pub fn encode_file(envelope: &CrispEnvelope) -> Result<Vec<u8>> {
    let envelope = Value::serialized(envelope)
        .map_err(|e| schema(format!("could not serialize the envelope: {e}")))?;
    let tagged = Value::Tag(
        SELF_DESCRIBE_TAG,
        Box::new(Value::Tag(CRISP_APP_TAG, Box::new(envelope))),
    );
    encode_canonical(&tagged)
}

/// Decodes the bytes of a `.crisp` file into a [`CrispEnvelope`].
///
/// Enforces the maximum bundle size, canonical CBOR, the two framing tags,
/// and the envelope schema.
///
/// # Errors
///
/// Returns [`ErrorCode::Limit`] if the file is too large, [`ErrorCode::Magic`]
/// if the framing tags are absent, or a `E.CBOR.*` / `E.SCHEMA` error for a
/// malformed or off-schema envelope.
pub fn decode_file(bytes: &[u8]) -> Result<CrispEnvelope> {
    enforce(bytes.len(), MAX_BUNDLE_SIZE, "bundle size")?;
    let value = decode_canonical_file(bytes)?;

    let inner = match value {
        Value::Tag(SELF_DESCRIBE_TAG, inner) => match *inner {
            Value::Tag(CRISP_APP_TAG, envelope) => *envelope,
            _ => {
                return Err(CrispError::check(
                    ErrorCode::Magic,
                    "missing the Crisp application tag",
                ));
            }
        },
        _ => {
            return Err(CrispError::check(
                ErrorCode::Magic,
                "missing the CBOR self-describe tag",
            ));
        }
    };
    inner
        .deserialized()
        .map_err(|e| schema(format!("envelope does not match the schema: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_payload;
    use crate::model::envelope::{
        CoseSign, ProtectedHeader, SigAlg, SigProtected, SigRole, Signature,
    };
    use crate::model::ids::DocId;
    use crate::model::payload::BundlePayload;
    use crate::model::primitives::Timestamp;

    #[test]
    fn bundle_payload_round_trips() {
        let payload = sample_payload();
        let bytes = to_canonical_cbor(&payload).expect("encode");
        let decoded: BundlePayload = from_canonical_cbor(&bytes).expect("decode");
        assert_eq!(payload, decoded);
        // Encoding is canonical and idempotent.
        assert_eq!(bytes, to_canonical_cbor(&decoded).expect("re-encode"));
    }

    #[test]
    fn protected_header_round_trips_with_integer_keys() {
        let header = ProtectedHeader {
            format_version: 0x01_0000,
            doc_id: DocId("rcp-placeholder".to_owned()),
            content_hash: vec![0xabu8; 32],
        };
        let bytes = to_canonical_cbor(&header).expect("encode");
        // Integer key 1 is encoded as the CBOR uint 0x01.
        assert_eq!(bytes[1], 0x01);
        let decoded: ProtectedHeader = from_canonical_cbor(&bytes).expect("decode");
        assert_eq!(header, decoded);
    }

    #[test]
    fn sig_protected_round_trips() {
        let sig = SigProtected {
            alg: SigAlg::MlDsa65Ed25519,
            role: SigRole::Author,
            kid: vec![0u8; 32],
            signed_at: Timestamp(1_700_000_000),
            binds: None,
        };
        let bytes = to_canonical_cbor(&sig).expect("encode");
        let decoded: SigProtected = from_canonical_cbor(&bytes).expect("decode");
        assert_eq!(sig, decoded);
    }

    #[test]
    fn crisp_file_round_trips_through_the_framing_tags() {
        let envelope = CrispEnvelope {
            container_version: 1,
            signed_bundle: CoseSign {
                protected: vec![0xa0],
                unprotected: Value::Map(vec![]),
                payload: to_canonical_cbor(&sample_payload()).expect("payload"),
                signatures: vec![Signature {
                    protected: vec![0xa0],
                    unprotected: Value::Map(vec![]),
                    sig_value: vec![9u8; 64],
                }],
            },
        };
        let bytes = encode_file(&envelope).expect("encode file");
        // The file begins with the self-describe tag D9 D9 F7.
        assert_eq!(&bytes[..3], &[0xd9, 0xd9, 0xf7]);
        let decoded = decode_file(&bytes).expect("decode file");
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn decoding_rejects_a_schema_mismatch() {
        // A bare integer is canonical CBOR but not a BundlePayload.
        let bytes = to_canonical_cbor(&42u64).expect("encode");
        let err = from_canonical_cbor::<BundlePayload>(&bytes).expect_err("must reject");
        assert_eq!(err.code(), Some(ErrorCode::Schema));
    }

    #[test]
    fn decoding_a_file_without_the_framing_tags_is_rejected() {
        let bytes = to_canonical_cbor(&42u64).expect("encode");
        let err = decode_file(&bytes).expect_err("must reject");
        assert_eq!(err.code(), Some(ErrorCode::Magic));
    }
}
