//! Regional English names from a phrase lexicon.
//!
//! The source has one English: a mix of US and UK usage. The lexicon lists
//! culinary terms that differ by region (`eggplant` / `aubergine`) and is
//! applied to *every* name as whole-word, case-insensitive phrase rewriting,
//! plural suffix kept — a class rule, not a per-entry decision. A regional
//! shard gets names only where its rewriting differs from US English; else
//! the reader falls back.

use std::path::Path;

use eyre::{Context, Result};
use regex::Regex;
use serde::Deserialize;

/// One term in its regional spellings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    /// The US English phrase.
    pub us: String,
    /// The British English phrase.
    pub gb: String,
    /// The Canadian English phrase.
    pub ca: String,
}

/// The lexicon file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    term: Vec<Term>,
}

/// A region to rewrite toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// United States.
    Us,
    /// United Kingdom.
    Gb,
    /// Canada.
    Ca,
}

/// Compiled phrase rewriting toward each region.
#[derive(Debug, Clone)]
pub struct Lexicon {
    /// (pattern matching any spelling of a term, the term).
    rules: Vec<(Regex, Term)>,
}

impl Lexicon {
    /// Compiles a lexicon from its terms; longer phrases are tried first.
    ///
    /// # Errors
    /// When a term cannot be compiled into a pattern.
    pub fn new(mut terms: Vec<Term>) -> Result<Self> {
        terms.sort_by_key(|t| std::cmp::Reverse(t.us.len().max(t.gb.len()).max(t.ca.len())));
        let mut rules = Vec::new();
        for term in terms {
            let mut spellings = vec![
                regex::escape(&term.us),
                regex::escape(&term.gb),
                regex::escape(&term.ca),
            ];
            spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
            spellings.dedup();
            let pattern = format!(r"(?i)\b(?:{})(s|es)?\b", spellings.join("|"));
            let re = Regex::new(&pattern).wrap_err_with(|| format!("term {:?}", term.us))?;
            rules.push((re, term));
        }
        Ok(Self { rules })
    }

    /// Loads the lexicon file.
    ///
    /// # Errors
    /// When the file cannot be read, parsed, or compiled.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("reading {}", path.display()))?;
        let file: File =
            toml::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))?;
        Self::new(file.term)
    }

    /// Rewrites every lexicon phrase in `name` toward `region`.
    #[must_use]
    pub fn rewrite(&self, name: &str, region: Region) -> String {
        let mut out = name.to_owned();
        for (re, term) in &self.rules {
            let target = match region {
                Region::Us => &term.us,
                Region::Gb => &term.gb,
                Region::Ca => &term.ca,
            };
            out = re
                .replace_all(&out, |caps: &regex::Captures<'_>| {
                    format!("{target}{}", caps.get(1).map_or("", |m| m.as_str()))
                })
                .into_owned();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon() -> Lexicon {
        let term = |us: &str, gb: &str, ca: &str| Term {
            us: us.to_owned(),
            gb: gb.to_owned(),
            ca: ca.to_owned(),
        };
        Lexicon::new(vec![
            term("eggplant", "aubergine", "eggplant"),
            term("heavy cream", "double cream", "whipping cream"),
            term("cream", "cream", "cream"),
        ])
        .expect("lexicon")
    }

    #[test]
    fn phrases_rewrite_whole_words_toward_a_region() {
        let lx = lexicon();
        assert_eq!(
            lx.rewrite("grilled aubergines", Region::Us),
            "grilled eggplants"
        );
        assert_eq!(lx.rewrite("Eggplant", Region::Gb), "aubergine");
        assert_eq!(lx.rewrite("heavy cream", Region::Ca), "whipping cream");
        assert_eq!(lx.rewrite("double cream", Region::Us), "heavy cream");
        assert_eq!(lx.rewrite("ice cream", Region::Gb), "ice cream");
        assert_eq!(lx.rewrite("eggplantine", Region::Gb), "eggplantine");
    }
}
