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
    use super::primitives::{Quantity, Rational, UnitCode};

    #[test]
    fn types_compose_as_expected() {
        let quantity = Quantity {
            min: Some(Rational::new(5, 2)),
            unit: Some(UnitCode("cup".to_owned())),
            ..Quantity::default()
        };
        assert_eq!(quantity.min, Some(Rational::new(5, 2)));
        assert_eq!(quantity.max, None);
        assert_eq!(Rational::new(5, 2).numerator, 5);
        assert_eq!(Rational::new(5, 2).denominator, 2);
    }
}
