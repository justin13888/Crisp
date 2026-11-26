//! The committed ingredient overrides this server was built with
//! (`tools/ingredients/rules/overrides/`), embedded by `build.rs`.
//!
//! The export merges its lines into the keys these files already hold: an
//! appended line for a key the file has would be a duplicate JSON key, and
//! the importer's last-wins read would silently drop the curated line. The
//! embedded index is generated from these same files, so they describe the
//! repository at the index's `VERSION`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::Internal;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/overrides.rs"));
}

/// One entry's names override in one shard, as `names/<shard>.json` holds
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameOverride {
    /// The display name it sets, if any.
    #[serde(default)]
    pub name: Option<String>,
    /// The aliases it adds.
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// The override files' existing keys and values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    /// `names/<shard>.json`: shard → slug → override.
    pub names: BTreeMap<String, BTreeMap<String, NameOverride>>,
    /// `merges.json`: retired slug → successor.
    pub merges: BTreeMap<String, String>,
}

impl Overrides {
    /// Parses override files: `names` as (shard, file contents) pairs, and
    /// `merges` as the contents of `merges.json`.
    ///
    /// # Errors
    ///
    /// Returns the parse error of the first file that is not an override
    /// file.
    pub fn parse(names: &[(&str, &str)], merges: &str) -> Result<Self, serde_json::Error> {
        let names = names
            .iter()
            .map(|(shard, text)| Ok(((*shard).to_owned(), serde_json::from_str(text)?)))
            .collect::<Result<_, serde_json::Error>>()?;
        Ok(Self {
            names,
            merges: serde_json::from_str(merges)?,
        })
    }

    /// The overrides embedded at build time, parsed once.
    ///
    /// # Errors
    ///
    /// Returns [`Internal`] if an embedded file does not parse, which the
    /// importer's own checks rule out for a committed file.
    pub fn embedded() -> Result<&'static Self, Internal> {
        static PARSED: OnceLock<Result<Overrides, String>> = OnceLock::new();
        let parsed = PARSED.get_or_init(|| {
            Self::parse(embedded::NAMES, embedded::MERGES).map_err(|e| e.to_string())
        });
        parsed
            .as_ref()
            .map_err(|e| Internal::logged("parsing the embedded ingredient overrides", e))
    }

    /// The override for `slug` in `shard`, if the file holds one.
    #[must_use]
    pub fn name(&self, shard: &str, slug: &str) -> Option<&NameOverride> {
        self.names.get(shard)?.get(slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_overrides_parse_and_hold_the_committed_keys() {
        let overrides = Overrides::embedded().unwrap();
        assert!(overrides.names.contains_key("en-US"));
        let aged = overrides.name("en-US", "kimchi-aged").unwrap();
        assert_eq!(aged.name.as_deref(), Some("aged kimchi"));
        assert!(overrides.name("en-US", "no-such-slug").is_none());
    }

    #[test]
    fn a_file_that_is_not_an_override_file_is_refused() {
        assert!(Overrides::parse(&[("en-US", r#"{"x": {"nom": "y"}}"#)], "{}").is_err());
        assert!(Overrides::parse(&[], "[]").is_err());
        let empty = Overrides::parse(&[("en-US", "{}")], "{}").unwrap();
        assert!(empty.merges.is_empty() && empty.names["en-US"].is_empty());
    }
}
