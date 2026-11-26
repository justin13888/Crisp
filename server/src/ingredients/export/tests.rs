//! Unit tests for rendering accepted suggestions as override patches.

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

fn name(slug: &str, locale: &str, text: &str) -> ingredient_suggestion::Model {
    accepted(20, SuggestionKind::Name, Some(slug), locale, text)
}

fn patch<'a>(export: &'a SuggestionExport, path: &str) -> &'a OverridePatch {
    let path = format!("{OVERRIDES}/{path}");
    let patch = export.files.iter().find(|patch| patch.path == path);
    patch.unwrap_or_else(|| panic!("no patch for {path}"))
}

fn file<'a>(export: &'a SuggestionExport, path: &str) -> &'a [String] {
    &patch(export, path).lines
}

/// Overrides holding one `en-US` names line for `kimchi-aged` and one merge.
fn committed() -> Overrides {
    Overrides::parse(
        &[(
            "en-US",
            r#"{"kimchi-aged": {"name": "aged kimchi", "aliases": ["sour kimchi"]}}"#,
        )],
        r#"{"kimchi-aged": "kimchi"}"#,
    )
    .unwrap()
}

#[test]
fn nothing_accepted_exports_nothing() {
    let export = export(&[], index(), &Overrides::default(), true).unwrap();
    assert!(export.files.is_empty() && export.notes.is_empty());
    assert_eq!(export.last_accepted_at, None);
    assert_eq!(export.last_id, None);
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
    let export = export(&rows, index(), &Overrides::default(), true).unwrap();
    assert_eq!(
        file(&export, "names/ja-JP.json"),
        [r#""kimchi-aged": {"name": "古漬けキムチ", "aliases": ["ムグンジ"]}"#]
    );
    assert_eq!(
        file(&export, "names/en-US.json"),
        [r#""kimchi-aged": {"aliases": ["mugunji"]}"#]
    );
    assert!(patch(&export, "names/en-US.json").replace.is_empty());
    assert_eq!(export.suggestions, [1, 2, 3, 4]);
    assert_eq!(export.last_accepted_at, Some(104));
    assert_eq!(export.last_id, Some(4));
    assert!(export.notes.is_empty());
}

#[test]
fn a_key_the_names_file_holds_is_replaced_by_a_merged_line_not_appended() {
    let export = export(
        &[name("kimchi-aged", "en-US", "mugunji")],
        index(),
        &committed(),
        true,
    )
    .unwrap();
    let patch = patch(&export, "names/en-US.json");
    assert!(patch.lines.is_empty(), "{:?}", patch.lines);
    assert_eq!(
        patch.replace,
        [r#""kimchi-aged": {"name": "aged kimchi", "aliases": ["sour kimchi", "mugunji"]}"#]
    );
}

#[test]
fn a_name_the_names_file_already_holds_changes_nothing() {
    let rows = [name("kimchi-aged", "en-US", "sour kimchi")];
    let export = export(&rows, index(), &committed(), true).unwrap();
    assert!(export.files.is_empty(), "{:?}", export.files);
}

#[test]
fn the_embedded_overrides_keep_the_curated_kimchi_aged_line() {
    let rows = [name("kimchi-aged", "en-US", "mugunji")];
    let export = export(&rows, index(), Overrides::embedded().unwrap(), true).unwrap();
    let patch = patch(&export, "names/en-US.json");
    assert!(patch.lines.is_empty());
    let [line] = patch.replace.as_slice() else {
        panic!("one replacement line: {:?}", patch.replace);
    };
    for kept in [
        "\"aged kimchi\"",
        "\"sour kimchi\"",
        "\"old kimchi\"",
        "\"mugunji\"",
    ] {
        assert!(line.contains(kept), "{kept} in {line}");
    }
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
    let export = export(&rows, index(), &Overrides::default(), true).unwrap();
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
    let export = export(&rows, index(), &Overrides::default(), false).unwrap();
    assert_eq!(file(&export, "merges.json"), [r#""kimchi-aged": "kimchi""#]);
    assert_eq!(export.notes.len(), 1);
    assert_eq!(export.notes[0].suggestion_id, 8);
    assert!(!export.complete);
}

#[test]
fn a_merge_the_merges_file_holds_is_left_out_and_a_conflicting_one_a_note() {
    let rows = [
        accepted(
            12,
            SuggestionKind::Merge,
            Some("kimchi-aged"),
            "en-US",
            "kimchi",
        ),
        accepted(
            13,
            SuggestionKind::Merge,
            Some("kimchi-aged"),
            "en-US",
            "white-cabbage",
        ),
    ];
    let export = export(&rows, index(), &committed(), true).unwrap();
    assert!(export.files.is_empty(), "{:?}", export.files);
    assert_eq!(export.notes.len(), 1);
    assert_eq!(export.notes[0].suggestion_id, 13);
    assert!(export.notes[0].action.contains("merges.json holds"));
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
    let export = export(&rows, index(), &Overrides::default(), true).unwrap();
    assert!(export.files.is_empty());
    let kinds: Vec<_> = export.notes.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, [SuggestionKind::NewEntry, SuggestionKind::Other]);
    assert!(export.notes[0].action.contains("entries.json"));
}

#[test]
fn an_unknown_stored_kind_is_an_internal_error() {
    let mut row = accepted(11, SuggestionKind::Other, None, "en-US", "x");
    row.kind = "rename".to_owned();
    assert!(export(&[row], index(), &Overrides::default(), true).is_err());
}
