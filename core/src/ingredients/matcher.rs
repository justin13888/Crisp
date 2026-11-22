//! Linking a typed ingredient name to index slugs.
//!
//! The editor contract (`docs/ingredient-index.md`): a name that is, in the
//! first shard of its fallback chain that knows it at all, an exact spelling
//! — accents included — of exactly one entry links automatically
//! ([`Match::Exact`]). Anything
//! else — an ambiguous spelling, a prefix, a partial word — is offered as
//! ranked [`Match::Candidates`] for the author to pick. Matching never
//! rewrites the author's `name`.

use std::collections::BTreeMap;

use tracing::trace;
use unicode_normalization::UnicodeNormalization;

use super::IngredientIndex;

/// The outcome of matching a typed name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    /// One unambiguous entry: safe to link without asking.
    Exact(String),
    /// Entries worth suggesting, best first.
    Candidates(Vec<Candidate>),
    /// Nothing in the index resembles the name.
    None,
}

/// One suggested entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The current slug (after `replaced_by`).
    pub slug: String,
    /// Higher is better; only meaningful relative to other candidates.
    pub score: u32,
    /// The index spelling that matched, as written in the dataset.
    pub spelling: String,
    /// The shard the spelling came from.
    pub shard: String,
    /// Whether that shard is exactly the tag asked about (not a fallback).
    pub exact_region: bool,
}

/// How a spelling relates to the typed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fit {
    /// Contains the text somewhere.
    Contains = 200,
    /// A later word starts with the text.
    WordPrefix = 400,
    /// Starts with the text.
    Prefix = 600,
    /// Is the text.
    Exact = 1000,
}

/// The score lost per step down the fallback chain.
const CHAIN_STEP: u32 = 25;
/// The score bonus for a spelling in exactly the asked-for shard.
const EXACT_REGION_BONUS: u32 = 50;

/// Folds a name for comparison: lowercase, compatibility-normalized, Latin
/// and Greek accents dropped (`crème` → `creme`), punctuation and hyphens
/// turned to spaces, whitespace collapsed. Scripts whose marks carry meaning
/// (Thai vowels, Japanese dakuten, Hangul) keep them. Idempotent.
#[must_use]
pub fn normalize(text: &str) -> String {
    fixpoint(text, true)
}

/// Like [`normalize`] but keeping every accent: the key an automatic link
/// must match, so `bò` (beef) never links to `bơ` (butter), nor `pâté` to
/// `pâte` (dough).
pub(super) fn exact_key(text: &str) -> String {
    fixpoint(text, false)
}

/// Repeats [`fold`] enough times to reach its fixed point (case mapping and
/// compatibility decomposition can each expose work for the other; three
/// passes settle every input the idempotence property test has found).
fn fixpoint(text: &str, strip_accents: bool) -> String {
    (0..3).fold(text.to_owned(), |acc, _| fold(&acc, strip_accents))
}

/// One pass of [`normalize`] (or of [`exact_key`], keeping accents).
fn fold(text: &str, strip_accents: bool) -> String {
    let lowered = text.to_lowercase();
    let stripped: String = lowered
        .nfkd()
        .filter(|c| !(strip_accents && ('\u{0300}'..='\u{036F}').contains(c)))
        .nfc()
        .collect::<String>()
        .to_lowercase();
    let spaced: String = stripped
        .chars()
        .map(|c| if is_separator(c) { ' ' } else { c })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a character only separates words for matching purposes.
fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || c.is_ascii_punctuation()
        || ('\u{2010}'..='\u{205E}').contains(&c) // general punctuation
        || ('\u{3000}'..='\u{303F}').contains(&c) // CJK symbols and punctuation
        || ('\u{FE30}'..='\u{FE4F}').contains(&c) // CJK compatibility forms
        || matches!(c, '\u{00B7}' | '\u{30FB}' | '\u{00A1}' | '\u{00BF}' | '\u{00AB}' | '\u{00BB}')
}

/// How `spelling` fits `text`, both already normalized.
fn fit(spelling: &str, text: &str) -> Option<Fit> {
    if spelling == text {
        Some(Fit::Exact)
    } else if spelling.starts_with(text) {
        Some(Fit::Prefix)
    } else if spelling
        .split(' ')
        .skip(1)
        .any(|word| word.starts_with(text))
    {
        Some(Fit::WordPrefix)
    } else if text.chars().count() >= 2 && spelling.contains(text) {
        Some(Fit::Contains)
    } else {
        None
    }
}

impl IngredientIndex {
    /// Matches a typed ingredient name in language `tag` against the index,
    /// returning at most `limit` candidates when there is no exact link.
    #[must_use]
    pub fn match_name(&self, text: &str, tag: &str, limit: usize) -> Match {
        let wanted = normalize(text);
        if wanted.is_empty() || limit == 0 {
            return Match::None;
        }
        let chain = self.fallback_chain(tag);
        let current = |slug: &str| -> String {
            self.resolve(slug)
                .map_or(slug, |e| e.slug.as_str())
                .to_owned()
        };
        let wanted_exact = exact_key(text);
        let first_hit = chain.iter().find_map(|shard| {
            let found = self.spellings.get(*shard)?.get(&wanted)?;
            let mut slugs: Vec<String> = found
                .iter()
                .filter(|(_, spelling)| exact_key(spelling) == wanted_exact)
                .map(|(s, _)| current(s))
                .collect();
            slugs.sort_unstable();
            slugs.dedup();
            Some(slugs)
        });
        if let Some([only]) = first_hit.as_deref() {
            trace!(text, tag, slug = %only, "ingredient name links exactly");
            return Match::Exact(only.clone());
        }
        let wanted_len = wanted.chars().count();
        let mut best: BTreeMap<String, Candidate> = BTreeMap::new();
        for (rank, shard) in chain.iter().enumerate() {
            let Some(table) = self.spellings.get(*shard) else {
                continue;
            };
            let exact_region = rank == 0 && shard.eq_ignore_ascii_case(tag.trim());
            let penalty = CHAIN_STEP.saturating_mul(u32::try_from(rank).unwrap_or(u32::MAX));
            for (folded, found) in table {
                let Some(fit) = fit(folded, &wanted) else {
                    continue;
                };
                let gap = folded.chars().count().saturating_sub(wanted_len).min(99);
                let score = (fit as u32 + if exact_region { EXACT_REGION_BONUS } else { 0 })
                    .saturating_sub(penalty)
                    .saturating_sub(u32::try_from(gap).unwrap_or(99));
                for (slug, spelling) in found {
                    let slug = current(slug);
                    if best.get(&slug).is_some_and(|kept| kept.score >= score) {
                        continue;
                    }
                    best.insert(
                        slug.clone(),
                        Candidate {
                            slug,
                            score,
                            spelling: spelling.clone(),
                            shard: (*shard).to_owned(),
                            exact_region,
                        },
                    );
                }
            }
        }
        let mut ranked: Vec<Candidate> = best.into_values().collect();
        ranked.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.slug.cmp(&b.slug)));
        ranked.truncate(limit);
        trace!(
            text,
            tag,
            candidates = ranked.len(),
            "ingredient name suggestions"
        );
        if ranked.is_empty() {
            Match::None
        } else {
            Match::Candidates(ranked)
        }
    }
}
