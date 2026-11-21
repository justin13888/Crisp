//! The committed import rules (`tools/ingredients/rules/rules.toml`).
//!
//! Every decision the importer makes is a rule over a *class* of source
//! entries — a subtree, a name pattern, a match kind. Decisions about one
//! entry live only in the overrides files. Changing the index means editing
//! a rule or an override and regenerating, never editing the output.

use std::collections::BTreeMap;
use std::path::Path;

use eyre::{Context, Result};
use serde::Deserialize;

/// The whole rules file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Pinned source snapshots.
    pub sources: Sources,
    /// Locale shards and fallback.
    pub locales: Locales,
    /// Which source entries become index entries.
    pub select: Select,
    /// Hint mappings from source properties to Crisp codes.
    pub hints: Hints,
    /// How densities are derived and which USDA matches are accepted.
    pub density: Density,
}

/// One pinned download.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Where to fetch it.
    pub url: String,
    /// The file name inside the cache directory.
    pub file: String,
    /// The SHA-256 (hex) the file must have.
    pub sha256: String,
}

/// Every pinned snapshot.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    /// The Open Food Facts ingredients taxonomy.
    pub off: Source,
    /// The USDA FoodData Central SR Legacy CSV archive.
    pub fdc: Source,
    /// OpenCC dictionaries, keyed by dictionary name (e.g. `STPhrases`).
    pub opencc: BTreeMap<String, Source>,
}

impl Sources {
    /// Every source, for fetching and verification.
    pub fn all(&self) -> Vec<&Source> {
        let mut out = vec![&self.off, &self.fdc];
        out.extend(self.opencc.values());
        out
    }
}

/// Locale shards and how source languages and tags map onto them.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locales {
    /// Every shard, in declared order.
    pub shards: Vec<String>,
    /// Shards tried after everything else.
    pub last_resort: Vec<String>,
    /// Lowercase tag → shards to try after an exact match.
    pub fallback: BTreeMap<String, Vec<String>>,
    /// OFF language code → the shard its names fill.
    pub off_language: BTreeMap<String, String>,
    /// Shards derived by script conversion from another shard.
    #[serde(default)]
    pub derive: Vec<Derive>,
    /// The English regional lexicon file, relative to the rules directory.
    pub english_lexicon: String,
    /// Which shards hold US, British, and Canadian English.
    pub english: EnglishShards,
}

/// The shards the English lexicon writes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnglishShards {
    /// US English; receives the source's English names.
    pub us: String,
    /// British English; receives names only where they differ.
    pub gb: String,
    /// Canadian English; receives names only where they differ.
    pub ca: String,
}

/// A shard derived from another by OpenCC conversion; when `shard` equals
/// `from`, the shard is converted in place (e.g. to normalize script).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Derive {
    /// The derived shard, e.g. `zh-Hant-TW`.
    pub shard: String,
    /// The shard converted from, e.g. `zh-Hans-CN`.
    pub from: String,
    /// Conversion stages, each a list of OpenCC dictionary names matched
    /// together, longest match first.
    pub stages: Vec<Vec<String>>,
}

/// A subtree of the source taxonomy and the category it maps to.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtree {
    /// The OFF id of the subtree root, e.g. `en:vegetable`.
    pub id: String,
    /// The Crisp category its entries get (the deepest mapped ancestor wins).
    pub category: String,
}

/// A named exclusion over English canonical ids.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    /// A short name, used in the report.
    pub name: String,
    /// A regular expression matched against the OFF id without its `en:`.
    pub pattern: String,
}

/// Which entries are imported.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Select {
    /// Subtrees to include, each mapped to a category.
    pub include: Vec<Subtree>,
    /// Subtrees to drop even inside an included subtree.
    #[serde(default)]
    pub exclude_subtrees: Vec<String>,
    /// Name patterns to drop.
    #[serde(default)]
    pub exclude: Vec<Exclusion>,
}

/// Mappings from OFF properties to Crisp hint codes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hints {
    /// OFF allergen id (e.g. `en:sesame-seeds`) → Crisp allergen code.
    pub allergens: BTreeMap<String, String>,
    /// OFF analysis property (`vegan`, `vegetarian`) whose value `no` means
    /// a conflict with the named Crisp diet code.
    pub diet_when_no: BTreeMap<String, String>,
    /// Crisp allergen code → diet codes it conflicts with.
    pub diet_from_allergen: BTreeMap<String, Vec<String>>,
}

/// Density derivation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Density {
    /// USDA match kinds whose candidates are accepted (`linked`, `exact`).
    pub accept: Vec<String>,
    /// Portion measure words → millilitres per one of them.
    pub volume_units: BTreeMap<String, f64>,
    /// The fixed denominator densities are rounded to.
    pub denominator: u64,
}

/// Reads and parses the rules file.
///
/// # Errors
/// When the file cannot be read or does not match the schema.
pub fn load(path: &Path) -> Result<Rules> {
    let text =
        std::fs::read_to_string(path).wrap_err_with(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_rules_load_and_list_every_source() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("rules/rules.toml");
        let rules = load(&path).expect("rules.toml parses");
        let all = rules.sources.all();
        assert_eq!(all.len(), 2 + rules.sources.opencc.len());
        assert_eq!(all[0].file, rules.sources.off.file);
        assert!(all.iter().all(|s| s.sha256.len() == 64));
        assert!(load(Path::new("/nonexistent/rules.toml")).is_err());
    }
}
