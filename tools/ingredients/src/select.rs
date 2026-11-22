//! Deciding which taxonomy entries become index entries.
//!
//! An entry is imported when it (or an ancestor) is an included subtree
//! root, no ancestor is an excluded subtree, its canonical id is English
//! (slugs derive from it), and no exclusion pattern matches it. Its category
//! comes from the deepest included root above it; its parent is its nearest
//! *imported* ancestor along the first-listed parent path. Every decision is
//! counted per rule for the report.

use std::collections::{BTreeMap, BTreeSet};

use eyre::{Context, Result};
use regex::Regex;

use crate::off::{OffEntry, Taxonomy};
use crate::rules::Select;

/// An imported entry before slugs are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The OFF id.
    pub id: String,
    /// The OFF id of the nearest imported ancestor.
    pub parent: Option<String>,
    /// The Crisp category.
    pub category: String,
}

/// Counts of what each rule decided, for the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decisions {
    /// Imported entries per category.
    pub included: BTreeMap<String, usize>,
    /// Dropped entries per reason.
    pub excluded: BTreeMap<String, usize>,
}

/// Walks up the first-listed parent path, yielding ids nearest first,
/// stopping at cycles.
fn first_path<'a>(id: &'a str, by_id: &'a BTreeMap<&str, &OffEntry>) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::from([id]);
    let mut at = by_id.get(id).and_then(|e| e.parents.first());
    while let Some(next) = at {
        if !seen.insert(next.as_str()) {
            break;
        }
        out.push(next.as_str());
        at = by_id.get(next.as_str()).and_then(|e| e.parents.first());
    }
    out
}

/// Every ancestor reachable through any parent, nearest first by breadth.
fn all_ancestors<'a>(id: &'a str, by_id: &'a BTreeMap<&str, &OffEntry>) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::from([id]);
    let mut frontier = vec![id];
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for at in frontier {
            for parent in by_id
                .get(at)
                .map(|e| e.parents.as_slice())
                .unwrap_or_default()
            {
                if seen.insert(parent.as_str()) {
                    out.push(parent.as_str());
                    next.push(parent.as_str());
                }
            }
        }
        frontier = next;
    }
    out
}

/// Applies the selection rules to the taxonomy.
///
/// # Errors
/// When an exclusion pattern is not a valid regular expression.
pub fn select(taxonomy: &Taxonomy, rules: &Select) -> Result<(Vec<Selected>, Decisions)> {
    let patterns: Vec<(String, Regex)> = rules
        .exclude
        .iter()
        .map(|x| {
            Regex::new(&x.pattern)
                .map(|re| (x.name.clone(), re))
                .wrap_err_with(|| format!("exclusion {:?}", x.name))
        })
        .collect::<Result<_>>()?;
    let roots: BTreeMap<&str, &str> = rules
        .include
        .iter()
        .map(|s| (s.id.as_str(), s.category.as_str()))
        .collect();
    let dropped: BTreeSet<&str> = rules.exclude_subtrees.iter().map(String::as_str).collect();
    let by_id: BTreeMap<&str, &OffEntry> = taxonomy
        .entries
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();

    let mut decisions = Decisions::default();
    let mut kept: BTreeMap<&str, String> = BTreeMap::new();
    for entry in &taxonomy.entries {
        let id = entry.id.as_str();
        let lineage: Vec<&str> = std::iter::once(id)
            .chain(all_ancestors(id, &by_id))
            .collect();
        let reason = if lineage.iter().any(|a| dropped.contains(a)) {
            Some("excluded subtree".to_owned())
        } else if !lineage.iter().any(|a| roots.contains_key(a)) {
            Some("outside every included subtree".to_owned())
        } else if !id.starts_with("en:") {
            Some("no English canonical id".to_owned())
        } else {
            let bare = &id[3..];
            patterns
                .iter()
                .find(|(_, re)| re.is_match(bare))
                .map(|(name, _)| format!("pattern: {name}"))
        };
        if let Some(reason) = reason {
            *decisions.excluded.entry(reason).or_default() += 1;
            continue;
        }
        let category = lineage
            .iter()
            .find_map(|a| roots.get(a))
            .copied()
            .unwrap_or_default()
            .to_owned();
        kept.insert(id, category);
    }
    let mut selected = Vec::new();
    for entry in &taxonomy.entries {
        let Some(category) = kept.get(entry.id.as_str()) else {
            continue;
        };
        let parent = first_path(&entry.id, &by_id)
            .into_iter()
            .find(|a| kept.contains_key(a))
            .map(str::to_owned);
        *decisions.included.entry(category.clone()).or_default() += 1;
        selected.push(Selected {
            id: entry.id.clone(),
            parent,
            category: category.clone(),
        });
    }
    Ok((selected, decisions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::off::parse;
    use crate::rules::{Exclusion, Subtree};

    const SAMPLE: &str = "\
en: vegetable

< en: vegetable
en: cabbage

< en: cabbage
en: kimchi

< en: vegetable
en: vegetable flavouring

< en: vegetable
en: e160a

< en: vegetable
fr: chou rouge

< en: vegetable
en: pickle

< en: pickle
en: pickled cucumber

en: plant

< en: plant
< en: vegetable
en: heather
";

    fn rules() -> Select {
        Select {
            include: vec![
                Subtree {
                    id: "en:vegetable".to_owned(),
                    category: "produce".to_owned(),
                },
                Subtree {
                    id: "en:pickle".to_owned(),
                    category: "fermented".to_owned(),
                },
            ],
            exclude_subtrees: vec!["en:plant".to_owned()],
            exclude: vec![
                Exclusion {
                    name: "flavourings".to_owned(),
                    pattern: "(^|-)flavou?ring(-|$)".to_owned(),
                },
                Exclusion {
                    name: "additive codes".to_owned(),
                    pattern: "^e[0-9]{3}".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn rules_decide_inclusion_category_and_parent() {
        let (selected, decisions) = select(&parse(SAMPLE), &rules()).expect("select");
        let rows: Vec<(&str, Option<&str>, &str)> = selected
            .iter()
            .map(|s| (s.id.as_str(), s.parent.as_deref(), s.category.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("en:vegetable", None, "produce"),
                ("en:cabbage", Some("en:vegetable"), "produce"),
                ("en:kimchi", Some("en:cabbage"), "produce"),
                ("en:pickle", Some("en:vegetable"), "fermented"),
                ("en:pickled-cucumber", Some("en:pickle"), "fermented"),
            ]
        );
        assert_eq!(decisions.included["produce"], 3);
        assert_eq!(decisions.excluded["pattern: flavourings"], 1);
        assert_eq!(decisions.excluded["pattern: additive codes"], 1);
        assert_eq!(decisions.excluded["no English canonical id"], 1);
        assert_eq!(decisions.excluded["excluded subtree"], 2);
    }

    #[test]
    fn a_bad_pattern_is_an_error() {
        let mut bad = rules();
        bad.exclude[0].pattern = "(".to_owned();
        assert!(select(&parse(SAMPLE), &bad).is_err());
    }
}
