//! Relating volume and mass through an ingredient's density.
//!
//! The author's `quantity` and `alt` stay authoritative; this is the pure
//! function a viewer may use to *offer* a derived measure (whether and how it
//! is shown is the client's decision). Results are always marked approximate.

use crate::model::primitives::{Quantity, Rational, UnitCode};
use crate::render::Units;
use crate::render::convert::target_unit;
use crate::render::number::{divide, multiply, round_to};
use crate::render::quantity::snap_step;
use crate::render::units::{UnitInfo, UnitKind, lookup};
use tracing::trace;

/// Converts a volume to a mass, or a mass to a volume, given a density in
/// grams per millilitre, expressed in `system` (metric when
/// [`Units::Original`]). `None` when the quantity has no numeric amount, its
/// unit is neither a mass nor a volume, or the density is not positive.
#[must_use]
pub fn convert_with_density(
    quantity: &Quantity,
    density: Rational,
    system: Units,
) -> Option<Quantity> {
    if density.numerator <= 0 {
        return None;
    }
    let from = lookup(&quantity.unit.as_ref()?.0)?;
    let to_kind = match from.kind {
        UnitKind::Volume => UnitKind::Mass,
        UnitKind::Mass => UnitKind::Volume,
        _ => return None,
    };
    let system = match system {
        Units::Us => Units::Us,
        Units::Original | Units::Metric => Units::Metric,
    };
    let other_base = |value: Rational| -> Option<Rational> {
        let base = multiply(value, from.factor).ok()?;
        if from.kind == UnitKind::Volume {
            multiply(base, density).ok()
        } else {
            divide(base, density).ok()
        }
    };
    let express = |base: Rational, to: &UnitInfo| -> Option<Rational> {
        let raw = divide(base, to.factor).ok()?;
        let snapped = round_to(raw, snap_step(Some(to), raw)).ok()?;
        Some(if snapped.numerator == 0 { raw } else { snapped })
    };
    let min_base = other_base(quantity.min?)?;
    let to = target_unit(to_kind, system, min_base)?;
    let min = express(min_base, to)?;
    let max = match quantity.max {
        Some(max) => Some(express(other_base(max)?, to)?),
        None => None,
    };
    Some(Quantity {
        min: Some(min),
        max,
        unit: Some(UnitCode(to.code.to_owned())),
        approx: Some(true),
        text: None,
    })
}

/// The measure an ingredient-index entry's density derives from `quantity`:
/// [`convert_with_density`] with the density of the entry `code` names
/// (resolved through `replaced_by`). `None` when the embedded index does not
/// load, the code is unknown or its entry has no density, or the quantity
/// does not convert.
#[must_use]
pub fn density_measure(quantity: &Quantity, code: &str, system: Units) -> Option<Quantity> {
    let index = super::index().ok()?;
    let Some(density) = index.resolve(code).and_then(|entry| entry.density) else {
        trace!(code, "no density for this ingredient code");
        return None;
    };
    let derived = convert_with_density(quantity, density, system);
    trace!(code, derived = derived.is_some(), "density measure");
    derived
}
