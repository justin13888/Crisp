//! A single traversal of a [`BundlePayload`] that reports every value a
//! check cares about together with its JSON-Pointer path.
//!
//! Every Phase B–D check is a [`Visitor`] over this walk, so the paths in a
//! verification report and in editor validation ([`crate::author`]) agree,
//! and a new field only has to be taught to the walk once. Opaque CBOR
//! (the raw payload, extension values) is walked by [`walk_value`], and
//! every map key becomes a path segment through [`pointer`].

use ciborium::value::Value;

use crate::crypto::bech32id::Hrp;
use crate::limits as l;
use crate::model::book::{Entry, Page};
use crate::model::ids::BlobId;
use crate::model::payload::{BundlePayload, ExtensionMap};
use crate::model::primitives::{LanguageTag, OpenCode, Quantity, Rational};
use crate::model::recipe::{Identity, Recipe};
use crate::model::text::{LocalizedProse, LocalizedText};

/// What a [`Quantity`] measures, which decides the unit rules it obeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuantityKind {
    /// An amount of something (ingredients, yield, nutrition, equipment).
    Amount,
    /// A span of time (`times`, step timers): needs a UCUM time unit.
    Duration,
}

/// What a localized text names, which decides its length limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextKind {
    /// A title (`meta.title`, recipe/part/chapter/page titles).
    Title,
    /// Any other localized string.
    Plain,
}

/// What a slug-shaped string is, which decides its syntax and limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlugKind {
    /// A free tag (`tags`, `cuisine`): a hyphenated slug.
    Tag,
    /// A code outside a known course, diet, or allergen vocabulary: a
    /// hyphenated slug.
    Code,
    /// A code outside the known nutrient vocabulary: a `snake_case` token.
    Nutrient,
}

/// The recipe whose `ref` handles a piece of prose may point at, if any.
pub(crate) type ProseScope<'a> = Option<&'a Recipe>;

/// Callbacks for the values a check inspects. Every method has an empty
/// default, so a check overrides only what it needs.
pub(crate) trait Visitor<'a> {
    /// A localized single-line string.
    fn text(&mut self, _path: &str, _text: &'a LocalizedText, _kind: TextKind) {}
    /// A localized rich-text block, with the recipe its refs resolve in.
    fn prose(&mut self, _path: &str, _prose: &'a LocalizedProse, _scope: ProseScope<'a>) {}
    /// A quantity.
    fn quantity(&mut self, _path: &str, _quantity: &'a Quantity, _kind: QuantityKind) {}
    /// A bare rational outside a quantity (temperatures).
    fn rational(&mut self, _path: &str, _value: &'a Rational) {}
    /// A reference to a blob in the store.
    fn blob_ref(&mut self, _path: &str, _blob: &'a BlobId) {}
    /// An identifier string and the HRP its field requires.
    fn id(&mut self, _path: &str, _id: &'a str, _hrp: Hrp) {}
    /// A namespaced extension map.
    fn ext(&mut self, _path: &str, _ext: &'a ExtensionMap) {}
    /// A language tag outside a localized map's keys.
    fn language(&mut self, _path: &str, _tag: &'a LanguageTag) {}
    /// An `https` URL field outside prose runs.
    fn url(&mut self, _path: &str, _url: &'a str) {}
    /// A recipe, before its parts are walked.
    fn recipe(&mut self, _path: &str, _index: usize, _recipe: &'a Recipe) {}
    /// A birth identity (a recipe's, or a book's `meta.identity`).
    fn identity(&mut self, _path: &str, _identity: &'a Identity) {}
    /// A bounded list or map: its length and the CDDL bound on it.
    fn count(&mut self, _path: &str, _len: usize, _max: usize) {}
    /// A slug-shaped string: a tag, or a code outside a known vocabulary.
    fn slug(&mut self, _path: &str, _slug: &'a str, _kind: SlugKind) {}
}

/// The JSON Pointer of `token` under `path`, escaped per RFC 6901 §3.
pub(crate) fn pointer(path: &str, token: &str) -> String {
    format!("{path}/{}", token.replace('~', "~0").replace('/', "~1"))
}

/// Calls `f` on `value` and every value inside it, in pre-order, with each
/// one's JSON Pointer under `path`. A map key and its value share the
/// pointer of their entry; a tag shares the pointer of its content.
pub(crate) fn walk_value(path: &str, value: &Value, f: &mut impl FnMut(&str, &Value)) {
    f(path, value);
    match value {
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk_value(&format!("{path}/{i}"), item, f);
            }
        }
        Value::Map(entries) => {
            for (key, item) in entries {
                let token = match key {
                    Value::Text(k) => k.clone(),
                    Value::Integer(n) => i128::from(*n).to_string(),
                    _ => "?".to_owned(),
                };
                let child = pointer(path, &token);
                walk_value(&child, key, f);
                walk_value(&child, item, f);
            }
        }
        Value::Tag(_, inner) => walk_value(path, inner, f),
        _ => {}
    }
}

/// Reports `list`'s length to [`Visitor::count`] when the field is present.
fn count_opt<'a, T, V: Visitor<'a>>(v: &mut V, path: &str, list: Option<&Vec<T>>, max: usize) {
    if let Some(list) = list {
        v.count(path, list.len(), max);
    }
}

/// Walks every reportable value in `payload`, in document order.
pub(crate) fn walk<'a, V: Visitor<'a>>(payload: &'a BundlePayload, v: &mut V) {
    v.id("/doc_id", &payload.doc_id.0, doc_hrp(payload));
    v.language("/default_language", &payload.default_language);
    v.count("/languages", payload.languages.len(), l::MAX_LANGUAGES);
    for (i, tag) in payload.languages.iter().enumerate() {
        v.language(&format!("/languages/{i}"), tag);
    }

    let meta = &payload.meta;
    v.text("/meta/title", &meta.title, TextKind::Title);
    v.count("/meta/authors", meta.authors.len(), l::MAX_AUTHORS);
    for (i, author) in meta.authors.iter().enumerate() {
        v.id(&format!("/meta/authors/{i}/uid"), &author.uid.0, Hrp::User);
    }
    if let Some(identity) = &meta.identity {
        v.identity("/meta/identity", identity);
        v.language("/meta/identity/language", &identity.language);
    }
    if let Some(description) = &meta.description {
        v.prose("/meta/description", description, None);
    }
    if let Some(cover) = &meta.cover {
        v.blob_ref("/meta/cover", cover);
    }
    if let Some(ext) = &meta.ext {
        v.ext("/meta/ext", ext);
    }

    v.count("/recipes", payload.recipes.len(), l::MAX_RECIPES);
    for (i, recipe) in payload.recipes.iter().enumerate() {
        walk_recipe(&format!("/recipes/{i}"), i, recipe, v);
    }

    if let Some(book) = &payload.book {
        count_opt(
            v,
            "/book/front_matter",
            book.front_matter.as_ref(),
            l::MAX_MATTER_PAGES,
        );
        for (i, page) in book.front_matter.iter().flatten().enumerate() {
            walk_page(&format!("/book/front_matter/{i}"), page, v);
        }
        v.count("/book/parts", book.parts.len(), l::MAX_BOOK_PARTS);
        for (pi, part) in book.parts.iter().enumerate() {
            let pp = format!("/book/parts/{pi}");
            v.text(&format!("{pp}/title"), &part.title, TextKind::Title);
            v.count(
                &format!("{pp}/chapters"),
                part.chapters.len(),
                l::MAX_CHAPTERS_PER_PART,
            );
            for (ci, chapter) in part.chapters.iter().enumerate() {
                let cp = format!("{pp}/chapters/{ci}");
                v.text(&format!("{cp}/title"), &chapter.title, TextKind::Title);
                v.count(
                    &format!("{cp}/entries"),
                    chapter.entries.len(),
                    l::MAX_ENTRIES_PER_CHAPTER,
                );
                if let Some(intro) = &chapter.intro {
                    v.prose(&format!("{cp}/intro"), intro, None);
                }
                for (ei, entry) in chapter.entries.iter().enumerate() {
                    if let Entry::Page { page } = entry {
                        walk_page(&format!("{cp}/entries/{ei}/page"), page, v);
                    }
                }
                if let Some(ext) = &chapter.ext {
                    v.ext(&format!("{cp}/ext"), ext);
                }
            }
            if let Some(ext) = &part.ext {
                v.ext(&format!("{pp}/ext"), ext);
            }
        }
        count_opt(
            v,
            "/book/back_matter",
            book.back_matter.as_ref(),
            l::MAX_MATTER_PAGES,
        );
        for (i, page) in book.back_matter.iter().flatten().enumerate() {
            walk_page(&format!("/book/back_matter/{i}"), page, v);
        }
    }

    v.count("/blobs", payload.blobs.len(), l::MAX_BLOBS);
    for id in payload.blobs.keys() {
        v.id(&pointer("/blobs", &id.0), &id.0, Hrp::Blob);
    }
    if let Some(ext) = &payload.ext {
        v.ext("/ext", ext);
    }
}

/// The HRP a payload's `doc_id` must carry.
pub(crate) fn doc_hrp(payload: &BundlePayload) -> Hrp {
    match payload.kind {
        crate::model::payload::Kind::Recipe => Hrp::Recipe,
        crate::model::payload::Kind::Book => Hrp::Book,
    }
}

fn walk_page<'a, V: Visitor<'a>>(path: &str, page: &'a Page, v: &mut V) {
    v.text(&format!("{path}/title"), &page.title, TextKind::Title);
    v.prose(&format!("{path}/body"), &page.body, None);
    if let Some(blob) = &page.blob {
        v.blob_ref(&format!("{path}/blob"), blob);
    }
    if let Some(ext) = &page.ext {
        v.ext(&format!("{path}/ext"), ext);
    }
}

fn walk_recipe<'a, V: Visitor<'a>>(rp: &str, index: usize, recipe: &'a Recipe, v: &mut V) {
    v.recipe(rp, index, recipe);
    let scope = Some(recipe);
    v.identity(&format!("{rp}/identity"), &recipe.identity);
    v.language(
        &format!("{rp}/identity/language"),
        &recipe.identity.language,
    );
    v.id(&format!("{rp}/id"), &recipe.id.0, Hrp::Recipe);
    v.id(&format!("{rp}/revision"), &recipe.revision.0, Hrp::Revision);
    if let Some(s) = &recipe.supersedes {
        v.id(&format!("{rp}/supersedes"), &s.0, Hrp::Revision);
    }
    if let Some(d) = &recipe.derived_from {
        v.id(&format!("{rp}/derived_from"), &d.0, Hrp::Recipe);
    }
    v.text(&format!("{rp}/title"), &recipe.title, TextKind::Title);
    if let Some(summary) = &recipe.summary {
        v.prose(&format!("{rp}/summary"), summary, scope);
    }
    v.quantity(
        &format!("{rp}/yield/amount"),
        &recipe.r#yield.amount,
        QuantityKind::Amount,
    );
    v.text(
        &format!("{rp}/yield/unit_text"),
        &recipe.r#yield.unit_text,
        TextKind::Plain,
    );
    if let Some(times) = &recipe.times {
        for (name, q) in [
            ("prep", &times.prep),
            ("cook", &times.cook),
            ("rest", &times.rest),
            ("total", &times.total),
        ] {
            if let Some(q) = q {
                v.quantity(&format!("{rp}/times/{name}"), q, QuantityKind::Duration);
            }
        }
    }
    count_opt(
        v,
        &format!("{rp}/ingredient_groups"),
        recipe.ingredient_groups.as_ref(),
        l::MAX_INGREDIENT_GROUPS,
    );
    for (i, group) in recipe.ingredient_groups.iter().flatten().enumerate() {
        v.text(
            &format!("{rp}/ingredient_groups/{i}"),
            group,
            TextKind::Plain,
        );
    }
    v.count(
        &format!("{rp}/ingredients"),
        recipe.ingredients.len(),
        l::MAX_INGREDIENTS,
    );
    for (i, ing) in recipe.ingredients.iter().enumerate() {
        let ip = format!("{rp}/ingredients/{i}");
        v.text(&format!("{ip}/name"), &ing.name, TextKind::Plain);
        if let Some(q) = &ing.quantity {
            v.quantity(&format!("{ip}/quantity"), q, QuantityKind::Amount);
        }
        count_opt(
            v,
            &format!("{ip}/alt"),
            ing.alt.as_ref(),
            l::MAX_ALT_QUANTITIES,
        );
        for (ai, q) in ing.alt.iter().flatten().enumerate() {
            v.quantity(&format!("{ip}/alt/{ai}"), q, QuantityKind::Amount);
        }
        if let Some(t) = &ing.preparation {
            v.text(&format!("{ip}/preparation"), t, TextKind::Plain);
        }
        if let Some(t) = &ing.note {
            v.text(&format!("{ip}/note"), t, TextKind::Plain);
        }
        count_opt(
            v,
            &format!("{ip}/substitutes"),
            ing.substitutes.as_ref(),
            l::MAX_SUBSTITUTES,
        );
        for (si, sub) in ing.substitutes.iter().flatten().enumerate() {
            let sp = format!("{ip}/substitutes/{si}");
            v.text(&format!("{sp}/name"), &sub.name, TextKind::Plain);
            if let Some(q) = &sub.quantity {
                v.quantity(&format!("{sp}/quantity"), q, QuantityKind::Amount);
            }
            if let Some(ext) = &sub.ext {
                v.ext(&format!("{sp}/ext"), ext);
            }
        }
        if let Some(r) = &ing.recipe {
            v.id(&format!("{ip}/recipe"), &r.0, Hrp::Recipe);
        }
        if let Some(b) = &ing.blob {
            v.blob_ref(&format!("{ip}/blob"), b);
        }
        if let Some(ext) = &ing.ext {
            v.ext(&format!("{ip}/ext"), ext);
        }
    }
    count_opt(
        v,
        &format!("{rp}/equipment"),
        recipe.equipment.as_ref(),
        l::MAX_EQUIPMENT,
    );
    for (i, eq) in recipe.equipment.iter().flatten().enumerate() {
        let ep = format!("{rp}/equipment/{i}");
        v.text(&format!("{ep}/name"), &eq.name, TextKind::Plain);
        if let Some(q) = &eq.quantity {
            v.quantity(&format!("{ep}/quantity"), q, QuantityKind::Amount);
        }
        if let Some(t) = &eq.note {
            v.text(&format!("{ep}/note"), t, TextKind::Plain);
        }
        if let Some(b) = &eq.blob {
            v.blob_ref(&format!("{ep}/blob"), b);
        }
        if let Some(ext) = &eq.ext {
            v.ext(&format!("{ep}/ext"), ext);
        }
    }
    count_opt(
        v,
        &format!("{rp}/step_groups"),
        recipe.step_groups.as_ref(),
        l::MAX_STEP_GROUPS,
    );
    for (i, group) in recipe.step_groups.iter().flatten().enumerate() {
        v.text(&format!("{rp}/step_groups/{i}"), group, TextKind::Plain);
    }
    v.count(&format!("{rp}/steps"), recipe.steps.len(), l::MAX_STEPS);
    for (i, step) in recipe.steps.iter().enumerate() {
        let sp = format!("{rp}/steps/{i}");
        v.prose(&format!("{sp}/text"), &step.text, scope);
        count_opt(
            v,
            &format!("{sp}/ingredients"),
            step.ingredients.as_ref(),
            l::MAX_STEP_INGREDIENTS,
        );
        count_opt(
            v,
            &format!("{sp}/equipment"),
            step.equipment.as_ref(),
            l::MAX_STEP_EQUIPMENT,
        );
        count_opt(
            v,
            &format!("{sp}/timers"),
            step.timers.as_ref(),
            l::MAX_STEP_TIMERS,
        );
        for (ui, used) in step.ingredients.iter().flatten().enumerate() {
            if let Some(q) = &used.quantity {
                v.quantity(
                    &format!("{sp}/ingredients/{ui}/quantity"),
                    q,
                    QuantityKind::Amount,
                );
            }
        }
        for (ti, timer) in step.timers.iter().flatten().enumerate() {
            let tp = format!("{sp}/timers/{ti}");
            v.quantity(
                &format!("{tp}/duration"),
                &timer.duration,
                QuantityKind::Duration,
            );
            if let Some(label) = &timer.label {
                v.text(&format!("{tp}/label"), label, TextKind::Plain);
            }
            if let Some(ext) = &timer.ext {
                v.ext(&format!("{tp}/ext"), ext);
            }
        }
        if let Some(t) = &step.temperature {
            v.rational(&format!("{sp}/temperature/min"), &t.min);
            if let Some(max) = &t.max {
                v.rational(&format!("{sp}/temperature/max"), max);
            }
        }
        if let Some(t) = &step.produces {
            v.text(&format!("{sp}/produces"), t, TextKind::Plain);
        }
        if let Some(note) = &step.note {
            v.prose(&format!("{sp}/note"), note, scope);
        }
        if let Some(b) = &step.blob {
            v.blob_ref(&format!("{sp}/blob"), b);
        }
        if let Some(ext) = &step.ext {
            v.ext(&format!("{sp}/ext"), ext);
        }
    }
    if let Some(n) = &recipe.nutrition {
        v.quantity(
            &format!("{rp}/nutrition/basis"),
            &n.basis,
            QuantityKind::Amount,
        );
        let vp = format!("{rp}/nutrition/values");
        v.count(&vp, n.values.len(), l::MAX_ARRAY_LEN);
        for (code, q) in &n.values {
            let cp = pointer(&vp, &code.as_wire());
            if let OpenCode::Other(other) = code {
                v.slug(&cp, other, SlugKind::Nutrient);
            }
            v.quantity(&cp, q, QuantityKind::Amount);
        }
    }
    let tags = [("tags", &recipe.tags), ("cuisine", &recipe.cuisine)];
    let tag_bounds = [l::MAX_TAGS, l::MAX_CUISINE];
    for ((field, list), max) in tags.into_iter().zip(tag_bounds) {
        let fp = format!("{rp}/{field}");
        count_opt(v, &fp, list.as_ref(), max);
        for (i, tag) in list.iter().flatten().enumerate() {
            v.slug(&format!("{fp}/{i}"), tag, SlugKind::Tag);
        }
    }
    walk_codes(
        v,
        &format!("{rp}/course"),
        recipe.course.as_ref(),
        l::MAX_COURSE,
    );
    walk_codes(v, &format!("{rp}/diet"), recipe.diet.as_ref(), l::MAX_DIET);
    walk_codes(
        v,
        &format!("{rp}/allergens"),
        recipe.allergens.as_ref(),
        l::MAX_ALLERGENS,
    );
    count_opt(
        v,
        &format!("{rp}/media"),
        recipe.media.as_ref(),
        l::MAX_MEDIA,
    );
    for (i, b) in recipe.media.iter().flatten().enumerate() {
        v.blob_ref(&format!("{rp}/media/{i}"), b);
    }
    if let Some(notes) = &recipe.notes {
        v.prose(&format!("{rp}/notes"), notes, scope);
    }
    if let Some(source) = &recipe.source
        && let Some(url) = &source.url
    {
        v.url(&format!("{rp}/source/url"), url);
    }
    if let Some(attribution) = &recipe.attribution {
        v.prose(&format!("{rp}/attribution"), attribution, scope);
    }
    if let Some(ext) = &recipe.ext {
        v.ext(&format!("{rp}/ext"), ext);
    }
}

/// Walks an open-vocabulary list: its length, then each unknown code.
fn walk_codes<'a, T, V: Visitor<'a>>(
    v: &mut V,
    path: &str,
    codes: Option<&'a Vec<OpenCode<T>>>,
    max: usize,
) {
    count_opt(v, path, codes, max);
    for (i, code) in codes.into_iter().flatten().enumerate() {
        if let OpenCode::Other(other) = code {
            v.slug(&format!("{path}/{i}"), other, SlugKind::Code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_payload;
    use crate::model::recipe::{AllergenCode, Nutrition};

    #[test]
    fn a_pointer_token_escapes_tilde_before_slash() {
        assert_eq!(pointer("/ext", "a.b"), "/ext/a.b");
        assert_eq!(pointer("/ext", "a/b~c"), "/ext/a~1b~0c");
        assert_eq!(pointer("", "~1"), "/~01");
    }

    #[test]
    fn walk_value_visits_every_node_in_pre_order() {
        let value = Value::Map(vec![
            (
                Value::Text("a/b".to_owned()),
                Value::Array(vec![Value::Null]),
            ),
            (
                Value::Integer(7.into()),
                Value::Tag(0, Box::new(Value::Bool(true))),
            ),
            (Value::Bool(false), Value::Null),
        ]);
        let mut seen = Vec::new();
        walk_value("", &value, &mut |path, v| {
            seen.push((path.to_owned(), format!("{v:?}")));
        });
        let paths: Vec<&str> = seen.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "", "/a~1b", "/a~1b", "/a~1b/0", "/7", "/7", "/7", "/?", "/?"
            ]
        );
        assert!(seen[6].1.contains("Bool(true)"), "{seen:?}");
    }

    /// Records every slug and count the walk reports.
    #[derive(Default)]
    struct Recorder(Vec<String>);

    impl<'a> Visitor<'a> for Recorder {
        fn slug(&mut self, path: &str, slug: &'a str, kind: SlugKind) {
            self.0.push(format!("slug {path} {slug} {kind:?}"));
        }
        fn identity(&mut self, path: &str, _identity: &'a Identity) {
            self.0.push(format!("identity {path}"));
        }
    }

    #[test]
    fn the_walk_reports_unknown_codes_and_tags_as_slugs() {
        let mut payload = sample_payload();
        let recipe = &mut payload.recipes[0];
        recipe.tags = Some(vec!["sweet".to_owned()]);
        recipe.allergens = Some(vec![
            OpenCode::Known(AllergenCode::Milk),
            OpenCode::Other("kiwi".to_owned()),
        ]);
        recipe.nutrition = Some(Nutrition {
            basis: crate::fixtures::amount(1, "g"),
            values: [(
                OpenCode::Other("x/y".to_owned()),
                crate::fixtures::amount(1, "g"),
            )]
            .into_iter()
            .collect(),
        });
        let mut recorder = Recorder::default();
        walk(&payload, &mut recorder);
        assert_eq!(
            recorder.0,
            [
                "identity /recipes/0/identity",
                "slug /recipes/0/nutrition/values/x~1y x/y Nutrient",
                "slug /recipes/0/tags/0 sweet Tag",
                "slug /recipes/0/allergens/1 kiwi Code",
            ]
        );
    }
}
