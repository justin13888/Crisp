//! The Crisp data model — Rust types mirroring `spec/crisp-v1.cddl`.
//!
//! Each module corresponds to a region of the normative CDDL schema and the
//! types are faithful, one-to-one translations of its productions. These are
//! pure data types; turning them into canonical CBOR bytes (and back, with
//! strict validation) is the job of [`crate::codec`].

pub mod blob;
pub mod book;
pub mod envelope;
pub mod ids;
pub mod meta;
pub mod payload;
pub mod primitives;
pub mod profile;
pub mod recipe;
pub mod text;

#[cfg(test)]
mod tests {
    use super::primitives::Quantity;
    use crate::codec::{from_canonical_cbor, to_canonical_cbor};

    #[test]
    fn a_default_quantity_encodes_to_an_empty_map() {
        let bytes = to_canonical_cbor(&Quantity::default()).expect("encode");
        // 0xa0: a CBOR map of zero entries — every absent field is omitted.
        assert_eq!(bytes, [0xa0]);
        let decoded: Quantity = from_canonical_cbor(&bytes).expect("decode");
        assert_eq!(decoded, Quantity::default());
    }
}
