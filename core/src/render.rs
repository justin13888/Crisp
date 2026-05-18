//! Client-side rendering helpers: locale resolution, recipe scaling, and
//! step-timer extraction.
//!
//! These are pure, non-trusted-decision conveniences for a viewer. They make
//! no security claims — a bundle must already have been verified.

use crate::error::{CrispError, Result};
use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::Recipe;
use crate::model::text::{LocalizedProse, LocalizedText, Prose};

/// Resolves the best string from a [`LocalizedText`] for a reader.
///
/// Tries each `preferred` language in order, then the bundle's
/// `default_language`, then any available value (spec §5).
#[must_use]
pub fn resolve_text<'a>(
    text: &'a LocalizedText,
    preferred: &[LanguageTag],
    default_language: &LanguageTag,
) -> Option<&'a str> {
    pick(&text.0, preferred, default_language).map(String::as_str)
}

/// Resolves the best [`Prose`] from a [`LocalizedProse`] for a reader.
#[must_use]
pub fn resolve_prose<'a>(
    prose: &'a LocalizedProse,
    preferred: &[LanguageTag],
    default_language: &LanguageTag,
) -> Option<&'a Prose> {
    pick(&prose.0, preferred, default_language)
}

/// Shared best-match logic over a localized map.
fn pick<'a, V>(
    map: &'a std::collections::BTreeMap<LanguageTag, V>,
    preferred: &[LanguageTag],
    default_language: &LanguageTag,
) -> Option<&'a V> {
    preferred
        .iter()
        .find_map(|tag| map.get(tag))
        .or_else(|| map.get(default_language))
        .or_else(|| map.values().next())
}

/// Computes the greatest common divisor of two unsigned integers.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Multiplies two rationals, returning the result in lowest terms.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if the reduced result does not fit the
/// signed-64-bit numerator / unsigned-64-bit denominator range.
pub fn multiply(a: Rational, b: Rational) -> Result<Rational> {
    let numerator = i128::from(a.numerator) * i128::from(b.numerator);
    let denominator = u128::from(a.denominator) * u128::from(b.denominator);
    let divisor = gcd(numerator.unsigned_abs(), denominator).max(1);
    let numerator = numerator / divisor as i128;
    let denominator = denominator / divisor;
    Ok(Rational {
        numerator: i64::try_from(numerator)
            .map_err(|_| CrispError::input("scaled numerator is out of range"))?,
        denominator: u64::try_from(denominator)
            .map_err(|_| CrispError::input("scaled denominator is out of range"))?,
    })
}

/// Scales a [`Quantity`]'s numeric bounds by `factor`, leaving its unit and
/// any textual amount unchanged.
///
/// # Errors
///
/// Returns an error if a scaled bound is out of range.
pub fn scale_quantity(quantity: &Quantity, factor: Rational) -> Result<Quantity> {
    Ok(Quantity {
        min: quantity.min.map(|r| multiply(r, factor)).transpose()?,
        max: quantity.max.map(|r| multiply(r, factor)).transpose()?,
        unit: quantity.unit.clone(),
        approx: quantity.approx,
        text: quantity.text.clone(),
    })
}

/// Scales a recipe by `factor`: the yield amount and every ingredient (and
/// substitute) quantity are multiplied. Durations, temperatures, and
/// nutrition — which do not scale with batch size — are left untouched.
///
/// # Errors
///
/// Returns an error if a scaled amount is out of range.
pub fn scale_recipe(recipe: &Recipe, factor: Rational) -> Result<Recipe> {
    let mut scaled = recipe.clone();
    scaled.r#yield.amount = scale_quantity(&recipe.r#yield.amount, factor)?;
    for ingredient in &mut scaled.ingredients {
        if let Some(quantity) = &ingredient.quantity {
            ingredient.quantity = Some(scale_quantity(quantity, factor)?);
        }
        for substitute in ingredient.substitutes.iter_mut().flatten() {
            if let Some(quantity) = &substitute.quantity {
                substitute.quantity = Some(scale_quantity(quantity, factor)?);
            }
        }
    }
    Ok(scaled)
}

/// A timer a viewer can offer for a step that declares a duration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepTimer {
    /// The `ref` handle of the step.
    pub step_ref: u64,
    /// The step's declared duration.
    pub duration: Quantity,
}

/// Extracts a [`StepTimer`] for every step that declares a duration.
#[must_use]
pub fn step_timers(recipe: &Recipe) -> Vec<StepTimer> {
    recipe
        .steps
        .iter()
        .filter_map(|step| {
            step.duration.clone().map(|duration| StepTimer {
                step_ref: step.r#ref,
                duration,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_recipe;

    fn tag(s: &str) -> LanguageTag {
        LanguageTag(s.to_owned())
    }

    #[test]
    fn resolve_prefers_a_preferred_language_then_the_default() {
        let mut text = crate::fixtures::english("Hello");
        text.0.insert(tag("fr"), "Bonjour".to_owned());
        let default = tag("en");
        assert_eq!(resolve_text(&text, &[tag("fr")], &default), Some("Bonjour"));
        assert_eq!(resolve_text(&text, &[tag("de")], &default), Some("Hello"));
    }

    #[test]
    fn multiply_reduces_to_lowest_terms() {
        // (5/2) * (2/1) = 10/2 = 5/1.
        let result = multiply(Rational::new(5, 2), Rational::new(2, 1)).expect("multiply");
        assert_eq!(result, Rational::new(5, 1));
    }

    #[test]
    fn scaling_doubles_ingredient_and_yield_amounts() {
        let recipe = sample_recipe();
        let scaled = scale_recipe(&recipe, Rational::new(2, 1)).expect("scale");
        // Yield 12 -> 24.
        assert_eq!(scaled.r#yield.amount.min, Some(Rational::new(24, 1)));
        // Flour 2 cups -> 4 cups.
        assert_eq!(
            scaled.ingredients[0].quantity.as_ref().unwrap().min,
            Some(Rational::new(4, 1))
        );
        // The unit is preserved.
        assert_eq!(
            scaled.ingredients[0].quantity.as_ref().unwrap().unit,
            recipe.ingredients[0].quantity.as_ref().unwrap().unit
        );
    }

    #[test]
    fn step_timers_are_extracted_only_for_steps_with_a_duration() {
        // The sample recipe's single step has no duration.
        assert!(step_timers(&sample_recipe()).is_empty());
    }
}
