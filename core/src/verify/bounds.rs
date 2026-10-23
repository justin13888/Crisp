//! B3 — every §12 limit and every inline `*N` bound of `crisp-v1.cddl`.

use ciborium::value::Value;

use crate::error::ErrorCode;
use crate::limits::{self as l};
use crate::model::payload::{BundlePayload, ExtensionMap};
use crate::model::recipe::Recipe;
use crate::model::text::{LocalizedProse, LocalizedText};
use crate::verify::issue::Issues;
use crate::verify::walk::{self, ProseScope, TextKind, Visitor};

const L: ErrorCode = ErrorCode::Limit;

/// Checks `payload` against every limit.
pub(crate) fn check(payload: &BundlePayload) -> Issues {
    let mut bounds = Bounds::default();
    walk::walk(payload, &mut bounds);
    let issues = &mut bounds.issues;

    count(issues, "/recipes", payload.recipes.len(), l::MAX_RECIPES);
    count(issues, "/blobs", payload.blobs.len(), l::MAX_BLOBS);
    count(
        issues,
        "/languages",
        payload.languages.len(),
        l::MAX_LANGUAGES,
    );
    count(
        issues,
        "/meta/authors",
        payload.meta.authors.len(),
        l::MAX_AUTHORS,
    );

    let mut total = 0usize;
    for (id, blob) in &payload.blobs {
        count(
            issues,
            &format!("/blobs/{}/bytes", id.0),
            blob.bytes.len(),
            l::MAX_BLOB_SIZE,
        );
        total = total.saturating_add(blob.bytes.len());
    }
    count(issues, "/blobs", total, l::MAX_TOTAL_BLOB_BYTES);

    for (i, recipe) in payload.recipes.iter().enumerate() {
        recipe_bounds(recipe, &format!("/recipes/{i}"), issues);
    }
    title(
        issues,
        "/meta/identity/title",
        payload.meta.identity.as_ref().map(|i| i.title.as_str()),
    );

    if let Some(book) = &payload.book {
        count(issues, "/book/parts", book.parts.len(), l::MAX_BOOK_PARTS);
        for (name, pages) in [
            ("front_matter", &book.front_matter),
            ("back_matter", &book.back_matter),
        ] {
            if let Some(pages) = pages {
                count(
                    issues,
                    &format!("/book/{name}"),
                    pages.len(),
                    l::MAX_MATTER_PAGES,
                );
            }
        }
        for (pi, part) in book.parts.iter().enumerate() {
            let pp = format!("/book/parts/{pi}/chapters");
            count(issues, &pp, part.chapters.len(), l::MAX_CHAPTERS_PER_PART);
            for (ci, chapter) in part.chapters.iter().enumerate() {
                count(
                    issues,
                    &format!("{pp}/{ci}/entries"),
                    chapter.entries.len(),
                    l::MAX_ENTRIES_PER_CHAPTER,
                );
            }
        }
    }
    bounds.issues
}

fn count(issues: &mut Issues, path: &str, actual: usize, max: usize) {
    issues.require(actual <= max, path, L, || {
        format!("{actual} exceeds the maximum of {max}")
    });
}

fn title(issues: &mut Issues, path: &str, title: Option<&str>) {
    if let Some(title) = title {
        count(issues, path, title.len(), l::MAX_TITLE_BYTES);
    }
}

fn recipe_bounds(recipe: &Recipe, rp: &str, issues: &mut Issues) {
    title(
        issues,
        &format!("{rp}/identity/title"),
        Some(&recipe.identity.title),
    );
    count(
        issues,
        &format!("{rp}/steps"),
        recipe.steps.len(),
        l::MAX_STEPS,
    );
    count(
        issues,
        &format!("{rp}/ingredients"),
        recipe.ingredients.len(),
        l::MAX_INGREDIENTS,
    );
    let optional: [(&str, Option<usize>, usize); 10] = [
        (
            "equipment",
            recipe.equipment.as_ref().map(Vec::len),
            l::MAX_EQUIPMENT,
        ),
        (
            "ingredient_groups",
            recipe.ingredient_groups.as_ref().map(Vec::len),
            l::MAX_INGREDIENT_GROUPS,
        ),
        (
            "step_groups",
            recipe.step_groups.as_ref().map(Vec::len),
            l::MAX_STEP_GROUPS,
        ),
        ("tags", recipe.tags.as_ref().map(Vec::len), l::MAX_TAGS),
        (
            "cuisine",
            recipe.cuisine.as_ref().map(Vec::len),
            l::MAX_CUISINE,
        ),
        (
            "course",
            recipe.course.as_ref().map(Vec::len),
            l::MAX_COURSE,
        ),
        ("diet", recipe.diet.as_ref().map(Vec::len), l::MAX_DIET),
        (
            "allergens",
            recipe.allergens.as_ref().map(Vec::len),
            l::MAX_ALLERGENS,
        ),
        ("media", recipe.media.as_ref().map(Vec::len), l::MAX_MEDIA),
        (
            "nutrition/values",
            recipe.nutrition.as_ref().map(|n| n.values.len()),
            l::MAX_ARRAY_LEN,
        ),
    ];
    for (field, len, max) in optional {
        if let Some(len) = len {
            count(issues, &format!("{rp}/{field}"), len, max);
        }
    }
    for (i, ing) in recipe.ingredients.iter().enumerate() {
        let ip = format!("{rp}/ingredients/{i}");
        if let Some(subs) = &ing.substitutes {
            count(
                issues,
                &format!("{ip}/substitutes"),
                subs.len(),
                l::MAX_SUBSTITUTES,
            );
        }
        if let Some(alt) = &ing.alt {
            count(
                issues,
                &format!("{ip}/alt"),
                alt.len(),
                l::MAX_ALT_QUANTITIES,
            );
        }
    }
    for (i, step) in recipe.steps.iter().enumerate() {
        let sp = format!("{rp}/steps/{i}");
        if let Some(used) = &step.ingredients {
            count(
                issues,
                &format!("{sp}/ingredients"),
                used.len(),
                l::MAX_STEP_INGREDIENTS,
            );
        }
        if let Some(eq) = &step.equipment {
            count(
                issues,
                &format!("{sp}/equipment"),
                eq.len(),
                l::MAX_STEP_EQUIPMENT,
            );
        }
        if let Some(timers) = &step.timers {
            count(
                issues,
                &format!("{sp}/timers"),
                timers.len(),
                l::MAX_STEP_TIMERS,
            );
        }
    }
    let strings: [(&str, &[String]); 2] = [
        ("tags", recipe.tags.as_deref().unwrap_or_default()),
        ("cuisine", recipe.cuisine.as_deref().unwrap_or_default()),
    ];
    for (field, values) in strings {
        for (i, value) in values.iter().enumerate() {
            count(
                issues,
                &format!("{rp}/{field}/{i}"),
                value.len(),
                l::MAX_TEXT_BYTES,
            );
        }
    }
    if let Some(source) = &recipe.source
        && let Some(url) = &source.url
    {
        count(
            issues,
            &format!("{rp}/source/url"),
            url.len(),
            l::MAX_TEXT_BYTES,
        );
    }
}

/// Bounds every array inside opaque extension data.
fn value_bounds(value: &Value, path: &str, issues: &mut Issues) {
    match value {
        Value::Text(text) => count(issues, path, text.len(), l::MAX_TEXT_BYTES),
        Value::Bytes(bytes) => count(issues, path, bytes.len(), l::MAX_BLOB_SIZE),
        Value::Array(items) => {
            count(issues, path, items.len(), l::MAX_ARRAY_LEN);
            for (i, item) in items.iter().enumerate() {
                value_bounds(item, &format!("{path}/{i}"), issues);
            }
        }
        Value::Map(entries) => {
            count(issues, path, entries.len(), l::MAX_ARRAY_LEN);
            for (i, (key, item)) in entries.iter().enumerate() {
                value_bounds(key, &format!("{path}/{i}"), issues);
                value_bounds(item, &format!("{path}/{i}"), issues);
            }
        }
        _ => {}
    }
}

/// The walk-driven part of B3.
#[derive(Default)]
struct Bounds {
    issues: Issues,
}

impl<'a> Visitor<'a> for Bounds {
    fn text(&mut self, path: &str, text: &'a LocalizedText, kind: TextKind) {
        let max = match kind {
            TextKind::Title => l::MAX_TITLE_BYTES,
            TextKind::Plain => l::MAX_TEXT_BYTES,
        };
        for (lang, value) in &text.0 {
            count(
                &mut self.issues,
                &format!("{path}/{}", lang.0),
                value.len(),
                max,
            );
        }
    }

    fn prose(&mut self, path: &str, prose: &'a LocalizedProse, _scope: ProseScope<'a>) {
        for (lang, body) in &prose.0 {
            let lp = format!("{path}/{}", lang.0);
            count(&mut self.issues, &lp, body.0.len(), l::MAX_PARAGRAPHS);
            for (pi, paragraph) in body.0.iter().enumerate() {
                let pp = format!("{lp}/{pi}");
                count(
                    &mut self.issues,
                    &pp,
                    paragraph.0.len(),
                    l::MAX_RUNS_PER_PARAGRAPH,
                );
                for (ri, run) in paragraph.0.iter().enumerate() {
                    let rp = format!("{pp}/{ri}");
                    count(
                        &mut self.issues,
                        &format!("{rp}/t"),
                        run.t.len(),
                        l::MAX_TEXT_BYTES,
                    );
                    if let Some(style) = &run.style {
                        count(
                            &mut self.issues,
                            &format!("{rp}/style"),
                            style.len(),
                            l::MAX_RUN_STYLES,
                        );
                    }
                    if let Some(link) = &run.link {
                        count(
                            &mut self.issues,
                            &format!("{rp}/link"),
                            link.len(),
                            l::MAX_TEXT_BYTES,
                        );
                    }
                }
            }
        }
    }

    fn ext(&mut self, path: &str, ext: &'a ExtensionMap) {
        count(&mut self.issues, path, ext.len(), l::MAX_ARRAY_LEN);
        for (key, value) in ext {
            value_bounds(value, &format!("{path}/{key}"), &mut self.issues);
        }
    }
}

#[cfg(test)]
mod tests {
    use ciborium::value::Value;

    use super::*;
    use crate::fixtures::{english, sample_identity, sample_payload};
    use crate::model::text::{Paragraph, Prose, Run, StyleFlag};

    /// The JSON Pointer of every B3 issue in `payload`, after checking each
    /// carries `E.LIMIT`.
    fn limit_paths(payload: &BundlePayload) -> Vec<String> {
        let issues = check(payload).0;
        for issue in &issues {
            assert_eq!(issue.code, ErrorCode::Limit, "{issue:?}");
        }
        issues.into_iter().map(|i| i.path).collect()
    }

    /// Applies `edit` to the sample payload and returns its B3 issue paths.
    fn paths_after(edit: impl FnOnce(&mut BundlePayload)) -> Vec<String> {
        let mut payload = sample_payload();
        edit(&mut payload);
        limit_paths(&payload)
    }

    /// Asserts `build(max)` passes B3 and `build(max + 1)` fails at `path`
    /// only.
    fn at_and_over(max: usize, path: &str, build: impl Fn(&mut BundlePayload, usize)) {
        assert_eq!(paths_after(|p| build(p, max)), Vec::<String>::new());
        assert_eq!(paths_after(|p| build(p, max + 1)), [path]);
    }

    fn run(t: &str) -> Run {
        Run {
            t: t.to_owned(),
            style: None,
            link: None,
            reference: None,
        }
    }

    /// Replaces the first step's English prose with `prose`.
    fn set_step_prose(payload: &mut BundlePayload, prose: Prose) {
        let text = &mut payload.recipes[0].steps[0].text.0;
        text.insert(
            crate::model::primitives::LanguageTag("en".to_owned()),
            prose,
        );
    }

    /// Sets the first recipe's extension map to one entry, `a.b => value`.
    fn set_ext(payload: &mut BundlePayload, value: Value) {
        payload.recipes[0].ext = Some(ExtensionMap::from([("a.b".to_owned(), value)]));
    }

    #[test]
    fn the_sample_payload_is_within_every_limit() {
        assert_eq!(limit_paths(&sample_payload()), Vec::<String>::new());
    }

    #[test]
    fn a_recipe_identity_title_is_bounded_by_the_title_limit() {
        at_and_over(l::MAX_TITLE_BYTES, "/recipes/0/identity/title", |p, n| {
            p.recipes[0].identity.title = "a".repeat(n);
        });
    }

    #[test]
    fn a_book_identity_title_is_bounded_by_the_title_limit() {
        at_and_over(l::MAX_TITLE_BYTES, "/meta/identity/title", |p, n| {
            let mut identity = sample_identity();
            identity.title = "a".repeat(n);
            p.meta.identity = Some(identity);
        });
    }

    #[test]
    fn a_localized_title_is_bounded_by_the_title_limit() {
        at_and_over(l::MAX_TITLE_BYTES, "/recipes/0/title/en", |p, n| {
            p.recipes[0].title = english(&"a".repeat(n));
        });
    }

    #[test]
    fn a_plain_localized_text_is_bounded_by_the_text_limit() {
        at_and_over(
            l::MAX_TEXT_BYTES,
            "/recipes/0/yield/unit_text/en",
            |p, n| {
                p.recipes[0].r#yield.unit_text = english(&"a".repeat(n));
            },
        );
    }

    #[test]
    fn prose_paragraphs_are_bounded() {
        at_and_over(l::MAX_PARAGRAPHS, "/recipes/0/steps/0/text/en", |p, n| {
            set_step_prose(p, Prose(vec![Paragraph(vec![run("x")]); n]));
        });
    }

    #[test]
    fn runs_per_paragraph_are_bounded() {
        at_and_over(
            l::MAX_RUNS_PER_PARAGRAPH,
            "/recipes/0/steps/0/text/en/0",
            |p, n| set_step_prose(p, Prose(vec![Paragraph(vec![run("x"); n])])),
        );
    }

    #[test]
    fn a_run_text_is_bounded_by_the_text_limit() {
        at_and_over(
            l::MAX_TEXT_BYTES,
            "/recipes/0/steps/0/text/en/0/0/t",
            |p, n| {
                set_step_prose(p, Prose(vec![Paragraph(vec![run(&"a".repeat(n))])]));
            },
        );
    }

    #[test]
    fn a_run_style_list_is_bounded() {
        at_and_over(
            l::MAX_RUN_STYLES,
            "/recipes/0/steps/0/text/en/0/0/style",
            |p, n| {
                let mut styled = run("x");
                styled.style = Some(vec![StyleFlag::Bold; n]);
                set_step_prose(p, Prose(vec![Paragraph(vec![styled])]));
            },
        );
    }

    #[test]
    fn a_run_link_is_bounded_by_the_text_limit() {
        at_and_over(
            l::MAX_TEXT_BYTES,
            "/recipes/0/steps/0/text/en/0/0/link",
            |p, n| {
                let mut linked = run("x");
                linked.link = Some("a".repeat(n));
                set_step_prose(p, Prose(vec![Paragraph(vec![linked])]));
            },
        );
    }

    #[test]
    fn an_extension_map_is_bounded_by_the_array_limit() {
        at_and_over(l::MAX_ARRAY_LEN, "/recipes/0/ext", |p, n| {
            p.recipes[0].ext = Some(
                (0..n)
                    .map(|i| (format!("a.k{i}"), Value::Bool(true)))
                    .collect(),
            );
        });
    }

    #[test]
    fn an_extension_text_is_bounded_by_the_text_limit() {
        at_and_over(l::MAX_TEXT_BYTES, "/recipes/0/ext/a.b", |p, n| {
            set_ext(p, Value::Text("a".repeat(n)));
        });
    }

    #[test]
    fn extension_bytes_are_bounded_by_the_blob_limit() {
        at_and_over(l::MAX_BLOB_SIZE, "/recipes/0/ext/a.b", |p, n| {
            set_ext(p, Value::Bytes(vec![0; n]));
        });
    }

    #[test]
    fn an_extension_array_is_bounded_by_the_array_limit() {
        at_and_over(l::MAX_ARRAY_LEN, "/recipes/0/ext/a.b", |p, n| {
            set_ext(p, Value::Array(vec![Value::Bool(true); n]));
        });
    }

    #[test]
    fn an_extension_array_item_is_bounded_recursively() {
        at_and_over(l::MAX_TEXT_BYTES, "/recipes/0/ext/a.b/1", |p, n| {
            let items = vec![Value::Bool(true), Value::Text("a".repeat(n))];
            set_ext(p, Value::Array(items));
        });
    }

    #[test]
    fn an_extension_map_value_is_bounded_by_the_array_limit() {
        at_and_over(l::MAX_ARRAY_LEN, "/recipes/0/ext/a.b", |p, n| {
            let entries = (0..n)
                .map(|i| (Value::Integer(i.into()), Value::Null))
                .collect();
            set_ext(p, Value::Map(entries));
        });
    }

    #[test]
    fn an_extension_map_key_is_bounded_recursively() {
        at_and_over(l::MAX_TEXT_BYTES, "/recipes/0/ext/a.b/0", |p, n| {
            let entries = vec![(Value::Text("a".repeat(n)), Value::Null)];
            set_ext(p, Value::Map(entries));
        });
    }

    #[test]
    fn an_extension_map_entry_value_is_bounded_recursively() {
        at_and_over(l::MAX_TEXT_BYTES, "/recipes/0/ext/a.b/0", |p, n| {
            let entries = vec![(Value::Integer(0.into()), Value::Text("a".repeat(n)))];
            set_ext(p, Value::Map(entries));
        });
    }

    #[test]
    fn extension_scalars_carry_no_limit() {
        let scalars = [
            Value::Integer(u64::MAX.into()),
            Value::Bool(false),
            Value::Null,
            Value::Float(1.5),
        ];
        for scalar in scalars {
            assert_eq!(
                paths_after(|p| set_ext(p, scalar.clone())),
                Vec::<String>::new(),
                "{scalar:?}"
            );
        }
    }
}
