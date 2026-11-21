//! Assembling the dataset from parsed sources, rules, and overrides.

use std::collections::{BTreeMap, BTreeSet};

use crisp_core::ingredients::{Dataset, Entry, LocaleTable};
use crisp_core::model::primitives::{OpenCode, Rational};
use eyre::{Result, bail};
use tracing::info;

use crate::english::Lexicon;
use crate::names::{self, NameOverride, ShardNames};
use crate::off::{OffEntry, Taxonomy};
use crate::opencc::Converter;
use crate::rules::Rules;
use crate::select::{Decisions, select};
use crate::slug;
use crate::usda::{self, Candidate, Food, Matchable};

/// Everything the pipeline consumes besides the rules.
pub struct Inputs<'a> {
    /// The parsed OFF taxonomy.
    pub taxonomy: &'a Taxonomy,
    /// USDA foods with densities.
    pub foods: &'a [Food],
    /// The English lexicon.
    pub lexicon: &'a Lexicon,
    /// OpenCC converters by derived shard.
    pub converters: &'a BTreeMap<String, Converter>,
    /// Entries added by override.
    pub added: &'a [Entry],
    /// Name overrides: shard → slug → override.
    pub name_overrides: &'a BTreeMap<String, BTreeMap<String, NameOverride>>,
    /// The previously published entries (slugs are permanent).
    pub previous: &'a [Entry],
    /// The previously published names, kept for retained entries.
    pub previous_names: &'a ShardNames,
    /// Recorded merges: slug → the entry it was merged into.
    pub merges: &'a BTreeMap<String, String>,
}

/// The pipeline's results.
pub struct Assembled {
    /// The dataset, version not yet set.
    pub dataset: Dataset,
    /// Rule decision counts.
    pub decisions: Decisions,
    /// USDA candidates, for `usda-map.json`.
    pub candidates: Vec<Candidate>,
    /// Published entries the sources no longer produce, kept as they were.
    pub retained: Vec<String>,
    /// Source keys that yield no usable slug.
    pub unusable: Vec<String>,
}

/// A property from the entry or its nearest ancestor along first parents.
fn inherited<'a>(id: &str, prop: &str, by_id: &BTreeMap<&str, &'a OffEntry>) -> Option<&'a str> {
    let mut seen = BTreeSet::new();
    let mut at = by_id.get(id).copied();
    while let Some(entry) = at {
        if !seen.insert(entry.id.as_str()) {
            return None;
        }
        if let Some(value) = entry.props.get(prop) {
            return Some(value.as_str());
        }
        at = entry
            .parents
            .first()
            .and_then(|p| by_id.get(p.as_str()).copied());
    }
    None
}

/// An open-vocabulary code from its wire string.
fn open_code<T: serde::de::DeserializeOwned>(code: &str) -> Result<OpenCode<T>> {
    Ok(serde_json::from_value(serde_json::Value::String(
        code.to_owned(),
    ))?)
}

/// Rounds a positive density to a rational with the given denominator.
fn rational(value: f64, denominator: u64) -> Option<Rational> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let numerator = (value * denominator as f64).round() as i64;
    if numerator <= 0 {
        return None;
    }
    let (mut a, mut b) = (numerator.unsigned_abs(), denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    i64::try_from(a)
        .ok()
        .map(|g| Rational::new(numerator / g, denominator / a))
}

/// Allergen and diet hints for one source entry.
fn hints(id: &str, rules: &Rules, by_id: &BTreeMap<&str, &OffEntry>) -> (Vec<String>, Vec<String>) {
    let mut allergens: BTreeSet<String> = BTreeSet::new();
    if let Some(list) = inherited(id, "allergens", by_id) {
        for off in list.split(',').map(str::trim) {
            if let Some(code) = rules.hints.allergens.get(off) {
                allergens.insert(code.clone());
            }
        }
    }
    let mut diets: BTreeSet<String> = BTreeSet::new();
    for (prop, diet) in &rules.hints.diet_when_no {
        if inherited(id, prop, by_id) == Some("no") {
            diets.insert(diet.clone());
        }
    }
    for allergen in &allergens {
        diets.extend(
            rules
                .hints
                .diet_from_allergen
                .get(allergen)
                .into_iter()
                .flatten()
                .cloned(),
        );
    }
    (allergens.into_iter().collect(), diets.into_iter().collect())
}

/// Runs the whole pipeline.
///
/// # Errors
/// When a rule is invalid or an override collides with an imported slug.
pub fn assemble(rules: &Rules, inputs: &Inputs<'_>) -> Result<Assembled> {
    let (selected, decisions) = select(inputs.taxonomy, &rules.select)?;
    info!(selected = selected.len(), "entries selected");
    let by_id: BTreeMap<&str, &OffEntry> = inputs
        .taxonomy
        .entries
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();
    let previous: BTreeMap<String, String> = inputs
        .previous
        .iter()
        .filter(|e| e.source.starts_with("off:"))
        .map(|e| (e.source.clone(), e.slug.clone()))
        .collect();
    let keys: Vec<String> = selected.iter().map(|s| format!("off:{}", s.id)).collect();
    // Every slug ever published stays taken, overrides included.
    let reserved: BTreeSet<String> = inputs
        .added
        .iter()
        .chain(inputs.previous)
        .map(|e| e.slug.clone())
        .collect();
    let (slugs, unusable) = slug::assign(&keys, &previous, &reserved);
    let mut owned = names::Owned::new();
    for s in &selected {
        let (Some(slug), Some(list)) = (
            slugs.get(&format!("off:{}", s.id)),
            by_id.get(s.id.as_str()).and_then(|e| e.names.get("en")),
        ) else {
            continue;
        };
        for name in list {
            owned
                .entry(crisp_core::ingredients::normalize(name))
                .or_default()
                .insert(slug.clone());
        }
    }
    let slug_of = |id: &str| slugs.get(&format!("off:{id}")).cloned();

    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let mut shards = ShardNames::new();
    let mut matchable = Vec::new();
    for s in &selected {
        let Some(slug) = slug_of(&s.id) else { continue };
        let (allergens, diets) = hints(&s.id, rules, &by_id);
        let off = by_id[s.id.as_str()];
        entries.insert(
            slug.clone(),
            Entry {
                slug: slug.clone(),
                parent: s.parent.as_deref().and_then(slug_of),
                category: s.category.clone(),
                density: None,
                allergens: allergens
                    .iter()
                    .map(|a| open_code(a))
                    .collect::<Result<_>>()?,
                diet_conflicts: diets.iter().map(|d| open_code(d)).collect::<Result<_>>()?,
                replaced_by: None,
                source: format!("off:{}", s.id),
            },
        );
        names::add_source(
            &mut shards,
            &slug,
            &off.names,
            &rules.locales.off_language,
            &rules.locales.english,
            inputs.lexicon,
            &owned,
        );
        matchable.push((slug, off));
    }
    for added in inputs.added {
        if entries.insert(added.slug.clone(), added.clone()).is_some() {
            bail!(
                "override entry {:?} collides with an imported slug",
                added.slug
            );
        }
    }
    let mut retained = Vec::new();
    for old in inputs.previous {
        if !entries.contains_key(&old.slug) {
            retained.push(old.slug.clone());
            entries.insert(old.slug.clone(), old.clone());
        }
    }
    for (slug, successor) in inputs.merges {
        if !entries.contains_key(successor) {
            bail!("merge of {slug:?} names unknown entry {successor:?}");
        }
        let Some(entry) = entries.get_mut(slug) else {
            bail!("merge names unknown entry {slug:?}");
        };
        entry.replaced_by = Some(successor.clone());
    }

    let derived: BTreeSet<&str> = rules
        .locales
        .derive
        .iter()
        .filter(|d| d.shard != d.from)
        .map(|d| d.shard.as_str())
        .collect();
    let (late, early): (BTreeMap<_, _>, BTreeMap<_, _>) = inputs
        .name_overrides
        .clone()
        .into_iter()
        .partition(|(shard, _)| derived.contains(shard.as_str()));
    names::apply_overrides(&mut shards, &early);
    for d in &rules.locales.derive {
        if let Some(converter) = inputs.converters.get(&d.shard) {
            names::derive(&mut shards, &d.from, &d.shard, converter);
        }
    }
    names::apply_overrides(&mut shards, &late);
    // A retained entry keeps the names it was published with.
    for (shard, by_slug) in inputs.previous_names {
        for slug in &retained {
            if let Some(names) = by_slug.get(slug) {
                shards
                    .entry(shard.clone())
                    .or_default()
                    .insert(slug.clone(), names.clone());
            }
        }
    }

    let matchables: Vec<Matchable<'_>> = matchable
        .iter()
        .map(|(slug, off)| Matchable {
            slug,
            linked: off
                .props
                .get("usda_fdc_code")
                .map(String::as_str)
                .into_iter()
                .collect(),
            names: off
                .names
                .get("en")
                .map(|v| v.iter().map(String::as_str).collect())
                .unwrap_or_default(),
        })
        .collect();
    let candidates = usda::candidates(&matchables, inputs.foods, &rules.density.accept);
    let mut accepted: BTreeMap<&str, (usize, Vec<f64>)> = BTreeMap::new();
    for c in candidates.iter().filter(|c| c.accepted) {
        let rank = rules
            .density
            .accept
            .iter()
            .position(|k| *k == c.kind)
            .unwrap_or(usize::MAX);
        let slot = accepted
            .entry(c.slug.as_str())
            .or_insert((rank, Vec::new()));
        if rank < slot.0 {
            *slot = (rank, Vec::new());
        }
        if rank == slot.0 {
            slot.1.push(c.density);
        }
    }
    for (slug, off) in &matchable {
        let own = off
            .props
            .get("density_g_per_ml")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|_| rules.density.accept.iter().any(|k| k == "off"));
        let value = own.or_else(|| usda::median(accepted.get(slug.as_str())?.1.clone()));
        if let (Some(v), Some(entry)) = (value, entries.get_mut(slug)) {
            entry.density = rational(v, rules.density.denominator);
        }
    }

    let locales = LocaleTable {
        shards: rules.locales.shards.clone(),
        fallback: rules.locales.fallback.clone(),
        last_resort: rules.locales.last_resort.clone(),
    };
    shards.retain(|shard, by_slug| {
        by_slug.retain(|slug, _| entries.contains_key(slug));
        locales.shards.contains(shard)
    });
    Ok(Assembled {
        dataset: Dataset {
            version: String::new(),
            locales,
            entries: entries.into_values().collect(),
            names: shards,
        },
        decisions,
        candidates,
        retained,
        unusable,
    })
}

#[cfg(test)]
mod tests;
