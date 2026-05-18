//! Scalar value types shared across the model: timestamps, language and unit
//! tags, exact rational numbers, and quantities.

use serde::de::Deserializer;
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};

use crate::model::text::LocalizedText;

/// An instant in time, as integer seconds since the Unix epoch (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub u64);

/// A BCP-47 language tag, e.g. `"en-US"` (`crisp-v1.cddl`: `language-tag`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageTag(pub String);

/// A UCUM unit code, e.g. `"g"` or `"cup"` (`crisp-v1.cddl`: `unit-code`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitCode(pub String);

/// An exact rational number — the format carries no floating-point values.
///
/// Encoded as the two-element CBOR array `[numerator, denominator]`
/// (`crisp-v1.cddl`: `rational`). The denominator must be `>= 1`; that
/// invariant is enforced during schema validation, not construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rational {
    /// The numerator.
    pub numerator: i64,
    /// The denominator; must be at least 1.
    pub denominator: u64,
}

impl Rational {
    /// Builds a rational from a numerator and denominator.
    #[must_use]
    pub const fn new(numerator: i64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

impl Serialize for Rational {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.numerator)?;
        seq.serialize_element(&self.denominator)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Rational {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (numerator, denominator) = <(i64, u64)>::deserialize(deserializer)?;
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

/// An amount, possibly a range, possibly non-numeric (`crisp-v1.cddl`:
/// `quantity`).
///
/// At least one of `min` or `text` must be present; that rule is a Phase D
/// verification check ([`crate::ErrorCode::Quantity`]), not a constructor
/// invariant.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    /// The amount, or the lower bound of a range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Rational>,
    /// The upper bound of a range, when the quantity spans one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Rational>,
    /// The unit the amount is measured in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitCode>,
    /// Whether the amount is approximate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approx: Option<bool>,
    /// A non-numeric amount, e.g. "to taste".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<LocalizedText>,
}
