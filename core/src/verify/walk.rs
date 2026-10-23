//! A single traversal of a [`BundlePayload`] that reports every value a
//! check cares about together with its JSON-Pointer path.
//!
//! Every Phase B–D check is a [`Visitor`] over this walk, so the paths in a
//! verification report and in editor validation ([`crate::author`]) agree,
//! and a new field only has to be taught to the walk once.

use crate::crypto::bech32id::Hrp;
use crate::model::book::{Entry, Page};
use crate::model::ids::BlobId;
use crate::model::payload::{BundlePayload, ExtensionMap};
use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::Recipe;
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
}

/// Walks every reportable value in `payload`, in document order.
pub(crate) fn walk<'a, V: Visitor<'a>>(payload: &'a BundlePayload, v: &mut V) {
    v.id("/doc_id", &payload.doc_id.0, doc_hrp(payload));
    v.language("/default_language", &payload.default_language);
    for (i, tag) in payload.languages.iter().enumerate() {
        v.language(&format!("/languages/{i}"), tag);
    }

    let meta = &payload.meta;
    v.text("/meta/title", &meta.title, TextKind::Title);
    for (i, author) in meta.authors.iter().enumerate() {
        v.id(&format!("/meta/authors/{i}/uid"), &author.uid.0, Hrp::User);
    }
    if let Some(identity) = &meta.identity {
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

    for (i, recipe) in payload.recipes.iter().enumerate() {
        walk_recipe(&format!("/recipes/{i}"), i, recipe, v);
    }

    if let Some(book) = &payload.book {
        for (i, page) in book.front_matter.iter().flatten().enumerate() {
            walk_page(&format!("/book/front_matter/{i}"), page, v);
        }
        for (pi, part) in book.parts.iter().enumerate() {
            let pp = format!("/book/parts/{pi}");
            v.text(&format!("{pp}/title"), &part.title, TextKind::Title);
            for (ci, chapter) in part.chapters.iter().enumerate() {
                let cp = format!("{pp}/chapters/{ci}");
                v.text(&format!("{cp}/title"), &chapter.title, TextKind::Title);
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
        for (i, page) in book.back_matter.iter().flatten().enumerate() {
            walk_page(&format!("/book/back_matter/{i}"), page, v);
        }
    }

    for id in payload.blobs.keys() {
        v.id(&format!("/blobs/{}", id.0), &id.0, Hrp::Blob);
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
    for (i, group) in recipe.ingredient_groups.iter().flatten().enumerate() {
        v.text(
            &format!("{rp}/ingredient_groups/{i}"),
            group,
            TextKind::Plain,
        );
    }
    for (i, ing) in recipe.ingredients.iter().enumerate() {
        let ip = format!("{rp}/ingredients/{i}");
        v.text(&format!("{ip}/name"), &ing.name, TextKind::Plain);
        if let Some(q) = &ing.quantity {
            v.quantity(&format!("{ip}/quantity"), q, QuantityKind::Amount);
        }
        for (ai, q) in ing.alt.iter().flatten().enumerate() {
            v.quantity(&format!("{ip}/alt/{ai}"), q, QuantityKind::Amount);
        }
        if let Some(t) = &ing.preparation {
            v.text(&format!("{ip}/preparation"), t, TextKind::Plain);
        }
        if let Some(t) = &ing.note {
            v.text(&format!("{ip}/note"), t, TextKind::Plain);
        }
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
    for (i, group) in recipe.step_groups.iter().flatten().enumerate() {
        v.text(&format!("{rp}/step_groups/{i}"), group, TextKind::Plain);
    }
    for (i, step) in recipe.steps.iter().enumerate() {
        let sp = format!("{rp}/steps/{i}");
        v.prose(&format!("{sp}/text"), &step.text, scope);
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
        for (code, q) in &n.values {
            v.quantity(
                &format!("{rp}/nutrition/values/{}", code.as_wire()),
                q,
                QuantityKind::Amount,
            );
        }
    }
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
