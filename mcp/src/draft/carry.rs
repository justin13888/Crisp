//! What a revision carries over from its base: stable step handles, and
//! only the photos something still references.

use std::collections::BTreeSet;

use crisp_core::model::payload::BundlePayload;
use crisp_core::model::recipe::Recipe;

/// Drops photos nothing references any more (a removed step's, say), so
/// their bytes are not carried into the signed file.
pub(super) fn prune_blobs(payload: &mut BundlePayload) {
    let recipe = &payload.recipes[0];
    let referenced: BTreeSet<String> = recipe
        .media
        .iter()
        .flatten()
        .chain(payload.meta.cover.iter())
        .chain(recipe.ingredients.iter().filter_map(|i| i.blob.as_ref()))
        .chain(recipe.steps.iter().filter_map(|s| s.blob.as_ref()))
        .chain(
            recipe
                .equipment
                .iter()
                .flatten()
                .filter_map(|e| e.blob.as_ref()),
        )
        .map(|b| b.0.clone())
        .collect();
    payload.blobs.retain(|id, _| referenced.contains(&id.0));
}

/// Each step's `ref`: its own when the draft gives one, else the lowest
/// number no step of the draft or the base recipe uses, so a new step never
/// takes over a removed step's photo.
pub(super) fn step_handles(steps: &[super::StepDraft], prior: &Recipe) -> Vec<u64> {
    let mut taken: BTreeSet<u64> = steps
        .iter()
        .filter_map(|s| s.handle)
        .chain(prior.steps.iter().map(|s| s.r#ref))
        .collect();
    let mut next = 1;
    steps
        .iter()
        .map(|s| {
            s.handle.unwrap_or_else(|| {
                while taken.contains(&next) {
                    next += 1;
                }
                taken.insert(next);
                next
            })
        })
        .collect()
}
