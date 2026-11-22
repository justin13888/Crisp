//! Draft → payload. Every conversion problem is collected with a JSON
//! Pointer into the draft, so one round trip reports all of them.

use std::collections::BTreeMap;

use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::{LanguageTag, Quantity};
use crisp_core::model::recipe::{
    Equipment, Ingredient, Recipe, Source, Step, StepIngredient, Substitute, Timer, Times, Yield,
};
use crisp_core::model::text::{LocalizedProse, LocalizedText};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::blank::{blank_payload, blank_recipe};
use super::carry::{prune_blobs, step_handles};
use super::{RecipeDraft, prose, values};

/// One problem with a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DraftIssue {
    /// A JSON Pointer into the draft, e.g. `/ingredients/2/quantity`; empty
    /// for the whole draft.
    pub path: String,
    /// The format's stable error code, when the format's checks raised it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// What is wrong.
    pub message: String,
}

/// Collects issues while converting.
struct Issues(Vec<DraftIssue>);

impl Issues {
    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.0.push(DraftIssue {
            path: path.into(),
            code: None,
            message: message.into(),
        });
    }

    fn take<T>(&mut self, path: impl Into<String>, result: Result<T, String>) -> Option<T> {
        result.map_err(|m| self.push(path, m)).ok()
    }
}

/// Builds the converters that close over the draft's language.
struct Lang(LanguageTag);

impl Lang {
    fn text(&self, s: &str) -> LocalizedText {
        LocalizedText(BTreeMap::from([(self.0.clone(), s.trim().to_owned())]))
    }

    fn prose(&self, issues: &mut Issues, path: &str, s: &str) -> Option<LocalizedProse> {
        issues
            .take(path, prose::parse(s))
            .map(|p| LocalizedProse(BTreeMap::from([(self.0.clone(), p)])))
    }

    fn quantity(&self, issues: &mut Issues, path: &str, s: &str) -> Option<Quantity> {
        issues.take(path, values::quantity(s, &self.0))
    }
}

fn nonempty<T>(items: Vec<T>) -> Option<Vec<T>> {
    (!items.is_empty()).then_some(items)
}

/// Converts a draft to a payload ready for `seal`.
///
/// With a `base` (the payload of the file being revised or forked), the
/// recipe's identity is kept so `seal` links the history, and everything the
/// draft cannot express — photos, nutrition, extension data, a component
/// recipe link — is carried over rather than dropped.
///
/// # Errors
///
/// Returns every problem found, each with a pointer into the draft.
pub fn to_payload(
    draft: &RecipeDraft,
    base: Option<&BundlePayload>,
) -> Result<BundlePayload, Vec<DraftIssue>> {
    let mut issues = Issues(Vec::new());
    let lang = Lang(LanguageTag(draft.language.trim().to_owned()));
    if lang.0.0.is_empty() {
        issues.push(
            "/language",
            "a BCP-47 language tag such as \"en\" is required",
        );
    }
    let mut payload = match base {
        Some(base) if base.kind == Kind::Recipe && base.recipes.len() == 1 => base.clone(),
        Some(_) => {
            issues.push("", "only single-recipe files can be edited, not books");
            return Err(issues.0);
        }
        None => blank_payload(&lang.0),
    };
    let prior = payload.recipes.remove(0);
    let mut recipe = Recipe {
        identity: prior.identity.clone(),
        id: prior.id.clone(),
        revision: prior.revision.clone(),
        supersedes: prior.supersedes.clone(),
        derived_from: prior.derived_from.clone(),
        nutrition: prior.nutrition.clone(),
        media: prior.media.clone(),
        ext: prior.ext.clone(),
        ..blank_recipe()
    };

    recipe.title = lang.text(&draft.title);
    if draft.title.trim().is_empty() {
        issues.push("/title", "a title is required");
    }
    recipe.summary = draft
        .summary
        .as_deref()
        .and_then(|s| lang.prose(&mut issues, "/summary", s));
    recipe.r#yield = Yield {
        amount: lang
            .quantity(&mut issues, "/yield/amount", &draft.yields.amount)
            .unwrap_or_default(),
        unit_text: lang.text(&draft.yields.unit_text),
    };
    recipe.times = draft.times.as_ref().map(|t| {
        let mut one = |name: &str, value: &Option<String>| {
            value
                .as_deref()
                .and_then(|v| issues.take(format!("/times/{name}"), values::duration(v)))
        };
        Times {
            prep: one("prep", &t.prep),
            cook: one("cook", &t.cook),
            rest: one("rest", &t.rest),
            total: one("total", &t.total),
        }
    });
    recipe.ingredient_groups = nonempty(
        draft
            .ingredient_groups
            .iter()
            .map(|g| lang.text(g))
            .collect(),
    );
    recipe.step_groups = nonempty(draft.step_groups.iter().map(|g| lang.text(g)).collect());

    recipe.ingredients = draft
        .ingredients
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let at = |field: &str| format!("/ingredients/{i}/{field}");
            let kept = prior.ingredients.iter().find(|p| p.r#ref == d.handle);
            Ingredient {
                r#ref: d.handle,
                group: d.group,
                name: lang.text(&d.name),
                code: kept.and_then(|k| k.code.clone()),
                quantity: d
                    .quantity
                    .as_deref()
                    .and_then(|q| lang.quantity(&mut issues, &at("quantity"), q)),
                alt: nonempty(
                    d.alt
                        .iter()
                        .enumerate()
                        .filter_map(|(j, q)| {
                            lang.quantity(&mut issues, &at(&format!("alt/{j}")), q)
                        })
                        .collect(),
                ),
                fixed: d.fixed.then_some(true),
                preparation: d.preparation.as_deref().map(|p| lang.text(p)),
                note: d.note.as_deref().map(|n| lang.text(n)),
                optional: d.optional.then_some(true),
                substitutes: nonempty(
                    d.substitutes
                        .iter()
                        .enumerate()
                        .map(|(j, s)| {
                            let name = lang.text(&s.name);
                            Substitute {
                                code: kept_substitute_code(kept, &name),
                                name,
                                quantity: s.quantity.as_deref().and_then(|q| {
                                    lang.quantity(
                                        &mut issues,
                                        &at(&format!("substitutes/{j}/quantity")),
                                        q,
                                    )
                                }),
                                ext: None,
                            }
                        })
                        .collect(),
                ),
                recipe: kept.and_then(|k| k.recipe.clone()),
                blob: kept.and_then(|k| k.blob.clone()),
                ext: kept.and_then(|k| k.ext.clone()),
            }
        })
        .collect();

    recipe.equipment = nonempty(
        draft
            .equipment
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let kept = prior
                    .equipment
                    .iter()
                    .flatten()
                    .find(|p| p.r#ref == d.handle);
                Equipment {
                    r#ref: d.handle,
                    name: lang.text(&d.name),
                    quantity: d.quantity.as_deref().and_then(|q| {
                        lang.quantity(&mut issues, &format!("/equipment/{i}/quantity"), q)
                    }),
                    note: d.note.as_deref().map(|n| lang.text(n)),
                    blob: kept.and_then(|k| k.blob.clone()),
                    ext: kept.and_then(|k| k.ext.clone()),
                }
            })
            .collect(),
    );

    if draft.steps.is_empty() {
        issues.push("/steps", "at least one step is required");
    }
    let handles = step_handles(&draft.steps, &prior);
    recipe.steps = draft
        .steps
        .iter()
        .zip(handles)
        .enumerate()
        .map(|(i, (d, handle))| step(&lang, &mut issues, &prior, i, handle, d))
        .collect();

    let trimmed = |items: &[String]| nonempty(items.iter().map(|s| s.trim().to_owned()).collect());
    recipe.tags = trimmed(&draft.tags);
    recipe.cuisine = trimmed(&draft.cuisine);
    recipe.course = nonempty(draft.course.iter().map(|c| values::open_code(c)).collect());
    recipe.diet = nonempty(draft.diet.iter().map(|c| values::open_code(c)).collect());
    recipe.allergens = nonempty(
        draft
            .allergens
            .iter()
            .map(|c| values::open_code(c))
            .collect(),
    );
    recipe.best_months = nonempty(draft.best_months.clone());
    recipe.notes = draft
        .notes
        .as_deref()
        .and_then(|s| lang.prose(&mut issues, "/notes", s));
    recipe.attribution = draft
        .attribution
        .as_deref()
        .and_then(|s| lang.prose(&mut issues, "/attribution", s));
    recipe.source = draft.source.as_ref().map(|s| Source {
        title: s.title.clone(),
        author: s.author.clone(),
        url: s.url.clone(),
    });

    payload.default_language = lang.0.clone();
    payload.languages = vec![lang.0.clone()];
    payload.meta.title = lang.text(&draft.title);
    if let Some(license) = &draft.license {
        license.trim().clone_into(&mut payload.meta.license);
    }
    payload.meta.description = payload.meta.description.take().and_then(|d| {
        let kept: BTreeMap<_, _> = d.0.into_iter().filter(|(l, _)| *l == lang.0).collect();
        (!kept.is_empty()).then_some(LocalizedProse(kept))
    });
    payload.recipes = vec![recipe];
    prune_blobs(&mut payload);

    if issues.0.is_empty() {
        Ok(payload)
    } else {
        Err(issues.0)
    }
}

/// Converts step `i`. Photos and extension data carry over only from the
/// base step with the same explicit `ref`.
fn step(
    lang: &Lang,
    issues: &mut Issues,
    prior: &Recipe,
    i: usize,
    handle: u64,
    d: &super::StepDraft,
) -> Step {
    let at = |field: &str| format!("/steps/{i}/{field}");
    let kept = d
        .handle
        .and_then(|h| prior.steps.iter().find(|p| p.r#ref == h));
    Step {
        r#ref: handle,
        group: d.group,
        text: lang
            .prose(issues, &at("text"), &d.text)
            .unwrap_or(LocalizedProse(BTreeMap::new())),
        ingredients: nonempty(
            d.ingredients
                .iter()
                .enumerate()
                .map(|(j, u)| StepIngredient {
                    r#ref: u.handle,
                    quantity: u.quantity.as_deref().and_then(|q| {
                        lang.quantity(issues, &at(&format!("ingredients/{j}/quantity")), q)
                    }),
                })
                .collect(),
        ),
        equipment: nonempty(d.equipment.clone()),
        timers: nonempty(
            d.timers
                .iter()
                .enumerate()
                .filter_map(|(j, t)| {
                    let duration = issues.take(
                        at(&format!("timers/{j}/duration")),
                        values::duration(&t.duration),
                    )?;
                    Some(Timer {
                        duration,
                        label: t.label.as_deref().map(|l| lang.text(l)),
                        passive: t.passive.then_some(true),
                        ext: None,
                    })
                })
                .collect(),
        ),
        heat: d
            .heat
            .as_deref()
            .and_then(|h| issues.take(at("heat"), values::heat(h))),
        temperature: d.temperature.as_ref().and_then(|t| {
            issues.take(
                at("temperature"),
                values::temperature(&t.value, t.target.as_deref(), &lang.0),
            )
        }),
        produces: d.produces.as_deref().map(|p| lang.text(p)),
        note: d
            .note
            .as_deref()
            .and_then(|n| lang.prose(issues, &at("note"), n)),
        blob: kept.and_then(|k| k.blob.clone()),
        ext: kept.and_then(|k| k.ext.clone()),
    }
}

/// The index slug a base ingredient gave the same-named substitute, so a
/// revision keeps codes the draft has no field for.
fn kept_substitute_code(kept: Option<&Ingredient>, name: &LocalizedText) -> Option<String> {
    kept?
        .substitutes
        .iter()
        .flatten()
        .find(|p| &p.name == name)?
        .code
        .clone()
}
