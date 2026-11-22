//! Re-roots a payload JSON Pointer (from `crisp-core`'s checks) onto the
//! draft, whose field names follow the payload's but whose values are
//! typed-in strings rather than nested structures.

/// Fields whose draft value is a single string: a pointer stops at them.
const LEAVES: &[&str] = &[
    "title",
    "summary",
    "notes",
    "attribution",
    "name",
    "preparation",
    "note",
    "text",
    "label",
    "produces",
    "unit_text",
    "quantity",
    "amount",
    "duration",
    "prep",
    "cook",
    "rest",
    "total",
    "temperature",
    "heat",
    "url",
    "author",
];

/// Lists of strings: a pointer keeps one index after them, then stops.
const STRING_LISTS: &[&str] = &[
    "ingredient_groups",
    "step_groups",
    "alt",
    "tags",
    "cuisine",
    "course",
    "diet",
    "allergens",
    "best_months",
];

/// Maps a payload pointer such as `/recipes/0/steps/1/timers/0/duration/min`
/// to the draft pointer `/steps/1/timers/0/duration`. Pointers at fields the
/// draft does not have (ids, signatures) become `""`, the whole draft.
#[must_use]
pub fn reroot(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').skip(1).collect();
    let rest: &[&str] = match segments.as_slice() {
        ["recipes", "0", "identity", ..] | ["meta", "title", ..] => &["title"],
        ["recipes", "0", rest @ ..] => rest,
        ["meta", "license", ..] => &["license"],
        ["default_language" | "languages", ..] => &["language"],
        _ => &[],
    };
    let mut out = Vec::new();
    let mut iter = rest.iter();
    while let Some(segment) = iter.next() {
        out.push(*segment);
        if LEAVES.contains(segment) {
            break;
        }
        if STRING_LISTS.contains(segment) {
            out.extend(iter.next());
            break;
        }
    }
    if matches!(
        out.first(),
        Some(
            &("id"
                | "revision"
                | "supersedes"
                | "derived_from"
                | "modified_at"
                | "media"
                | "nutrition"
                | "ext")
        )
    ) {
        return String::new();
    }
    out.iter().map(|s| format!("/{s}")).collect()
}

#[cfg(test)]
mod tests {
    use super::reroot;

    #[test]
    fn payload_pointers_land_on_draft_fields() {
        let cases = [
            (
                "/recipes/0/steps/1/timers/0/duration/min",
                "/steps/1/timers/0/duration",
            ),
            (
                "/recipes/0/ingredients/2/quantity/unit",
                "/ingredients/2/quantity",
            ),
            ("/recipes/0/ingredients/2/alt/1/max", "/ingredients/2/alt/1"),
            (
                "/recipes/0/ingredients/0/substitutes/1/name/en",
                "/ingredients/0/substitutes/1/name",
            ),
            ("/recipes/0/summary/en/0/1/link", "/summary"),
            ("/recipes/0/yield/unit_text/en", "/yield/unit_text"),
            ("/recipes/0/tags/3", "/tags/3"),
            ("/recipes/0/best_months", "/best_months"),
            ("/recipes/0/steps/0/temperature/max", "/steps/0/temperature"),
            ("/recipes/0/source/url", "/source/url"),
            ("/recipes/0/steps", "/steps"),
            ("/recipes/0/identity/title", "/title"),
            ("/meta/title/en", "/title"),
            ("/meta/license", "/license"),
            ("/default_language", "/language"),
            ("/languages/0", "/language"),
        ];
        for (payload, draft) in cases {
            assert_eq!(reroot(payload), draft, "{payload}");
        }
    }

    #[test]
    fn pointers_outside_the_draft_become_the_whole_draft() {
        for payload in [
            "",
            "/doc_id",
            "/meta/authors/0",
            "/blobs/x",
            "/recipes/0/id",
            "/recipes/0/revision",
            "/recipes/0/media/0",
            "/recipes/0/nutrition/basis",
        ] {
            assert_eq!(reroot(payload), "", "{payload}");
        }
    }
}
