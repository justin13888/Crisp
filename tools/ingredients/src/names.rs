//! Filling locale shards with names.
//!
//! Source languages map onto their default shard; English goes through the
//! regional lexicon; derived shards are converted from their source shard;
//! overrides replace display names and add aliases. A shard never receives
//! a copy of another shard's names to fill a gap — readers fall back.

use std::collections::{BTreeMap, BTreeSet};

use crisp_core::ingredients::{Names, normalize};
use serde::Deserialize;

use crate::english::{Lexicon, Region};
use crate::opencc::Converter;
use crate::rules::EnglishShards;

/// Shard → slug → names.
pub type ShardNames = BTreeMap<String, BTreeMap<String, Names>>;

/// An override for one entry's names in one shard.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameOverride {
    /// A new display name; the old one becomes an alias.
    #[serde(default)]
    pub name: Option<String>,
    /// Extra aliases.
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// Builds names from a display-first list, dropping blanks and spellings
/// that normalize alike.
#[must_use]
pub fn from_list(list: &[String]) -> Option<Names> {
    let mut seen = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for raw in list {
        let name = raw.trim();
        let key = normalize(name);
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        kept.push(name.to_owned());
    }
    let mut kept = kept.into_iter();
    let name = kept.next()?;
    Some(Names {
        name,
        aliases: kept.collect(),
    })
}

/// The source's English spellings (normalized) → the slugs that own them.
pub type Owned = BTreeMap<String, BTreeSet<String>>;

/// Adds one entry's source names (OFF language → names) to the shards.
///
/// A lexicon rewrite is dropped when another entry owns that spelling in the
/// source: the lexicon renames a term, it never merges two entries.
pub fn add_source(
    shards: &mut ShardNames,
    slug: &str,
    by_language: &BTreeMap<String, Vec<String>>,
    language_shard: &BTreeMap<String, String>,
    english: &EnglishShards,
    lexicon: &Lexicon,
    owned: &Owned,
) {
    for (lang, list) in by_language {
        let Some(shard) = language_shard.get(lang) else {
            continue;
        };
        if *shard != english.us {
            if let Some(names) = from_list(list) {
                shards
                    .entry(shard.clone())
                    .or_default()
                    .insert(slug.to_owned(), names);
            }
            continue;
        }
        let foreign = |name: &String| {
            owned
                .get(&normalize(name))
                .is_some_and(|slugs| slugs.iter().any(|s| s != slug))
        };
        let toward = |region: Region| -> Vec<String> {
            let mut out: Vec<String> = list
                .iter()
                .map(|n| lexicon.rewrite(n, region))
                .filter(|n| !foreign(n))
                .collect();
            out.extend(list.iter().cloned());
            out
        };
        let Some(us) = from_list(&toward(Region::Us)) else {
            continue;
        };
        for (region, shard) in [(Region::Gb, &english.gb), (Region::Ca, &english.ca)] {
            let names = from_list(&toward(region));
            if let Some(names) = names.filter(|n| normalize(&n.name) != normalize(&us.name)) {
                shards
                    .entry(shard.clone())
                    .or_default()
                    .insert(slug.to_owned(), names);
            }
        }
        shards
            .entry(english.us.clone())
            .or_default()
            .insert(slug.to_owned(), us);
    }
}

/// Applies overrides for some shards.
pub fn apply_overrides(
    shards: &mut ShardNames,
    overrides: &BTreeMap<String, BTreeMap<String, NameOverride>>,
) {
    for (shard, by_slug) in overrides {
        for (slug, o) in by_slug {
            let existing = shards.get(shard).and_then(|s| s.get(slug)).cloned();
            let mut list: Vec<String> = Vec::new();
            list.extend(o.name.clone());
            if let Some(names) = existing {
                list.push(names.name);
                list.extend(names.aliases);
            }
            list.extend(o.aliases.iter().cloned());
            if let Some(names) = from_list(&list) {
                shards
                    .entry(shard.clone())
                    .or_default()
                    .insert(slug.clone(), names);
            }
        }
    }
}

/// Converts every name of `from` into `to` (replacing what `to` held for
/// those slugs).
pub fn derive(shards: &mut ShardNames, from: &str, to: &str, converter: &Converter) {
    let Some(source) = shards.get(from).cloned() else {
        return;
    };
    let target = shards.entry(to.to_owned()).or_default();
    for (slug, names) in source {
        let list: Vec<String> = std::iter::once(&names.name)
            .chain(&names.aliases)
            .map(|n| converter.convert(n))
            .collect();
        if let Some(converted) = from_list(&list) {
            target.insert(slug, converted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::english::Term;
    use crate::opencc::Stage;

    fn english() -> EnglishShards {
        EnglishShards {
            us: "en-US".to_owned(),
            gb: "en-GB".to_owned(),
            ca: "en-CA".to_owned(),
        }
    }

    fn lexicon() -> Lexicon {
        Lexicon::new(vec![Term {
            us: "eggplant".to_owned(),
            gb: "aubergine".to_owned(),
            ca: "eggplant".to_owned(),
        }])
        .expect("lexicon")
    }

    fn list(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn english_splits_by_region_and_other_languages_map_to_their_shard() {
        let mut shards = ShardNames::new();
        let by_language = BTreeMap::from([
            ("en".to_owned(), list(&["aubergine", "eggplant"])),
            ("fr".to_owned(), list(&["aubergine"])),
            ("xx".to_owned(), list(&["ignored"])),
        ]);
        let map = BTreeMap::from([
            ("en".to_owned(), "en-US".to_owned()),
            ("fr".to_owned(), "fr-FR".to_owned()),
        ]);
        add_source(
            &mut shards,
            "eggplant",
            &by_language,
            &map,
            &english(),
            &lexicon(),
            &Owned::new(),
        );
        assert_eq!(shards["en-US"]["eggplant"].name, "eggplant");
        assert_eq!(shards["en-US"]["eggplant"].aliases, ["aubergine"]);
        assert_eq!(shards["en-GB"]["eggplant"].name, "aubergine");
        assert!(!shards.contains_key("en-CA"), "Canada matches the US");
        assert_eq!(shards["fr-FR"]["eggplant"].name, "aubergine");
        assert_eq!(shards.len(), 3);
    }

    #[test]
    fn a_rewrite_never_takes_a_spelling_another_entry_owns() {
        let mut shards = ShardNames::new();
        let by_language = BTreeMap::from([("en".to_owned(), list(&["aubergine"]))]);
        let map = BTreeMap::from([("en".to_owned(), "en-US".to_owned())]);
        let owned = Owned::from([(
            "eggplant".to_owned(),
            BTreeSet::from(["eggplant".to_owned()]),
        )]);
        add_source(
            &mut shards,
            "aubergine",
            &by_language,
            &map,
            &english(),
            &lexicon(),
            &owned,
        );
        assert_eq!(shards["en-US"]["aubergine"].name, "aubergine");
        assert!(shards["en-US"]["aubergine"].aliases.is_empty());
    }

    #[test]
    fn overrides_replace_display_names_and_derivation_converts() {
        let mut shards = ShardNames::new();
        shards.entry("zh-Hans-CN".to_owned()).or_default().insert(
            "kimchi".to_owned(),
            from_list(&list(&["泡菜"])).expect("names"),
        );
        let overrides = BTreeMap::from([(
            "zh-Hans-CN".to_owned(),
            BTreeMap::from([(
                "kimchi".to_owned(),
                NameOverride {
                    name: Some("韩国泡菜".to_owned()),
                    aliases: list(&["辣白菜", "泡菜"]),
                },
            )]),
        )]);
        apply_overrides(&mut shards, &overrides);
        assert_eq!(shards["zh-Hans-CN"]["kimchi"].name, "韩国泡菜");
        assert_eq!(shards["zh-Hans-CN"]["kimchi"].aliases, ["泡菜", "辣白菜"]);
        let conv = Converter::new(vec![Stage::from_texts(&["韩\t韓\n国\t國\n"])]);
        derive(&mut shards, "zh-Hans-CN", "zh-Hant-TW", &conv);
        assert_eq!(shards["zh-Hant-TW"]["kimchi"].name, "韓國泡菜");
        derive(&mut shards, "nowhere", "zh-Hant-HK", &conv);
        assert!(!shards.contains_key("zh-Hant-HK"));
    }

    #[test]
    fn duplicate_and_blank_spellings_collapse() {
        let names =
            from_list(&list(&["  Fish sauce", "fish-sauce", "", "nuoc mam"])).expect("names");
        assert_eq!(names.name, "Fish sauce");
        assert_eq!(names.aliases, ["nuoc mam"]);
        assert!(from_list(&list(&["", " "])).is_none());
    }
}
