//! Payload → draft, for editing an existing recipe. Whatever the draft
//! cannot express is listed in plain words, so nothing is lost unseen:
//! [`super::to_payload`] carries photos, nutrition, and extension data
//! over on save, and the notices say what it cannot carry.

use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::LanguageTag;
use crisp_core::model::text::{LocalizedProse, LocalizedText};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    EquipmentDraft, IngredientDraft, RecipeDraft, SourceDraft, StepDraft, StepIngredientDraft,
    SubstituteDraft, TemperatureDraft, TimerDraft, TimesDraft, YieldDraft, prose, values,
};

/// A recipe as a draft, plus what the draft does not show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Extracted {
    /// The editable draft.
    pub draft: RecipeDraft,
    /// Plain-language notes about content outside the draft.
    pub notices: Vec<String>,
}

struct Reader {
    lang: LanguageTag,
    flattened: Vec<&'static str>,
}

impl Reader {
    fn text(&self, text: &LocalizedText) -> String {
        text.0
            .get(&self.lang)
            .or_else(|| text.0.values().next())
            .cloned()
            .unwrap_or_default()
    }

    fn prose(&mut self, field: &'static str, prose: &LocalizedProse) -> String {
        let Some(chosen) = prose.0.get(&self.lang).or_else(|| prose.0.values().next()) else {
            return String::new();
        };
        let (text, flattened) = prose::render(chosen);
        if flattened && !self.flattened.contains(&field) {
            self.flattened.push(field);
        }
        text
    }
}

/// Converts a single-recipe payload to a draft in its default language.
///
/// # Errors
///
/// Returns a message for a book or a bundle without exactly one recipe.
pub fn from_payload(payload: &BundlePayload) -> Result<Extracted, String> {
    let [recipe] = payload.recipes.as_slice() else {
        return Err("only single-recipe files can be edited, not books".into());
    };
    if payload.kind != Kind::Recipe {
        return Err("only single-recipe files can be edited, not books".into());
    }
    let mut r = Reader {
        lang: payload.default_language.clone(),
        flattened: Vec::new(),
    };
    let lang = r.lang.clone();
    let q = |quantity| values::quantity_text(quantity, &lang);

    let draft = RecipeDraft {
        language: lang.0.clone(),
        title: r.text(&recipe.title),
        summary: recipe.summary.as_ref().map(|s| r.prose("summary", s)),
        yields: YieldDraft {
            amount: q(&recipe.r#yield.amount),
            unit_text: r.text(&recipe.r#yield.unit_text),
        },
        times: recipe.times.as_ref().map(|t| TimesDraft {
            prep: t.prep.as_ref().map(q),
            cook: t.cook.as_ref().map(q),
            rest: t.rest.as_ref().map(q),
            total: t.total.as_ref().map(q),
        }),
        ingredient_groups: recipe
            .ingredient_groups
            .iter()
            .flatten()
            .map(|g| r.text(g))
            .collect(),
        ingredients: recipe
            .ingredients
            .iter()
            .map(|i| IngredientDraft {
                handle: i.r#ref,
                group: i.group,
                name: r.text(&i.name),
                quantity: i.quantity.as_ref().map(q),
                alt: i.alt.iter().flatten().map(q).collect(),
                fixed: i.fixed == Some(true),
                preparation: i.preparation.as_ref().map(|p| r.text(p)),
                note: i.note.as_ref().map(|n| r.text(n)),
                optional: i.optional == Some(true),
                substitutes: i
                    .substitutes
                    .iter()
                    .flatten()
                    .map(|s| SubstituteDraft {
                        name: r.text(&s.name),
                        quantity: s.quantity.as_ref().map(q),
                    })
                    .collect(),
            })
            .collect(),
        equipment: recipe
            .equipment
            .iter()
            .flatten()
            .map(|e| EquipmentDraft {
                handle: e.r#ref,
                name: r.text(&e.name),
                quantity: e.quantity.as_ref().map(q),
                note: e.note.as_ref().map(|n| r.text(n)),
            })
            .collect(),
        step_groups: recipe
            .step_groups
            .iter()
            .flatten()
            .map(|g| r.text(g))
            .collect(),
        steps: recipe
            .steps
            .iter()
            .map(|s| StepDraft {
                handle: Some(s.r#ref),
                group: s.group,
                text: r.prose("steps", &s.text),
                ingredients: s
                    .ingredients
                    .iter()
                    .flatten()
                    .map(|u| StepIngredientDraft {
                        handle: u.r#ref,
                        quantity: u.quantity.as_ref().map(q),
                    })
                    .collect(),
                equipment: s.equipment.clone().unwrap_or_default(),
                timers: s
                    .timers
                    .iter()
                    .flatten()
                    .map(|t| TimerDraft {
                        duration: q(&t.duration),
                        label: t.label.as_ref().map(|l| r.text(l)),
                        passive: t.passive == Some(true),
                    })
                    .collect(),
                heat: s.heat.as_ref().map(values::wire),
                temperature: s.temperature.as_ref().map(|t| TemperatureDraft {
                    value: values::temperature_text(t, &lang),
                    target: t.target.as_ref().map(values::wire),
                }),
                produces: s.produces.as_ref().map(|p| r.text(p)),
                note: s.note.as_ref().map(|n| r.prose("step notes", n)),
            })
            .collect(),
        tags: recipe.tags.clone().unwrap_or_default(),
        cuisine: recipe.cuisine.clone().unwrap_or_default(),
        course: recipe
            .course
            .iter()
            .flatten()
            .map(|c| c.as_wire())
            .collect(),
        diet: recipe.diet.iter().flatten().map(|c| c.as_wire()).collect(),
        allergens: recipe
            .allergens
            .iter()
            .flatten()
            .map(|c| c.as_wire())
            .collect(),
        best_months: recipe.best_months.clone().unwrap_or_default(),
        notes: recipe.notes.as_ref().map(|n| r.prose("notes", n)),
        source: recipe.source.as_ref().map(|s| SourceDraft {
            title: s.title.clone(),
            author: s.author.clone(),
            url: s.url.clone(),
        }),
        attribution: recipe
            .attribution
            .as_ref()
            .map(|a| r.prose("attribution", a)),
        license: Some(payload.meta.license.clone()),
    };
    Ok(Extracted {
        draft,
        notices: notices(payload, &r.flattened),
    })
}

/// Plain-language notes about everything the draft does not carry.
fn notices(payload: &BundlePayload, flattened: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let others: Vec<&str> = payload
        .languages
        .iter()
        .filter(|l| **l != payload.default_language)
        .map(|l| l.0.as_str())
        .collect();
    if !others.is_empty() {
        out.push(format!(
            "This recipe also has text in {}; the draft shows only {}, and saving keeps only {}.",
            others.join(", "),
            payload.default_language.0,
            payload.default_language.0
        ));
    }
    if !flattened.is_empty() {
        out.push(format!(
            "Formatting (bold, italic, internal references) in {} is shown as plain text and \
             will be saved as plain text.",
            flattened.join(", ")
        ));
    }
    let recipe = &payload.recipes[0];
    let mut kept = Vec::new();
    if let Some(media) = recipe.media.as_ref().filter(|m| !m.is_empty()) {
        kept.push(format!("{} photo(s)", media.len()));
    }
    if recipe.nutrition.is_some() {
        kept.push("nutrition facts".to_owned());
    }
    let attached = recipe
        .ingredients
        .iter()
        .any(|i| i.blob.is_some() || i.recipe.is_some())
        || recipe.steps.iter().any(|s| s.blob.is_some())
        || recipe.equipment.iter().flatten().any(|e| e.blob.is_some());
    if attached {
        kept.push("per-item photos and component-recipe links (matched by ref)".to_owned());
    }
    if recipe.ext.is_some() || payload.ext.is_some() || payload.meta.ext.is_some() {
        kept.push("extension data".to_owned());
    }
    if payload.meta.cover.is_some() {
        kept.push("the cover photo".to_owned());
    }
    if !kept.is_empty() {
        out.push(format!(
            "Not shown in the draft but kept on save: {}.",
            kept.join(", ")
        ));
    }
    let lost_ext = recipe
        .ingredients
        .iter()
        .flat_map(|i| i.substitutes.iter().flatten())
        .any(|s| s.ext.is_some())
        || recipe
            .steps
            .iter()
            .flat_map(|s| s.timers.iter().flatten())
            .any(|t| t.ext.is_some());
    if lost_ext {
        out.push("Extension data on substitutes or timers will not be kept on save.".to_owned());
    }
    out
}
