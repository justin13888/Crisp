//! Deterministic canonical CBOR (RFC 8949 §4.2.1).
//!
//! Reproducible signatures and stable identities depend on *one* input
//! producing *exactly one* byte string. This module is the load-bearing
//! guarantee of that property. It is built on the [`ciborium`] crate — a
//! mature, well-maintained CBOR implementation — which supplies the
//! [`Value`] model and the byte-level serializer.
//!
//! `ciborium`'s decoder is intentionally permissive (it accepts non-canonical
//! input), so canonical form is enforced here by two cooperating pieces:
//!
//! - [`scan_canonical`] — a single-pass, recursive-descent validator over the
//!   raw bytes. It is the authority for the precise `E.CBOR.*` error codes:
//!   it rejects indefinite-length items, non-shortest arguments, unsorted or
//!   duplicated map keys, floating-point values, disallowed tags, and
//!   over-deep nesting.
//! - [`encode_canonical`] — produces canonical bytes by canonicalizing the
//!   [`Value`] tree (sorting map keys by their encoded bytes, rejecting
//!   floats) before serializing.
//!
//! The crate invariant, exercised by the tests, is that
//! `scan_canonical(encode_canonical(v))` always succeeds and that decoding is
//! the exact inverse of encoding for any canonical input.

mod decoder;
mod encoder;

pub use ciborium::value::Value;
pub use decoder::{decode_canonical, decode_canonical_file, scan_canonical, scan_canonical_file};
pub use encoder::encode_canonical;

/// The CBOR tags of a `.crisp` file's framing, outermost first: the RFC 8949
/// §3.4.6 self-describe tag (`55799`) and the Crisp application tag
/// (`1129468752`). They are permitted only there; any other tag, or these
/// anywhere else, is rejected with [`crate::ErrorCode::CborTag`].
pub const ALLOWED_TAGS: [u64; 2] = [55_799, 1_129_468_752];

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ErrorCode;

    fn code(bytes: &[u8]) -> ErrorCode {
        scan_canonical(bytes)
            .expect_err("expected non-canonical input to be rejected")
            .code()
            .expect("a CBOR scan failure must carry a stable code")
    }

    fn sample_values() -> Vec<Value> {
        vec![
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            Value::Integer(0.into()),
            Value::Integer(23.into()),
            Value::Integer(24.into()),
            Value::Integer(255.into()),
            Value::Integer(256.into()),
            Value::Integer(65_535.into()),
            Value::Integer(65_536.into()),
            Value::Integer(i64::MAX.into()),
            Value::Integer((-1).into()),
            Value::Integer(i64::MIN.into()),
            Value::Bytes(vec![]),
            Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
            Value::Text(String::new()),
            Value::Text("crisp".to_owned()),
            Value::Array(vec![]),
            Value::Array(vec![
                Value::Integer(1.into()),
                Value::Text("two".to_owned()),
            ]),
            Value::Map(vec![
                (Value::Text("b".to_owned()), Value::Integer(2.into())),
                (Value::Text("a".to_owned()), Value::Integer(1.into())),
            ]),
        ]
    }

    #[test]
    fn encode_then_decode_round_trips() {
        for v in sample_values() {
            let bytes = encode_canonical(&v).expect("encode");
            let decoded = decode_canonical(&bytes).expect("decode");
            let reencoded = encode_canonical(&decoded).expect("re-encode");
            assert_eq!(bytes, reencoded, "encoding must be idempotent for {v:?}");
        }
    }

    #[test]
    fn canonical_encoder_output_passes_the_scanner() {
        for v in sample_values() {
            let bytes = encode_canonical(&v).expect("encode");
            scan_canonical(&bytes)
                .unwrap_or_else(|e| panic!("encoder produced non-canonical bytes for {v:?}: {e}"));
        }
    }

    /// A scalar the Crisp format permits anywhere, map keys included.
    fn scalar() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Integer(n.into())),
            prop::collection::vec(any::<u8>(), 0..24).prop_map(Value::Bytes),
            any::<String>().prop_map(Value::Text),
        ]
    }

    /// An arbitrary tree of permitted values. Map keys are deduplicated by
    /// their encoding, so every generated tree is encodable.
    fn value_tree() -> impl Strategy<Value = Value> {
        scalar().prop_recursive(4, 64, 6, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                prop::collection::vec((scalar(), inner), 0..6).prop_map(|entries| {
                    let mut seen = std::collections::BTreeSet::new();
                    Value::Map(
                        entries
                            .into_iter()
                            .filter(|(k, _)| seen.insert(encode_canonical(k).expect("scalar")))
                            .collect(),
                    )
                }),
            ]
        })
    }

    proptest! {
        #[test]
        fn encoding_is_canonical_and_decoding_inverts_it(v in value_tree()) {
            let bytes = encode_canonical(&v).expect("every generated tree encodes");
            prop_assert!(scan_canonical(&bytes).is_ok(), "the scanner accepts encoder output");
            let decoded = decode_canonical(&bytes).expect("decode");
            prop_assert_eq!(&encode_canonical(&decoded).expect("re-encode"), &bytes);
            // The decoded tree is the canonical fixed point.
            let again = decode_canonical(&encode_canonical(&decoded).expect("encode"));
            prop_assert_eq!(again.expect("decode"), decoded);
        }

        #[test]
        fn integers_beyond_the_signed_range_are_refused(n in (i64::MAX as u64 + 1)..=u64::MAX) {
            let err = encode_canonical(&Value::Integer(n.into())).expect_err("out of range");
            prop_assert_eq!(err.code(), Some(ErrorCode::Limit));
        }
    }

    #[test]
    fn encoder_sorts_map_keys() {
        // Keys supplied out of order must come back sorted by encoded bytes.
        let unsorted = Value::Map(vec![
            (Value::Text("zzz".to_owned()), Value::Null),
            (Value::Text("a".to_owned()), Value::Null),
        ]);
        let bytes = encode_canonical(&unsorted).expect("encode");
        // a2  61 61 f6  63 7a 7a 7a f6  — "a" precedes the longer "zzz".
        assert_eq!(
            bytes,
            [0xa2, 0x61, 0x61, 0xf6, 0x63, 0x7a, 0x7a, 0x7a, 0xf6]
        );
    }

    #[test]
    fn rejects_indefinite_length_array() {
        assert_eq!(code(&[0x9f, 0x01, 0xff]), ErrorCode::CborIndefinite);
    }

    #[test]
    fn rejects_indefinite_length_map() {
        assert_eq!(code(&[0xbf, 0x01, 0x01, 0xff]), ErrorCode::CborIndefinite);
    }

    #[test]
    fn rejects_indefinite_length_byte_string() {
        assert_eq!(code(&[0x5f, 0x41, 0x00, 0xff]), ErrorCode::CborIndefinite);
    }

    #[test]
    fn rejects_non_shortest_integer() {
        // uint 5 written with a needless 1-byte argument (0x05 would suffice).
        assert_eq!(code(&[0x18, 0x05]), ErrorCode::CborNonDeterministic);
        // uint 5 written with a 2-byte argument.
        assert_eq!(code(&[0x19, 0x00, 0x05]), ErrorCode::CborNonDeterministic);
    }

    #[test]
    fn rejects_unsorted_map_keys() {
        // {"b": 2, "a": 1} — keys out of order.
        assert_eq!(
            code(&[0xa2, 0x61, 0x62, 0x02, 0x61, 0x61, 0x01]),
            ErrorCode::CborNonDeterministic
        );
    }

    #[test]
    fn rejects_duplicate_map_keys() {
        // {1: 1, 1: 2} — the key 1 appears twice.
        assert_eq!(code(&[0xa2, 0x01, 0x01, 0x01, 0x02]), ErrorCode::CborDupKey);
    }

    #[test]
    fn rejects_floating_point() {
        // f64 1.0, and f16 1.0 — no floats are permitted anywhere.
        assert_eq!(
            code(&[0xfb, 0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            ErrorCode::CborNonDeterministic
        );
        assert_eq!(code(&[0xf9, 0x3c, 0x00]), ErrorCode::CborNonDeterministic);
    }

    #[test]
    fn rejects_disallowed_tag() {
        // Tag 0 (standard date/time string) wrapping an empty text string.
        assert_eq!(code(&[0xc0, 0x60]), ErrorCode::CborTag);
    }

    #[test]
    fn framing_tags_are_permitted_only_at_the_outer_frame() {
        // Tag 55799 wrapping the app tag wrapping uint 0.
        let file = [0xd9, 0xd9, 0xf7, 0xda, 0x43, 0x52, 0x53, 0x50, 0x00];
        scan_canonical_file(&file).expect("framing tags are allowed in a file");
        // Not inside embedded content…
        assert_eq!(code(&[0xd9, 0xd9, 0xf7, 0x00]), ErrorCode::CborTag);
        // …nor nested deeper in a file.
        let nested = [
            0xd9, 0xd9, 0xf7, 0xda, 0x43, 0x52, 0x53, 0x50, 0x81, 0xd9, 0xd9, 0xf7, 0x00,
        ];
        assert_eq!(
            scan_canonical_file(&nested).unwrap_err().code(),
            Some(ErrorCode::CborTag)
        );
    }

    #[test]
    fn rejects_undefined_and_unassigned_simple_values() {
        assert_eq!(code(&[0xf7]), ErrorCode::CborMalformed);
        assert_eq!(code(&[0xe0]), ErrorCode::CborMalformed);
        assert_eq!(code(&[0xf8, 0x20]), ErrorCode::CborMalformed);
        scan_canonical(&[0xf6]).expect("null is well-formed");
    }

    #[test]
    fn rejects_malformed_and_truncated_input() {
        assert_eq!(code(&[]), ErrorCode::CborMalformed);
        // Says a 1-byte argument follows, but the input ends.
        assert_eq!(code(&[0x18]), ErrorCode::CborMalformed);
        // Reserved additional-information value 28.
        assert_eq!(code(&[0x1c]), ErrorCode::CborMalformed);
        // A valid item (uint 0) followed by a stray trailing byte.
        assert_eq!(code(&[0x00, 0x00]), ErrorCode::CborMalformed);
        // A text string claiming invalid UTF-8 content.
        assert_eq!(code(&[0x61, 0xff]), ErrorCode::CborMalformed);
    }

    #[test]
    fn rejects_nesting_beyond_the_depth_limit() {
        // 33 nested single-element arrays push the innermost item past depth 32.
        let mut bytes = vec![0x81_u8; 33];
        bytes.push(0x00);
        assert_eq!(code(&bytes), ErrorCode::Limit);
        // 32 levels stay within the limit.
        let mut ok = vec![0x81_u8; 32];
        ok.push(0x00);
        scan_canonical(&ok).expect("32 levels of nesting is allowed");
    }
}
