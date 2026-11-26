//! Accepted suggestions as override patches (`docs/ingredient-index.md` §6,
//! step 3).
//!
//! The server has no checkout of the repository, so a patch is the lines to
//! change in one file of `tools/ingredients/rules/overrides/`, in that
//! file's one-line-per-entry form; an administrator pastes them into the
//! upstream pull request. Lines are rendered against the override files
//! this server was built with ([`Overrides`]): a key the file lacks is a
//! line to add, and a key it already holds is a line that replaces the
//! existing one, merged with it, so no curated name or alias is dropped by
//! a duplicate key. What only a maintainer can decide — the permanent slug
//! and category of a new entry, a rule for a class of names, anything else
//! — is a note instead.
//!
//! - `name` and `regional_spelling` → `names/<locale>.json`: the text
//!   becomes the entry's display name in that shard when neither the index
//!   nor the existing override gives it one, and an alias otherwise. A
//!   regional spelling also gets a note: a spelling shared by a class of
//!   entries belongs in `en-regional.toml`.
//! - `merge` → `merges.json`: `"slug": "successor"`. A merge the file
//!   already holds is left out; a second merge of the same slug to another
//!   successor, accepted or already in the file, is a note.
//! - `new_entry` and `other` → a note.

use std::collections::BTreeMap;

use crisp_core::ingredients::IngredientIndex;
use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::overrides::{NameOverride, Overrides};
use super::suggestions::SuggestionKind;
use crate::db::ingredient_suggestion;
use crate::error::Internal;

/// The directory every patch path is relative to.
pub const OVERRIDES: &str = "tools/ingredients/rules/overrides";

/// The lines to change in one override file.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct OverridePatch {
    /// The file, from the repository root, e.g.
    /// `tools/ingredients/rules/overrides/names/en-GB.json`.
    pub path: String,
    /// The lines to add inside the file's top-level object, one per entry,
    /// without trailing commas; the file holds none of their keys.
    pub lines: Vec<String>,
    /// Lines whose key the file already holds: each replaces the existing
    /// line for its key and already carries that line's name and aliases.
    pub replace: Vec<String>,
}

/// An accepted suggestion a maintainer must turn into a change by hand.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct ExportNote {
    /// The suggestion id.
    pub suggestion_id: i64,
    /// What it proposes.
    pub kind: SuggestionKind,
    /// The entry it is about, if it names one.
    pub slug: Option<String>,
    /// The locale shard it is for.
    pub locale: String,
    /// The proposed text.
    pub text: String,
    /// The suggester's explanation (possibly empty).
    pub note: String,
    /// What the maintainer should do with it.
    pub action: String,
}

/// The `GET /admin/ingredients/suggestions/export` response body.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct SuggestionExport {
    /// The `VERSION` of the ingredient index the patches were rendered
    /// against.
    pub index_version: String,
    /// The ids of the accepted suggestions exported, in acceptance order.
    pub suggestions: Vec<i64>,
    /// The acceptance time of the last suggestion exported (epoch seconds);
    /// pass it as `since`, with `last_id` as `after`, to continue an
    /// incomplete export.
    pub last_accepted_at: Option<i64>,
    /// The id of the last suggestion exported; pass it as `after` to
    /// continue an incomplete export.
    pub last_id: Option<i64>,
    /// Whether every accepted suggestion since `since` is included; `false`
    /// when the export reached its row cap.
    pub complete: bool,
    /// The override patches, one per file, ordered by path.
    pub files: Vec<OverridePatch>,
    /// The suggestions that need a maintainer's decision.
    pub notes: Vec<ExportNote>,
}

/// One entry's line in one shard's names file: the existing override's
/// name and aliases, if the file has one, plus what accepted suggestions
/// add to it.
#[derive(Debug)]
struct NameLine {
    /// The display name: the existing override's, or one a suggestion
    /// gives an entry the shard has no name for.
    name: Option<String>,
    /// The existing override's aliases, then the accepted ones in the
    /// order they were accepted.
    aliases: Vec<String>,
    /// Whether the file already holds this key.
    existing: bool,
    /// Whether a suggestion added anything to the line.
    changed: bool,
}

impl NameLine {
    /// The line for a key, starting from the file's `existing` override.
    fn new(existing: Option<&NameOverride>) -> Self {
        Self {
            existing: existing.is_some(),
            name: existing.and_then(|o| o.name.clone()),
            aliases: existing.map(|o| o.aliases.clone()).unwrap_or_default(),
            changed: false,
        }
    }

    /// Adds `text`: as the display name when `unnamed` and the line sets
    /// none, and otherwise as an alias, unless the line already has it.
    fn add(&mut self, text: &str, unnamed: bool) {
        if self.name.as_deref() == Some(text) || self.aliases.iter().any(|a| a == text) {
            return;
        }
        if unnamed && self.name.is_none() {
            self.name = Some(text.to_owned());
        } else {
            self.aliases.push(text.to_owned());
        }
        self.changed = true;
    }

    /// The line as `names/<shard>.json` writes it.
    fn render(&self, slug: &str) -> Result<String, Internal> {
        let mut fields = Vec::new();
        if let Some(name) = &self.name {
            fields.push(format!("\"name\": {}", json(name)?));
        }
        if !self.aliases.is_empty() {
            let aliases: Vec<String> = self.aliases.iter().map(json).collect::<Result<_, _>>()?;
            fields.push(format!("\"aliases\": [{}]", aliases.join(", ")));
        }
        Ok(format!("{}: {{{}}}", json(slug)?, fields.join(", ")))
    }
}

/// The patch for `path` in `files`, created empty if it is not there yet.
fn patch(files: &mut BTreeMap<String, OverridePatch>, path: String) -> &mut OverridePatch {
    files.entry(path.clone()).or_insert_with(|| OverridePatch {
        path,
        lines: Vec::new(),
        replace: Vec::new(),
    })
}

/// `value` as a JSON string literal.
fn json<T: Serialize + ?Sized>(value: &T) -> Result<String, Internal> {
    serde_json::to_string(value).map_err(|e| Internal::logged("rendering an override line", &e))
}

/// A note for `row`, asking the maintainer to `action`.
fn note(row: &ingredient_suggestion::Model, kind: SuggestionKind, action: &str) -> ExportNote {
    ExportNote {
        suggestion_id: row.id,
        kind,
        slug: row.slug.clone(),
        locale: row.locale.clone(),
        text: row.text.clone(),
        note: row.note.clone(),
        action: action.to_owned(),
    }
}

/// The action note for a new entry.
const NEW_ENTRY: &str = "choose a permanent slug and a category, add the entry to \
    overrides/entries.json (parent: the suggestion's slug, source: \"crisp:override\"), \
    and its name to overrides/names/<locale>.json";
/// The action note for a regional spelling.
const REGIONAL: &str = "the names patch adds this spelling to this entry only; if the \
    regions name a whole class of entries this way, add a [[term]] to \
    tools/ingredients/rules/en-regional.toml instead";
/// The action note for anything else.
const OTHER: &str = "review by hand: a class-level fix is a rule change in \
    tools/ingredients/rules/, a one-off is an override";

/// Renders accepted suggestions `rows` as override patches against `index`
/// and the override files `overrides` the index was generated from.
///
/// `complete` is whether `rows` holds every accepted suggestion asked for.
///
/// # Errors
///
/// Returns [`Internal`] for a row whose stored kind no version of the server
/// writes, or a line that cannot be rendered.
pub fn export(
    rows: &[ingredient_suggestion::Model],
    index: &IngredientIndex,
    overrides: &Overrides,
    complete: bool,
) -> Result<SuggestionExport, Internal> {
    let mut names: BTreeMap<(String, String), NameLine> = BTreeMap::new();
    let mut merges: BTreeMap<String, String> = BTreeMap::new();
    let mut notes = Vec::new();
    for row in rows {
        let kind = SuggestionKind::stored(&row.kind)?;
        match (kind, row.slug.as_deref()) {
            (SuggestionKind::Name | SuggestionKind::RegionalSpelling, Some(slug)) => {
                let line = names
                    .entry((row.locale.clone(), slug.to_owned()))
                    .or_insert_with(|| NameLine::new(overrides.name(&row.locale, slug)));
                line.add(&row.text, index.names(slug, &row.locale).is_none());
                if kind == SuggestionKind::RegionalSpelling {
                    notes.push(note(row, kind, REGIONAL));
                }
            }
            (SuggestionKind::Merge, Some(slug)) => {
                let committed = overrides.merges.get(slug).map(|s| (s, "merges.json holds"));
                let accepted = merges.get(slug).map(|s| (s, "conflicts with an accepted"));
                match committed.or(accepted) {
                    None => {
                        merges.insert(slug.to_owned(), row.text.clone());
                    }
                    Some((successor, _)) if *successor == row.text => {}
                    Some((successor, source)) => {
                        let action =
                            format!("{source} merge of {slug:?} into {successor:?}; keep one");
                        notes.push(note(row, kind, &action));
                    }
                }
            }
            (SuggestionKind::NewEntry, _) => notes.push(note(row, kind, NEW_ENTRY)),
            _ => notes.push(note(row, kind, OTHER)),
        }
    }

    let mut files: BTreeMap<String, OverridePatch> = BTreeMap::new();
    for ((shard, slug), line) in names.iter().filter(|(_, line)| line.changed) {
        let patch = patch(&mut files, format!("{OVERRIDES}/names/{shard}.json"));
        let rendered = line.render(slug)?;
        if line.existing {
            patch.replace.push(rendered);
        } else {
            patch.lines.push(rendered);
        }
    }
    for (slug, successor) in &merges {
        let line = format!("{}: {}", json(slug)?, json(successor)?);
        patch(&mut files, format!("{OVERRIDES}/merges.json"))
            .lines
            .push(line);
    }
    tracing::debug!(
        suggestions = rows.len(),
        files = files.len(),
        notes = notes.len(),
        "rendered the suggestion export"
    );
    Ok(SuggestionExport {
        index_version: index.version().to_owned(),
        suggestions: rows.iter().map(|row| row.id).collect(),
        last_accepted_at: rows.last().and_then(|row| row.resolved_at),
        last_id: rows.last().map(|row| row.id),
        complete,
        files: files.into_values().collect(),
        notes,
    })
}

#[cfg(test)]
mod tests;
