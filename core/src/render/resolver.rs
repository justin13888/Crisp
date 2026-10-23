//! Resolving a recipe's localized text for one view: the language the view
//! is shown in, and the text, prose, and amounts written in it.

use crate::model::payload::BundlePayload;
use crate::model::primitives::{LanguageTag, Quantity};
use crate::model::text::{LocalizedProse, LocalizedText, RunRefKind, RunTarget, StyleFlag};
use crate::render::convert::convert_quantity;
use crate::render::quantity::{Units, format_quantity};
use crate::render::view::{ParagraphView, RunView};
use crate::render::{resolve_prose, resolve_text};

/// Resolves text in the view's language.
pub(super) struct Resolver<'a> {
    preferred: Vec<LanguageTag>,
    default: &'a LanguageTag,
    /// The language the view resolves to.
    pub(super) lang: LanguageTag,
}

impl<'a> Resolver<'a> {
    /// A resolver for `payload` read by someone preferring `languages`:
    /// the view language is the first preferred language the bundle
    /// declares, else its default; the rest of `languages` are fallbacks.
    pub(super) fn new(payload: &'a BundlePayload, languages: &[LanguageTag]) -> Self {
        let lang = languages
            .iter()
            .find(|tag| payload.languages.contains(tag))
            .cloned()
            .unwrap_or_else(|| payload.default_language.clone());
        let mut preferred = vec![lang.clone()];
        preferred.extend(languages.iter().cloned());
        Self {
            preferred,
            default: &payload.default_language,
            lang,
        }
    }

    /// The text in the view language, or `""`.
    pub(super) fn text(&self, text: &LocalizedText) -> String {
        resolve_text(text, &self.preferred, self.default)
            .unwrap_or_default()
            .to_owned()
    }

    /// The text in the view language, or `None` when absent or empty.
    pub(super) fn opt(&self, text: Option<&LocalizedText>) -> Option<String> {
        text.map(|t| self.text(t)).filter(|t| !t.is_empty())
    }

    /// The prose in the view language as styled runs, or no paragraphs.
    pub(super) fn prose(&self, prose: Option<&LocalizedProse>) -> Vec<ParagraphView> {
        let Some(body) = prose.and_then(|p| resolve_prose(p, &self.preferred, self.default)) else {
            return Vec::new();
        };
        body.0
            .iter()
            .map(|paragraph| {
                paragraph
                    .0
                    .iter()
                    .map(|run| {
                        let has = |flag| run.style.iter().flatten().any(|s| *s == flag);
                        RunView {
                            text: run.t.clone(),
                            bold: has(StyleFlag::Bold),
                            italic: has(StyleFlag::Italic),
                            underline: has(StyleFlag::Underline),
                            code: has(StyleFlag::Code),
                            link: run.link.clone(),
                            reference: run.reference.as_ref().map(|r| {
                                let kind = match r.kind {
                                    RunRefKind::Ingredient => "ingredient",
                                    RunRefKind::Step => "step",
                                    RunRefKind::Equipment => "equipment",
                                    RunRefKind::Recipe => "recipe",
                                };
                                match &r.target {
                                    RunTarget::Handle(h) => format!("{kind}:{h}"),
                                    RunTarget::Recipe(id) => format!("{kind}:{}", id.0),
                                }
                            }),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// An amount converted to `units` and formatted, or `""`.
    pub(super) fn amount(&self, quantity: Option<&Quantity>, units: Units) -> String {
        quantity
            .map(|q| format_quantity(&convert_quantity(q, units), &self.lang))
            .unwrap_or_default()
    }
}
