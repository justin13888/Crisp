//! Exact rational arithmetic and human number formatting.

use core::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::error::{CrispError, Result};
use crate::model::primitives::Rational;

/// Computes the greatest common divisor of two unsigned integers.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Builds a rational in lowest terms from a wide numerator and denominator.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if `denominator` is zero or the reduced
/// value does not fit the format's integer range.
pub fn reduce(numerator: i128, denominator: u128) -> Result<Rational> {
    if denominator == 0 {
        return Err(CrispError::input(
            "a rational's denominator must not be zero",
        ));
    }
    let divisor = gcd(numerator.unsigned_abs(), denominator).max(1);
    let wide_divisor =
        i128::try_from(divisor).map_err(|_| CrispError::input("rational is out of range"))?;
    Ok(Rational {
        numerator: i64::try_from(numerator / wide_divisor)
            .map_err(|_| CrispError::input("rational numerator is out of range"))?,
        denominator: u64::try_from(denominator / divisor)
            .map_err(|_| CrispError::input("rational denominator is out of range"))?,
    })
}

/// Multiplies two rationals, returning the result in lowest terms.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if a denominator is zero or the result is
/// out of range.
pub fn multiply(a: Rational, b: Rational) -> Result<Rational> {
    reduce(
        i128::from(a.numerator) * i128::from(b.numerator),
        u128::from(a.denominator) * u128::from(b.denominator),
    )
}

/// Divides `a` by `b`.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if `b` is zero or the result is out of
/// range.
pub fn divide(a: Rational, b: Rational) -> Result<Rational> {
    if b.numerator == 0 {
        return Err(CrispError::input("division by zero"));
    }
    let sign = if b.numerator < 0 { -1 } else { 1 };
    reduce(
        sign * i128::from(a.numerator) * i128::from(b.denominator),
        u128::from(a.denominator) * u128::from(b.numerator.unsigned_abs()),
    )
}

/// Adds two rationals.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if a denominator is zero or the result is
/// out of range.
pub fn add(a: Rational, b: Rational) -> Result<Rational> {
    reduce(
        i128::from(a.numerator) * i128::from(b.denominator)
            + i128::from(b.numerator) * i128::from(a.denominator),
        u128::from(a.denominator) * u128::from(b.denominator),
    )
}

/// Compares two rationals exactly.
#[must_use]
pub fn compare(a: Rational, b: Rational) -> Ordering {
    (i128::from(a.numerator) * i128::from(b.denominator))
        .cmp(&(i128::from(b.numerator) * i128::from(a.denominator)))
}

/// Rounds `value` to the nearest multiple of `step` (halves round away from
/// zero).
///
/// # Errors
///
/// Returns [`CrispError::Input`] if `step` is zero or the result is out of
/// range.
pub fn round_to(value: Rational, step: Rational) -> Result<Rational> {
    let ratio = divide(value, step)?;
    let n = i128::from(ratio.numerator);
    let d = i128::from(ratio.denominator);
    let k = if n >= 0 {
        (2 * n + d) / (2 * d)
    } else {
        -((-2 * n + d) / (2 * d))
    };
    multiply(
        Rational::new(
            i64::try_from(k).map_err(|_| CrispError::input("rounded value is out of range"))?,
            1,
        ),
        step,
    )
}

/// An approximate `f64`, for choosing display units — never for storage.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn to_f64(value: Rational) -> f64 {
    value.numerator as f64 / value.denominator.max(1) as f64
}

/// The Unicode vulgar fractions a cook reads at a glance.
const VULGAR: &[(i64, u64, &str)] = &[
    (1, 2, "½"),
    (1, 3, "⅓"),
    (2, 3, "⅔"),
    (1, 4, "¼"),
    (3, 4, "¾"),
    (1, 5, "⅕"),
    (2, 5, "⅖"),
    (3, 5, "⅗"),
    (4, 5, "⅘"),
    (1, 6, "⅙"),
    (5, 6, "⅚"),
    (1, 8, "⅛"),
    (3, 8, "⅜"),
    (5, 8, "⅝"),
    (7, 8, "⅞"),
];

/// How to write a non-integer amount.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumberStyle {
    /// Vulgar fractions (`1½`), for US-customary and count amounts.
    Fraction,
    /// Decimals (`1.5`), for metric amounts.
    Decimal,
}

/// Whether `value` can be written exactly in the given style.
#[must_use]
pub fn is_exactly_writable(value: Rational, style: NumberStyle) -> bool {
    let value = reduce(
        i128::from(value.numerator),
        u128::from(value.denominator.max(1)),
    )
    .unwrap_or(value);
    match style {
        NumberStyle::Fraction => {
            value.denominator == 1 || VULGAR.iter().any(|(_, d, _)| *d == value.denominator)
        }
        NumberStyle::Decimal => {
            let mut d = value.denominator;
            while d.is_multiple_of(2) {
                d /= 2;
            }
            while d.is_multiple_of(5) {
                d /= 5;
            }
            d == 1 && value.denominator <= 100
        }
    }
}

/// Formats a rational for a cook: integers plainly, then vulgar fractions or
/// up to two decimals.
#[must_use]
pub fn format_number(value: Rational, style: NumberStyle) -> String {
    let value = reduce(
        i128::from(value.numerator),
        u128::from(value.denominator.max(1)),
    )
    .unwrap_or(value);
    let sign = if value.numerator < 0 { "-" } else { "" };
    let n = value.numerator.unsigned_abs();
    let d = value.denominator;
    let whole = n / d;
    let rem = n % d;
    if rem == 0 {
        return format!("{sign}{whole}");
    }
    if style == NumberStyle::Fraction {
        let frac = reduce(i128::from(rem), u128::from(d)).unwrap_or(value);
        if let Some((_, _, glyph)) = VULGAR
            .iter()
            .find(|(fn_, fd, _)| *fn_ == frac.numerator && *fd == frac.denominator)
        {
            return if whole == 0 {
                format!("{sign}{glyph}")
            } else {
                format!("{sign}{whole}{glyph}")
            };
        }
    }
    let text = format!(
        "{:.2}",
        to_f64(Rational::new(i64::try_from(n).unwrap_or(i64::MAX), d))
    );
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{sign}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_styles_cross_as_lowercase_names() {
        assert_eq!(
            serde_json::to_string(&NumberStyle::Fraction).unwrap(),
            "\"fraction\""
        );
        assert_eq!(
            serde_json::from_str::<NumberStyle>("\"decimal\"").unwrap(),
            NumberStyle::Decimal
        );
    }

    #[test]
    fn arithmetic_reduces_to_lowest_terms() {
        assert_eq!(
            multiply(Rational::new(5, 2), Rational::new(2, 1)).unwrap(),
            Rational::new(5, 1)
        );
        assert_eq!(
            add(Rational::new(1, 3), Rational::new(1, 6)).unwrap(),
            Rational::new(1, 2)
        );
        assert_eq!(
            divide(Rational::new(1, 2), Rational::new(-1, 4)).unwrap(),
            Rational::new(-2, 1)
        );
        assert!(divide(Rational::new(1, 2), Rational::new(0, 1)).is_err());
        assert!(reduce(1, 0).is_err());
    }

    #[test]
    fn rounding_snaps_to_the_nearest_step() {
        let eighth = Rational::new(1, 8);
        assert_eq!(
            round_to(Rational::new(7, 12), eighth).unwrap(),
            Rational::new(5, 8)
        );
        assert_eq!(
            round_to(Rational::new(1, 16), eighth).unwrap(),
            Rational::new(1, 8)
        );
        assert_eq!(
            round_to(Rational::new(-3, 2), Rational::new(1, 1)).unwrap(),
            Rational::new(-2, 1)
        );
    }

    #[test]
    fn formatting_prefers_glyphs_for_fractions_and_decimals_for_metric() {
        let f = NumberStyle::Fraction;
        assert_eq!(format_number(Rational::new(3, 2), f), "1½");
        assert_eq!(format_number(Rational::new(2, 3), f), "⅔");
        assert_eq!(format_number(Rational::new(4, 2), f), "2");
        assert_eq!(format_number(Rational::new(7, 12), f), "0.58");
        assert_eq!(
            format_number(Rational::new(5, 4), NumberStyle::Decimal),
            "1.25"
        );
        assert_eq!(
            format_number(Rational::new(1, 2), NumberStyle::Decimal),
            "0.5"
        );
        assert!(is_exactly_writable(
            Rational::new(5, 4),
            NumberStyle::Decimal
        ));
        assert!(!is_exactly_writable(
            Rational::new(1, 3),
            NumberStyle::Decimal
        ));
        assert!(!is_exactly_writable(
            Rational::new(7, 12),
            NumberStyle::Fraction
        ));
    }

    #[test]
    fn negative_rounding_snaps_to_the_nearest_step_with_halves_away_from_zero() {
        let one = Rational::new(1, 1);
        let eighth = Rational::new(1, 8);
        let cases = [
            // value / step = -14/3 = -4.67 → -5 steps.
            (Rational::new(-7, 12), eighth, Rational::new(-5, 8)),
            // -1/2 step exactly: away from zero.
            (Rational::new(-1, 16), eighth, Rational::new(-1, 8)),
            (Rational::new(-1, 3), one, Rational::new(0, 1)),
            (Rational::new(-2, 3), one, Rational::new(-1, 1)),
            (Rational::new(-5, 2), one, Rational::new(-3, 1)),
            (Rational::new(-12, 5), one, Rational::new(-2, 1)),
        ];
        for (value, step, expected) in cases {
            assert_eq!(round_to(value, step).unwrap(), expected, "{value:?}");
        }
    }

    #[test]
    fn decimal_writability_allows_factors_of_two_and_five_up_to_a_hundredth() {
        let d = NumberStyle::Decimal;
        for (n, den) in [(1, 5), (2, 5), (3, 20), (1, 25), (7, 50), (1, 64), (3, 100)] {
            assert!(is_exactly_writable(Rational::new(n, den), d), "{n}/{den}");
        }
        for (n, den) in [(1, 15), (1, 125), (1, 128), (1, 3), (1, 200)] {
            assert!(!is_exactly_writable(Rational::new(n, den), d), "{n}/{den}");
        }
        // Writability is judged in lowest terms.
        assert!(is_exactly_writable(Rational::new(3, 300), d));
        assert!(is_exactly_writable(Rational::new(3, 3), d));
    }

    #[test]
    fn negative_and_zero_amounts_format_with_their_sign() {
        let f = NumberStyle::Fraction;
        let d = NumberStyle::Decimal;
        let cases = [
            (Rational::new(-3, 2), f, "-1½"),
            (Rational::new(-1, 2), f, "-½"),
            (Rational::new(-4, 2), f, "-2"),
            (Rational::new(-7, 12), f, "-0.58"),
            (Rational::new(-5, 4), d, "-1.25"),
            (Rational::new(-1, 2), d, "-0.5"),
            (Rational::new(0, 1), f, "0"),
            (Rational::new(0, 7), d, "0"),
            (Rational::new(1, 1000), d, "0"),
            // A negative amount that rounds to zero keeps its sign
            // (current behaviour; whether it should read "0" is undecided).
            (Rational::new(-1, 1000), d, "-0"),
        ];
        for (value, style, expected) in cases {
            assert_eq!(format_number(value, style), expected, "{value:?}");
        }
    }

    #[test]
    fn comparison_is_exact() {
        assert_eq!(
            compare(Rational::new(1, 3), Rational::new(2, 6)),
            Ordering::Equal
        );
        assert_eq!(
            compare(Rational::new(1, 3), Rational::new(1, 2)),
            Ordering::Less
        );
    }
}
