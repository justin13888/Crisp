use super::*;

fn index() -> &'static IngredientIndex {
    crisp_core::ingredients::index().expect("the embedded index loads")
}

fn request(
    kind: SuggestionKind,
    slug: Option<&str>,
    locale: &str,
    text: &str,
) -> SuggestionRequest {
    SuggestionRequest {
        kind,
        slug: slug.map(str::to_owned),
        locale: locale.to_owned(),
        text: text.to_owned(),
        note: None,
    }
}

fn refused(request: SuggestionRequest) -> FileSuggestionError {
    check(request, index()).expect_err("the suggestion is refused")
}

/// Asserts `request` is refused 400.
fn assert_bad(request: SuggestionRequest) {
    let error = refused(request);
    assert!(
        matches!(error, FileSuggestionError::BadRequest(_)),
        "{error}"
    );
}

#[test]
fn kinds_and_statuses_round_trip_through_their_stored_form() {
    for kind in SuggestionKind::ALL {
        assert_eq!(SuggestionKind::stored(kind.as_str()).unwrap(), kind);
        assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
    }
    for status in SuggestionStatus::ALL {
        assert_eq!(SuggestionStatus::stored(status.as_str()).unwrap(), status);
        assert_eq!(serde_json::to_value(status).unwrap(), status.as_str());
    }
    assert!(SuggestionKind::stored("rename").is_err());
    assert!(SuggestionStatus::stored("closed").is_err());
}

#[test]
fn a_well_formed_suggestion_is_trimmed_and_kept() {
    let mut alias = request(
        SuggestionKind::Name,
        Some(" kimchi "),
        " en-GB ",
        " napa pickle ",
    );
    alias.note = Some("  common in shops  ".to_owned());
    assert_eq!(
        check(alias, index()).unwrap(),
        Checked {
            kind: SuggestionKind::Name,
            slug: Some("kimchi".to_owned()),
            locale: "en-GB".to_owned(),
            text: "napa pickle".to_owned(),
            note: "common in shops".to_owned(),
        }
    );
    let fresh = request(SuggestionKind::NewEntry, None, "en-US", "black garlic");
    assert_eq!(check(fresh, index()).unwrap().slug, None);
    let other = request(SuggestionKind::Other, None, "ja-JP", "split the soy sauces");
    assert!(check(other, index()).is_ok());
}

#[test]
fn the_locale_must_be_a_declared_shard() {
    assert_bad(request(SuggestionKind::Other, None, "xx-YY", "anything"));
}

#[test]
fn text_and_note_are_bounded_in_characters() {
    let at_cap = "é".repeat(MAX_TEXT_CHARS);
    let fits = request(SuggestionKind::Other, None, "en-US", &at_cap);
    assert!(check(fits, index()).is_ok());
    let over = "x".repeat(MAX_TEXT_CHARS + 1);
    for text in [over.as_str(), "   ", "two\nlines"] {
        assert_bad(request(SuggestionKind::Other, None, "en-US", text));
    }
    let mut long_note = request(SuggestionKind::Other, None, "en-US", "x");
    long_note.note = Some("x".repeat(MAX_NOTE_CHARS + 1));
    assert_bad(long_note);
}

#[test]
fn slugs_must_be_known_and_present_where_the_kind_needs_one() {
    for kind in SuggestionKind::ALL {
        assert_eq!(
            kind.needs_slug(),
            check(request(kind, None, "en-US", "kimchi-aged"), index()).is_err(),
            "{kind:?}"
        );
        assert_bad(request(kind, Some("no-such-entry-anywhere"), "en-US", "x"));
    }
}

#[test]
fn a_merge_names_another_known_entry() {
    let merge = request(
        SuggestionKind::Merge,
        Some("kimchi-aged"),
        "en-US",
        "kimchi",
    );
    assert_eq!(check(merge, index()).unwrap().text, "kimchi");
    for successor in ["kimchi-aged", "no-such-entry-anywhere"] {
        assert_bad(request(
            SuggestionKind::Merge,
            Some("kimchi-aged"),
            "en-US",
            successor,
        ));
    }
}

#[test]
fn a_name_the_entry_already_has_in_that_shard_conflicts() {
    let cases = [
        (SuggestionKind::Name, "en-US", "Sour Kimchi"),
        (SuggestionKind::RegionalSpelling, "ko-KR", "묵은지"),
    ];
    for (kind, locale, text) in cases {
        let known = request(kind, Some("kimchi-aged"), locale, text);
        assert!(matches!(refused(known), FileSuggestionError::Conflict(_)));
    }
    let elsewhere = request(
        SuggestionKind::Name,
        Some("kimchi-aged"),
        "ja-JP",
        "sour kimchi",
    );
    assert!(check(elsewhere, index()).is_ok());
}
