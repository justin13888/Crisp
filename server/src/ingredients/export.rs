//! Accepted suggestions as override patches (`docs/ingredient-index.md` §6,
//! step 3).
//!
//! The server has no checkout of the repository, so a patch is the lines to
//! add to one file of `tools/ingredients/rules/overrides/`, in that file's
//! one-line-per-entry form; an administrator pastes them into the upstream
//! pull request. What only a maintainer can decide — the permanent slug and
//! category of a new entry, a rule for a class of names, anything else — is
//! a note instead.
//!
//! - `name` and `regional_spelling` → `names/<locale>.json`: the text
//!   becomes the entry's display name in that shard when the index gives it
//!   none, and an alias otherwise. A regional spelling also gets a note: a
//!   spelling shared by a class of entries belongs in `en-regional.toml`.
//! - `merge` → `merges.json`: `"slug": "successor"`. A second accepted
//!   merge of the same slug to another successor is a note.
//! - `new_entry` and `other` → a note.

use std::collections::BTreeMap;

use crisp_core::ingredients::IngredientIndex;
use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::suggestions::SuggestionKind;
use crate::db::ingredient_suggestion;
use crate::error::Internal;

/// The directory every patch path is relative to.
pub const OVERRIDES: &str = "tools/ingredients/rules/overrides";

/// The lines to add to one override file.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct OverridePatch {
    /// The file, from the repository root, e.g.
    /// `tools/ingredients/rules/overrides/names/en-GB.json`.
    pub path: String,
    /// The lines to add inside the file's top-level object, one per entry,
    /// without trailing commas.
    pub lines: Vec<String>,
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
    /// The latest acceptance time exported (epoch seconds); pass it as
    /// `since` to continue an incomplete export.
    pub last_accepted_at: Option<i64>,
    /// Whether every accepted suggestion since `since` is included; `false`
    /// when the export reached its row cap.
    pub complete: bool,
    /// The override patches, one per file, ordered by path.
    pub files: Vec<OverridePatch>,
    /// The suggestions that need a maintainer's decision.
    pub notes: Vec<ExportNote>,
}

/// The names one entry gains in one shard.
#[derive(Debug, Default)]
struct NameLine {
    /// A display name, when the shard has none.
    name: Option<String>,
    /// Aliases, in the order they were accepted.
    aliases: Vec<String>,
}

impl NameLine {
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

/// Renders accepted suggestions `rows` as override patches against `index`.
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
                    .or_default();
                let unnamed = index.names(slug, &row.locale).is_none();
                if unnamed && line.name.is_none() {
                    line.name = Some(row.text.clone());
                } else if line.name.as_ref() != Some(&row.text) && !line.aliases.contains(&row.text)
                {
                    line.aliases.push(row.text.clone());
                }
                if kind == SuggestionKind::RegionalSpelling {
                    notes.push(note(row, kind, REGIONAL));
                }
            }
            (SuggestionKind::Merge, Some(slug)) => match merges.get(slug) {
                None => {
                    merges.insert(slug.to_owned(), row.text.clone());
                }
                Some(successor) if *successor == row.text => {}
                Some(successor) => {
                    let action = format!(
                        "conflicts with an accepted merge of {slug:?} into {successor:?}; \
                         keep one"
                    );
                    notes.push(note(row, kind, &action));
                }
            },
            (SuggestionKind::NewEntry, _) => notes.push(note(row, kind, NEW_ENTRY)),
            _ => notes.push(note(row, kind, OTHER)),
        }
    }

    let mut files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ((shard, slug), line) in &names {
        let path = format!("{OVERRIDES}/names/{shard}.json");
        files.entry(path).or_default().push(line.render(slug)?);
    }
    for (slug, successor) in &merges {
        let path = format!("{OVERRIDES}/merges.json");
        let line = format!("{}: {}", json(slug)?, json(successor)?);
        files.entry(path).or_default().push(line);
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
        last_accepted_at: rows.iter().filter_map(|row| row.resolved_at).max(),
        complete,
        files: files
            .into_iter()
            .map(|(path, lines)| OverridePatch { path, lines })
            .collect(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> &'static IngredientIndex {
        crisp_core::ingredients::index().expect("the embedded index loads")
    }

    fn accepted(
        id: i64,
        kind: SuggestionKind,
        slug: Option<&str>,
        locale: &str,
        text: &str,
    ) -> ingredient_suggestion::Model {
        ingredient_suggestion::Model {
            id,
            suggester: Some("usr1a".to_owned()),
            kind: kind.as_str().to_owned(),
            slug: slug.map(str::to_owned),
            locale: locale.to_owned(),
            text: text.to_owned(),
            note: String::new(),
            status: "accepted".to_owned(),
            created_at: 1,
            resolved_at: Some(100 + id),
            resolved_by: Some("usr1admin".to_owned()),
        }
    }

    fn file<'a>(export: &'a SuggestionExport, path: &str) -> &'a [String] {
        let path = format!("{OVERRIDES}/{path}");
        let patch = export.files.iter().find(|patch| patch.path == path);
        &patch.unwrap_or_else(|| panic!("no patch for {path}")).lines
    }

    #[test]
    fn nothing_accepted_exports_nothing() {
        let export = export(&[], index(), true).unwrap();
        assert!(export.files.is_empty() && export.notes.is_empty());
        assert_eq!(export.last_accepted_at, None);
        assert_eq!(export.index_version, index().version());
    }

    #[test]
    fn names_become_a_display_name_where_the_shard_has_none_and_aliases_otherwise() {
        let rows = [
            accepted(
                1,
                SuggestionKind::Name,
                Some("kimchi-aged"),
                "ja-JP",
                "古漬けキムチ",
            ),
            accepted(
                2,
                SuggestionKind::Name,
                Some("kimchi-aged"),
                "ja-JP",
                "ムグンジ",
            ),
            accepted(
                3,
                SuggestionKind::Name,
                Some("kimchi-aged"),
                "ja-JP",
                "ムグンジ",
            ),
            accepted(
                4,
                SuggestionKind::Name,
                Some("kimchi-aged"),
                "en-US",
                "mugunji",
            ),
        ];
        let export = export(&rows, index(), true).unwrap();
        assert_eq!(
            file(&export, "names/ja-JP.json"),
            [r#""kimchi-aged": {"name": "古漬けキムチ", "aliases": ["ムグンジ"]}"#]
        );
        assert_eq!(
            file(&export, "names/en-US.json"),
            [r#""kimchi-aged": {"aliases": ["mugunji"]}"#]
        );
        assert_eq!(export.suggestions, [1, 2, 3, 4]);
        assert_eq!(export.last_accepted_at, Some(104));
        assert!(export.notes.is_empty());
    }

    #[test]
    fn a_regional_spelling_is_a_names_line_and_a_rule_note() {
        let rows = [accepted(
            5,
            SuggestionKind::RegionalSpelling,
            Some("kimchi"),
            "en-GB",
            "kimchee",
        )];
        let export = export(&rows, index(), true).unwrap();
        let line = &file(&export, "names/en-GB.json")[0];
        assert!(line.contains("\"kimchee\""), "{line}");
        assert_eq!(export.notes.len(), 1);
        assert!(export.notes[0].action.contains("en-regional.toml"));
    }

    #[test]
    fn merges_become_merges_json_lines_and_a_conflicting_one_a_note() {
        let rows = [
            accepted(
                6,
                SuggestionKind::Merge,
                Some("kimchi-aged"),
                "en-US",
                "kimchi",
            ),
            accepted(
                7,
                SuggestionKind::Merge,
                Some("kimchi-aged"),
                "en-US",
                "kimchi",
            ),
            accepted(
                8,
                SuggestionKind::Merge,
                Some("kimchi-aged"),
                "en-US",
                "white-cabbage",
            ),
        ];
        let export = export(&rows, index(), false).unwrap();
        assert_eq!(file(&export, "merges.json"), [r#""kimchi-aged": "kimchi""#]);
        assert_eq!(export.notes.len(), 1);
        assert_eq!(export.notes[0].suggestion_id, 8);
        assert!(!export.complete);
    }

    #[test]
    fn new_entries_and_other_suggestions_are_notes() {
        let rows = [
            accepted(
                9,
                SuggestionKind::NewEntry,
                Some("kimchi"),
                "en-US",
                "white kimchi",
            ),
            accepted(10, SuggestionKind::Other, None, "en-US", "split soy sauces"),
        ];
        let export = export(&rows, index(), true).unwrap();
        assert!(export.files.is_empty());
        let kinds: Vec<_> = export.notes.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, [SuggestionKind::NewEntry, SuggestionKind::Other]);
        assert!(export.notes[0].action.contains("entries.json"));
    }

    #[test]
    fn an_unknown_stored_kind_is_an_internal_error() {
        let mut row = accepted(11, SuggestionKind::Other, None, "en-US", "x");
        row.kind = "rename".to_owned();
        assert!(export(&[row], index(), true).is_err());
    }
}
