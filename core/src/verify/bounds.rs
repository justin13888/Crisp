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
