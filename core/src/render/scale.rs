//! Scaling a recipe by an exact factor.

use std::collections::BTreeSet;

use crate::error::Result;
use crate::model::primitives::{Quantity, Rational};
use crate::model::recipe::Recipe;
use crate::render::number::multiply;

/// Scales a quantity's numeric bounds by `factor`, leaving its unit and any
/// textual amount unchanged.
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

/// Scales a recipe by `factor`: the yield, every ingredient amount and
/// alternative measure, substitutes, and per-step portions. Ingredients
/// marked `fixed`, durations, temperatures, equipment, and nutrition do not
/// scale.
///
/// # Errors
///
/// Returns an error if a scaled amount is out of range.
pub fn scale_recipe(recipe: &Recipe, factor: Rational) -> Result<Recipe> {
    let mut scaled = recipe.clone();
    scaled.r#yield.amount = scale_quantity(&recipe.r#yield.amount, factor)?;
    let fixed: BTreeSet<u64> = recipe
        .ingredients
        .iter()
        .filter(|i| i.fixed == Some(true))
        .map(|i| i.r#ref)
        .collect();
    for ingredient in &mut scaled.ingredients {
        if ingredient.fixed == Some(true) {
            continue;
        }
        if let Some(q) = &ingredient.quantity {
            ingredient.quantity = Some(scale_quantity(q, factor)?);
        }
        for alt in ingredient.alt.iter_mut().flatten() {
            *alt = scale_quantity(alt, factor)?;
        }
        for substitute in ingredient.substitutes.iter_mut().flatten() {
            if let Some(q) = &substitute.quantity {
                substitute.quantity = Some(scale_quantity(q, factor)?);
            }
        }
    }
    for step in &mut scaled.steps {
        for used in step.ingredients.iter_mut().flatten() {
            if fixed.contains(&used.r#ref) {
                continue;
            }
            if let Some(q) = &used.quantity {
                used.quantity = Some(scale_quantity(q, factor)?);
            }
        }
    }
    Ok(scaled)
}

/// The factor that turns a recipe's yield into `target` servings, or
/// `None` when the yield has no numeric amount.
#[must_use]
pub fn factor_for_yield(recipe: &Recipe, target: Rational) -> Option<Rational> {
    let base = recipe.r#yield.amount.min?;
    if base.numerator <= 0 {
        return None;
    }
    crate::render::number::divide(target, base).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_recipe;
    use crate::model::recipe::StepIngredient;

    #[test]
    fn scaling_doubles_amounts_but_not_fixed_ingredients() {
        let mut recipe = sample_recipe();
        let mut salt = recipe.ingredients[0].clone();
        salt.r#ref = 1;
        salt.fixed = Some(true);
        recipe.ingredients.push(salt);
        recipe.steps[0].ingredients = Some(vec![
            StepIngredient {
                r#ref: 0,
                quantity: recipe.ingredients[0].quantity.clone(),
            },
            StepIngredient {
                r#ref: 1,
                quantity: recipe.ingredients[1].quantity.clone(),
            },
        ]);
        let scaled = scale_recipe(&recipe, Rational::new(2, 1)).expect("scale");
        assert_eq!(scaled.r#yield.amount.min, Some(Rational::new(24, 1)));
        let amount = |q: &Option<Quantity>| q.as_ref().and_then(|q| q.min);
        assert_eq!(
            amount(&scaled.ingredients[0].quantity),
            Some(Rational::new(4, 1))
        );
        assert_eq!(
            amount(&scaled.ingredients[1].quantity),
            Some(Rational::new(2, 1))
        );
        let used = scaled.steps[0].ingredients.as_ref().expect("used");
        assert_eq!(amount(&used[0].quantity), Some(Rational::new(4, 1)));
        assert_eq!(amount(&used[1].quantity), Some(Rational::new(2, 1)));
    }

    #[test]
    fn yield_factor_targets_servings() {
        let recipe = sample_recipe();
        assert_eq!(
            factor_for_yield(&recipe, Rational::new(6, 1)),
            Some(Rational::new(1, 2))
        );
    }

    #[test]
    fn scaling_a_quantity_scales_both_bounds_and_keeps_the_rest() {
        let quantity = Quantity {
            min: Some(Rational::new(1, 2)),
            max: Some(Rational::new(3, 4)),
            unit: Some(crate::model::primitives::UnitCode("[cup_us]".to_owned())),
            approx: Some(true),
            text: Some(crate::fixtures::english("heaping")),
        };
        let scaled = scale_quantity(&quantity, Rational::new(3, 1)).expect("scale");
        assert_eq!(
            scaled,
            Quantity {
                min: Some(Rational::new(3, 2)),
                max: Some(Rational::new(9, 4)),
                ..quantity
            }
        );
    }

    #[test]
    fn scaling_reaches_alternatives_and_substitutes_but_not_fixed_ones() {
        use crate::fixtures::{amount, english};
        use crate::model::recipe::Substitute;

        let mut recipe = sample_recipe();
        let flour = &mut recipe.ingredients[0];
        flour.alt = Some(vec![amount(280, "g")]);
        flour.substitutes = Some(vec![Substitute {
            name: english("bread flour"),
            quantity: Some(amount(270, "g")),
            ext: None,
        }]);
        let mut fixed = flour.clone();
        fixed.r#ref = 1;
        fixed.fixed = Some(true);
        recipe.ingredients.push(fixed);

        let scaled = scale_recipe(&recipe, Rational::new(1, 2)).expect("scale");
        let half = &scaled.ingredients[0];
        assert_eq!(half.alt, Some(vec![amount(140, "g")]));
        let sub = &half.substitutes.as_ref().expect("substitutes")[0];
        assert_eq!(sub.quantity, Some(amount(135, "g")));
        assert_eq!(scaled.ingredients[1], recipe.ingredients[1]);
    }

    #[test]
    fn scaling_leaves_durations_untouched() {
        use crate::model::recipe::Timer;

        let mut recipe = sample_recipe();
        recipe.steps[0].timers = Some(vec![Timer {
            duration: crate::fixtures::amount(10, "min"),
            label: None,
            passive: None,
            ext: None,
        }]);
        let scaled = scale_recipe(&recipe, Rational::new(2, 1)).expect("scale");
        assert_eq!(scaled.steps[0].timers, recipe.steps[0].timers);
    }

    #[test]
    fn yield_factor_needs_a_positive_numeric_amount() {
        let target = Rational::new(6, 1);
        let mut recipe = sample_recipe();
        recipe.r#yield.amount.min = None;
        assert_eq!(factor_for_yield(&recipe, target), None);
        recipe.r#yield.amount.min = Some(Rational::new(0, 1));
        assert_eq!(factor_for_yield(&recipe, target), None);
        recipe.r#yield.amount.min = Some(Rational::new(-4, 1));
        assert_eq!(factor_for_yield(&recipe, target), None);
        recipe.r#yield.amount.min = Some(Rational::new(4, 1));
        assert_eq!(factor_for_yield(&recipe, target), Some(Rational::new(3, 2)));
    }
}
