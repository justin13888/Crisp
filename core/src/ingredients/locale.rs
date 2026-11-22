//! Locale shards and the best-effort fallback chain across them.
//!
//! Names are sharded by region-qualified tag (`en-GB`, `zh-Hant-HK`), never
//! by bare language: the same language names ingredients differently across
//! regions. A recipe's text, though, may be tagged broadly (`en`, `zh-Hant`)
//! or with a region that has no shard (`en-AU`), so every lookup walks a
//! chain: the exact shard, the tag's fallback list and those of each
//! truncation of it, the language's other shards, then English.

use super::IndexError;
use super::dataset::LocaleTable;

/// Checks that every tag the table mentions is a declared shard.
pub(super) fn validate(table: &LocaleTable) -> Result<(), IndexError> {
    if let Some(key) = table
        .fallback
        .keys()
        .find(|k| **k != k.to_ascii_lowercase())
    {
        return Err(IndexError::FallbackKey(key.clone()));
    }
    let mentioned = table.fallback.values().flatten().chain(&table.last_resort);
    for tag in mentioned {
        if !table.shards.contains(tag) {
            return Err(IndexError::UnknownShard(tag.clone()));
        }
    }
    Ok(())
}

/// The primary language subtag, lowercased.
fn language(tag: &str) -> String {
    tag.split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The shards to consult for `tag`, best first, each once.
pub(super) fn chain<'a>(table: &'a LocaleTable, tag: &str) -> Vec<&'a str> {
    let lower = tag.trim().to_ascii_lowercase();
    let mut out: Vec<&'a str> = Vec::new();
    let mut push = |shard: &'a str| {
        if !out.contains(&shard) {
            out.push(shard);
        }
    };
    if let Some(exact) = table.shards.iter().find(|s| s.eq_ignore_ascii_case(&lower)) {
        push(exact);
    }
    let mut key = lower.as_str();
    loop {
        if let Some((_, list)) = table.fallback.get_key_value(key) {
            for shard in list {
                push(shard);
            }
        }
        match key.rfind('-') {
            Some(cut) => key = &key[..cut],
            None => break,
        }
    }
    let lang = language(&lower);
    for shard in &table.shards {
        if language(shard) == lang {
            push(shard);
        }
    }
    for shard in &table.last_resort {
        push(shard);
    }
    out
}
